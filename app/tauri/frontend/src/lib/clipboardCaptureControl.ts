import { configuredLabel } from "./keyboardShortcuts.ts";

/** Platform-aware display label for the global local-capture toggle. */
export function captureToggleShortcutLabel(
  platformOs: string | null | undefined,
): string {
  const configured = configuredLabel("toggle_clipboard_capture", platformOs === "macos");
  if (configured) return configured;
  return platformOs === "macos" ? "⌘⌥⇧B" : "Ctrl+Alt+Shift+B";
}
