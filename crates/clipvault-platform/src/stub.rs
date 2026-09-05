//! Default [`PlatformInfo`] implementation. Clipboard access, hotkeys and
//! tray will be added by the `desktop-platform-integration` change.
//!
//! The stub is intentionally narrow: it answers the questions the bootstrap
//! needs (where to store the database, what OS we run on, what Linux
//! display server is in use) without pulling in any windowing or
//! clipboard dependency.

use std::env;
use std::path::PathBuf;

use thiserror::Error;

use crate::info::{DisplayServer, OsFamily, PlatformInfo};

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("could not resolve the user's home directory")]
    HomeDirectoryNotFound,
}

pub struct DefaultPlatform;

impl DefaultPlatform {
    pub fn detect() -> Result<PlatformInfo, PlatformError> {
        let home = dirs::home_dir().ok_or(PlatformError::HomeDirectoryNotFound)?;
        let data_dir = home.join(".clipvault");
        let os_family = detect_os_family();
        let display_server = if matches!(os_family, OsFamily::Linux) {
            detect_linux_display_server()
        } else {
            DisplayServer::Unknown
        };

        Ok(PlatformInfo {
            home_dir: home,
            data_dir,
            os_family,
            display_server,
        })
    }
}

fn detect_os_family() -> OsFamily {
    match env::consts::OS {
        "macos" => OsFamily::Macos,
        "linux" => OsFamily::Linux,
        "windows" => OsFamily::Windows,
        _ => OsFamily::Other,
    }
}

fn detect_linux_display_server() -> DisplayServer {
    if env::var_os("WAYLAND_DISPLAY").is_some() {
        DisplayServer::Wayland
    } else if env::var_os("DISPLAY").is_some() {
        DisplayServer::X11
    } else {
        DisplayServer::Unknown
    }
}

/// Resolves the platform data directory used by ClipVault.
#[allow(dead_code)]
pub fn data_dir() -> Result<PathBuf, PlatformError> {
    dirs::home_dir()
        .ok_or(PlatformError::HomeDirectoryNotFound)
        .map(|home| home.join(".clipvault"))
}
