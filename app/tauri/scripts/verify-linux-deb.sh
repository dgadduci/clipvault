#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../.." && pwd)"
package_path="${1:-}"
config="$repo_root/app/tauri/src-tauri/tauri.conf.json"
linux_config="$repo_root/app/tauri/src-tauri/tauri.linux.conf.json"
template="$repo_root/app/tauri/src-tauri/desktop-template.desktop"

if [[ -z "$package_path" ]]; then
  echo "Usage: bash app/tauri/scripts/verify-linux-deb.sh <package.deb>" >&2
  exit 2
fi
if [[ ! -f "$package_path" ]]; then
  echo "Package not found: $package_path" >&2
  exit 2
fi

app_identifier="$(sed -n 's/.*"identifier"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$config" | head -n 1)"
if [[ -z "$app_identifier" ]] || \
  ! grep -Eq '"enableGTKAppId"[[:space:]]*:[[:space:]]*true' "$linux_config" || \
  ! grep -Fxq "StartupWMClass=$app_identifier" "$template"; then
  echo "Linux GTK app ID and desktop template do not match the Tauri identifier." >&2
  exit 1
fi

desktop_path="$(ar p "$package_path" data.tar.gz | tar -tzf - | awk '
  /^usr\/share\/applications\/[^/]+\.desktop$/ && !desktop { desktop = $0 }
  END { print desktop }
')"
if [[ -z "$desktop_path" ]]; then
  echo "Package does not contain a desktop entry." >&2
  exit 1
fi

desktop_entry="$(ar p "$package_path" data.tar.gz | tar -xzOf - "$desktop_path")"
startup_wm_class="$(printf '%s\n' "$desktop_entry" | sed -n 's/^StartupWMClass=//p' | head -n 1)"
icon_name="$(printf '%s\n' "$desktop_entry" | sed -n 's/^Icon=//p' | head -n 1)"
if [[ "$startup_wm_class" != "$app_identifier" ]]; then
  echo "Package StartupWMClass is ${startup_wm_class:-missing}; expected $app_identifier." >&2
  exit 1
fi
if [[ -z "$icon_name" ]]; then
  echo "Package desktop entry has no Icon value." >&2
  exit 1
fi

icon_path="$(ar p "$package_path" data.tar.gz | tar -tzf - | awk -v icon="$icon_name" '
  index($0, "usr/share/icons/hicolor/") == 1 &&
  index($0, "/apps/") > 0 &&
  substr($0, length($0) - length(icon) - 3) == icon ".png" && !result { result = $0 }
  END { print result }
')"
if [[ -z "$icon_path" ]]; then
  echo "Package does not contain a PNG icon named $icon_name." >&2
  exit 1
fi

icon_tmp="$(mktemp "${TMPDIR:-/tmp}/clipvault-icon-check.XXXXXX")"
trap 'rm -f "$icon_tmp"' EXIT
ar p "$package_path" data.tar.gz | tar -xzOf - "$icon_path" > "$icon_tmp"
icon_header="$(od -An -tx1 -N8 "$icon_tmp" | tr -d ' \n')"
icon_size="$(wc -c < "$icon_tmp" | tr -d ' ')"
if [[ "$icon_header" != "89504e470d0a1a0a" ]] || (( icon_size == 0 )); then
  echo "Packaged icon $icon_path is empty or is not a PNG." >&2
  exit 1
fi

echo "Validated $(basename "$package_path"): StartupWMClass=$startup_wm_class; Icon=$icon_name ($icon_size bytes)."
