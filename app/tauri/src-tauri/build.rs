//! Tauri build script.
//!
//! The script is intentionally minimal — it just calls
//! `tauri_build::build()` so Tauri generates its built-in
//! `Info.plist` template and merges the macOS Local Network /
//! Bonjour declarations the `local-peer-discovery` change
//! owns. The actual `Info.plist` lives in
//! `app/tauri/src-tauri/macos/Info.plist`; Tauri merges it
//! into the bundled `Info.plist` automatically through the
//! `bundle.macOS.infoPlist` config key. The merge keeps the
//! runtime permission prompt scoped to the opt-in toggle the
//! `clipvault-peer-sharing-*` bridge drives.
//!
//! Why a real file instead of a build.rs comment:
//!
//! - `tauri-bundler` reads `bundle.macOS.infoPlist` during
//!   `tauri build` and copies the keys into the bundled
//!   `Info.plist`. Without a real file the bundle ships
//!   without the `NSLocalNetworkUsageDescription` /
//!   `NSBonjourServices` declarations and macOS silently
//!   blocks the discovery traffic.
//! - The merged keys are visible in `*.app/Contents/Info.plist`
//!   so the contract is auditable from the binary, not just
//!   from the source.

fn main() {
    tauri_build::build();
}
