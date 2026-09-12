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
  bundled_missing:
    "Esta build no incluye los archivos de la extensión GNOME. Reiniciá ClipVault desde la build actual.",
  install_error:
    "No se pudo instalar la extensión local. Verificá que tu directorio de usuario permita crear extensiones de GNOME y reintentá.",
  listener_error:
    "No se pudo preparar la conexión local con GNOME. No se aplicó la integración; reintentá tras cerrar otras instancias de ClipVault.",
  consent_error:
    "No se pudo guardar tu decisión de integración. Reintentá.",
  feature_disabled:
    "La integración GNOME no está incluida en esta build.",
  invalid_consent: "La decisión de consentimiento no es válida. Reintentá.",
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
  return "No se pudo completar la operación de integración GNOME. Reintentá.";
}
