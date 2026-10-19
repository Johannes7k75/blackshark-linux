#!/usr/bin/env bash
set -uo pipefail

# ---------------------------------------------------------------------------
# blackshark-ctl uninstaller
# Removes everything install.sh (or a manual install) put on the system.
# Usage: ./uninstall.sh [--purge]
#   --purge   also delete ~/.config/blackshark (your saved settings)
# ---------------------------------------------------------------------------

PURGE=false
[[ "${1:-}" == "--purge" ]] && PURGE=true

BINS=(blacksharkd blackshark-ctl blackshark-tray blackshark-gui)
BIN_DIRS=("${HOME}/.local/bin" /usr/local/bin /usr/bin)

BOLD='\033[1m'; NC='\033[0m'
info() { echo -e "${BOLD}==> $*${NC}"; }

info "Stopping services and processes"
systemctl --user disable --now blacksharkd 2>/dev/null || true
for bin in blackshark-tray blackshark-gui blacksharkd; do
    pkill -x "$bin" 2>/dev/null || true
done

info "Removing user files"
rm -fv "${HOME}/.config/systemd/user/blacksharkd.service" \
       "${HOME}/.config/autostart/blackshark-tray.desktop" \
       "${HOME}/.local/share/applications/blackshark-gui.desktop"
systemctl --user daemon-reload 2>/dev/null || true
systemctl --user reset-failed blacksharkd 2>/dev/null || true

info "Removing binaries"
need_sudo=()
for dir in "${BIN_DIRS[@]}"; do
    for bin in "${BINS[@]}"; do
        f="${dir}/${bin}"
        [[ -e "$f" ]] || continue
        if [[ -w "$dir" ]]; then rm -fv "$f"; else need_sudo+=("$f"); fi
    done
done

UDEV=/etc/udev/rules.d/99-blackshark.rules
[[ -e "$UDEV" ]] && need_sudo+=("$UDEV")

if (( ${#need_sudo[@]} )); then
    info "Removing system files (requires sudo)"
    sudo rm -fv "${need_sudo[@]}"
    sudo udevadm control --reload-rules
fi

if $PURGE; then
    info "Removing config"
    rm -rfv "${HOME}/.config/blackshark"
else
    [[ -d "${HOME}/.config/blackshark" ]] && \
        echo "Kept ${HOME}/.config/blackshark (run with --purge to delete)."
fi

echo
echo "Uninstalled. To reinstall: ./install.sh, then unplug and re-plug the dongle."
for bin in "${BINS[@]}"; do
    command -v "$bin" >/dev/null 2>&1 && echo "warning: $bin still found at $(command -v "$bin")"
done
exit 0
