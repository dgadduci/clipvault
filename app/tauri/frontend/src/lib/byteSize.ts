// Bytes-to-human formatter used by the image-card metadata row.
//
// The helper formats the persisted `content_size` of an image row in
// base-1024 bytes, KB or MB. It deliberately uses the binary
// convention (1 KB = 1024 B) the rest of the desktop already follows
// so the user never sees "1 KB" for a 999-byte payload. The exact
// byte count is always exposed through `accessible` so a screen reader
// reports the precise payload length.

export interface ByteSize {
  /** Compact label rendered next to the size icon. */
  visual: string;
  /** Long-form label used by `aria-label`. */
  accessible: string;
}

const KB = 1024;
const MB = 1024 * KB;
const GB = 1024 * MB;
import type { Locale } from "./localization.ts";
import { translate } from "./localization.ts";

/**
 * Format `bytes` as B / KB / MB / GB using base-1024. Negative or
 * non-finite inputs collapse to a deterministic safe fallback so a
 * legacy / corrupted row never throws the renderer.
 */
export function formatByteSize(bytes: number, locale: Locale = "es"): ByteSize {
  if (!Number.isFinite(bytes) || bytes < 0) {
    const unknown = translate("size.unknown", {}, locale);
    return { visual: "—", accessible: unknown };
  }
  if (bytes < KB) {
    const amount = new Intl.NumberFormat(locale).format(bytes);
    return {
      visual: `${amount} B`,
      accessible: new Intl.NumberFormat(locale, {
        style: "unit",
        unit: "byte",
        unitDisplay: "long",
      }).format(bytes),
    };
  }
  if (bytes < MB) {
    const kb = bytes / KB;
    const value = formatAmount(kb, locale, kb < 10 ? 2 : kb < 100 ? 1 : 0);
    return {
      visual: `${value} KB`,
      accessible: `${formatAmount(Math.round(bytes), locale, 0)} B (${value} KB)`,
    };
  }
  if (bytes < GB) {
    const mb = bytes / MB;
    const value = formatAmount(mb, locale, mb < 10 ? 2 : mb < 100 ? 1 : 0);
    return {
      visual: `${value} MB`,
      accessible: `${formatAmount(Math.round(bytes), locale, 0)} B (${value} MB)`,
    };
  }
  const gb = bytes / GB;
  const value = formatAmount(gb, locale, gb < 10 ? 2 : 1);
  return {
    visual: `${value} GB`,
    accessible: `${formatAmount(Math.round(bytes), locale, 0)} B (${value} GB)`,
  };
}

function formatAmount(value: number, locale: Locale, maximumFractionDigits: number): string {
  return new Intl.NumberFormat(locale, {
    maximumFractionDigits,
  }).format(value);
}
