use tokio::sync::{mpsc, oneshot, watch};
use zbus::interface;

use crate::config::{AppRoute, Config};
use crate::hid_actor::{BatteryState, HidCommand};
use crate::state::SharedState;

pub struct HeadsetInterface {
    cmd_tx:    mpsc::Sender<HidCommand>,
    state_rx:  watch::Receiver<SharedState>,
    state_tx:  watch::Sender<SharedState>,
    config_tx: watch::Sender<Config>,
}

impl HeadsetInterface {
    pub fn new(
        cmd_tx:    mpsc::Sender<HidCommand>,
        state_rx:  watch::Receiver<SharedState>,
        state_tx:  watch::Sender<SharedState>,
        config_tx: watch::Sender<Config>,
    ) -> Self {
        Self { cmd_tx, state_rx, state_tx, config_tx }
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

    /// Update one field in the config and trigger a debounced save + apply.
    fn update_config<F: FnOnce(&mut Config)>(&self, f: F) {
        self.config_tx.send_modify(f);
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
        self.send_cmd(HidCommand::SetSidetone { level, reply: tx }, rx).await?;
        self.update_config(|c| c.sidetone = level);
        Ok(())
    }

    /// Enable or disable THX Spatial Audio. false = Stereo.
    async fn set_thx(&self, enabled: bool) -> zbus::fdo::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send_cmd(HidCommand::SetThx { enabled, reply: tx }, rx).await?;
        self.update_config(|c| c.thx_enabled = enabled);
        Ok(())
    }

    /// Set Active Noise Cancellation. level = 1–4.
    async fn set_anc(&self, enabled: bool, level: u8) -> zbus::fdo::Result<()> {
        if level < 1 || level > 4 {
            return Err(zbus::fdo::Error::InvalidArgs("level must be 1–4".into()));
        }
        let (tx, rx) = oneshot::channel();
        self.send_cmd(HidCommand::SetAnc { enabled, level, reply: tx }, rx).await?;
        self.update_config(|c| { c.anc_enabled = enabled; c.anc_level = level; });
        Ok(())
    }

    /// Set EQ preset (0–8). Preset 0 = flat.
    async fn set_eq(&self, preset: u8) -> zbus::fdo::Result<()> {
        if preset >= 9 {
            return Err(zbus::fdo::Error::InvalidArgs("preset must be 0–8".into()));
        }
        let (tx, rx) = oneshot::channel();
        self.send_cmd(HidCommand::SetEq { preset, reply: tx }, rx).await?;
        self.update_config(|c| c.eq_preset = preset);
        Ok(())
    }

    /// Set power savings timeout. minutes = 0 (off), 15, 30, 45, or 60.
    async fn set_power_savings(&self, minutes: u8) -> zbus::fdo::Result<()> {
        if ![0u8, 15, 30, 45, 60].contains(&minutes) {
            return Err(zbus::fdo::Error::InvalidArgs("minutes must be 0, 15, 30, 45, or 60".into()));
        }
        let (tx, rx) = oneshot::channel();
        self.send_cmd(HidCommand::SetPowerSavings { minutes, reply: tx }, rx).await?;
        self.update_config(|c| c.power_savings_minutes = minutes);
        Ok(())
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

    #[zbus(property)]
    async fn eq_preset(&self) -> u8 {
        self.state_rx.borrow().eq_preset
    }

    #[zbus(property)]
    async fn thx_enabled(&self) -> bool {
        self.state_rx.borrow().thx_enabled
    }

    #[zbus(property)]
    async fn anc_enabled(&self) -> bool {
        self.state_rx.borrow().anc_enabled
    }

    #[zbus(property)]
    async fn anc_level(&self) -> u8 {
        self.state_rx.borrow().anc_level
    }

    #[zbus(property)]
    async fn power_savings_minutes(&self) -> u8 {
        self.state_rx.borrow().power_savings_minutes
    }

    /// Set game/chat crossfader (0 = all chat, 50 = equal, 100 = all game).
    async fn set_game_chat_mix(&self, mix: u8) -> zbus::fdo::Result<()> {
        if mix > 100 {
            return Err(zbus::fdo::Error::InvalidArgs("mix must be 0–100".into()));
        }
        self.state_tx.send_modify(|s| s.game_chat_mix = mix);
        self.update_config(|c| c.game_chat_mix = mix);
        crate::pipewire::apply_mix_volumes(mix).await;
        Ok(())
    }

    #[zbus(property)]
    async fn game_chat_mix(&self) -> u8 {
        self.state_rx.borrow().game_chat_mix
    }

    /// List all non-loopback sink-inputs currently playing.
    /// Returns Vec of (sink_input_id, app_name, route) where route is
    /// "game", "chat", or "" for unassigned.
    async fn list_sink_inputs(&self) -> zbus::fdo::Result<Vec<(u32, String, String)>> {
        let inputs = crate::pipewire::list_sink_inputs().await;
        Ok(inputs.into_iter().map(|s| (s.id, s.app_name, s.route)).collect())
    }

    /// Assign a sink-input's app to game or chat routing, persisting the rule.
    /// route = "game", "chat", or "" to clear the rule.
    async fn set_sink_input_route(&self, sink_input_id: u32, route: String) -> zbus::fdo::Result<()> {
        // Find the app name for this sink-input.
        let inputs = crate::pipewire::list_sink_inputs().await;
        let Some(input) = inputs.iter().find(|s| s.id == sink_input_id) else {
            return Err(zbus::fdo::Error::InvalidArgs("sink-input not found".into()));
        };
        let app_name = input.app_name.clone();
        let sink_name = match route.as_str() {
            "game" => "blackshark-game",
            "chat" => "blackshark-chat",
            "" => {
                // Clear rule — move back to default headset output.
                self.update_config(|c| { c.app_routing.remove(&app_name); });
                if let Some(headset) = crate::pipewire::find_headset_sink().await {
                    crate::pipewire::move_sink_input(sink_input_id, &headset).await;
                }
                return Ok(());
            }
            _ => return Err(zbus::fdo::Error::InvalidArgs("route must be 'game', 'chat', or ''".into())),
        };

        // Persist rule.
        let route_enum = if route == "game" { AppRoute::Game } else { AppRoute::Chat };
        self.update_config(|c| { c.app_routing.insert(app_name.clone(), route_enum); });

        // Move all current streams for this app to the target sink.
        for input in inputs.iter().filter(|s| s.app_name == app_name) {
            crate::pipewire::move_sink_input(input.id, sink_name).await;
        }

        Ok(())
    }

    /// Return all saved app routing rules as Vec of (app_name, route).
    async fn get_app_routes(&self) -> Vec<(String, String)> {
        self.config_tx
            .borrow()
            .app_routing
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().to_owned()))
            .collect()
    }

    /// Remove a saved app routing rule by app name.
    async fn remove_app_route(&self, app_name: String) -> zbus::fdo::Result<()> {
        self.update_config(|c| { c.app_routing.remove(&app_name); });
        Ok(())
    }

    /// Emitted when the battery level changes.
    #[zbus(signal)]
    pub async fn battery_changed(
        signal_ctxt: &zbus::SignalContext<'_>,
        percentage: u8,
        charging: bool,
    ) -> zbus::Result<()>;
}
