use tokio::sync::{mpsc, oneshot, watch};
use zbus::interface;

use crate::hid_actor::{BatteryState, HidCommand};
use crate::state::SharedState;

pub struct HeadsetInterface {
    cmd_tx:   mpsc::Sender<HidCommand>,
    state_rx: watch::Receiver<SharedState>,
}

impl HeadsetInterface {
    pub fn new(cmd_tx: mpsc::Sender<HidCommand>, state_rx: watch::Receiver<SharedState>) -> Self {
        Self { cmd_tx, state_rx }
    }

    async fn send_cmd<T>(
        &self,
        cmd: HidCommand,
        rx: oneshot::Receiver<anyhow::Result<T>>,
    ) -> zbus::fdo::Result<T> {
        self.cmd_tx
            .send(cmd)
            .await
            .map_err(|_| zbus::fdo::Error::Failed("daemon shutting down".into()))?;
        rx.await
            .map_err(|_| zbus::fdo::Error::Failed("HID actor died".into()))?
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }
}

#[interface(name = "net.blackshark1.Headset")]
impl HeadsetInterface {
    /// Set sidetone level (0–15).
    async fn set_sidetone(&self, level: u8) -> zbus::fdo::Result<()> {
        if level > 15 {
            return Err(zbus::fdo::Error::InvalidArgs("level must be 0–15".into()));
        }
        let (tx, rx) = oneshot::channel();
        self.send_cmd(HidCommand::SetSidetone { level, reply: tx }, rx).await
    }

    /// Returns (percentage, charging).
    async fn get_battery(&self) -> zbus::fdo::Result<(u8, bool)> {
        let (tx, rx) = oneshot::channel::<anyhow::Result<BatteryState>>();
        let state = self.send_cmd(HidCommand::GetBattery { reply: tx }, rx).await?;
        Ok((state.percentage, state.charging))
    }

    /// Whether the headset is currently reachable.
    #[zbus(property)]
    async fn connected(&self) -> bool {
        self.state_rx.borrow().connected
    }

    /// Cached battery percentage (updated every 5 minutes or on explicit GetBattery call).
    #[zbus(property)]
    async fn battery_percentage(&self) -> u8 {
        self.state_rx.borrow().battery_pct
    }

    /// Cached sidetone level (0–15).
    #[zbus(property)]
    async fn sidetone(&self) -> u8 {
        self.state_rx.borrow().sidetone
    }

    /// Emitted when the battery level changes.
    #[zbus(signal)]
    pub async fn battery_changed(
        signal_ctxt: &zbus::SignalContext<'_>,
        percentage: u8,
        charging: bool,
    ) -> zbus::Result<()>;
}
