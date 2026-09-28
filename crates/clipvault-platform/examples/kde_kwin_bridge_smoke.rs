//! Manual smoke test for the KDE KWin integration.
//!
//! Runs the bridge against the live session bus and emits an
//! envelope with `dbus-send` so the operator can verify the
//! receiver end-to-end without booting the Tauri shell. The
//! example only prints metadata-only diagnostics; it never reads
//! clipboard bytes or window titles.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example kde_kwin_bridge_smoke \
//!   --features linux-kde-kwin-integration -- --live
//! ```
//!
//! The example is intentionally NOT compiled into production
//! builds — it sits under `examples/` so `cargo build` never
//! links it. The runtime smoke test drives the bridge directly
//! to confirm the session-bus wiring before the shell is wired
//! up.

#![cfg(all(target_os = "linux", feature = "linux-kde-kwin-integration"))]

use std::time::Duration;

use clipvault_platform::runtime::linux_kde_kwin_integration::{
    detect_session, start_bridge, KdeKwinActiveApplication, KdeKwinIntegrationState,
    SharedKdeKwinSnapshot,
};
use clipvault_platform::{ActiveApplicationProbe, KDE_KWIN_BUS_NAME, KDE_KWIN_OBJECT_PATH};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = detect_session();
    println!("session classification: {session:?}");
    if !matches!(
        session,
        clipvault_platform::runtime::linux_kde_kwin_integration::KdeSessionKind::KdePlasmaWayland
    ) {
        eprintln!(
            "kde_kwin_bridge_smoke: host is not KDE Plasma Wayland; \
             the example refuses to take the bus name. Rerun on a \
             KDE Plasma Wayland session."
        );
        std::process::exit(2);
    }

    let snapshot = SharedKdeKwinSnapshot::new();
    let handle = start_bridge(snapshot.clone()).await?;
    println!("bridge bound to {KDE_KWIN_BUS_NAME} at {KDE_KWIN_OBJECT_PATH}");

    // The shell would normally publish an envelope on focus
    // changes. The example waits so the operator can run
    // `dbus-send` from another shell. Replace this block with
    // an automated call if the operator wants a non-interactive
    // check.
    println!("snapshot state before any envelope: {:?}", snapshot.state());

    // Briefly expose the probe name so the operator can confirm
    // the cache is wired to the same probe used by the capture
    // pipeline.
    let probe = KdeKwinActiveApplication::new(snapshot.clone());
    println!("probe backend: {}", probe.name());

    // Wait up to 5 seconds for the operator to push envelopes
    // manually. After the wait the example prints the latest
    // snapshot diagnostics so the operator can confirm the
    // bridge accepted the envelope.
    let wait = Duration::from_secs(5);
    println!(
        "listening for {wait:?}; use `dbus-send` from another shell \
         (or press Ctrl-C) to terminate",
    );
    tokio::time::sleep(wait).await;

    let diagnostics = handle.diagnostics();
    println!("final snapshot state: {:?}", diagnostics.state);
    println!("final backend: {:?}", diagnostics.backend);
    println!("final active_app_id: {:?}", diagnostics.active_app_id);
    if let Some(error) = diagnostics.error.as_deref() {
        println!("last error: {error}");
    }
    println!(
        "drop probe state: {:?}",
        KdeKwinIntegrationState::as_str(snapshot.state())
    );
    Ok(())
}
