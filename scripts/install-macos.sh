#!/usr/bin/env bash
# Builds fulltalk dev and installs it as a regular macOS app bundle in
# /Applications (or ~/Applications when that is not writable), so it shows up
# in Spotlight, Launchpad and the Dock like any other application.
#
#   scripts/install-macos.sh              build and install (or update)
#   scripts/install-macos.sh --uninstall  remove it; settings, history and the model stay
set -euo pipefail
cd "$(dirname "$0")/.."

name="fulltalk dev"
if [[ -w /Applications ]]; then apps="/Applications"; else apps="$HOME/Applications"; fi
app="$apps/$name.app"

if [[ "${1:-}" == "--uninstall" ]]; then
  rm -rf "/Applications/$name.app" "$HOME/Applications/$name.app"
  echo "Removed $name. Settings, history and the model were left in place."
  exit 0
fi

command -v cargo >/dev/null || PATH="$HOME/.cargo/bin:$PATH"
cargo build --release -p fulltalk

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"

# Assemble the bundle on the side, then swap it in.
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
bundle="$stage/$name.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
install -m 755 target/release/fulltalk "$bundle/Contents/MacOS/fulltalk"
install -m 644 packaging/macos/fulltalk.icns "$bundle/Contents/Resources/fulltalk.icns"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$bundle/Contents/Info.plist"

# Ad-hoc signature: ties Info.plist (microphone prompt text, bundle id) to the
# binary. Not notarised, which is fine for an app built on this machine.
codesign --force --sign - "$bundle"

mkdir -p "$apps"
rm -rf "$app"
mv "$bundle" "$app"

# Ask LaunchServices to index it now instead of whenever it next scans.
lsregister="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
[[ -x "$lsregister" ]] && "$lsregister" -f "$app" || true

echo "Installed $app"
echo "Open it from Spotlight (Cmd+Space, \"fulltalk\"). On first use, allow the microphone and"
echo "enable it under System Settings > Privacy & Security > Accessibility."
