<script lang="ts">
  /**
   * Editable text editor for a single history entry.
   *
   * The modal reuses the shared `Modal.svelte` shell so the focus
   * trap, Escape handling, backdrop dismissal, initial focus and
   * return-focus match the rest of the desktop. The editing surface
   * itself is a native `<textarea>`: the open spec deliberately
   * keeps the implementation plain (no CodeMirror, Monaco, Tiptap,
   * ProseMirror or contenteditable wrappers) so the persisted text
   * round-trips byte-for-byte through the IPC bridge and the
   * browser keeps its native IME / selection / accessibility
   * behaviour.
   *
   * ## Privacy contract
   *
   * The draft the user types stays inside the component instance;
   * nothing in the props, the events or the IPC bridge ever carries
   * clipboard content, content hashes, snippets, source identifiers
   * or asset references. The `clipvault://history-updated` event
   * the Tauri shell emits after a successful commit uses the
   * metadata-only `()` payload the rest of the desktop already
   * subscribes to, so the bridge keeps its privacy invariants.
   *
   * ## State machine
   *
   * - `idle`: the modal is open and the user is editing the draft.
   * - `saving`: a single in-flight `clipvault_update_text_entry`
   *   call is active. Guardar is disabled and Cancelar / Escape /
   *   backdrop are intercepted by the shared shell so the user
   *   cannot start a second submission mid-flight.
   * - The error surface stays scoped to the modal: a failed call
   *   keeps the draft visible and shows a localised message so the
   *   user can retry or correct the input.
   *
   * ## Cancel / Escape / backdrop
   *
   * The shared shell refuses to close on Escape / backdrop while the
   * mutation is in flight (`busy=true`). When the shell closes, the
   * component drops the draft and returns focus to the element the
   * parent supplied through `returnFocusTo` so the card menu trigger
   * recovers focus.
   */
  import { createEventDispatcher, tick } from "svelte";

  import type { EntryRecord } from "./types";
  import { isEditableTextEntry } from "./types";
  import Modal from "./Modal.svelte";
  import { updateTextEntryCommand } from "./lib/tauri";

  export let open: boolean;
  export let entry: EntryRecord;
  /**
   * Resolved capture title the card already computes for the rail
   * (`displayTitle`). The modal reuses the exact same string —
   * including the existing fallback for captures without a custom
   * title — so the dialog heading matches the card title the user
   * just opened. Never use the entry's full content as the dialog
   * title and never fall back to the generic `Editar captura`
   * label.
   */
  export let displayTitle: string;
  /** Element the modal returns focus to when it closes. */
  export let returnFocusTo: HTMLElement | null = null;

  /**
   * The modal must never be the canonical source of truth for the
   * `open` flag: `HistoryCard` owns `textEditorOpen` and the modal
   * only signals a close request through this dispatcher. Routing
   * the close through the parent is what keeps the
   * `true -> false -> true` lifecycle alive for the same entry — a
   * naive `open = false` inside the child only updates the child's
   * local prop, so the next "Editar captura" click would set
   * `textEditorOpen = true` against a value that never went back to
   * `false` and the modal would not reopen.
   */
  const dispatch = createEventDispatcher<{ close: void }>();

  /**
   * Stable id used by the shared `Modal` shell to wire
   * `aria-labelledby` and by the regression suite to assert the
   * title. Including the entry id keeps the label unique even when
   * two modals are present in the same DOM tree.
   */
  $: titleId = `entry-text-editor-title-${entry.id}`;
  $: editorId = `entry-text-editor-${entry.id}`;
  /**
   * The dialog heading is the card's resolved capture title. The
   * string is passed in by `HistoryCard` (the same `displayTitle`
   * the card already computes for the rail, including its existing
   * fallback) so the heading reads as the capture the user picked
   * and the `aria-labelledby` association matches that label. The
   * generic `Editar captura` heading and the explanatory summary
   * that used to live below the editor are both removed from this
   * modal; validation errors keep their `role="alert"` so the
   * screen reader announces them when they appear.
   */
  $: modalTitle = displayTitle;

  /**
   * Local draft isolated from the `entry` prop. Editing never
   * mutates the persisted record; the modal keeps a per-instance
   * string and only persists through the IPC call the user triggers
   * with **Guardar**.
   */
  let draft = "";
  /**
   * Snapshot of the entry the modal mounted with. Used to detect
   * "no-op" submissions (the persisted text equals the persisted
   * payload) and to seed the draft when the modal opens.
   */
  let baselineEntry: EntryRecord | null = null;
  let saving = false;
  let errorMessage: string | null = null;
  let textareaEl: HTMLTextAreaElement | null = null;

  $: isEligible = isEditableTextEntry(entry);

  /**
   * Seed the draft only when the modal transitions from closed to
   * open. The reactive block intentionally does NOT refresh the
   * draft on every entry change because the user may have started
   * editing an existing draft and we must not clobber it.
   */
  let lastOpenedEntryId: number | null = null;
  $: if (open && entry && lastOpenedEntryId !== entry.id) {
    draft = entry.content;
    baselineEntry = entry;
    errorMessage = null;
    saving = false;
    lastOpenedEntryId = entry.id;
    void focusEditor();
  } else if (!open) {
    lastOpenedEntryId = null;
    baselineEntry = null;
  }

  /**
   * Move the initial focus to the textarea so the user can start
   * typing immediately. The shared `Modal` shell already traps the
   * focus and restores it on close, so the helper only takes care
   * of the first paint.
   */
  async function focusEditor(): Promise<void> {
    await tick();
    if (textareaEl) {
      textareaEl.focus();
      // Place the caret at the end of the existing content so
      // appending is the default gesture, matching the behaviour
      // every other text editor in the desktop exposes.
      const length = textareaEl.value.length;
      try {
        textareaEl.setSelectionRange(length, length);
      } catch {
        // Some WebKit revisions throw on programmatic selection
        // changes for detached nodes; ignoring the error keeps the
        // initial focus working without raising the modal.
      }
    }
  }

  $: canSave =
    !saving &&
    isEligible &&
    draft.length > 0 &&
    baselineEntry !== null &&
    draft !== (baselineEntry.content ?? "");

  function describeError(kind: string | null): string {
    switch (kind) {
      case "not_found":
        return "La entrada ya no está disponible.";
      case "not_editable":
        return "Esta entrada no se puede editar.";
      case "empty_content":
        return "El texto no puede estar vacío.";
      case "duplicate_content":
        return "Otra captura ya tiene ese contenido.";
      case "history_error":
        return "No se pudo guardar la edición.";
      default:
        return "No se pudo guardar la edición.";
    }
  }

  /**
   * Persist the draft through the typed IPC bridge. The command is
   * metadata-only: the response and the metadata-only
   * `clipvault://history-updated` event the shell emits after a
   * successful commit never carry the draft bytes.
   */
  async function save(): Promise<void> {
    if (saving || !canSave) return;
    saving = true;
    errorMessage = null;
    try {
      const response = await updateTextEntryCommand({
        id: entry.id,
        content: draft,
      });
      switch (response.kind) {
        case "updated":
        case "noop":
          // The `clipvault://history-updated` event the shell
          // emits after a successful commit triggers the rail
          // refresh; the rail patches `entries` /
          // `visibleEntries` and the card renders the new
          // preview from the refreshed record. Ask the parent
          // to flip `textEditorOpen` through the `close`
          // dispatcher instead of mutating the local `open`
          // prop; mutating the prop alone leaves the parent in
          // `textEditorOpen = true`, which makes the next
          // "Editar captura" click a no-op for the same entry.
          dispatch("close");
          break;
        case "not_found":
          errorMessage = describeError("not_found");
          break;
        case "not_editable":
          errorMessage = describeError("not_editable");
          break;
        case "empty_content":
          errorMessage = describeError("empty_content");
          break;
        case "duplicate_content":
          errorMessage = describeError("duplicate_content");
          break;
      }
    } catch (caught) {
      errorMessage = describeError(null);
      console.error("clipvault_update_text_entry failed", caught);
    } finally {
      saving = false;
    }
  }

  /**
   * Reset the draft to the persisted text the modal opened with.
   * The action is local UI state; it never reaches the database
   * and never emits any IPC event.
   */
  function resetDraft(): void {
    if (!baselineEntry) return;
    draft = baselineEntry.content;
    errorMessage = null;
  }

  /**
   * Close the modal without persisting the draft. The action
   * signals the parent through the `close` dispatcher so
   * `HistoryCard` flips its canonical `textEditorOpen` flag and
   * the shared `Modal` shell restores focus to `returnFocusTo`.
   * The shell intercepts Escape / backdrop / close while a save
   * is in flight so the user cannot race a pending mutation. The
   * dispatcher route is the only way `open` becomes `false` from
   * the child; mutating the local prop alone would leave the
   * parent in `textEditorOpen = true` and break the reopen path
   * for the same entry.
   */
  function cancel(): void {
    dispatch("close");
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      // The shared shell already closes the dialog on Escape,
      // so we only need to keep the caret movement defaults intact
      // for the textarea itself.
      return;
    }
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      // Cmd/Ctrl+Enter matches the platform-aware shortcut the
      // preview modal exposes and gives keyboard-only users a
      // single shortcut for "save and close".
      event.preventDefault();
      void save();
    }
  }
</script>

<Modal
  {open}
  {titleId}
  title={modalTitle}
  busy={saving}
  {returnFocusTo}
  onClose={cancel}
>
  <div
    class="entry-text-editor-modal"
    data-testid="entry-text-editor-modal"
    data-entry-id={entry.id}
  >
    <label class="visually-hidden" id={editorId} for={`${editorId}-textarea`}>
      Contenido editable de la captura
    </label>
    <textarea
      id={`${editorId}-textarea`}
      class="entry-text-editor-textarea"
      data-testid="entry-text-editor-textarea"
      rows="8"
      bind:this={textareaEl}
      bind:value={draft}
      disabled={saving || !isEligible}
      aria-labelledby={editorId}
      spellcheck="false"
      on:keydown={onKeydown}
    ></textarea>
    {#if errorMessage}
      <p
        class="entry-text-editor-error"
        role="alert"
        data-testid="entry-text-editor-error"
      >
        {errorMessage}
      </p>
    {/if}
    <div class="entry-text-editor-actions">
      <button
        type="button"
        class="entry-text-editor-cancel"
        data-testid="entry-text-editor-cancel"
        on:click={cancel}
        disabled={saving}
      >
        Cancelar
      </button>
      <button
        type="button"
        class="entry-text-editor-reset"
        data-testid="entry-text-editor-reset"
        on:click={resetDraft}
        disabled={saving || draft === (baselineEntry?.content ?? "")}
      >
        Restablecer
      </button>
      <button
        type="button"
        class="entry-text-editor-save"
        data-testid="entry-text-editor-save"
        on:click={() => void save()}
        disabled={!canSave}
      >
        {saving ? "Guardando" : "Guardar"}
      </button>
    </div>
  </div>
</Modal>

<style>
  :global(.entry-text-editor-modal) {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    min-width: 320px;
  }
  :global(.entry-text-editor-textarea) {
    width: 100%;
    box-sizing: border-box;
    min-height: 8rem;
    resize: vertical;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: var(--cv-body, 0.9rem);
    line-height: 1.4;
    color: var(--cv-fg, #f0f4f8);
    background: #0e1116;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 12px);
    padding: 0.6rem 0.75rem;
  }
  :global(.entry-text-editor-textarea:focus-visible) {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 1px;
  }
  :global(.entry-text-editor-textarea:disabled) {
    opacity: 0.6;
    cursor: not-allowed;
  }
  :global(.entry-text-editor-error) {
    margin: 0;
    padding: 0.45rem 0.65rem;
    border-radius: var(--cv-radius-sm, 6px);
    background: rgba(248, 113, 113, 0.15);
    border: 1px solid rgba(248, 113, 113, 0.45);
    color: #fecaca;
    font-size: var(--cv-meta, 0.7rem);
    line-height: 1.4;
  }
  :global(.entry-text-editor-actions) {
    display: flex;
    flex-direction: row;
    justify-content: flex-end;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  :global(.entry-text-editor-actions > button) {
    background: #1f2937;
    color: inherit;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-sm, 6px);
    padding: 0.4rem 0.75rem;
    font-size: var(--cv-control, 0.85rem);
    cursor: pointer;
  }
  :global(.entry-text-editor-actions > button:hover:not(:disabled)) {
    background: #2d3748;
  }
  :global(.entry-text-editor-actions > button:disabled) {
    opacity: 0.5;
    cursor: not-allowed;
  }
  :global(.entry-text-editor-actions > .entry-text-editor-save) {
    background: #2563eb;
    color: #f0f4f8;
    border-color: #2563eb;
  }
  :global(.entry-text-editor-actions > .entry-text-editor-save:hover:not(:disabled)) {
    background: #1d4ed8;
  }
</style>