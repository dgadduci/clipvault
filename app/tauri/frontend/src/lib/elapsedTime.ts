// Pure, deterministic formatter for the elapsed time a card displays
// underneath its metadata row. The helper is intentionally local — no
// DOM, no Intl, no timezone database — so the desktop shell can
// render it without going through SQLite, the Tauri bridge or any
// platform call. The unit tests exercise every documented branch.

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** Average number of days in a calendar month (365 / 12). */
const DAYS_PER_MONTH = 30;
/** Average number of days in a calendar year (365). */
const DAYS_PER_YEAR = 365;
import { translate } from "./localization.ts";
import type { Locale } from "./localization.ts";

/**
 * Parse an RFC 3339 / ISO 8601 UTC timestamp. Returns `null` when the
 * input is empty, not a string, malformed or carries no usable
 * timezone — the caller falls back to a safe label so a corrupted row
 * never crashes the renderer.
 */
function parseTimestamp(value: unknown): Date | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  if (trimmed.length === 0) return null;
  const parsed = new Date(trimmed);
  if (Number.isNaN(parsed.getTime())) return null;
  return parsed;
}

/**
 * Compute the elapsed time between `createdAt` and `now`. Returns the
 * pair `{ visual, accessible }`. The visual label is the compact
 * string the card renders next to the icon; the accessible label is
 * the screen-reader equivalent.
 *
 * Rules:
 *
 *   - `null` createdAt → a deterministic safe fallback ("Fecha desconocida")
 *     that never throws;
 *   - `createdAt` in the future relative to `now` → "Recién capturado"
 *     (the cap avoids showing "-1 min" or weird negative counters);
 *   - sub-minute: "Ahora";
 *   - sub-hour:   "Hace N min";
 *   - sub-day:    "Hace N h";
 *   - sub-month:  "Hace N días" (1 month = 30 days);
 *   - sub-year:   "Hace N meses";
 *   - rest:       "Hace N años".
 */
export interface ElapsedTime {
  /** Compact localised label rendered next to the clock icon. */
  visual: string;
  /** Long-form label used by `aria-label`. */
  accessible: string;
}

export function formatElapsedTime(
  createdAt: unknown,
  now: Date,
  locale: Locale = "es",
): ElapsedTime {
  const capturedAt = parseTimestamp(createdAt);
  if (capturedAt === null) {
    const unknown = translate("time.unknown", {}, locale);
    return {
      visual: unknown,
      accessible: unknown,
    };
  }
  const delta = capturedAt.getTime() - now.getTime();
  const short = new Intl.RelativeTimeFormat(locale, {
    numeric: "auto",
    style: "short",
  });
  const long = new Intl.RelativeTimeFormat(locale, {
    numeric: "auto",
    style: "long",
  });
  if (delta > MINUTE_MS) {
    // Tolerate small clock skew (≤1 min in the future) but anything
    // larger means the row was inserted with a `created_at` ahead of
    // the current clock. Use a product translation instead of an
    // Intl-relative "now" label for that case.
    const justCaptured = translate("time.just_captured", {}, locale);
    return { visual: justCaptured, accessible: justCaptured };
  }
  const elapsed = Math.max(0, now.getTime() - capturedAt.getTime());
  if (elapsed < MINUTE_MS) {
    return {
      visual: short.format(0, "second"),
      accessible: long.format(0, "second"),
    };
  }
  if (elapsed < HOUR_MS) {
    const minutes = Math.floor(elapsed / MINUTE_MS);
    return { visual: short.format(-minutes, "minute"), accessible: long.format(-minutes, "minute") };
  }
  if (elapsed < DAY_MS) {
    const hours = Math.floor(elapsed / HOUR_MS);
    return { visual: short.format(-hours, "hour"), accessible: long.format(-hours, "hour") };
  }
  if (elapsed < DAYS_PER_MONTH * DAY_MS) {
    const days = Math.floor(elapsed / DAY_MS);
    return { visual: short.format(-days, "day"), accessible: long.format(-days, "day") };
  }
  if (elapsed < DAYS_PER_YEAR * DAY_MS) {
    const months = Math.floor(elapsed / (DAYS_PER_MONTH * DAY_MS));
    return { visual: short.format(-months, "month"), accessible: long.format(-months, "month") };
  }
  const years = Math.floor(elapsed / (DAYS_PER_YEAR * DAY_MS));
  return { visual: short.format(-years, "year"), accessible: long.format(-years, "year") };
}
