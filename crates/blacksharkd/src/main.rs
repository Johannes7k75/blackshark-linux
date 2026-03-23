mod dbus;
mod hid_actor;
mod state;

use std::time::Duration;

use anyhow::Result;
use tokio::sync::{mpsc, watch};
use zbus::ConnectionBuilder;

use state::SharedState;

const TICK_INTERVAL: Duration = Duration::from_secs(5);
const DBUS_PATH: &str = "/net/blackshark1/Headset";
const DBUS_NAME: &str = "net.blackshark1";

#[tokio::main]
async fn main() -> Result<()> {
    let (cmd_tx, cmd_rx) = mpsc::channel::<hid_actor::HidCommand>(32);
    let (state_tx, state_rx) = watch::channel(SharedState::default());

    // Spawn HID actor on a dedicated OS thread (hidapi is synchronous).
    hid_actor::spawn(cmd_rx, state_tx);

    // Periodic tick → drives reconnect attempts and battery polling.
    let tick_tx = cmd_tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK_INTERVAL);
        loop {
            interval.tick().await;
            if tick_tx.send(hid_actor::HidCommand::Tick).await.is_err() {
                break;
            }
        }
    });

    // D-Bus service.
    let iface = dbus::HeadsetInterface::new(cmd_tx, state_rx.clone());

    let conn = ConnectionBuilder::session()?
        .name(DBUS_NAME)?
        .serve_at(DBUS_PATH, iface)?
        .build()
        .await?;

    eprintln!("blacksharkd running on {DBUS_NAME}");

    // Watch for battery changes and emit the BatteryChanged signal.
    let mut watch_rx = state_rx;
    let conn2 = conn.clone();
    tokio::spawn(async move {
        let mut last_pct = 255u8; // sentinel — forces emission on first real value
        loop {
            if watch_rx.changed().await.is_err() {
                break;
            }
            let state = watch_rx.borrow().clone();
            if state.connected && state.battery_pct != last_pct {
                last_pct = state.battery_pct;
                let iface_ref = conn2
                    .object_server()
                    .interface::<_, dbus::HeadsetInterface>(DBUS_PATH)
                    .await;
                if let Ok(iface_ref) = iface_ref {
                    dbus::HeadsetInterface::battery_changed(
                        iface_ref.signal_context(),
                        state.battery_pct,
                        state.charging,
                    )
                    .await
                    .ok();
                }
            }
        }
    });

    // Block forever — the connection keeps the D-Bus service alive.
    std::future::pending::<()>().await;
    Ok(())
}
