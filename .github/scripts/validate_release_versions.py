import json
import os
import re
import tomllib
from pathlib import Path


release_tag = os.environ["RELEASE_TAG"]
if re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", release_tag) is None:
    raise SystemExit(f"Release tag is not stable SemVer: {release_tag}")
expected = release_tag[1:]

with Path("Cargo.toml").open("rb") as source:
    cargo_version = tomllib.load(source)["workspace"]["package"]["version"]
tauri_version = json.loads(
    Path("app/tauri/src-tauri/tauri.conf.json").read_text(encoding="utf-8")
)["version"]
frontend_version = json.loads(
    Path("app/tauri/frontend/package.json").read_text(encoding="utf-8")
)["version"]
versions = {
    "tag": expected,
    "Cargo workspace": cargo_version,
    "Tauri config": tauri_version,
    "frontend package": frontend_version,
}
mismatched = {name: version for name, version in versions.items() if version != expected}
if mismatched:
    detail = ", ".join(f"{name}={version}" for name, version in mismatched.items())
    raise SystemExit(f"Version mismatch: {detail}")
print(f"Validated release version {expected} across all manifests.")
