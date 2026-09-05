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

/**
 * Format `bytes` as B / KB / MB / GB using base-1024. Negative or
 * non-finite inputs collapse to a deterministic safe fallback so a
 * legacy / corrupted row never throws the renderer.
 */
export function formatByteSize(bytes: number): ByteSize {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return { visual: "—", accessible: "Tamaño desconocido" };
  }
  if (bytes < KB) {
    return {
      visual: `${bytes} B`,
      accessible: `${bytes} bytes`,
    };
  }
  if (bytes < MB) {
    const kb = bytes / KB;
    return {
      visual: `${kb.toFixed(kb < 10 ? 2 : kb < 100 ? 1 : 0)} KB`,
      accessible: `${Math.round(bytes)} bytes (${formatKb(bytes)} kibibytes)`,
    };
  }
  if (bytes < GB) {
    const mb = bytes / MB;
    return {
      visual: `${mb.toFixed(mb < 10 ? 2 : mb < 100 ? 1 : 0)} MB`,
      accessible: `${Math.round(bytes)} bytes (${formatMb(bytes)} mebibytes)`,
    };
  }
  const gb = bytes / GB;
  return {
    visual: `${gb.toFixed(gb < 10 ? 2 : 1)} GB`,
    accessible: `${Math.round(bytes)} bytes (${formatGb(bytes)} gibibytes)`,
  };
}

function formatKb(bytes: number): string {
  return (bytes / KB).toFixed(1);
}

function formatMb(bytes: number): string {
  return (bytes / MB).toFixed(2);
}

function formatGb(bytes: number): string {
  return (bytes / GB).toFixed(2);
}
