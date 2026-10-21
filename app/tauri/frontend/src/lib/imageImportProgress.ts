import type { ImageImportProgressEvent } from "../types.ts";

const BYTE_PROGRESS_WEIGHT = 0.9;
const FILE_PROGRESS_WEIGHT = 0.1;

/**
 * Convert backend byte and file counters to a percentage. File completion
 * accounts for decoding and persistence after the encoded bytes are read.
 * Event-driven progress stays below 100 until the invoke itself resolves.
 */
export function calculateImageImportProgressPercent(
  progress: ImageImportProgressEvent,
): number {
  if (!Number.isFinite(progress.total) || progress.total <= 0) return 0;

  const completedRatio = Math.max(
    0,
    Math.min(1, progress.completed / progress.total),
  );
  const totalBytes = Number.isFinite(progress.total_bytes)
    ? Math.max(0, progress.total_bytes)
    : 0;
  const bytesRead = Number.isFinite(progress.bytes_read)
    ? Math.max(0, progress.bytes_read)
    : 0;

  let ratio: number;
  if (totalBytes === 0) {
    ratio = completedRatio;
  } else {
    const bytesRatio = Math.max(0, Math.min(1, bytesRead / totalBytes));
    ratio =
      bytesRatio * BYTE_PROGRESS_WEIGHT +
      completedRatio * FILE_PROGRESS_WEIGHT;
  }

  return Math.max(0, Math.min(99, Math.floor(ratio * 100)));
}
