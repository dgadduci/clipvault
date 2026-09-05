// Platform detection helpers.
//
// The frontend never talks to the OS directly; instead it asks the
// backend through `clipvault_diagnostics` for the `platform_os` string
// the core computed at bootstrap. This helper turns that string into a
// stable `Platform` enum so the rest of the UI can branch on a single
// value without parsing free-form strings.
//
// The hint is intentionally **advisory**: the actual shortcut binding
// still comes from the backend through `clipvault_settings_get`. The
// detection here only chooses the user-facing label (Cmd + Shift + V
// on macOS, Ctrl + Shift + V everywhere else) and never influences
// the listener registration.

export type Platform = "macos" | "linux" | "windows" | "unknown";

export function platformFromDiagnostics(
  platformOs: string | null | undefined,
): Platform {
  if (!platformOs) return "unknown";
  const value = platformOs.trim().toLowerCase();
  if (value.startsWith("macos") || value.startsWith("darwin") || value === "osx") {
    return "macos";
  }
  if (value.startsWith("linux")) return "linux";
  if (value.startsWith("windows")) return "windows";
  return "unknown";
}

/**
 * Resolve the shortcut label the **Atajo de pegado rápido** modal
 * renders for the current platform. macOS uses `Cmd + Shift + V`;
 * every other supported host falls back to `Ctrl + Shift + V`. The
 * caller MUST still surface the effective binding the backend
 * returned so a future cross-platform swap is reflected in the UI
 * without a frontend release.
 */
export function defaultQuickPasteShortcutLabel(platform: Platform): string {
  return platform === "macos" ? "Cmd + Shift + V" : "Ctrl + Shift + V";
}