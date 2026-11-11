import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";

const target = document.getElementById("app");
if (!target) {
  throw new Error("ClipVault frontend: missing #app mount point");
}

let isStartupSplash = false;
let isImagePreviewWindow = false;
try {
  const label = getCurrentWindow().label;
  isStartupSplash = label === "startup-splash";
  isImagePreviewWindow = label === "image-preview";
} catch {
  // Browser previews render the main desktop without a Tauri window.
}

// Keep the splash path light: importing App.svelte up front evaluates the
// complete desktop module graph even when this webview only needs the splash.
// That work can delay the first visible splash frame on slower Linux systems.
void (async () => {
  const [{ default: Root }, { initializeLocalization }] = await Promise.all([
    isStartupSplash
      ? import("./StartupSplash.svelte")
      : isImagePreviewWindow
        ? import("./ImagePreviewWindow.svelte")
        : import("./App.svelte"),
    import("./lib/localization.ts"),
  ]);
  await initializeLocalization({ waitForBackend: !isStartupSplash });
  mount(Root, { target });
})();
