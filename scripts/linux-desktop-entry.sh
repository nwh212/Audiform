#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (c) The Audiform contributors.
# This file is part of Audiform, based on Altered Audio Editor.
#
# Adds Audiform as built from this source tree to the Linux desktop, for the
# current user only: the app icon in every size and a launcher entry
# (audiform.desktop) that runs the built program. The desktop then shows
# Audiform's icon in the taskbar, the window switcher and the app launcher.
# On Wayland the desktop finds a window's icon only through such an entry,
# matched by the window's app id ("audiform", the program's name). Release
# packages (.deb, AppImage) bring their own; this is for builds from source.
#
#   scripts/linux-desktop-entry.sh install   Add or update the entry and icons.
#   scripts/linux-desktop-entry.sh remove    Take away what install added.
#
# AUDIFORM_BIN names another program to run (default: the release build).
# Nothing outside ~/.local/share (or $XDG_DATA_HOME) is touched; no sudo.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="${AUDIFORM_BIN:-$root/src-tauri/target/release/audiform}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
apps="$data/applications"
icons="$data/icons/hicolor"
entry="$apps/audiform.desktop"
# Marks entries this script wrote, so remove never deletes someone else's.
marker="X-Audiform-Written-By=scripts/linux-desktop-entry.sh"
# Icon sizes and the file in src-tauri/icons each comes from.
sizes=("32:32x32.png" "64:64x64.png" "128:128x128.png" "256:128x128@2x.png" "512:icon.png")

refresh() {
  command -v update-desktop-database >/dev/null && update-desktop-database -q "$apps" || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t -f "$icons" 2>/dev/null || true
  command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
}

install_entry() {
  if [[ ! -x "$bin" ]]; then
    echo "No program at $bin. Build it first: npm run tauri build -- --no-bundle" >&2
    exit 1
  fi
  if [[ "$bin" == *[\"\`\$\\]* || "$bin" == *$'\n'* ]]; then
    echo "The program's path has characters a launcher entry cannot hold: $bin" >&2
    exit 1
  fi
  if [[ -e "$entry" ]] && ! grep -qxF "$marker" "$entry"; then
    echo "$entry exists and was not written by this script; leaving it alone." >&2
    exit 1
  fi
  for s in "${sizes[@]}"; do
    install -Dm644 "$root/src-tauri/icons/${s#*:}" "$icons/${s%%:*}x${s%%:*}/apps/audiform.png"
  done
  mkdir -p "$apps"
  cat >"$entry" <<EOF
[Desktop Entry]
Type=Application
Name=Audiform
GenericName=Multitrack Audio Editor
Comment=Record, edit and mix audio
Exec="$bin" %F
Icon=audiform
StartupWMClass=audiform
Terminal=false
Categories=AudioVideo;Audio;AudioVideoEditing;
$marker
EOF
  refresh
  echo "Added $entry and the icons under $icons. It runs: $bin"
  echo "Start Audiform again (from the launcher or the terminal) to see its icon."
}

remove_entry() {
  if [[ -e "$entry" ]]; then
    if grep -qxF "$marker" "$entry"; then
      rm -f "$entry"
      echo "Removed $entry"
    else
      echo "$entry was not written by this script; leaving it alone." >&2
    fi
  fi
  # Only icons that are still exactly the ones install copied.
  for s in "${sizes[@]}"; do
    icon="$icons/${s%%:*}x${s%%:*}/apps/audiform.png"
    if [[ -f "$icon" ]] && cmp -s "$icon" "$root/src-tauri/icons/${s#*:}"; then
      rm -f "$icon"
      echo "Removed $icon"
    fi
  done
  refresh
}

case "${1:-}" in
  install) install_entry ;;
  remove) remove_entry ;;
  *)
    echo "Usage: $0 install|remove" >&2
    exit 2
    ;;
esac
