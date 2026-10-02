<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type { HotkeySpec, KeyboardShortcutsSnapshot } from "./types.ts";
  import {
    applyKeyboardShortcutsSnapshot,
    keyboardShortcutStatuses,
    keyboardShortcuts,
    shortcutLabel,
    shortcutStatusTranslationKey,
    type KeyboardShortcutId,
  } from "./lib/keyboardShortcuts.ts";
  import {
    keyboardShortcutsGetCommand,
    keyboardShortcutSetCommand,
  } from "./lib/tauri.ts";
  import { t } from "./lib/localization.ts";

  export let platformOs: string | null = null;

  const rows: { id: KeyboardShortcutId; title: string; context: string }[] = [
    { id: "open_quick_paste", title: "keyboard_shortcuts.action.open_quick_paste", context: "keyboard_shortcuts.context.global" },
    { id: "toggle_clipboard_capture", title: "keyboard_shortcuts.action.toggle_capture", context: "keyboard_shortcuts.context.global" },
    { id: "focus_main_search", title: "keyboard_shortcuts.action.focus_main_search", context: "keyboard_shortcuts.context.main_window" },
    { id: "focus_quick_search", title: "keyboard_shortcuts.action.focus_quick_search", context: "keyboard_shortcuts.context.quick_paste" },
    { id: "preview_selected", title: "keyboard_shortcuts.action.preview_selected", context: "keyboard_shortcuts.context.selected_item" },
    { id: "edit_selected_text", title: "keyboard_shortcuts.action.edit_selected_text", context: "keyboard_shortcuts.context.selected_text" },
    { id: "open_entry_note", title: "keyboard_shortcuts.action.open_entry_note", context: "keyboard_shortcuts.context.selected_capture" },
    { id: "open_history", title: "keyboard_shortcuts.action.open_history", context: "keyboard_shortcuts.context.main_window" },
    { id: "create_text_capture", title: "keyboard_shortcuts.action.create_text_capture", context: "keyboard_shortcuts.context.main_window" },
    { id: "copy_plain_text", title: "keyboard_shortcuts.action.copy_plain_text", context: "keyboard_shortcuts.context.quick_paste" },
  ];

  let loading = true;
  let saving = false;
  let recordingId: KeyboardShortcutId | null = null;
  let errorKey: string | null = null;
  $: macos = platformOs === "macos";

  function captureKey(event: KeyboardEvent): void {
    if (recordingId === null || saving) return;
    const normalizedKey = event.key === " " ? "space" : event.key.toLowerCase();
    const supported = /^[a-z0-9]$/.test(normalizedKey) ||
      normalizedKey === "enter" || normalizedKey === "escape" || normalizedKey === "space";
    if (!supported) {
      if (["shift", "control", "alt", "meta", "super"].includes(normalizedKey)) return;
      event.preventDefault();
      errorKey = "keyboard_shortcuts.error.unsupported_key";
      return;
    }
    const hasModifier = event.metaKey || event.ctrlKey || event.altKey || event.shiftKey;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (!hasModifier) {
      errorKey = "keyboard_shortcuts.error.modifier_required";
      return;
    }

    const binding: HotkeySpec = {
      id: recordingId,
      key: normalizedKey,
      cmd_or_ctrl: macos ? event.metaKey : event.ctrlKey,
      meta: macos ? event.ctrlKey : event.metaKey,
      alt: event.altKey,
      shift: event.shiftKey,
    };
    void saveBinding(binding);
  }

  async function saveBinding(binding: HotkeySpec): Promise<void> {
    saving = true;
    errorKey = null;
    try {
      const snapshot = await keyboardShortcutSetCommand(binding);
      applyKeyboardShortcutsSnapshot(snapshot);
      recordingId = null;
    } catch (error) {
      const payload = error && typeof error === "object"
        ? error as { kind?: unknown; message?: unknown }
        : null;
      const detail = [payload?.kind, payload?.message, error]
        .filter((value): value is string => typeof value === "string")
        .join(" ")
        .toLowerCase();
      errorKey = detail.includes("conflict")
        ? "keyboard_shortcuts.error.conflict"
        : detail.includes("unsupported")
          ? "keyboard_shortcuts.error.unsupported_platform"
          : "keyboard_shortcuts.error.save";
    } finally {
      saving = false;
    }
  }

  function startRecording(id: KeyboardShortcutId): void {
    recordingId = id;
    errorKey = null;
  }

  onMount(() => {
    document.addEventListener("keydown", captureKey, true);
    void keyboardShortcutsGetCommand()
      .then((snapshot: KeyboardShortcutsSnapshot) => applyKeyboardShortcutsSnapshot(snapshot))
      .catch(() => { errorKey = "keyboard_shortcuts.error.load"; })
      .finally(() => { loading = false; });
  });

  onDestroy(() => document.removeEventListener("keydown", captureKey, true));
</script>

<section class="keyboard-shortcuts" data-testid="keyboard-shortcuts-modal">
  <p class="muted">{$t("keyboard_shortcuts.description")}</p>
  {#if loading}
    <p class="muted" role="status">{$t("settings.loading")}</p>
  {:else}
    <div class="shortcut-list">
      {#each rows as row (row.id)}
        {@const binding = $keyboardShortcuts[row.id]}
        <article class="shortcut-row" data-testid={`keyboard-shortcut-${row.id}`}>
          <div class="shortcut-copy">
            <h3>{$t(row.title)}</h3>
            <p class="muted">{$t(row.context)}</p>
          </div>
          <kbd>{binding ? shortcutLabel(binding, macos) : "—"}</kbd>
          <span class="status" data-status={$keyboardShortcutStatuses[row.id] ?? "unknown"}>
            {$t(shortcutStatusTranslationKey($keyboardShortcutStatuses[row.id]))}
          </span>
          <button
            type="button"
            disabled={saving}
            data-testid={`keyboard-shortcut-change-${row.id}`}
            on:click={() => startRecording(row.id)}
          >{$t(recordingId === row.id ? "keyboard_shortcuts.recording" : "keyboard_shortcuts.change")}</button>
        </article>
      {/each}
    </div>
  {/if}
  {#if recordingId}
    <p class="recording" role="status" data-testid="keyboard-shortcut-recording">
      {$t("keyboard_shortcuts.recording_prompt")}
    </p>
  {/if}
  {#if errorKey}
    <p class="error" role="alert" data-testid="keyboard-shortcuts-error">{$t(errorKey)}</p>
  {/if}
</section>

<style>
  .keyboard-shortcuts { display: flex; flex-direction: column; gap: 0.75rem; }
  .shortcut-list { display: flex; flex-direction: column; gap: 0.5rem; }
  .shortcut-row {
    display: grid;
    grid-template-columns: minmax(10rem, 1fr) auto minmax(6rem, auto) auto;
    align-items: center;
    gap: 0.7rem;
    padding: 0.7rem 0.8rem;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-sm, 6px);
    background: var(--cv-bg-surface, #0e1116);
  }
  h3, p { margin: 0; }
  h3 { font-size: var(--cv-title-sm, 0.9rem); font-weight: 600; }
  .muted, .status { color: var(--cv-fg-muted, #94a3b8); font-size: var(--cv-muted, 0.78rem); }
  kbd { white-space: nowrap; }
  button { white-space: nowrap; }
  .recording { color: var(--cv-accent, #7dd3fc); }
  .error { color: var(--cv-danger, #f87171); }
  @media (max-width: 640px) {
    .shortcut-row { grid-template-columns: 1fr auto; }
    .status { grid-column: 1; }
    .shortcut-row button { grid-column: 2; grid-row: 2; }
  }
</style>
