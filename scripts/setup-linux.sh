#!/usr/bin/env bash
# One-time Linux setup for fulltalk dev (Debian/Ubuntu; adapt package names elsewhere).
#
#  1. Build dependency: ALSA headers (microphone access via cpal).
#  2. Auto-paste on Wayland: the synthetic Ctrl+V goes through a uinput virtual
#     keyboard, so /dev/uinput must be writable by the logged-in user.
#  3. Hold-to-talk on Wayland, only where it is needed. GNOME 48+ and KDE Plasma
#     bind the shortcut through the desktop portal and need nothing here. Other
#     compositors give apps no way to grab keys, so fulltalk reads the keyboards
#     via evdev, and for that your user must be in the `input` group.
#
#   scripts/setup-linux.sh                 steps 1 and 2, and 3 only if this desktop needs it
#   scripts/setup-linux.sh --input-group   always do step 3
#
# Security note for step 3: members of `input` can read every keystroke on the
# machine. Only do this on a single-user computer you trust.
set -euo pipefail

say() { printf '\033[1;35m==>\033[0m %s\n' "$*"; }

# Whether the desktop can bind global shortcuts itself (no keyboard access needed).
has_shortcut_portal() {
  command -v gdbus >/dev/null || return 1
  gdbus introspect --session --dest org.freedesktop.portal.Desktop \
    --object-path /org/freedesktop/portal/desktop 2>/dev/null |
    grep -q 'org.freedesktop.portal.GlobalShortcuts'
}

needs_input_group() {
  [[ "${1:-}" == "--input-group" ]] && return 0
  [[ "${XDG_SESSION_TYPE:-}" == "wayland" ]] || return 1 # X11 has real global hotkeys
  ! has_shortcut_portal
}

if command -v apt-get >/dev/null; then
  say "Installing build dependencies (libasound2-dev, pkg-config)"
  sudo apt-get install -y libasound2-dev pkg-config
else
  say "Not a Debian-based system: install ALSA development headers and pkg-config manually."
fi

say "Giving the logged-in user access to /dev/uinput (auto-paste on Wayland)"
# `uaccess` grants the active session's user; the group is the fallback for
# systems without logind.
echo 'KERNEL=="uinput", SUBSYSTEM=="misc", GROUP="input", MODE="0660", TAG+="uaccess", OPTIONS+="static_node=uinput"' |
  sudo tee /etc/udev/rules.d/60-fulltalk-uinput.rules >/dev/null
echo uinput | sudo tee /etc/modules-load.d/fulltalk-uinput.conf >/dev/null
sudo modprobe uinput
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=misc --sysname-match=uinput

if needs_input_group "${1:-}"; then
  say "Adding $USER to the 'input' group (this desktop has no global-shortcuts portal)"
  sudo usermod -aG input "$USER"
  say "Done. Log out and back in so the new group membership takes effect."
else
  say "Done. The shortcut is bound by your desktop: it asks once, the first time fulltalk dev starts."
fi
