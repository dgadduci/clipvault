// Last-mile visibility bridge for the desktop webview.
//
// The Rust shell requests visibility during startup, but on some
// GTK/Wayland combinations that request can arrive before the native
// WebKit surface has been mapped. This bridge runs only after the
// desktop frontend has mounted and asks Tauri to show the *current*
// main window again. It never hides, focuses, resizes or moves a
// window, and it carries no clipboard data.

export const MAIN_WINDOW_LABEL = "main";

export interface MainWindowSurface {
  label: string;
  show(): Promise<void>;
}

/** Present a main-window surface and report whether it was applicable. */
export async function presentMountedMainWindow(
  window: MainWindowSurface,
): Promise<boolean> {
  if (window.label !== MAIN_WINDOW_LABEL) {
    return false;
  }
  await window.show();
  return true;
}

/**
 * Request visibility after Svelte mounts. Dynamic import keeps regular
 * browser/unit-test execution independent from the Tauri bridge.
 */
export function requestMountedMainWindowVisibility(): void {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return;
  }

  void import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) => presentMountedMainWindow(getCurrentWindow()))
    .catch((error: unknown) => {
      console.warn(
        "main window visibility request failed:",
        error instanceof Error ? error.message : String(error),
      );
    });
}
