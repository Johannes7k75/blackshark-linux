use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{info, warn};

/// Create a null sink and return its pactl module ID.
pub async fn load_null_sink(name: &str, desc: &str) -> Result<u32> {
    let output = Command::new("pactl")
        .args([
            "load-module",
            "module-null-sink",
            &format!("sink_name={name}"),
            &format!("sink_properties=device.description={desc}"),
        ])
        .output()
        .await
        .context("pactl not found")?;

    let id_str = String::from_utf8(output.stdout)
        .context("pactl output not utf8")?;
    let id: u32 = id_str
        .trim()
        .parse()
        .with_context(|| format!("unexpected pactl output: {}", id_str.trim()))?;

    info!("loaded PipeWire null sink '{name}' (module {id})");
    Ok(id)
}

/// Create a loopback from `source` monitor to `sink` and return the module ID.
pub async fn load_loopback(source_monitor: &str, sink: &str) -> Result<u32> {
    let output = Command::new("pactl")
        .args([
            "load-module",
            "module-loopback",
            &format!("source={source_monitor}"),
            &format!("sink={sink}"),
            "latency_msec=1",
        ])
        .output()
        .await
        .context("pactl not found")?;

    let id_str = String::from_utf8(output.stdout)
        .context("pactl output not utf8")?;
    let id: u32 = id_str
        .trim()
        .parse()
        .with_context(|| format!("unexpected pactl output: {}", id_str.trim()))?;

    info!("loaded PipeWire loopback {source_monitor} → {sink} (module {id})");
    Ok(id)
}

/// Unload any leftover blackshark-game / blackshark-chat modules from a
/// previous daemon run. Called once at startup before creating new sinks.
pub async fn cleanup_stale_sinks() {
    let output = match Command::new("pactl")
        .args(["list", "short", "modules"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(_) => return,
    };

    let stdout = match String::from_utf8(output.stdout) {
        Ok(s) => s,
        Err(_) => return,
    };

    for line in stdout.lines() {
        if line.contains("blackshark-game") || line.contains("blackshark-chat") {
            if let Some(id_str) = line.split_whitespace().next() {
                if let Ok(id) = id_str.parse::<u32>() {
                    info!("removing stale PipeWire module {id}");
                    unload_module(id).await;
                }
            }
        }
    }
}

/// Find the real ALSA sink name for the BlackShark V3 Pro headset.
///
/// Returns the sink name (e.g. `alsa_output.usb-Razer_Inc_BlackShark_V3_Pro_...`)
/// by scanning `pactl list sinks short` for a Razer/BlackShark entry that isn't
/// one of our own virtual sinks.
pub async fn find_headset_sink() -> Option<String> {
    let output = Command::new("pactl")
        .args(["list", "sinks", "short"])
        .output()
        .await
        .ok()?;

    let stdout = String::from_utf8(output.stdout).ok()?;
    for line in stdout.lines() {
        let lower = line.to_lowercase();
        if lower.contains("blackshark_v3") || lower.contains("blackshark v3") {
            // Fields: id <tab> name <tab> driver ...
            let name = line.split_whitespace().nth(1)?;
            return Some(name.to_owned());
        }
    }
    None
}

/// Unload a pactl module by ID.
pub async fn unload_module(id: u32) {
    let status = Command::new("pactl")
        .args(["unload-module", &id.to_string()])
        .status()
        .await;
    match status {
        Ok(s) if s.success() => info!("unloaded PipeWire module {id}"),
        Ok(s) => warn!("pactl unload-module {id} exited with {s}"),
        Err(e) => warn!("pactl unload-module {id} failed: {e}"),
    }
}

/// Set sink volumes for a game/chat crossfader position.
///
/// mix = 0   → game 0%,   chat 100%
/// mix = 50  → game 100%, chat 100%  (equal loudness)
/// mix = 100 → game 100%, chat 0%
pub async fn apply_mix_volumes(mix: u8) {
    let mix = mix.min(100);
    let game_pct: u32 = if mix >= 50 { 100 } else { mix as u32 * 2 };
    let chat_pct: u32 = if mix <= 50 { 100 } else { (100 - mix as u32) * 2 };

    for (sink, pct) in [("blackshark-game", game_pct), ("blackshark-chat", chat_pct)] {
        let result = Command::new("pactl")
            .args(["set-sink-volume", sink, &format!("{pct}%")])
            .status()
            .await;
        if let Err(e) = result {
            warn!("pactl set-sink-volume {sink} {pct}% failed: {e}");
        }
    }
}
