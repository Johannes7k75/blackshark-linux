use std::time::{Duration, Instant};

use anyhow::Result;
use hidapi::HidDevice;
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{debug, info, warn};

use blackshark_device as device;
use blackshark_protocol::{cmd, Report};

use crate::config::Config;
use crate::state::SharedState;

// ---------------------------------------------------------------------------
// Public command API
// ---------------------------------------------------------------------------

pub struct BatteryState {
    pub percentage: u8,
    pub charging:   bool,
}

pub enum HidCommand {
    SetSidetone     { level: u8,             reply: oneshot::Sender<Result<()>> },
    GetBattery      {                         reply: oneshot::Sender<Result<BatteryState>> },
    SetThx          { enabled: bool,          reply: oneshot::Sender<Result<()>> },
    SetAnc          { enabled: bool, level: u8, reply: oneshot::Sender<Result<()>> },
    SetPowerSavings { minutes: u8,            reply: oneshot::Sender<Result<()>> },
    /// Sent when config changes — restores all settings to the device.
    ApplyConfig { config: Config },
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
pub fn spawn(rx: mpsc::Receiver<HidCommand>, state_tx: watch::Sender<SharedState>, initial_config: Config) {
    std::thread::Builder::new()
        .name("hid-actor".into())
        .spawn(move || run(rx, state_tx, initial_config))
        .expect("failed to spawn hid-actor thread");
}

fn run(mut rx: mpsc::Receiver<HidCommand>, state_tx: watch::Sender<SharedState>, initial_config: Config) {
    let mut dev: Option<HidDevice> = try_open(&state_tx, Some(&initial_config));
    let mut next_battery_poll = Instant::now(); // poll immediately on first tick

    while let Some(cmd) = rx.blocking_recv() {
        match cmd {
            HidCommand::Tick => {
                if dev.is_none() {
                    dev = try_open(&state_tx, None);
                }
                if Instant::now() >= next_battery_poll {
                    if let Some(d) = &dev {
                        match query_battery(d) {
                            Ok(b) => {
                                next_battery_poll = Instant::now() + BATTERY_POLL_INTERVAL;
                                debug!(percentage = b.percentage, charging = b.charging, "battery poll");
                                state_tx.send_modify(|s| {
                                    s.battery_pct = b.percentage;
                                    s.charging    = b.charging;
                                });
                            }
                            Err(e) => {
                                warn!("battery poll failed: {e}");
                                dev = None;
                                state_tx.send_modify(|s| s.connected = false);
                            }
                        }
                    }
                }
            }

            HidCommand::SetSidetone { level, reply } => {
                info!(level, "set_sidetone");
                let result = with_dev(&mut dev, &state_tx, |d| set_sidetone(d, level));
                match &result {
                    Ok(()) => {
                        info!(level, "set_sidetone ok");
                        state_tx.send_modify(|s| s.sidetone = level);
                    }
                    Err(e) => warn!("set_sidetone failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::SetThx { enabled, reply } => {
                info!(enabled, "set_thx");
                let result = with_dev(&mut dev, &state_tx, |d| set_thx(d, enabled));
                match &result {
                    Ok(()) => {
                        info!(enabled, "set_thx ok");
                        state_tx.send_modify(|s| s.thx_enabled = enabled);
                    }
                    Err(e) => warn!("set_thx failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::SetAnc { enabled, level, reply } => {
                info!(enabled, level, "set_anc");
                let result = with_dev(&mut dev, &state_tx, |d| set_anc(d, enabled, level));
                match &result {
                    Ok(()) => {
                        info!(enabled, level, "set_anc ok");
                        state_tx.send_modify(|s| { s.anc_enabled = enabled; s.anc_level = level; });
                    }
                    Err(e) => warn!("set_anc failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::SetPowerSavings { minutes, reply } => {
                info!(minutes, "set_power_savings");
                let result = with_dev(&mut dev, &state_tx, |d| set_power_savings(d, minutes));
                match &result {
                    Ok(()) => {
                        info!(minutes, "set_power_savings ok");
                        state_tx.send_modify(|s| s.power_savings_minutes = minutes);
                    }
                    Err(e) => warn!("set_power_savings failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::GetBattery { reply } => {
                info!("get_battery");
                let result = with_dev(&mut dev, &state_tx, query_battery);
                match &result {
                    Ok(b) => {
                        info!(percentage = b.percentage, charging = b.charging, "get_battery ok");
                        state_tx.send_modify(|s| {
                            s.battery_pct = b.percentage;
                            s.charging    = b.charging;
                        });
                    }
                    Err(e) => warn!("get_battery failed: {e}"),
                }
                let _ = reply.send(result);
            }

            HidCommand::ApplyConfig { config } => {
                if let Some(d) = &dev {
                    restore_config(d, &config);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Open the device and optionally restore config immediately.
fn try_open(state_tx: &watch::Sender<SharedState>, config: Option<&Config>) -> Option<HidDevice> {
    match device::open() {
        Ok(d) => {
            let battery  = query_battery(&d).ok();
            let sidetone = query_sidetone(&d).ok();
            info!(
                battery_pct = battery.as_ref().map(|b| b.percentage),
                sidetone,
                "headset connected"
            );
            state_tx.send_modify(|s| {
                s.connected = true;
                if let Some(b) = &battery  { s.battery_pct = b.percentage; s.charging = b.charging; }
                if let Some(v) = sidetone   { s.sidetone = v; }
            });
            if let Some(cfg) = config {
                restore_config(&d, cfg);
            }
            Some(d)
        }
        Err(_) => None,
    }
}

/// Apply all config values to the device. Logs but does not fail on errors —
/// best-effort restore so a single bad command doesn't block the rest.
fn restore_config(dev: &HidDevice, config: &Config) {
    info!(
        sidetone = config.sidetone,
        thx = config.thx_enabled,
        anc = config.anc_enabled,
        power_savings = config.power_savings_minutes,
        "restoring config to device"
    );

    if let Err(e) = set_sidetone(dev, config.sidetone) {
        warn!("restore sidetone failed: {e}");
    }
    if let Err(e) = set_thx(dev, config.thx_enabled) {
        warn!("restore thx failed: {e}");
    }
    if let Err(e) = set_anc(dev, config.anc_enabled, config.anc_level) {
        warn!("restore anc failed: {e}");
    }
    if let Err(e) = set_power_savings(dev, config.power_savings_minutes) {
        warn!("restore power_savings failed: {e}");
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
                warn!("headset disconnected");
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

fn set_thx(dev: &HidDevice, enabled: bool) -> Result<()> {
    let mode = if enabled { cmd::THX_SPATIAL } else { cmd::THX_STEREO };
    let report = Report::new(0x60, cmd::THX_CLASS, cmd::THX_ID, &[mode, 0x00]);
    device::send(dev, &report)?;
    Ok(())
}

fn set_anc(dev: &HidDevice, enabled: bool, level: u8) -> Result<()> {
    let level = level.clamp(cmd::ANC_LEVEL_MIN, cmd::ANC_LEVEL_MAX);
    let report = Report::new(0x60, cmd::ANC_CLASS, cmd::ANC_ID,
                             &[enabled as u8, level, 0x00]);
    device::send(dev, &report)?;
    Ok(())
}

fn set_power_savings(dev: &HidDevice, minutes: u8) -> Result<()> {
    let report = Report::new(0x60, cmd::POWER_SAVINGS_CLASS, cmd::POWER_SAVINGS_ID,
                             &[minutes, 0x00]);
    device::send(dev, &report)?;
    Ok(())
}

fn query_sidetone(dev: &HidDevice) -> Result<u8> {
    let report   = Report::new(0x60, cmd::SIDETONE_READ_CLASS, 0x00, &[0x00]);
    let response = device::send(dev, &report)?;
    let args     = response.args();
    anyhow::ensure!(!args.is_empty(), "sidetone response empty");
    Ok(args[0])
}
