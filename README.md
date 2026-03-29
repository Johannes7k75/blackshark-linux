# blackshark-ctl

Linux userspace driver for the **Razer BlackShark V3 Pro** wireless headset.

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

---

## Requirements

- Linux (tested on Arch/CachyOS with KDE Plasma + Wayland)
- Rust (stable) — [rustup.rs](https://rustup.rs)
- systemd (user session)
- PipeWire or PulseAudio (optional — only needed for the experimental game/chat mix feature)

---

## Quick install

```bash
git clone https://github.com/RiskRunner0/blackshark-linux.git
cd blackshark-linux
./install.sh
```

The script:
1. Builds release binaries with `cargo build --release`
2. Installs them to `~/.local/bin/`
3. Installs and starts the systemd user service (`blacksharkd`)
4. Installs the udev rule (requires `sudo`) so the daemon can access the HID device without root

After install, plug in the USB dongle and the daemon will connect automatically.

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

## Screenshots

![Device tab](assets/Device_page.png)

![System tray](assets/tray_icon.png)

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
                    BlackShark V3 Pro dongle (USB)
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
  99-blackshark.rules   udev rule
  blacksharkd.service   systemd user unit
install.sh             one-shot build + install script
```

---

## udev rule

The udev rule grants the `users` group read/write access to the headset's HID interface:

```
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1532", ATTRS{idProduct}=="0577", MODE="0660", GROUP="users"
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

Security audit runs weekly via `cargo audit`. Release builds for `x86_64` and `aarch64` are produced automatically on version tags.

---

## Device info

- USB VID/PID: `0x1532` / `0x0577`
- HID reports: 64 bytes, report ID `0x02`
- Interface: HID interface 5, endpoint `0x84`
- Protocol: custom Razer HID (not HID++ or OpenRazer-compatible)
