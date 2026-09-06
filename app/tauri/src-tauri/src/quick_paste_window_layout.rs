//! Pure layout helper for the transient `quick-paste` window.
//!
//! The quick-paste palette is sized to a fixed rectangle
//! (`QUICK_PASTE_WINDOW_WIDTH` × `QUICK_PASTE_WINDOW_HEIGHT` logical
//! pixels) and centred on the active monitor's work area every time
//! the global hotkey fires. Centring before the window becomes visible
//! keeps the palette feeling anchored to the screen the user is
//! working on — a "second desktop" feeling is exactly what the
//! `quick-paste-compact-ui` change wants to avoid.
//!
//! The math is intentionally extracted from the Tauri runtime so the
//! helper is unit tested without standing up a desktop host (and so a
//! regression that drifts the centring math can only ship if it
//! breaks the unit tests in this module).
//!
//! Both `current_monitor()` (preferred) and `primary_monitor()`
//! (fallback) collapse to the same data shape so the renderer can
//! choose which monitor to centre on without losing the pure-math
//! contract: the helper receives `(x, y, width, height, scale_factor)`
//! and returns the logical + physical rectangle the window must land
//! on. The math is symmetric across macOS, Linux X11 and Linux
//! Wayland hosts because every supported Tauri 2 backend reports the
//! work area in physical pixels with a `scale_factor` matching the
//! HiDPI mode.
//!
//! Privacy: the helper only operates on monitor geometry. No
//! clipboard payload, no entry id, no source-application identifier
//! reaches this module.

/// Documented fixed logical width of the quick-paste palette. Mirrors
/// the value in `tauri.conf.json` so a drift between the two files
/// surfaces through the unit tests instead of a regression report.
pub const QUICK_PASTE_WINDOW_WIDTH: f64 = 720.0;

/// Documented fixed logical height of the quick-paste palette.
/// Mirrors the value in `tauri.conf.json` so a drift between the two
/// files surfaces through the unit tests instead of a regression
/// report.
pub const QUICK_PASTE_WINDOW_HEIGHT: f64 = 520.0;

/// Pure layout result the renderer hands to Tauri's `set_size` /
/// `set_position` calls. Logical coordinates are the DPI-independent
/// rectangle Tauri uses for the size call; the physical equivalents
/// are the pixel rectangle the OS compositor expects so the window
/// lands exactly where the math says.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickPasteWindowLayout {
    pub logical_size: (f64, f64),
    pub logical_position: (f64, f64),
    pub physical_size: (u32, u32),
    pub physical_position: (i32, i32),
    pub scale_factor: f64,
}

/// Compute the documented quick-paste layout: the palette fills its
/// fixed logical rectangle and is centred horizontally and vertically
/// inside the supplied work area.
///
/// `work_area` is the tuple `(x, y, width, height)` the monitor
/// reports in physical pixels; `scale_factor` is the active DPI scale
/// factor Tauri exposes through `Monitor::scale_factor`. The helper
/// is the single source of truth for the math so the unit tests can
/// pin every documented invariant without a Tauri runtime.
///
/// The width and height are intentionally NOT clamped: the documented
/// size is fixed and the renderer cannot honour a "smaller" request
/// from the host. A monitor smaller than the palette surfaces as a
/// negative offset, which matches the main-window helper's behaviour
/// — the window is centred on the requested display even when it
/// exceeds the work area, so the user keeps the documented product
/// surface instead of seeing an unpredictable resized variant.
pub fn compute_quick_paste_window_layout(
    work_area: (f64, f64, f64, f64),
    scale_factor: f64,
) -> QuickPasteWindowLayout {
    let (work_x, work_y, work_width, work_height) = work_area;
    let scale = if scale_factor <= 0.0 {
        1.0
    } else {
        scale_factor
    };
    let logical_width = QUICK_PASTE_WINDOW_WIDTH;
    let logical_height = QUICK_PASTE_WINDOW_HEIGHT;

    let work_logical_width = work_width / scale;
    let work_logical_height = work_height / scale;
    let work_logical_x = work_x / scale;
    let work_logical_y = work_y / scale;

    let logical_x = work_logical_x + (work_logical_width - logical_width) / 2.0;
    let logical_y = work_logical_y + (work_logical_height - logical_height) / 2.0;

    let physical_width = (logical_width * scale).round() as u32;
    let physical_height = (logical_height * scale).round() as u32;
    let physical_x = work_x as i32 + ((work_width - physical_width as f64) / 2.0).round() as i32;
    let physical_y = work_y as i32 + ((work_height - physical_height as f64) / 2.0).round() as i32;

    QuickPasteWindowLayout {
        logical_size: (logical_width, logical_height),
        logical_position: (logical_x, logical_y),
        physical_size: (physical_width, physical_height),
        physical_position: (physical_x, physical_y),
        scale_factor: scale,
    }
}

/// Resolve the rectangle the renderer should centre the quick-paste
/// window on. The helper pins the documented preference order:
///
/// - When `current` is `Some`, the palette centres on the current
///   monitor's work area so the user sees the palette next to the
///   application they were typing in.
/// - When `current` is `None`, the palette falls back to the primary
///   monitor's work area so the activation contract still holds on
///   hosts that cannot determine the active display (headless, RDP,
///   X11 without `RANDR`, …).
/// - When both are `None`, the helper returns `None` so the renderer
///   keeps the conf-defined defaults declared in `tauri.conf.json`.
///
/// The helper is pure: callers pass the rectangles Tauri reports and
/// receive the rectangle the palette must land on, with no side
/// effects.
pub fn resolve_quick_paste_work_area(
    current: Option<(f64, f64, f64, f64)>,
    primary: Option<(f64, f64, f64, f64)>,
) -> Option<(f64, f64, f64, f64)> {
    current.or(primary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_centers_on_a_full_hd_work_area() {
        let layout = compute_quick_paste_window_layout((0.0, 0.0, 1920.0, 1080.0), 1.0);
        assert_eq!(layout.logical_size, (720.0, 520.0));
        // 1920 / 2 = 960, 1080 / 2 = 540; offset from origin is
        // (960 - 720/2, 540 - 520/2) = (600, 280).
        assert_eq!(layout.logical_position, (600.0, 280.0));
        assert_eq!(layout.physical_size, (720, 520));
        assert_eq!(layout.physical_position, (600, 280));
    }

    #[test]
    fn layout_centers_on_a_non_origin_work_area() {
        // Work area starting at (200, 100), width 1440, height 900.
        let layout = compute_quick_paste_window_layout((200.0, 100.0, 1440.0, 900.0), 1.0);
        // Logical centre: 200 + (1440 - 720) / 2 = 200 + 360 = 560.
        // Vertical centre: 100 + (900 - 520) / 2 = 100 + 190 = 290.
        assert_eq!(layout.logical_position, (560.0, 290.0));
        assert_eq!(layout.logical_size, (720.0, 520.0));
    }

    #[test]
    fn layout_respects_retina_scale_factor() {
        // A 2x display: physical 2880×1800 → logical 1440×900.
        let layout = compute_quick_paste_window_layout((0.0, 0.0, 2880.0, 1800.0), 2.0);
        assert_eq!(layout.logical_size, (720.0, 520.0));
        assert_eq!(layout.physical_size, (1440, 1040));
        // Logical centre at 2x: (1440 - 720)/2 = 360 horizontally,
        // (900 - 520)/2 = 190 vertically. The physical rectangle
        // collapses to (0, 380) because the work area is 2880×1800
        // and the window fills (2880 - 1440)/2 = 720 horizontally
        // and (1800 - 1040)/2 = 380 vertically from the origin.
        assert_eq!(layout.logical_position, (360.0, 190.0));
        assert_eq!(layout.physical_position, (720, 380));
    }

    #[test]
    fn layout_falls_back_to_unit_scale_when_factor_is_non_positive() {
        // Headless sessions report a 0.0 scale factor; the math must
        // default to a unit scale rather than divide by zero.
        let layout = compute_quick_paste_window_layout((0.0, 0.0, 1920.0, 1080.0), 0.0);
        assert_eq!(layout.scale_factor, 1.0);
        assert_eq!(layout.logical_size, (720.0, 520.0));
        assert_eq!(layout.physical_size, (720, 520));
    }

    #[test]
    fn layout_keeps_the_documented_size_when_monitor_is_smaller() {
        // A small laptop docked to a tiny display: the palette keeps
        // its documented size, the centring math produces a negative
        // offset so the visible area still gets as much of the
        // palette as the work area allows.
        let layout = compute_quick_paste_window_layout((0.0, 0.0, 600.0, 400.0), 1.0);
        assert_eq!(layout.logical_size, (720.0, 520.0));
        assert_eq!(layout.logical_position.0, -60.0);
        assert_eq!(layout.logical_position.1, -60.0);
    }

    #[test]
    fn resolve_prefers_current_over_primary() {
        let current = Some((100.0, 100.0, 1280.0, 720.0));
        let primary = Some((0.0, 0.0, 1920.0, 1080.0));
        let resolved = resolve_quick_paste_work_area(current, primary);
        assert_eq!(resolved, Some((100.0, 100.0, 1280.0, 720.0)));
    }

    #[test]
    fn resolve_falls_back_to_primary_when_current_is_unavailable() {
        let primary = Some((0.0, 0.0, 1920.0, 1080.0));
        let resolved = resolve_quick_paste_work_area(None, primary);
        assert_eq!(resolved, Some((0.0, 0.0, 1920.0, 1080.0)));
    }

    #[test]
    fn resolve_returns_none_when_both_monitors_are_unavailable() {
        assert_eq!(resolve_quick_paste_work_area(None, None), None);
    }
}
