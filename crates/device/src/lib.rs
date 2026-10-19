use anyhow::{bail, Context, Result};
use hidapi::{HidApi, HidDevice};
use std::time::{Duration, Instant};

use tracing::{debug, info};

use blackshark_protocol::{Report, ResponseStatus, REPORT_LEN};

const VID: u16 = 0x1532;
const PID: u16 = 0x0577;

/// Open the BlackShark V3 Pro HID device.
///
/// Must open interface 5 specifically — the dongle exposes multiple HID interfaces
/// and api.open(VID, PID) picks the first enumerated, which varies across systems.
/// Interface 5 is the proprietary control interface (interrupt IN, endpoint 0x84).
pub fn open() -> Result<HidDevice> {
    let api = HidApi::new().context("failed to initialise hidapi")?;

    let mut target = None;
    for info in api.device_list() {
        if info.vendor_id() == VID && info.product_id() == PID {
            let path = info.path().to_string_lossy();
            debug!(
                interface = info.interface_number(),
                path = %path,
                "found BlackShark hidraw interface"
            );
            if info.interface_number() == 5 {
                target = Some(info.clone());
            }
        }
    }

    match target {
        None => bail!("BlackShark V3 Pro not found — is the dongle plugged in and do you have udev permission?"),
        Some(info) => {
            let path = info.path().to_string_lossy().into_owned();
            let dev = info
                .open_device(&api)
                .context("found BlackShark V3 Pro but failed to open control interface — check udev permissions")?;
            info!(path = %path, "opened BlackShark control interface");
            Ok(dev)
        }
    }
}

/// The device accepted the write but sent no matching response within the timeout.
///
/// Distinct from an I/O error: the hidraw handle is still valid, but the dongle's
/// wireless link to the headset is not up (headset off, unpaired, or on Bluetooth).
#[derive(Debug)]
pub struct NoResponse;

impl std::fmt::Display for NoResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no response from dongle")
    }
}

impl std::error::Error for NoResponse {}

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const BUSY_RETRIES: u32 = 3;

/// Discard any reports already queued on the interrupt IN endpoint.
///
/// The headset pushes unsolicited reports (e.g. charging-state changes), which would
/// otherwise be read as the reply to the next command.
fn drain(dev: &HidDevice) {
    let mut buf = [0u8; REPORT_LEN];
    while matches!(dev.read_timeout(&mut buf, 0), Ok(n) if n > 0) {}
}

/// Read until a response to `request` arrives (same class, sub=0x01), skipping
/// unsolicited pushes and malformed reports. `Ok(None)` means the deadline passed.
fn read_response(dev: &HidDevice, request: &Report) -> Result<Option<Report>> {
    let class = request.as_bytes()[10];
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    let mut buf = [0u8; REPORT_LEN];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        let n = dev
            .read_timeout(&mut buf, remaining.as_millis().max(1) as i32)
            .context("HID read failed")?;
        if n == 0 {
            return Ok(None);
        }
        if n != REPORT_LEN {
            debug!(len = n, "ignoring short report");
            continue;
        }
        if buf[10] == class && buf[11] == 0x01 {
            return Ok(Some(Report::from_bytes(buf)));
        }
        debug!(class = buf[10], sub = buf[11], "ignoring unrelated report");
    }
}

/// Send a report and read back the matching response.
///
/// Errors: an `NoResponse` source means the link is silent; any other error is an
/// I/O failure (device unplugged) or a device-reported failure status.
pub fn send(dev: &HidDevice, report: &Report) -> Result<Report> {
    for _ in 0..BUSY_RETRIES {
        drain(dev);
        dev.write(report.as_bytes()).context("HID write failed")?;
        let Some(response) = read_response(dev, report)? else {
            return Err(NoResponse.into());
        };
        match response.status() {
            ResponseStatus::Ok => return Ok(response),
            ResponseStatus::Busy => std::thread::sleep(Duration::from_millis(150)),
            other => bail!(
                "device returned error status: {other:?} (raw=0x{:02x})",
                response.as_bytes()[1]
            ),
        }
    }
    bail!("device busy")
}
