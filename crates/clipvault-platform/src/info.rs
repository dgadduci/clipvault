use std::fmt;
use std::path::PathBuf;

/// Operating system families relevant to ClipVault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsFamily {
    Macos,
    Linux,
    Windows,
    Other,
}

impl fmt::Display for OsFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            OsFamily::Macos => "macos",
            OsFamily::Linux => "linux",
            OsFamily::Windows => "windows",
            OsFamily::Other => "other",
        };
        f.write_str(label)
    }
}

/// Linux display server detected at startup. `Unknown` is used on non-Linux
/// hosts or when the session could not be classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    X11,
    Wayland,
    Unknown,
}

impl fmt::Display for DisplayServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            DisplayServer::X11 => "x11",
            DisplayServer::Wayland => "wayland",
            DisplayServer::Unknown => "unknown",
        };
        f.write_str(label)
    }
}

/// Read-only information about the host environment that ClipVault runs on.
#[derive(Debug, Clone)]
pub struct PlatformInfo {
    pub home_dir: PathBuf,
    pub data_dir: PathBuf,
    pub os_family: OsFamily,
    pub display_server: DisplayServer,
}
