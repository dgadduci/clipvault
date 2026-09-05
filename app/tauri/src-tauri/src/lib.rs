//! Tauri shell for ClipVault.
//!
//! This crate is intentionally a thin adapter: every command is at
//! most a few lines long and delegates to `clipvault-core`. No
//! business logic lives here.
//!
//! The binary target lives in `main.rs` and the integration tests
//! under `tests/` consume the test-friendly helpers re-exported from
//! [`commands`]. The library target exists so the same code paths
//! the Tauri runtime executes can be exercised from `cargo test`
//! without standing up a full Tauri runtime.

pub mod bootstrap;
pub mod commands;
pub mod main_window_layout;
pub mod metadata_scheduler;
pub mod state;
pub mod tray;
