/// Battery reporting via the UPower D-Bus interface.
///
/// We expose ourselves as a UPower device so GNOME (and anything else that
/// speaks UPower) picks up the headset battery level automatically — same
/// integration as a kernel power_supply driver, but from userspace.
///
/// UPower interface: org.freedesktop.UPower.Device
/// Well-known path:  /org/freedesktop/UPower/devices/headset_blackshark_v3_pro
///
/// TODO: implement UPower D-Bus service once basic querying is confirmed working.

use anyhow::{bail, Context, Result};

use crate::device;
use crate::protocol::{cmd, Report};

/// Battery state as reported to UPower.
#[derive(Debug, Clone, Copy)]
pub struct BatteryState {
    /// Percentage, 0–100.
    pub percentage: u8,
    /// Whether the device is currently on charge.
    pub charging: bool,
}

/// Query the headset for its current battery state.
///
/// Protocol (confirmed from Synapse startup pcap):
///   GET  cls=0x21, id=0x00, args=[0x00]
///   Response args[0] = percentage (0–100 direct)
///   Response args[1] = charging flag (0x00 = not charging)
pub fn query(dev: &hidapi::HidDevice) -> Result<BatteryState> {
    let report = Report::new(0x60, cmd::BATTERY_CLASS, cmd::BATTERY_ID, &[0x00]);
    let response = device::send(dev, &report).context("battery query failed")?;

    let args = response.args();
    if args.len() < 2 {
        bail!("battery response too short: got {} bytes, expected 2", args.len());
    }

    Ok(BatteryState {
        percentage: args[0],
        charging:   args[1] != 0x00,
    })
}

/// Publish battery state over D-Bus so UPower / GNOME can display it.
///
/// TODO: implement the UPower Device interface with zbus.
pub async fn publish_upower(_state: BatteryState) -> Result<()> {
    bail!("UPower D-Bus publishing not yet implemented")
}
