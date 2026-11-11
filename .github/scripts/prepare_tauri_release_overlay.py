import json
import os
from pathlib import Path


overlay = {
    "bundle": {
        "createUpdaterArtifacts": True,
    },
    "plugins": {
        "updater": {
            "pubkey": os.environ["TAURI_UPDATER_PUBLIC_KEY"],
        },
    },
}
Path("app/tauri/src-tauri/tauri.release.conf.json").write_text(
    json.dumps(overlay), encoding="utf-8"
)
