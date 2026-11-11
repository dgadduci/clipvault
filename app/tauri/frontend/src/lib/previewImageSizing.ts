/** Maximum scale applied to an image in the shared preview. */
export const PREVIEW_IMAGE_SCALE = 0.8;
const PREVIEW_WINDOW_HORIZONTAL_CHROME = 56;
const PREVIEW_WINDOW_VERTICAL_CHROME = 160;

export interface PreviewImageSize {
  width: number;
  height: number;
}
/**
 * Choose a native image-preview window size that is independent of its opener
 * while leaving room for the preview header, footer and image padding.
 */
export function initialImagePreviewWindowSize(
  intrinsicWidth: number,
  intrinsicHeight: number,
  availableWidth: number,
  availableHeight: number,
): PreviewImageSize {
  const safeAvailableWidth = Number.isFinite(availableWidth) && availableWidth > 0
    ? availableWidth
    : 1280;
  const safeAvailableHeight = Number.isFinite(availableHeight) && availableHeight > 0
    ? availableHeight
    : 800;
  const maxWidth = Math.max(1, Math.floor(safeAvailableWidth * 0.94));
  const maxHeight = Math.max(1, Math.floor(safeAvailableHeight * 0.92));
  const fallback = {
    width: Math.min(960, maxWidth),
    height: Math.min(720, maxHeight),
  };
  if (
    !Number.isFinite(intrinsicWidth) ||
    !Number.isFinite(intrinsicHeight) ||
    intrinsicWidth <= 0 ||
    intrinsicHeight <= 0
  ) {
    return fallback;
  }

  const imageSize = fitPreviewImage(
    intrinsicWidth,
    intrinsicHeight,
    maxWidth - PREVIEW_WINDOW_HORIZONTAL_CHROME,
    maxHeight - PREVIEW_WINDOW_VERTICAL_CHROME,
  );
  if (!imageSize) return fallback;

  const minWidth = Math.min(420, maxWidth);
  const minHeight = Math.min(320, maxHeight);
  return {
    width: Math.round(
      Math.max(minWidth, Math.min(maxWidth, imageSize.width + PREVIEW_WINDOW_HORIZONTAL_CHROME)),
    ),
    height: Math.round(
      Math.max(minHeight, Math.min(maxHeight, imageSize.height + PREVIEW_WINDOW_VERTICAL_CHROME)),
    ),
  };
}

/**
 * Fit an image inside the preview's available area without changing its
 * aspect ratio. Set `maxScale` to `null` to allow the image to grow beyond its
 * intrinsic dimensions. Returns `null` when dimensions or area are unusable.
 */
export function fitPreviewImage(
  intrinsicWidth: number,
  intrinsicHeight: number,
  availableWidth: number,
  availableHeight: number,
  maxScale: number | null = PREVIEW_IMAGE_SCALE,
): PreviewImageSize | null {
  if (
    !Number.isFinite(intrinsicWidth) ||
    !Number.isFinite(intrinsicHeight) ||
    !Number.isFinite(availableWidth) ||
    !Number.isFinite(availableHeight) ||
    (maxScale !== null && !Number.isFinite(maxScale)) ||
    intrinsicWidth <= 0 ||
    intrinsicHeight <= 0 ||
    availableWidth <= 0 ||
    availableHeight <= 0 ||
    (maxScale !== null && maxScale <= 0)
  ) {
    return null;
  }

  const viewportScale = Math.min(
    availableWidth / intrinsicWidth,
    availableHeight / intrinsicHeight,
  );
  const scale = maxScale === null ? viewportScale : Math.min(maxScale, viewportScale);

  return {
    width: intrinsicWidth * scale,
    height: intrinsicHeight * scale,
  };
}
