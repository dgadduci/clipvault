/** Platform-aware display label for the global local-capture toggle. */
export function captureToggleShortcutLabel(
  platformOs: string | null | undefined,
): string {
  return platformOs === "macos" ? "⌘⌥⇧B" : "Ctrl+Alt+Shift+B";
}
