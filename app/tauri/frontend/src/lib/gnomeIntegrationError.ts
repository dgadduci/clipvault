/**
 * Safe presentation of typed failures from the opt-in GNOME bridge.
 *
 * Tauri rejects a failed command with its serialized `CommandError`,
 * rather than an `Error` instance. Rendering it with `String(error)`
 * produced `[object Object]` and hid the next action from the user.
 * Keep the UI metadata-only: never show the backend's raw message,
 * because I/O errors may include a local path.
 */

const GNOME_ERROR_MESSAGES: Record<string, string> = {
  bundled_missing: "gnome.error.bundled_missing",
  install_error: "gnome.error.install",
  disable_error: "gnome.error.disable",
  listener_error: "gnome.error.listener",
  consent_error: "gnome.error.consent",
  feature_disabled: "gnome.error.feature_disabled",
  invalid_consent: "gnome.error.invalid_consent",
};

function commandErrorKind(error: unknown): string | null {
  if (typeof error !== "object" || error === null) return null;
  const kind = (error as Record<string, unknown>).kind;
  return typeof kind === "string" ? kind : null;
}

/**
 * Turn the serialised command boundary into a stable, actionable and
 * path-free message for the GNOME configuration modal.
 */
export function describeGnomeIntegrationError(error: unknown): string {
  const kind = commandErrorKind(error);
  if (kind && GNOME_ERROR_MESSAGES[kind]) {
    return GNOME_ERROR_MESSAGES[kind];
  }
  return "gnome.error.generic";
}
