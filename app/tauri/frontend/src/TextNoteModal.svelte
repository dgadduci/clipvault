<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import { t } from "./lib/localization.ts";
  import Modal from "./Modal.svelte";
  import {
    collectionNoteCommand,
    entryNoteCommand,
    setCollectionNoteCommand,
    setEntryNoteCommand,
  } from "./lib/tauri";

  export let open: boolean;
  export let targetKind: "entry" | "collection";
  export let targetId: number;
  export let targetLabel: string;
  export let returnFocusTo: HTMLElement | null = null;

  const dispatch = createEventDispatcher<{ close: void }>();
  let draft = "";
  let loading = false;
  let saving = false;
  let errorKey: string | null = null;
  let textareaEl: HTMLTextAreaElement | null = null;
  let requestToken = 0;
  let openedTarget = "";

  $: targetKey = `${targetKind}:${targetId}`;
  $: if (open && openedTarget !== targetKey) {
    openedTarget = targetKey;
    void loadNote(targetKey);
  } else if (!open) {
    openedTarget = "";
  }
  $: titleId = `text-note-title-${targetKind}-${targetId}`;
  $: busy = loading || saving;

  async function loadNote(key: string): Promise<void> {
    const token = ++requestToken;
    loading = true;
    saving = false;
    errorKey = null;
    draft = "";
    try {
      const note = targetKind === "entry"
        ? await entryNoteCommand({ entryId: targetId })
        : await collectionNoteCommand({ collectionId: targetId });
      if (token !== requestToken || !open || key !== targetKey) return;
      draft = note?.body ?? "";
      await tick();
      textareaEl?.focus();
    } catch {
      if (token === requestToken) {
        errorKey = "notes.error.load";
      }
    } finally {
      if (token === requestToken) loading = false;
    }
  }

  async function save(): Promise<void> {
    if (busy) return;
    saving = true;
    errorKey = null;
    try {
      if (targetKind === "entry") {
        await setEntryNoteCommand({ entryId: targetId, body: draft });
      } else {
        await setCollectionNoteCommand({ collectionId: targetId, body: draft });
      }
      dispatch("close");
    } catch {
      errorKey = "notes.error.save";
    } finally {
      saving = false;
    }
  }

  function cancel(): void {
    if (!busy) dispatch("close");
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void save();
    }
  }
</script>

<Modal
  {open}
  {titleId}
  title={$t("notes.modal.title", { target: targetLabel })}
  busy={busy}
  {returnFocusTo}
  onClose={cancel}
>
  <div class="text-note-modal" data-testid="text-note-modal" data-target-kind={targetKind} data-target-id={targetId}>
    <label class="visually-hidden" for={`${titleId}-textarea`}>{$t("notes.modal.label", { target: targetLabel })}</label>
    <textarea
      id={`${titleId}-textarea`}
      bind:this={textareaEl}
      bind:value={draft}
      rows="8"
      class="text-note-textarea"
      data-testid="text-note-textarea"
      disabled={busy}
      aria-label={$t("notes.modal.label", { target: targetLabel })}
      placeholder={$t("notes.modal.placeholder")}
      on:keydown={onKeydown}
    ></textarea>
    {#if loading}
      <p class="text-note-status" data-testid="text-note-loading">{$t("notes.loading")}</p>
    {/if}
    {#if errorKey}
      <p class="text-note-error" role="alert" data-testid="text-note-error">{$t(errorKey)}</p>
    {/if}
    <div class="text-note-actions">
      <button type="button" class="text-note-cancel" on:click={cancel} disabled={busy}>{$t("common.cancel")}</button>
      <button type="button" class="text-note-save" data-testid="text-note-save" on:click={() => void save()} disabled={busy}>
        {saving ? $t("settings.saving") : $t("common.save")}
      </button>
    </div>
  </div>
</Modal>

<style>
  :global(.text-note-modal) { display: flex; flex-direction: column; gap: 0.8rem; min-width: min(34rem, 72vw); }
  :global(.text-note-textarea) {
    box-sizing: border-box; width: 100%; min-height: 10rem; resize: vertical;
    padding: 0.65rem 0.75rem; border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 12px); color: var(--cv-fg, #f0f4f8);
    background: #0e1116; font: inherit; line-height: 1.45;
  }
  :global(.text-note-textarea:focus-visible) { outline: 2px solid var(--cv-focus-ring, rgba(37,99,235,.45)); outline-offset: 1px; }
  .text-note-actions { display: flex; justify-content: flex-end; gap: 0.5rem; }
  .text-note-cancel, .text-note-save { padding: 0.45rem 0.85rem; border: 0; border-radius: var(--cv-radius-sm, 6px); cursor: pointer; }
  .text-note-cancel { color: var(--cv-fg, #f0f4f8); background: #374151; }
  .text-note-save { color: white; background: var(--cv-accent, #2563eb); }
  .text-note-error { margin: 0; color: var(--cv-danger, #f87171); }
  .text-note-status { margin: 0; color: var(--cv-fg-muted, #94a3b8); }
  .text-note-actions button:disabled { opacity: 0.6; cursor: wait; }
</style>
