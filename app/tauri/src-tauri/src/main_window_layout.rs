//! Pure layout helper for the main window on startup.
//!
//! The desktop shell positions and sizes the main window once during
//! the Tauri `setup` callback so the first paint already lands at
//! the documented product position: the window fills the primary
//! monitor's work area horizontally, is bounded to the documented
//! height and is centered horizontally on the top edge of the work
//! area.
//!
//! The math is intentionally extracted from the Tauri runtime so it
//! is unit tested without a desktop host (and so a regression that
//! drifts the centring math can only ship if it breaks the unit
//! tests in this module).
//!
//! The constants below are the documented product limits: the
//! minimum height is enough to render every desktop region with
//! breathing room, the maximum caps the window so a tall monitor
//! does not introduce an empty band beneath the rail, and the
//! minimum width protects narrow viewports from collapsing the
//! toolbar.

/// Pure layout result the setup callback hands to Tauri's
/// `set_size` / `set_position` calls. Logical coordinates are the
/// DPI-independent rectangle Tauri uses for the size call; the
/// physical equivalents are the pixel rectangle the OS compositor
/// expects so the window lands exactly where the math says.
#[derive(Debug, Clone, PartialEq)]
pub struct MainWindowLayout {
    pub logical_size: (f64, f64),
    pub logical_position: (f64, f64),
    pub physical_size: (u32, u32),
    pub physical_position: (i32, i32),
    pub scale_factor: f64,
}

/// Documented minimum window height. Pins the desktop to a compact
/// surface: the toolbar, the rail and the collection panel all fit
/// without leaving a large empty band beneath them.
pub const MAIN_MIN_HEIGHT: f64 = 380.0;

/// Documented maximum window height. Caps the height even on a
/// tall monitor so the desktop stays compact and the OS does not
/// waste vertical space on an unused band beneath the rail.
pub const MAIN_TARGET_HEIGHT: f64 = 460.0;

/// Documented minimum window width. Protects narrow viewports from
/// collapsing the toolbar and the rail.
pub const MAIN_MIN_WIDTH: f64 = 720.0;

/// Source selected for the initial monitor-dependent layout.  Wayland
/// compositors are allowed to omit a primary monitor, so startup must
/// distinguish that ordinary condition from an unusable window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMonitorSource {
    Current,
    Primary,
    Available,
    ConfigurationDefaults,
}

impl MainMonitorSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Primary => "primary",
            Self::Available => "available",
            Self::ConfigurationDefaults => "configuration_defaults",
        }
    }
}

/// Select a monitor for the one-shot startup layout without treating a
/// missing primary monitor as a fatal condition.  This is deliberately
/// generic so the fallback policy is testable without a Tauri runtime.
pub fn select_main_monitor<T>(
    current: Option<T>,
    primary: Option<T>,
    available: impl IntoIterator<Item = T>,
) -> (Option<T>, MainMonitorSource) {
    if let Some(monitor) = current {
        return (Some(monitor), MainMonitorSource::Current);
    }
    if let Some(monitor) = primary {
        return (Some(monitor), MainMonitorSource::Primary);
    }
    if let Some(monitor) = available.into_iter().next() {
        return (Some(monitor), MainMonitorSource::Available);
    }
    (None, MainMonitorSource::ConfigurationDefaults)
}

/// Compute the documented startup layout: the main window fills the
/// work-area width (clamped to the minimum), uses the bounded
/// height, is centered horizontally inside the work area and is
/// pinned to the top edge.
///
/// `work_area` is the tuple `(x, y, width, height)` the monitor
/// reports in physical pixels; `scale_factor` is the active DPI
/// scale factor Tauri exposes through `Monitor::scale_factor`. The
/// helper is the single source of truth for the math so the unit
/// tests can pin every documented invariant without a Tauri
/// runtime.
pub fn compute_main_window_layout(
    work_area: (f64, f64, f64, f64),
    scale_factor: f64,
) -> MainWindowLayout {
    let (work_x, work_y, work_width, work_height) = work_area;
    let scale = if scale_factor <= 0.0 {
        1.0
    } else {
        scale_factor
    };
    let logical_width = (work_width / scale).max(MAIN_MIN_WIDTH);
    let logical_height = (work_height / scale).clamp(MAIN_MIN_HEIGHT, MAIN_TARGET_HEIGHT);
    let work_logical_width = work_width / scale;
    let work_logical_x = work_x / scale;

    let logical_x = work_logical_x + (work_logical_width - logical_width) / 2.0;
    let logical_y = work_y / scale;

    let physical_width = (logical_width * scale).round() as u32;
    let physical_height = (logical_height * scale).round() as u32;
    let physical_x = work_x as i32 + ((work_width - physical_width as f64) / 2.0).round() as i32;
    let physical_y = work_y as i32;

    MainWindowLayout {
        logical_size: (logical_width, logical_height),
        logical_position: (logical_x, logical_y),
        physical_size: (physical_width, physical_height),
        physical_position: (physical_x, physical_y),
        scale_factor: scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_centers_horizontally_and_pins_to_top() {
        // 1920×1080 work area at 1.0 scale: width 1920, height 1080.
        let layout = compute_main_window_layout((0.0, 0.0, 1920.0, 1080.0), 1.0);
        // Width fills the work area (already at or above the floor).
        assert_eq!(layout.logical_size.0, 1920.0);
        // Height is capped to the bounded target.
        assert_eq!(layout.logical_size.1, MAIN_TARGET_HEIGHT);
        // The window is centered horizontally (offset = 0 here).
        assert_eq!(layout.logical_position.0, 0.0);
        // Top of the work area.
        assert_eq!(layout.logical_position.1, 0.0);
        assert_eq!(layout.physical_size.0, 1920);
        assert_eq!(layout.physical_size.1, 460);
    }

    #[test]
    fn layout_offsets_for_non_origin_work_areas() {
        // Work area starting at (200, 100), width 1440, height 900.
        let layout = compute_main_window_layout((200.0, 100.0, 1440.0, 900.0), 1.0);
        // Window centers at (200 + (1440-1440)/2, 100) = (200, 100).
        assert_eq!(layout.logical_position.0, 200.0);
        assert_eq!(layout.logical_position.1, 100.0);
        assert_eq!(layout.logical_size.0, 1440.0);
    }

    #[test]
    fn layout_respects_retina_scale_factor() {
        // A 2x display: physical 2880×1800 → logical 1440×900.
        let layout = compute_main_window_layout((0.0, 0.0, 2880.0, 1800.0), 2.0);
        assert_eq!(layout.logical_size.0, 1440.0);
        assert_eq!(layout.logical_size.1, MAIN_TARGET_HEIGHT);
        assert_eq!(layout.physical_size.0, 2880);
        // Logical center of the window is at (0, 0) on a fullscreen
        // work area; physical size scales by the factor.
        assert_eq!(layout.logical_position.0, 0.0);
        assert_eq!(layout.logical_position.1, 0.0);
        // Physical X is centered inside the work area; the work area
        // starts at 0,0 with width 2880 and the window is 2880 wide,
        // so the physical X equals 0.
        assert_eq!(layout.physical_position.0, 0);
        assert_eq!(layout.physical_position.1, 0);
    }

    #[test]
    fn layout_clamps_minimums_and_maximums() {
        // Extremely small work area (laptop docked to a tiny
        // display). Width below MAIN_MIN_WIDTH must clamp up.
        let layout = compute_main_window_layout((0.0, 0.0, 500.0, 200.0), 1.0);
        assert_eq!(layout.logical_size.0, MAIN_MIN_WIDTH);
        // Height below MAIN_MIN_HEIGHT must clamp up.
        assert_eq!(layout.logical_size.1, MAIN_MIN_HEIGHT);
        // The window is wider than the work area: the centering
        // formula yields a negative offset so the window still fits
        // the visible area as much as possible.
        assert!(layout.logical_position.0 <= 0.0);
        // Top of the work area.
        assert_eq!(layout.logical_position.1, 0.0);
    }

    #[test]
    fn layout_falls_back_to_unit_scale_when_factor_is_non_positive() {
        // Some headless sessions report a 0.0 scale factor; the math
        // must default to a unit scale rather than divide by zero.
        let layout = compute_main_window_layout((0.0, 0.0, 1920.0, 1080.0), 0.0);
        assert_eq!(layout.scale_factor, 1.0);
        assert_eq!(layout.logical_size.0, 1920.0);
        assert_eq!(layout.physical_size.0, 1920);
    }

    #[test]
    fn monitor_selection_prefers_current_then_primary_then_available() {
        assert_eq!(
            select_main_monitor(Some("current"), Some("primary"), ["available"]),
            (Some("current"), MainMonitorSource::Current)
        );
        assert_eq!(
            select_main_monitor::<&str>(None, Some("primary"), ["available"]),
            (Some("primary"), MainMonitorSource::Primary)
        );
        assert_eq!(
            select_main_monitor::<&str>(None, None, ["available"]),
            (Some("available"), MainMonitorSource::Available)
        );
    }

    #[test]
    fn monitor_selection_keeps_configuration_defaults_without_any_monitor() {
        assert_eq!(
            select_main_monitor::<&str>(None, None, []),
            (None, MainMonitorSource::ConfigurationDefaults)
        );
    }
}
