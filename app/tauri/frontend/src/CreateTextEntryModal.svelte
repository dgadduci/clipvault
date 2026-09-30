<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import Modal from "./Modal.svelte";
  import { createManualTextCommand } from "./lib/tauri";

  export let open: boolean;
  export let collectionId: number;
  export let collectionName: string;
  export let returnFocusTo: HTMLElement | null = null;

  const dispatch = createEventDispatcher<{ close: void; created: { id: number } }>();
  const titleId = "create-text-entry-title";
  let draft = "";
  let saving = false;
  let errorMessage: string | null = null;
  let textareaEl: HTMLTextAreaElement | null = null;
  let previousOpen = false;

  $: if (open && !previousOpen) {
    draft = "";
    errorMessage = null;
    void focusEditor();
  }
  $: previousOpen = open;

  async function focusEditor(): Promise<void> {
    await tick();
    textareaEl?.focus();
  }

  async function save(): Promise<void> {
    if (saving) return;
    if (draft.length === 0) {
      errorMessage = "Escribe algún texto antes de guardar.";
      return;
    }
    saving = true;
    errorMessage = null;
    try {
      const result = await createManualTextCommand({ collectionId, content: draft });
      if (result.kind === "stored" || result.kind === "duplicate") {
        dispatch("created", { id: result.id });
        dispatch("close");
      } else if (result.kind === "empty_content") {
        errorMessage = "Escribe algún texto antes de guardar.";
      } else if (result.kind === "invalid_collection_target") {
        errorMessage = "No se pueden crear capturas en esta colección.";
      } else {
        errorMessage = "La colección ya no está disponible.";
      }
    } catch {
      errorMessage = "No se pudo guardar la captura. Inténtalo de nuevo.";
    } finally {
      saving = false;
    }
  }

  function cancel(): void {
    if (!saving) dispatch("close");
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
  title={`Nueva captura · ${collectionName}`}
  busy={saving}
  {returnFocusTo}
  onClose={cancel}
>
  <div class="create-text-entry-modal" data-testid="create-text-entry-modal">
    <label class="visually-hidden" for="create-text-entry-textarea">Texto de la nueva captura</label>
    <textarea
      id="create-text-entry-textarea"
      bind:this={textareaEl}
      bind:value={draft}
      rows="10"
      class="create-text-entry-textarea"
      data-testid="create-text-entry-textarea"
      disabled={saving}
      aria-label="Texto de la nueva captura"
      placeholder="Escribe el texto de la captura…"
      on:keydown={onKeydown}
    ></textarea>
    <p class="create-text-entry-hint">⌘/Ctrl + Enter para guardar</p>
    {#if errorMessage}
      <p class="create-text-entry-error" role="alert" data-testid="create-text-entry-error">{errorMessage}</p>
    {/if}
    <div class="create-text-entry-actions">
      <button type="button" on:click={cancel} disabled={saving}>Cancelar</button>
      <button type="button" data-testid="create-text-entry-save" on:click={() => void save()} disabled={saving}>
        {saving ? "Guardando…" : "Guardar captura"}
      </button>
    </div>
  </div>
</Modal>

<style>
  :global(.create-text-entry-modal) { display: flex; flex-direction: column; gap: 0.65rem; min-width: min(36rem, 74vw); }
  :global(.create-text-entry-textarea) {
    box-sizing: border-box; width: 100%; min-height: 12rem; resize: vertical;
    padding: 0.65rem 0.75rem; border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 12px); color: var(--cv-fg, #f0f4f8);
    background: #0e1116; font: inherit; line-height: 1.45;
  }
  :global(.create-text-entry-textarea:focus-visible) { outline: 2px solid var(--cv-focus-ring, rgba(37,99,235,.45)); outline-offset: 1px; }
  .create-text-entry-hint, .create-text-entry-error { margin: 0; color: var(--cv-fg-muted, #94a3b8); font-size: var(--cv-meta, 0.75rem); }
  .create-text-entry-error { color: var(--cv-danger, #f87171); }
  .create-text-entry-actions { display: flex; justify-content: flex-end; gap: 0.5rem; }
  .create-text-entry-actions button { padding: 0.45rem 0.85rem; border: 0; border-radius: var(--cv-radius-sm, 6px); cursor: pointer; color: white; background: var(--cv-accent, #2563eb); }
  .create-text-entry-actions button:first-child { background: #374151; }
  .create-text-entry-actions button:disabled { opacity: 0.6; cursor: wait; }
</style>
