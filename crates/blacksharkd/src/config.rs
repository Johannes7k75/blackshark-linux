use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Where an app's audio stream should be routed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppRoute {
    Game,
    Chat,
}

impl AppRoute {
    pub fn as_str(&self) -> &'static str {
        match self {
            AppRoute::Game => "game",
            AppRoute::Chat => "chat",
        }
    }
}

/// Persisted user configuration.
///
/// Saved to `~/.config/blackshark/config.toml` on every change (debounced).
/// Restored to the headset on every device connect.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub sidetone: u8,
    pub eq_preset: u8,
    pub thx_enabled: bool,
    pub anc_enabled: bool,
    pub anc_level: u8,              // 1–4
    pub power_savings_minutes: u8,  // 0=off, 15/30/45/60
    /// Game/chat crossfader: 0 = all chat, 50 = equal, 100 = all game.
    pub game_chat_mix: u8,
    /// Per-app sink routing rules.  Key = app name, value = game or chat.
    #[serde(default)]
    pub app_routing: HashMap<String, AppRoute>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sidetone:               0,
            eq_preset:              0,
            thx_enabled:            false,
            anc_enabled:            false,
            anc_level:              1,
            power_savings_minutes:  0,
            game_chat_mix:          50,
            app_routing:            HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Load / save
// ---------------------------------------------------------------------------

pub fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("could not determine config directory")?;
    Ok(base.join("blackshark").join("config.toml"))
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&text).context("failed to parse config.toml")
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(config).context("failed to serialise config")?;
    std::fs::write(&path, text)
        .with_context(|| format!("failed to write {}", path.display()))
}
