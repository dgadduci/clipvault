export const IMAGE_PREVIEW_ZOOM_MIN = 0.25;
export const IMAGE_PREVIEW_ZOOM_MAX = 4;
export const IMAGE_PREVIEW_ZOOM_STEP = 1.25;

/** Apply a relative zoom factor without escaping the supported image scale. */
export function scaleImagePreviewZoom(currentScale: number, factor: number): number {
  const safeCurrentScale = Number.isFinite(currentScale) && currentScale > 0
    ? currentScale
    : 1;
  const boundedCurrentScale = Math.min(
    IMAGE_PREVIEW_ZOOM_MAX,
    Math.max(IMAGE_PREVIEW_ZOOM_MIN, safeCurrentScale),
  );
  if (!Number.isFinite(factor) || factor <= 0) return boundedCurrentScale;

  return Math.min(
    IMAGE_PREVIEW_ZOOM_MAX,
    Math.max(IMAGE_PREVIEW_ZOOM_MIN, boundedCurrentScale * factor),
  );
}
