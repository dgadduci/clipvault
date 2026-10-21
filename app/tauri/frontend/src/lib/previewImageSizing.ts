/** Maximum scale applied to an image in the shared preview. */
export const PREVIEW_IMAGE_SCALE = 0.8;
export const RESIZED_PREVIEW_IMAGE_SCALE = 1;
export const MIN_PREVIEW_CARD_WIDTH = 160;
export const MIN_PREVIEW_CARD_HEIGHT = 120;

export interface PreviewImageSize {
  width: number;
  height: number;
}

/**
 * Fit an image inside the preview's available area without enlarging it past
 * the requested scale or changing its aspect ratio. Returns `null` when the
 * image dimensions or available area are not usable yet.
 */
export function fitPreviewImage(
  intrinsicWidth: number,
  intrinsicHeight: number,
  availableWidth: number,
  availableHeight: number,
  maxScale = PREVIEW_IMAGE_SCALE,
): PreviewImageSize | null {
  if (
    !Number.isFinite(intrinsicWidth) ||
    !Number.isFinite(intrinsicHeight) ||
    !Number.isFinite(availableWidth) ||
    !Number.isFinite(availableHeight) ||
    !Number.isFinite(maxScale) ||
    intrinsicWidth <= 0 ||
    intrinsicHeight <= 0 ||
    availableWidth <= 0 ||
    availableHeight <= 0 ||
    maxScale <= 0
  ) {
    return null;
  }

  const scale = Math.min(
    maxScale,
    availableWidth / intrinsicWidth,
    availableHeight / intrinsicHeight,
  );

  return {
    width: intrinsicWidth * scale,
    height: intrinsicHeight * scale,
  };
}

/** Clamp a requested preview card size to the available overlay bounds. */
export function clampPreviewCardSize(
  width: number,
  height: number,
  availableWidth: number,
  availableHeight: number,
): PreviewImageSize | null {
  if (
    !Number.isFinite(width) ||
    !Number.isFinite(height) ||
    !Number.isFinite(availableWidth) ||
    !Number.isFinite(availableHeight) ||
    width <= 0 ||
    height <= 0 ||
    availableWidth <= 0 ||
    availableHeight <= 0
  ) {
    return null;
  }

  return {
    width: Math.round(
      Math.max(Math.min(MIN_PREVIEW_CARD_WIDTH, availableWidth), Math.min(width, availableWidth)),
    ),
    height: Math.round(
      Math.max(Math.min(MIN_PREVIEW_CARD_HEIGHT, availableHeight), Math.min(height, availableHeight)),
    ),
  };
}
