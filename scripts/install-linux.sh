#!/usr/bin/env bash
# Builds fulltalk dev and installs it for the current user (no root needed):
# ~/.local/bin/fulltalk, the icon and an app-menu entry, so it shows up in the
# desktop's app search like any other application.
#
#   scripts/install-linux.sh              build and install (or update)
#   scripts/install-linux.sh --uninstall  remove it; settings, history and the model stay
set -euo pipefail
cd "$(dirname "$0")/.."

data="${XDG_DATA_HOME:-$HOME/.local/share}"
bin="$HOME/.local/bin/fulltalk"
icon="$data/icons/hicolor/scalable/apps/fulltalk.svg"
desktop="$data/applications/fulltalk.desktop"
autostart="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/fulltalk dev.desktop"

# Makes the desktop notice the change without a new login.
refresh() {
  update-desktop-database "$data/applications" 2>/dev/null || true
  touch "$data/icons/hicolor" 2>/dev/null || true
  if [[ -f "$data/icons/hicolor/icon-theme.cache" ]]; then
    gtk-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true
  fi
}

if [[ "${1:-}" == "--uninstall" ]]; then
  rm -f "$bin" "$icon" "$desktop" "$autostart"
  refresh
  echo "Removed fulltalk dev. Settings, history and the model were left in place."
  exit 0
fi

command -v cargo >/dev/null || PATH="$HOME/.cargo/bin:$PATH"
cargo build --release -p fulltalk

install -Dm755 target/release/fulltalk "$bin"
install -Dm644 docs/logo.svg "$icon"
install -Dm644 packaging/linux/fulltalk.desktop "$desktop"
# An absolute path keeps the launcher working when ~/.local/bin is not on the
# session's PATH (it is only added at login, and only if it already existed).
exec_path="$bin"
[[ "$bin" == *[[:space:]]* ]] && exec_path="\"$bin\""
sed -i "s|^Exec=.*|Exec=$exec_path|; s|^TryExec=.*|TryExec=$bin|" "$desktop"
refresh

echo "Installed. Search for \"fulltalk\" in your app launcher, or run: $bin"
