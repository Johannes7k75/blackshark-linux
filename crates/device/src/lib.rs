use anyhow::{bail, Context, Result};
use hidapi::{HidApi, HidDevice};
use tracing::info;

use blackshark_protocol::{Report, ResponseStatus, REPORT_LEN};

const VID: u16 = 0x1532;
const PID_V3_PRO: u16 = 0x0577;
const PID_V2_HS: u16 = 0x0565;
const PID_V3_X: u16 = 0x057d;

/// Open the BlackShark HID device.
///
/// Must open the proprietary control interface specifically (Interface 5 for V3 Pro, Interface 3 for V2 HS)
/// — the dongle exposes multiple HID interfaces and api.open(VID, PID) picks the first
/// enumerated, which varies across systems.
pub fn open() -> Result<HidDevice> {
    let api = HidApi::new().context("failed to initialise hidapi")?;

    let mut target = None;
    let mut interfaces_found = Vec::new();
    for info in api.device_list() {
        if info.vendor_id() == VID {
            let pid = info.product_id();
            if pid == PID_V3_PRO || pid == PID_V2_HS || pid == PID_V3_X {
                let path = info.path().to_string_lossy();
                info!(
                    interface = info.interface_number(),
                    path = %path,
                    pid = pid,
                    "found BlackShark hidraw interface"
                );
                interfaces_found.push((pid, info.clone()));
            }
        }
    }

    // Try to match the specific target interface number first
    for &(pid, ref info) in &interfaces_found {
        let target_interface = if pid == PID_V3_PRO { 5 } else { 3 };
        if info.interface_number() == target_interface {
            target = Some(info.clone());
        }
    }

    // Fallback: If we only found exactly one HID interface for this device, use it!
    if target.is_none() && interfaces_found.len() == 1 {
        target = Some(interfaces_found[0].1.clone());
    }

    match target {
        None => bail!("BlackShark headset not found — is the dongle plugged in and do you have udev permission?"),
        Some(info) => {
            let path = info.path().to_string_lossy().into_owned();
            let dev = info
                .open_device(&api)
                .context("found BlackShark headset but failed to open control interface — check udev permissions")?;
            info!(path = %path, "opened BlackShark control interface");
            Ok(dev)
        }
    }
}

/// Send a report and return Ok if ANY 64-byte response arrives (regardless of status).
/// Used as a wireless link readiness probe — any response means the link is up.
pub fn send_probe(dev: &HidDevice, report: &Report) -> Result<()> {
    dev.write(report.as_bytes()).context("HID write failed")?;
    let mut buf = [0u8; REPORT_LEN];
    let n = dev
        .read_timeout(&mut buf, 2_000)
        .context("HID read failed")?;
    if n != REPORT_LEN {
        bail!("short read: expected {REPORT_LEN} bytes, got {n}");
    }
    Ok(())
}

/// Write a report without waiting for a response (fire-and-forget).
/// Used for init handshake commands where the side-effect of sending
/// matters but the response may not arrive or may be ignored.
pub fn send_no_wait(dev: &HidDevice, report: &Report) -> Result<()> {
    dev.write(report.as_bytes()).context("HID write failed")?;
    Ok(())
}

#[derive(Debug)]
pub struct DeviceStatusError {
    pub status: ResponseStatus,
    pub raw: u8,
}

impl std::fmt::Display for DeviceStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "device returned error status: {:?} (raw=0x{:02x})", self.status, self.raw)
    }
}

impl std::error::Error for DeviceStatusError {}

/// Send a report and read back the response.
///
/// Razer devices echo the command back with the status byte set.
pub fn send(dev: &HidDevice, report: &Report) -> Result<Report> {
    dev.write(report.as_bytes()).context("HID write failed")?;

    let mut buf = [0u8; REPORT_LEN];
    let n = dev
        .read_timeout(&mut buf, 5_000)
        .context("HID read failed")?;

    if n != REPORT_LEN {
        bail!("short read: expected {REPORT_LEN} bytes, got {n}");
    }

    let response = Report::from_bytes(buf);

    match response.status() {
        ResponseStatus::Ok => Ok(response),
        other => Err(anyhow::Error::new(DeviceStatusError {
            status: other,
            raw: response.as_bytes()[1],
        })),
    }
}
