import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { EntryRecord } from "../types.ts";
import { initialImagePreviewWindowSize } from "./previewImageSizing.ts";

export const IMAGE_PREVIEW_WINDOW_LABEL = "image-preview";

export type ImagePreviewOrigin = "main" | "quick-paste";

export function imagePreviewRoute(
  entryId: number,
  origin: ImagePreviewOrigin,
): string {
  const query = new URLSearchParams({
    entry_id: String(entryId),
    origin,
  });
  return `/preview.html?${query.toString()}`;
}

/** Open one independent native preview window, replacing any previous image. */
export async function openImagePreviewWindow(
  entry: Pick<EntryRecord, "id" | "payload_width" | "payload_height">,
  origin: ImagePreviewOrigin,
  title: string,
): Promise<void> {
  if (
    !Number.isSafeInteger(entry.id) ||
    entry.id <= 0 ||
    !window.__TAURI_INTERNALS__
  ) {
    throw new Error("Image preview window is unavailable");
  }

  const size = initialImagePreviewWindowSize(
    entry.payload_width ?? 0,
    entry.payload_height ?? 0,
    window.screen.availWidth,
    window.screen.availHeight,
  );
  const existing = await WebviewWindow.getByLabel(IMAGE_PREVIEW_WINDOW_LABEL);
  if (existing) await existing.destroy();

  const previewWindow = new WebviewWindow(IMAGE_PREVIEW_WINDOW_LABEL, {
    url: imagePreviewRoute(entry.id, origin),
    title,
    width: size.width,
    height: size.height,
    minWidth: Math.min(360, size.width),
    minHeight: Math.min(280, size.height),
    resizable: true,
    center: true,
    visible: false,
    focus: false,
  });

  await new Promise<void>((resolve, reject) => {
    void previewWindow.once("tauri://created", () => {
      void previewWindow
        .show()
        .then(() => previewWindow.setFocus())
        .then(resolve, reject);
    }).catch(reject);
    void previewWindow.once("tauri://error", () => {
      reject(new Error("Image preview window could not be created"));
    }).catch(reject);
  });
}
