import json
import os
from pathlib import Path


release_source_dir = Path(os.environ.get("RELEASE_SOURCE_DIR", "."))
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
(release_source_dir / "app/tauri/src-tauri/tauri.release.conf.json").write_text(
    json.dumps(overlay), encoding="utf-8"
)
