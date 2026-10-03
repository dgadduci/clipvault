/** Menu actions delivered by the native tray to the main webview. */
export const TRAY_MENU_ACTION_EVENT = "clipvault://tray-menu-action";

export type TrayMenuAction = "clear_history" | "open_settings" | "open_about";

export interface TrayMenuActionHandlers {
  clearHistory: () => void;
  openSettings: () => void;
  openAbout: () => void;
}

/** Route an untrusted event payload only when it names a supported UI action. */
export function dispatchTrayMenuAction(
  action: unknown,
  handlers: TrayMenuActionHandlers,
): boolean {
  switch (action) {
    case "clear_history":
      handlers.clearHistory();
      return true;
    case "open_settings":
      handlers.openSettings();
      return true;
    case "open_about":
      handlers.openAbout();
      return true;
    default:
      return false;
  }
}
