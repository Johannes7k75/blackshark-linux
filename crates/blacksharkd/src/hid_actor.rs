use std::time::{Duration, Instant};

use anyhow::Result;
use hidapi::HidDevice;
use tokio::sync::{mpsc, oneshot, watch};

use blackshark_device as device;
use blackshark_protocol::{cmd, Report};

use crate::state::SharedState;

// ---------------------------------------------------------------------------
// Public command API
// ---------------------------------------------------------------------------

pub struct BatteryState {
    pub percentage: u8,
    pub charging:   bool,
}

pub enum HidCommand {
    SetSidetone { level: u8, reply: oneshot::Sender<Result<()>> },
    GetBattery  { reply: oneshot::Sender<Result<BatteryState>> },
    /// Periodic wakeup sent by a tokio timer — drives reconnect + battery poll.
    Tick,
}

// ---------------------------------------------------------------------------
// Actor entry point
// ---------------------------------------------------------------------------

const BATTERY_POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Spawn the HID actor on a dedicated OS thread.
///
/// `HidDevice` is not `Send`, so all HID I/O stays on this thread.
/// Communication with async callers is via the mpsc channel + oneshot replies.
pub fn spawn(rx: mpsc::Receiver<HidCommand>, state_tx: watch::Sender<SharedState>) {
    std::thread::Builder::new()
        .name("hid-actor".into())
        .spawn(move || run(rx, state_tx))
        .expect("failed to spawn hid-actor thread");
}

fn run(mut rx: mpsc::Receiver<HidCommand>, state_tx: watch::Sender<SharedState>) {
    let mut dev: Option<HidDevice> = try_open(&state_tx);
    let mut next_battery_poll = Instant::now(); // poll immediately on first tick

    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            HidCommand::Tick => {
                if dev.is_none() {
                    dev = try_open(&state_tx);
                }
                if Instant::now() >= next_battery_poll {
                    if let Some(d) = &dev {
                        match query_battery(d) {
                            Ok(b) => {
                                next_battery_poll = Instant::now() + BATTERY_POLL_INTERVAL;
                                state_tx.send_modify(|s| {
                                    s.battery_pct = b.percentage;
                                    s.charging    = b.charging;
                                });
                            }
                            Err(e) => {
                                eprintln!("battery poll failed: {e}");
                                dev = None;
                                state_tx.send_modify(|s| s.connected = false);
                            }
                        }
                    }
                }
            }

            HidCommand::SetSidetone { level, reply } => {
                let result = with_dev(&mut dev, &state_tx, |d| set_sidetone(d, level));
                if result.is_ok() {
                    state_tx.send_modify(|s| s.sidetone = level);
                }
                let _ = reply.send(result);
            }

            HidCommand::GetBattery { reply } => {
                let result = with_dev(&mut dev, &state_tx, query_battery);
                if let Ok(ref b) = result {
                    state_tx.send_modify(|s| {
                        s.battery_pct = b.percentage;
                        s.charging    = b.charging;
                    });
                }
                let _ = reply.send(result);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn try_open(state_tx: &watch::Sender<SharedState>) -> Option<HidDevice> {
    match device::open() {
        Ok(d) => {
            eprintln!("headset connected");
            // Read initial state from device.
            let battery  = query_battery(&d).ok();
            let sidetone = query_sidetone(&d).ok();
            state_tx.send_modify(|s| {
                s.connected = true;
                if let Some(b) = battery  { s.battery_pct = b.percentage; s.charging = b.charging; }
                if let Some(v) = sidetone  { s.sidetone = v; }
            });
            Some(d)
        }
        Err(e) => {
            eprintln!("headset not found: {e}");
            None
        }
    }
}

/// Run `f` with the current device, clearing it on I/O failure.
fn with_dev<T, F>(
    dev: &mut Option<HidDevice>,
    state_tx: &watch::Sender<SharedState>,
    f: F,
) -> Result<T>
where
    F: FnOnce(&HidDevice) -> Result<T>,
{
    match dev {
        None => anyhow::bail!("headset not connected"),
        Some(d) => {
            let result = f(d);
            if result.is_err() {
                eprintln!("headset disconnected");
                *dev = None;
                state_tx.send_modify(|s| s.connected = false);
            }
            result
        }
    }
}

// ---------------------------------------------------------------------------
// HID operations
// ---------------------------------------------------------------------------

fn set_sidetone(dev: &HidDevice, level: u8) -> Result<()> {
    let get = Report::new(0x60, cmd::SIDETONE_GET_CLASS, cmd::SIDETONE_ID, &[cmd::SIDETONE_GET_ARG, 0x00]);
    device::send(dev, &get)?;
    let set = Report::new(0x60, cmd::SIDETONE_SET_CLASS, cmd::SIDETONE_ID, &[level, 0x00]);
    device::send(dev, &set)?;
    Ok(())
}

fn query_battery(dev: &HidDevice) -> Result<BatteryState> {
    let report   = Report::new(0x60, cmd::BATTERY_CLASS, cmd::BATTERY_ID, &[0x00]);
    let response = device::send(dev, &report)?;
    let args     = response.args();
    anyhow::ensure!(args.len() >= 2, "battery response too short");
    Ok(BatteryState { percentage: args[0], charging: args[1] != 0x00 })
}

fn query_sidetone(dev: &HidDevice) -> Result<u8> {
    let report   = Report::new(0x60, cmd::SIDETONE_READ_CLASS, 0x00, &[0x00]);
    let response = device::send(dev, &report)?;
    let args     = response.args();
    anyhow::ensure!(!args.is_empty(), "sidetone response empty");
    Ok(args[0])
}
