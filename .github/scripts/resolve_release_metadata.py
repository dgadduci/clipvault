import json
import os
import re
from pathlib import Path


event_name = os.environ["EVENT_NAME"]
release_tag = (
    os.environ["RELEASE_TAG_INPUT"]
    if event_name == "workflow_dispatch"
    else os.environ["PUSH_TAG"]
)
if re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", release_tag) is None:
    raise SystemExit(f"Release tag is not stable SemVer: {release_tag}")

mac_arm = {
    "label": "macOS Apple Silicon",
    "runner": "macos-latest",
    "rust_target": "aarch64-apple-darwin",
    "args": "--target aarch64-apple-darwin --bundles app,dmg",
}
mac_intel = {
    "label": "macOS Intel",
    "runner": "macos-latest",
    "rust_target": "x86_64-apple-darwin",
    "args": "--target x86_64-apple-darwin --bundles app,dmg",
}
linux = {
    "label": "Linux x86_64",
    "runner": "ubuntu-22.04",
    "rust_target": "x86_64-unknown-linux-gnu",
    "args": "--bundles appimage,deb,rpm",
}
targets = [linux] if event_name == "workflow_dispatch" else [mac_arm, mac_intel, linux]

with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
    output.write(f"release_tag={release_tag}\n")
    matrix = json.dumps({"include": targets}, separators=(",", ":"))
    output.write(f"build_matrix={matrix}\n")
