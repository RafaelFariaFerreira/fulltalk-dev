#!/usr/bin/env bash
# Builds fulltalk dev and installs it for the current user (no root needed):
#   ~/.local/bin/fulltalk, an app-menu entry and the icon.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p fulltalk

install -Dm755 target/release/fulltalk "$HOME/.local/bin/fulltalk"
install -Dm644 docs/logo.svg "$HOME/.local/share/icons/hicolor/scalable/apps/fulltalk.svg"
install -Dm644 packaging/linux/fulltalk.desktop "$HOME/.local/share/applications/fulltalk.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true

echo "Installed. Launch \"fulltalk dev\" from the app menu or run: fulltalk"
