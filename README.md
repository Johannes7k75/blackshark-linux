# blackshark-ctl

Linux userspace driver for **Razer BlackShark** headsets (including V3 Pro, V3 X, and V2 HS).

Controls sidetone, EQ presets, THX Spatial Audio, Active Noise Cancellation, and power savings — without Razer Synapse or Windows.

---

## Features

- **Sidetone** — mic monitoring level (0–15)
- **EQ presets** — all 9 named Synapse presets (Flat, Bass Boost, FPS, etc.)
- **THX Spatial Audio** — toggle surround sound on/off
- **ANC** — enable/disable and set level (1–4)
- **Power savings** — auto-off timeout (off, 15, 30, 45, 60 min)
- **Battery** — percentage and charging status, polled every 5 minutes
- **Settings persist** — config saved to `~/.config/blackshark/config.toml`, restored on reconnect
- **System tray** — battery %, quick toggles, EQ/sidetone submenus, daemon controls
- **GUI** — full settings panel with live updates
- **CLI** — scriptable control and JSON status output

![Device tab showing battery, connection status and audio controls](assets/Device_page.png)
*GUI settings panel — Device tab*

![System tray menu with headset controls and daemon status](assets/tray_icon.png)
*System tray with quick-access controls*

---

## Requirements

- Linux (tested on Arch/CachyOS with KDE Plasma + Wayland)
- Rust (stable) — [rustup.rs](https://rustup.rs)
- systemd (user session)
- PipeWire or PulseAudio (optional — only needed for the experimental game/chat mix feature)

> **Firmware note:** Firmware 1.3.x or later is required on both the headset and dongle.
> Users on 1.2.x have reported the daemon sees the dongle but cannot communicate with the headset.
> Update firmware via Razer Synapse on Windows before switching to Linux. (See [#1](https://github.com/RiskRunner0/blackshark-linux/issues/1))

---

## Quick install

**Option A — pre-built release (recommended):**

1. Download the latest release tarball from the [Releases page](https://github.com/RiskRunner0/blackshark-linux/releases)
2. Extract and run:
   ```bash
   tar xzf blackshark-ctl-*.tar.gz
   cd blackshark-ctl-*
   ./install.sh
   ```

**Option B — build from source:**

```bash
git clone https://github.com/RiskRunner0/blackshark-linux.git
cd blackshark-linux
./install.sh
```

The script installs the binaries to `~/.local/bin/`, starts the daemon as a systemd user service, and installs the udev rule (requires `sudo`) so the daemon can access the headset without root.

---

## Getting started

After install, plug in the USB dongle. Then:

**1. Check the daemon is running:**
```bash
systemctl --user status blacksharkd
```

**2. Verify the headset is detected:**
```bash
blackshark-ctl status
```

**3. Start the system tray:**
```bash
blackshark-tray &
```
The tray icon appears in your taskbar with battery %, quick toggles for EQ, sidetone, THX, and ANC, and a Daemon submenu if you need to restart it.

**4. Open the settings GUI:**
```bash
blackshark-gui
```
All settings are applied immediately and sync back to the tray in real time.

**5. Add the tray to autostart** so it launches on login:
```bash
install -m644 pkg/blackshark-tray.desktop ~/.config/autostart/
```
Or on KDE: Settings → Autostart → Add Application → search "BlackShark Tray".

---

## Usage

### Daemon

```bash
systemctl --user status blacksharkd
systemctl --user restart blacksharkd
```

### CLI

```bash
blackshark-ctl status           # human-readable status
blackshark-ctl status --json    # JSON output for waybar/scripts
blackshark-ctl battery          # battery percentage and charging state
blackshark-ctl sidetone <0-15>  # set sidetone level
blackshark-ctl eq <0-8>         # set EQ preset (0 = Flat)
blackshark-ctl thx <on|off>     # toggle THX Spatial Audio
blackshark-ctl anc <on|off> [level]  # toggle ANC, optional level 1-4
blackshark-ctl power-savings <0|15|30|45|60>  # auto-off timeout in minutes
blackshark-ctl monitor          # stream live D-Bus property changes
```

### System tray

```bash
blackshark-tray &
```

Add to your desktop autostart. Shows battery % in the tooltip, quick toggles and submenus for all settings in the menu, and a Daemon submenu to start/stop/restart the daemon.

### GUI

```bash
blackshark-gui
```

Full settings panel. All changes are applied immediately via D-Bus and sync back to the tray and CLI in real time. The Advanced tab has daemon controls, a live log viewer, and an opt-in toggle for the experimental PipeWire game/chat mix feature.

---

## Architecture

```
blackshark-ctl  (CLI)  ──┐
blackshark-tray (tray) ──┤  D-Bus: net.blackshark1 (session bus)
blackshark-gui  (GUI)  ──┘
                          │
                    blacksharkd  (systemd user service)
                          │
                    /dev/hidraw*  (hidapi)
                          │
                    BlackShark headset dongle (USB)
```

The daemon owns the HID device exclusively. All other tools talk to it over D-Bus (`net.blackshark1`, session bus, path `/net/blackshark1/Headset`). No tool other than the daemon touches `/dev/hidraw*`.

---

## Repository layout

```
crates/
  protocol/          HID report format and command constants
  device/            hidapi open/send/recv
  blackshark-client/ zbus D-Bus proxy (shared by CLI, tray, GUI)
  blacksharkd/       daemon: HID ownership, D-Bus service, battery polling
  blackshark-ctl/    CLI client
  blackshark-tray/   ksni system tray
  blackshark-gui/    slint settings GUI
pkg/
  99-blackshark.rules      udev rule
  blacksharkd.service      systemd user unit
  blackshark-gui.desktop   app launcher entry (copy to ~/.local/share/applications/)
  blackshark-tray.desktop  autostart entry (copy to ~/.config/autostart/)
install.sh                 one-shot build + install script
```

---

## udev rule

The udev rule grants the `users` group read/write access to the headset's HID interface:

```
# Razer BlackShark V3 Pro
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1532", ATTRS{idProduct}=="0577", MODE="0660", GROUP="users"
# Razer BlackShark V2 HS 2.4
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1532", ATTRS{idProduct}=="0565", MODE="0660", GROUP="users"
```

Make sure your user is in the `users` group (`groups $USER`). If not:

```bash
sudo usermod -aG users $USER
# log out and back in, then re-run install.sh
```

---

## CI

GitHub Actions runs on every push:
- `cargo fmt --check`
- `cargo clippy -D warnings`
- `cargo build --all`
- `cargo test --all`

Security audit runs weekly via `cargo audit`. Release builds for `x86_64` are produced automatically on version tags.

---

## Device info

- USB VID: `0x1532` (Razer USA, Ltd)
- Supported PIDs:
  - `0x0577` (Razer BlackShark V3 Pro)
  - `0x0565` (Razer BlackShark V2 HS 2.4)
  - `0x057d` (Razer BlackShark V3 X)
- HID reports: 64 bytes, report ID `0x02`
- Interface: HID control interface (interface 5 for V3 Pro, interface 3/fallback for V2 HS and V3 X), endpoint `0x84`
- Protocol: custom Razer HID (not HID++ or OpenRazer-compatible)

---

## Device Support Matrix & Limitations

| Feature | Razer BlackShark V3 Pro (`0x0577`) | Razer BlackShark V2 HS (`0x0565`) | Razer BlackShark V3 X (`0x057d`) |
| :--- | :---: | :---: | :---: |
| **Connection & Battery** | Yes | Yes | Yes |
| **Sidetone (Mic Monitoring)** | Yes | Yes (Hardware support, volume can be low) | Untested (Inherited fallback) |
| **Power Savings (Auto Off)** | Yes | Yes | Untested (Inherited fallback) |
| **PipeWire Game/Chat Mix** | Yes | Yes (Requires client-side routing) | Untested (Inherited fallback) |
| **Hardware EQ Presets** | Yes | No (Dongle returns Timeout status) | Untested (Inherited fallback) |
| **Active Noise Cancellation**| Yes | No (No ANC hardware on device) | No (No ANC hardware on device) |
| **THX Spatial Audio** | Yes | No (Software-only feature on V2 HS) | No (Software-only feature on V3 X) |

> [!NOTE]
> For headsets with limited or missing hardware EQ support (like the V2 HS and V3 X), we recommend using **EasyEffects** on Linux for custom, high-quality software equalization.
>
> When unsupported hardware settings are changed in the GUI/CLI, the daemon will print a warning log message (`headset returned command error`) instead of disconnecting the device.
