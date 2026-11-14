#!/usr/bin/env bash
# One-time Linux setup for fulltalk dev (Debian/Ubuntu; adapt package names elsewhere).
#
#  1. Build dependency: ALSA headers (microphone access via cpal).
#  2. Hold-to-talk on Wayland: compositors don't let apps grab global keys, so
#     fulltalk reads keyboards via evdev → your user must be in the `input` group.
#  3. Auto-paste on Wayland: synthetic Ctrl+V goes through a uinput virtual
#     keyboard → /dev/uinput must be writable by the `input` group.
#
# Security note: members of `input` can read every keystroke on the machine.
# Only do this on a single-user computer you trust.
set -euo pipefail

say() { printf '\033[1;35m==>\033[0m %s\n' "$*"; }

if command -v apt-get >/dev/null; then
  say "Installing build dependencies (libasound2-dev, pkg-config)"
  sudo apt-get install -y libasound2-dev pkg-config
else
  say "Not a Debian-based system: install ALSA development headers and pkg-config manually."
fi

say "Adding $USER to the 'input' group (global shortcut on Wayland)"
sudo usermod -aG input "$USER"

say "Allowing the 'input' group to use /dev/uinput (auto-paste on Wayland)"
echo 'KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"' |
  sudo tee /etc/udev/rules.d/60-fulltalk-uinput.rules >/dev/null
echo uinput | sudo tee /etc/modules-load.d/fulltalk-uinput.conf >/dev/null
sudo modprobe uinput
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=misc --sysname-match=uinput

say "Done. Log out and back in so the new group membership takes effect."
