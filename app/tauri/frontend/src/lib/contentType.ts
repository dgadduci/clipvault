// UI helpers for the `clipboard-type-detection` capability. The
// helpers translate the backend's `content_type` strings into
// accessible labels without re-implementing the detector on the
// frontend.
//
// The detector itself lives in `clipvault-core::content_type`; the
// frontend ONLY maps the documented backend values to user-visible
// labels. Unknown or empty values fall back to `"Texto"` so the UI
// keeps rendering meaningful metadata even when the backend extends
// the taxonomy in the future.

/** Canonical display order shared by cards and the desktop type filter. */
export const CONTENT_TYPE_VALUES = [
  "text",
  "url",
  "email",
  "json",
  "jwt",
  "uuid",
  "ipv4",
  "ipv6",
  "hex_color",
  "html",
  "file_path",
  "shell_command",
  "sql",
  "code",
  "image",
] as const;

export type ContentTypeValue = (typeof CONTENT_TYPE_VALUES)[number];

/** Keep only supported, available values and return them in canonical order. */
export function orderAvailableContentTypes(
  available: readonly ContentTypeValue[],
): ContentTypeValue[] {
  const availableSet = new Set(available);
  return CONTENT_TYPE_VALUES.filter((contentType) => availableSet.has(contentType));
}

const CONTENT_TYPE_LABELS: Record<ContentTypeValue, string> = {
  text: "Texto",
  url: "URL",
  email: "Email",
  json: "JSON",
  jwt: "JWT",
  uuid: "UUID",
  ipv4: "IPv4",
  ipv6: "IPv6",
  hex_color: "Hex color",
  html: "HTML",
  file_path: "Ruta",
  shell_command: "Shell",
  sql: "SQL",
  code: "Código",
  image: "Imagen",
};

const FALLBACK_LABEL = "Texto";

/**
 * Map the `content_type` returned by the backend into a localized
 * label. Unknown values (empty string, future variants, legacy data
 * from older databases) all fall back to the safe default
 * `"Texto"` so the UI never renders an empty badge or panics on an
 * unexpected value.
 */
export function contentTypeLabel(value: string | null | undefined): string {
  if (typeof value !== "string" || value.length === 0) {
    return FALLBACK_LABEL;
  }
  const known = (CONTENT_TYPE_LABELS as Record<string, string | undefined>)[value];
  return known ?? FALLBACK_LABEL;
}

/**
 * Whether the value coming back from the backend is one of the
 * documented textual variants. The check is metadata-only and never
 * touches the clipboard payload.
 */
export function isKnownContentType(value: string | null | undefined): value is ContentTypeValue {
  if (typeof value !== "string") return false;
  return Object.prototype.hasOwnProperty.call(CONTENT_TYPE_LABELS, value);
}

/**
 * Default title the card shows when no custom title is set. The
 * function is the canonical entry point so every UI surface falls
 * back to the same label.
 */
export function defaultCardTitle(contentType: string | null | undefined): string {
  return contentTypeLabel(contentType);
}

/**
 * Maximum length of a card title. Mirrors the constant the
 * `history-card-layout` service enforces in Rust.
 */
export const MAX_TITLE_LENGTH = 80;

export function validateTitle(
  raw: string,
): { ok: true; title: string | null } | { ok: false; reason: "too_long" } {
  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    return { ok: true, title: null };
  }
  if (trimmed.length > MAX_TITLE_LENGTH) {
    return { ok: false, reason: "too_long" };
  }
  return { ok: true, title: trimmed };
}
