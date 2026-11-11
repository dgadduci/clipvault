export const IMAGE_PREVIEW_ZOOM_MIN = 0.25;
export const IMAGE_PREVIEW_ZOOM_MAX = 4;
export const IMAGE_PREVIEW_ZOOM_STEP = 1.25;

export interface ImagePreviewDimensions {
  width: number;
  height: number;
}

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

/**
 * Return layout dimensions for the zoomed image. Scaling both axes by the same
 * factor lets the preview's scroll container measure the full image while
 * preserving its aspect ratio.
 */
export function zoomedImagePreviewSize(
  size: ImagePreviewDimensions | null,
  scale: number,
): ImagePreviewDimensions | null {
  if (
    size === null ||
    !Number.isFinite(size.width) ||
    !Number.isFinite(size.height) ||
    size.width <= 0 ||
    size.height <= 0
  ) {
    return null;
  }

  const boundedScale = scaleImagePreviewZoom(scale, 1);
  return {
    width: size.width * boundedScale,
    height: size.height * boundedScale,
  };
}
