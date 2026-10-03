import { get, writable } from "svelte/store";
import type {
  HotkeySpec,
  KeyboardShortcutsSnapshot,
} from "../types.ts";
import {
  keyboardShortcutsGetCommand,
} from "./tauri.ts";

export type KeyboardShortcutId =
  | "open_quick_paste"
  | "toggle_clipboard_capture"
  | "focus_main_search"
  | "focus_quick_search"
  | "preview_selected"
  | "edit_selected_text"
  | "open_entry_note"
  | "open_history"
  | "create_text_capture"
  | "copy_plain_text"
  | "open_keyboard_shortcuts";

export const KEYBOARD_SHORTCUTS_CHANGED_EVENT =
  "clipvault://keyboard-shortcuts-changed";

export const keyboardShortcuts = writable<Record<string, HotkeySpec>>({});
export const keyboardShortcutStatuses = writable<KeyboardShortcutsSnapshot["status"]>({});

let initialization: Promise<void> | undefined;
let stopUpdates: (() => void) | undefined;

function update(snapshot: KeyboardShortcutsSnapshot): void {
  keyboardShortcuts.set(
    Object.fromEntries(snapshot.bindings.map((binding) => [binding.id, binding])),
  );
  keyboardShortcutStatuses.set(snapshot.status ?? {});
}

export function currentKeyboardShortcut(
  id: KeyboardShortcutId | string,
): HotkeySpec | null {
  return get(keyboardShortcuts)[id] ?? null;
}

export async function initializeKeyboardShortcuts(): Promise<void> {
  if (initialization) return initialization;
  initialization = (async () => {
    try {
      const { listen } = await import("@tauri-apps/api/event");
      stopUpdates = await listen<KeyboardShortcutsSnapshot>(
        KEYBOARD_SHORTCUTS_CHANGED_EVENT,
        ({ payload }) => update(payload),
      );
    } catch {
      // The browser preview and unit tests have no Tauri event bridge.
    }
    try {
      update(await keyboardShortcutsGetCommand());
    } catch {
      // Existing shortcut helpers keep their platform defaults until the
      // desktop bridge provides the persisted catalog.
    }
  })();
  return initialization;
}

export function applyKeyboardShortcutsSnapshot(
  snapshot: KeyboardShortcutsSnapshot,
): void {
  update(snapshot);
}

export function disposeKeyboardShortcutUpdates(): void {
  stopUpdates?.();
  stopUpdates = undefined;
  initialization = undefined;
}

export function matchesConfiguredShortcut(
  event: {
    key: string;
    metaKey?: boolean;
    ctrlKey?: boolean;
    altKey?: boolean;
    shiftKey?: boolean;
  },
  id: KeyboardShortcutId | string,
  macos: boolean,
): boolean {
  const binding = currentKeyboardShortcut(id);
  if (!binding) return false;

  const key = event.key.toLowerCase();
  const expectedKey = binding.key.toLowerCase();
  if (key !== expectedKey && !(expectedKey === "space" && key === " ")) {
    return false;
  }
  const expectedMeta = macos ? binding.cmd_or_ctrl : binding.meta;
  const expectedControl = macos ? binding.meta : binding.cmd_or_ctrl;
  if (Boolean(event.metaKey) !== expectedMeta || Boolean(event.ctrlKey) !== expectedControl) {
    return false;
  }
  return binding.shift === Boolean(event.shiftKey) &&
    binding.alt === Boolean(event.altKey);
}

export function currentPlatformIsMacos(): boolean {
  return typeof navigator !== "undefined" &&
    /mac|iphone|ipad/i.test(navigator.platform);
}

export function shortcutLabel(
  binding: HotkeySpec,
  macos: boolean,
): string {
  const parts: string[] = [];
  if (binding.cmd_or_ctrl) parts.push(macos ? "⌘" : "Ctrl");
  if (binding.meta) parts.push(macos ? "⌃" : "Super");
  if (binding.alt) parts.push(macos ? "⌥" : "Alt");
  if (binding.shift) parts.push(macos ? "⇧" : "Shift");
  parts.push(displayKey(binding.key));
  return macos ? parts.join("") : parts.join("+");
}

export function shortcutStatusTranslationKey(status: string | undefined): string {
  switch (status) {
    case "registered": return "keyboard_shortcuts.status.registered";
    case "ready": return "keyboard_shortcuts.status.ready";
    case "conflict": return "keyboard_shortcuts.status.conflict";
    case "unsupported": return "keyboard_shortcuts.status.unsupported";
    case "failed": return "keyboard_shortcuts.status.failed";
    default: return "keyboard_shortcuts.status.unknown";
  }
}

export function ariaShortcut(
  binding: HotkeySpec,
  macos: boolean,
): string {
  const parts: string[] = [];
  if (binding.cmd_or_ctrl) parts.push(macos ? "Meta" : "Control");
  if (binding.meta) parts.push(macos ? "Control" : "Meta");
  if (binding.alt) parts.push("Alt");
  if (binding.shift) parts.push("Shift");
  const key = binding.key.toLowerCase();
  parts.push(key === "enter" ? "Enter" : key === "escape" ? "Escape" :
    key === "space" ? "Space" : key.toUpperCase());
  return parts.join("+");
}

export function configuredLabel(
  id: KeyboardShortcutId | string,
  macos: boolean,
): string | null {
  const binding = currentKeyboardShortcut(id);
  return binding ? shortcutLabel(binding, macos) : null;
}

function displayKey(key: string): string {
  switch (key.toLowerCase()) {
    case "enter": return "Enter";
    case "escape": return "Escape";
    case "space": return "Space";
    default: return key.toUpperCase();
  }
}
