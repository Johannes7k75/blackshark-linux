mod proxy;

use anyhow::Result;
use slint::ComponentHandle;
use zbus::Connection;

use proxy::HeadsetProxy;

slint::include_modules!();

#[tokio::main]
async fn main() -> Result<()> {
    let conn = Connection::session().await?;

    let window = MainWindow::new()?;

    // Load initial state
    if let Ok(proxy) = HeadsetProxy::new(&conn).await {
        if let Ok(connected) = proxy.connected().await {
            window.set_connected(connected);
            if connected {
                window.set_battery_pct(proxy.battery_percentage().await.unwrap_or(0) as i32);
                window.set_sidetone(proxy.sidetone().await.unwrap_or(0) as i32);
                window.set_thx_enabled(proxy.thx_enabled().await.unwrap_or(false));
                window.set_anc_enabled(proxy.anc_enabled().await.unwrap_or(false));
                window.set_anc_level(proxy.anc_level().await.unwrap_or(1) as i32);
                window.set_power_savings(proxy.power_savings_minutes().await.unwrap_or(0) as i32);
            }
        }
    }

    // Wire up callbacks
    {
        let conn = conn.clone();
        window.on_set_sidetone(move |level| {
            let conn = conn.clone();
            tokio::spawn(async move {
                if let Ok(proxy) = HeadsetProxy::new(&conn).await {
                    let _ = proxy.set_sidetone(level as u8).await;
                }
            });
        });
    }

    {
        let conn = conn.clone();
        window.on_set_thx(move |enabled| {
            let conn = conn.clone();
            tokio::spawn(async move {
                if let Ok(proxy) = HeadsetProxy::new(&conn).await {
                    let _ = proxy.set_thx(enabled).await;
                }
            });
        });
    }

    {
        let conn = conn.clone();
        window.on_set_anc(move |enabled, level| {
            let conn = conn.clone();
            tokio::spawn(async move {
                if let Ok(proxy) = HeadsetProxy::new(&conn).await {
                    let _ = proxy.set_anc(enabled, level as u8).await;
                }
            });
        });
    }

    {
        let conn = conn.clone();
        window.on_set_power_savings(move |minutes| {
            let conn = conn.clone();
            tokio::spawn(async move {
                if let Ok(proxy) = HeadsetProxy::new(&conn).await {
                    let _ = proxy.set_power_savings(minutes as u8).await;
                }
            });
        });
    }

    // Background task: watch D-Bus signals and update UI
    {
        let window_weak = window.as_weak();
        let conn = conn.clone();
        tokio::spawn(async move {
            use futures_util::StreamExt;
            let Ok(proxy) = HeadsetProxy::new(&conn).await else { return };

            let mut battery_stream   = proxy.receive_battery_changed().await.ok();
            let mut connected_stream = proxy.receive_connected_changed().await;
            let mut sidetone_stream  = proxy.receive_sidetone_changed().await;
            let mut thx_stream       = proxy.receive_thx_enabled_changed().await;
            let mut anc_stream       = proxy.receive_anc_enabled_changed().await;
        let mut anc_level_stream = proxy.receive_anc_level_changed().await;
        let mut ps_stream        = proxy.receive_power_savings_minutes_changed().await;

            loop {
                tokio::select! {
                    Some(sig) = async { battery_stream.as_mut()?.next().await } => {
                        if let Ok(args) = sig.args() {
                            let pct      = args.percentage as i32;
                            let charging = args.charging;
                            let w = window_weak.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(win) = w.upgrade() {
                                    win.set_battery_pct(pct);
                                    win.set_charging(charging);
                                }
                            }).ok();
                        }
                    }
                    Some(change) = connected_stream.next() => {
                        if let Ok(val) = change.get().await {
                            let w = window_weak.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(win) = w.upgrade() { win.set_connected(val); }
                            }).ok();
                        }
                    }
                    Some(change) = sidetone_stream.next() => {
                        if let Ok(val) = change.get().await {
                            let w = window_weak.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(win) = w.upgrade() { win.set_sidetone(val as i32); }
                            }).ok();
                        }
                    }
                    Some(change) = thx_stream.next() => {
                        if let Ok(val) = change.get().await {
                            let w = window_weak.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(win) = w.upgrade() { win.set_thx_enabled(val); }
                            }).ok();
                        }
                    }
                    Some(change) = anc_stream.next() => {
                        if let Ok(val) = change.get().await {
                            let w = window_weak.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(win) = w.upgrade() { win.set_anc_enabled(val); }
                            }).ok();
                        }
                    }
                    Some(change) = anc_level_stream.next() => {
                        if let Ok(val) = change.get().await {
                            let w = window_weak.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(win) = w.upgrade() { win.set_anc_level(val as i32); }
                            }).ok();
                        }
                    }
                    Some(change) = ps_stream.next() => {
                        if let Ok(val) = change.get().await {
                            let w = window_weak.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(win) = w.upgrade() { win.set_power_savings(val as i32); }
                            }).ok();
                        }
                    }
                }
            }
        });
    }

    window.run()?;
    Ok(())
}
