#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
icon_source="$script_dir/../src-tauri/icons/128x128.png"
xdg_data_home="${XDG_DATA_HOME:-${HOME:?HOME or XDG_DATA_HOME must be set}/.local/share}"
desktop_dir="$xdg_data_home/applications"
icon_dir="$xdg_data_home/icons/hicolor/128x128/apps"
desktop_entry="$desktop_dir/com.clipvault.desktop.desktop"
icon_path="$icon_dir/clipvault-app.png"

if [[ "$xdg_data_home" != /* ]]; then
  echo "XDG_DATA_HOME must be an absolute path." >&2
  exit 2
fi

case "${1:-install}" in
  install)
    if [[ ! -f "$icon_source" ]]; then
      echo "ClipVault icon not found: $icon_source" >&2
      exit 1
    fi
    mkdir -p "$desktop_dir" "$icon_dir"
    cp "$icon_source" "$icon_path"
    cat > "$desktop_entry" <<'EOF'
[Desktop Entry]
Type=Application
Name=ClipVault
Exec=clipvault-app
Icon=clipvault-app
Terminal=false
Categories=Utility;
StartupWMClass=com.clipvault.desktop
EOF
    if command -v kbuildsycoca6 >/dev/null 2>&1; then
      kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
    fi
    echo "Installed the hidden ClipVault Wayland identity entry at $desktop_entry"
    ;;
  --remove)
    rm -f "$desktop_entry" "$icon_path"
    if command -v kbuildsycoca6 >/dev/null 2>&1; then
      kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
    fi
    echo "Removed the ClipVault development identity entry."
    ;;
  *)
    echo "Usage: bash app/tauri/scripts/install-kde-dev-desktop-entry.sh [--remove]" >&2
    exit 2
    ;;
esac
