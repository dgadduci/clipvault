import json
import os
from pathlib import Path


configured_key = json.loads(
    Path("app/tauri/src-tauri/tauri.conf.json").read_text(encoding="utf-8")
)["plugins"]["updater"]["pubkey"]
public_key = os.environ.get("TAURI_UPDATER_PUBLIC_KEY", "").strip()
private_key = os.environ.get("TAURI_SIGNING_PRIVATE_KEY", "").strip()
if not public_key:
    raise SystemExit("::error::Required GitHub variable is missing: TAURI_UPDATER_PUBLIC_KEY")
if not private_key:
    raise SystemExit("::error::Required GitHub secret is missing: TAURI_SIGNING_PRIVATE_KEY")
if public_key != configured_key:
    raise SystemExit("::error::TAURI_UPDATER_PUBLIC_KEY does not match tauri.conf.json")
print("Updater signing configuration is consistent. macOS bundles will use ad-hoc signing.")
