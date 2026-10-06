export function matchesDevelopmentShortcut(
  event: {
    key: string;
    ctrlKey?: boolean;
    altKey?: boolean;
    shiftKey?: boolean;
    metaKey?: boolean;
  },
  macos: boolean,
): boolean {
  if (event.key.toLowerCase() !== "d") return false;
  if (!event.ctrlKey || !event.altKey || !event.shiftKey) return false;
  return macos ? Boolean(event.metaKey) : !event.metaKey;
}
