#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../.." && pwd)"
config="$repo_root/app/tauri/src-tauri/tauri.conf.json"
app_dir="${1:-}"

if [[ -z "$app_dir" ]]; then
  echo "Usage: bash app/tauri/scripts/verify-linux-appimage-entry.sh <AppDir>" >&2
  exit 2
fi
if [[ ! -d "$app_dir/usr/share/applications" ]]; then
  echo "AppDir does not contain usr/share/applications: $app_dir" >&2
  exit 2
fi

app_identifier="$(sed -n 's/.*"identifier"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$config" | head -n 1)"
product_name="$(sed -n 's/.*"productName"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$config" | head -n 1)"
desktop_entry="$app_dir/usr/share/applications/$app_identifier.desktop"
if [[ -z "$app_identifier" ]] || [[ -z "$product_name" ]] || [[ ! -f "$desktop_entry" ]]; then
  echo "AppDir is missing the desktop entry matching the Linux app ID." >&2
  exit 1
fi

startup_wm_class="$(sed -n 's/^StartupWMClass=//p' "$desktop_entry" | head -n 1)"
icon_name="$(sed -n 's/^Icon=//p' "$desktop_entry" | head -n 1)"
if grep -Eq '^NoDisplay=(true|1)$' "$desktop_entry"; then
  echo "AppDir Wayland desktop entry is hidden from the desktop shell." >&2
  exit 1
fi
if [[ "$startup_wm_class" != "$app_identifier" ]]; then
  echo "AppDir StartupWMClass is ${startup_wm_class:-missing}; expected $app_identifier." >&2
  exit 1
fi
if [[ -z "$icon_name" ]]; then
  echo "AppDir desktop entry has no Icon value." >&2
  exit 1
fi

generated_launcher="$app_dir/usr/share/applications/$product_name.desktop"
if [[ ! -f "$generated_launcher" ]] || \
  ! grep -Fxq '[Desktop Entry]' "$generated_launcher" || \
  ! grep -Eq '^NoDisplay=(true|1)$' "$generated_launcher" || \
  grep -Fq '{{' "$generated_launcher"; then
  echo "AppDir generated launcher should be hidden to avoid a duplicate menu item." >&2
  exit 1
fi
if grep -Fq '{{' "$desktop_entry"; then
  echo "AppDir Wayland desktop entry contains unresolved template values." >&2
  exit 1
fi

shopt -s nullglob
icon_files=("$app_dir"/usr/share/icons/hicolor/*/apps/"$icon_name".png)
if (( ${#icon_files[@]} == 0 )); then
  echo "AppDir does not contain a PNG icon named $icon_name." >&2
  exit 1
fi
icon_header="$(od -An -tx1 -N8 "${icon_files[0]}" | tr -d ' \n')"
if [[ "$icon_header" != "89504e470d0a1a0a" ]]; then
  echo "AppDir icon ${icon_files[0]} is not a PNG." >&2
  exit 1
fi

echo "Validated $(basename "$app_dir"): visible desktop ID=$app_identifier; hidden duplicate launcher=$product_name.desktop; StartupWMClass=$startup_wm_class; Icon=$icon_name."
