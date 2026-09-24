<script lang="ts">
  /**
   * Sidebar that exposes `Historial` first and the user-defined
   * collections below.
   *
   * The component shares the height token with the card rail so the
   * panel and the rail stay visually aligned. Only the list of
   * collections scrolls vertically when many user collections are
   * defined; the header, the "+ Nueva" icon and the inline creation
   * form remain pinned.
   *
   * User actions (rename, delete, create) flow through events the
   * parent supplies so the side effect lives next to the IPC bridge.
   * The component never inspects clipboard content; it only renders
   * names and ids.
   *
   * The collection delete confirmation flows through a modal backed
   * by the shared `Modal` shell so the destructive branch never
   * grows the sidebar above the documented rail height and the
   * focus trap, Escape handling and return-focus stay consistent
   * with the rest of the desktop.
   */
  import { createEventDispatcher, onDestroy, onMount, tick } from "svelte";
  import type { Collection, PeerSnapshot } from "./types";
  import {
    COLLECTION_DROP_TARGET_VALUE,
    createCollectionDropZoneHandlers,
  } from "./lib/collectionDropZone";
  import { endDragSession, isDropTarget } from "./lib/dragAndDrop";
  import { POINTER_DRAG_END_EVENT } from "./lib/pointerDragAndDrop";
  import Modal from "./Modal.svelte";
  import CollectionColorModal from "./CollectionColorModal.svelte";
  import LinkedPeers from "./LinkedPeers.svelte";

  export let collections: Collection[] = [];
  export let activeCollectionId: number | null = null;
  /**
   * Snapshot the parent polls on every refresh / pairing close so
   * the `Equipos vinculados` scroller below the collection list
   * stays in lockstep with the runtime. The sidebar renders the
   * trusted peers only; the rest of the snapshot stays inside
   * `PeerSharingModal` where pairing / revocation / blocking
   * actually live.
   */
  export let linkedPeers: PeerSnapshot | null = null;
  /**
   * Currently-selected peer id. The sidebar forwards it through
   * `LinkedPeers` so the row reflects the active panel without
   * owning any of the panel state itself.
   */
  export let activePeerId: string | null = null;

  type DispatchEvents = {
    select: { collectionId: number | null };
    create: { name: string };
    rename: { collectionId: number; name: string };
    delete: { collectionId: number };
    "card-drop": { entryId: number; collectionId: number };
    "set-color": { collectionId: number; colorHex: string };
    "select-peer": { peerId: string };
  };
  const dispatch = createEventDispatcher<DispatchEvents>();

  function forwardPeerSelection(
    event: CustomEvent<{ peerId: string }>,
  ): void {
    dispatch("select-peer", { peerId: event.detail.peerId });
  }

  let creating = false;
  let newCollectionName = "";
  let renamingId: number | null = null;
  let renameDraft = "";
  /**
   * Modal state for the collection-delete confirmation. The modal
   * owns the destructive branch so the panel never grows an inline
   * confirmation row above the rail's stable height.
   */
  let pendingDelete: Collection | null = null;
  let pendingDeleteBusy = false;
  let pendingDeleteTrigger: HTMLElement | null = null;
  let lastDeleteError: string | null = null;
  /**
   * Modal state for the colour editor. The sidebar owns the
   * triggering row (the colour square) so the modal can re-focus
   * the square on close and the picker never drifts out of sync.
   */
  let pendingColorCollection: Collection | null = null;
  let lastColorError: string | null = null;
  /**
   * Id of the collection row currently receiving a drag. The visual
   * feedback is purely decorative: the helper clears the highlight
   * on `dragleave`, `drop`, `dragend` and the `pointercancel`
   * fallback so the row can never stay highlighted while a drag
   * was cancelled outside the sidebar.
   */
  let dragOverCollectionId: number | null = null;

  function selectCollection(id: number | null): void {
    if (renamingId !== null) {
      cancelRename();
    }
    if (creating) {
      cancelCreate();
    }
    if (pendingColorCollection !== null) {
      cancelColorEdit();
    }
    dispatch("select", { collectionId: id });
  }

  function startCreate(): void {
    if (renamingId !== null) {
      cancelRename();
    }
    if (pendingDelete !== null) {
      cancelDelete();
    }
    if (pendingColorCollection !== null) {
      cancelColorEdit();
    }
    creating = true;
    newCollectionName = "";
  }

  function cancelCreate(): void {
    creating = false;
    newCollectionName = "";
  }

  function submitCreate(): void {
    const trimmed = newCollectionName.trim();
    if (!trimmed) {
      cancelCreate();
      return;
    }
    dispatch("create", { name: trimmed });
    cancelCreate();
  }

  function startRename(collection: Collection): void {
    if (collection.kind !== "user") {
      return;
    }
    if (creating) {
      cancelCreate();
    }
    if (pendingDelete !== null) {
      cancelDelete();
    }
    if (pendingColorCollection !== null) {
      cancelColorEdit();
    }
    renamingId = collection.id;
    renameDraft = collection.name;
  }

  function cancelRename(): void {
    renamingId = null;
    renameDraft = "";
  }

  function submitRename(): void {
    if (renamingId === null) return;
    const trimmed = renameDraft.trim();
    if (!trimmed) {
      cancelRename();
      return;
    }
    dispatch("rename", { collectionId: renamingId, name: trimmed });
    cancelRename();
  }

  /**
   * Open the delete-collection modal. The trigger element is
   * captured so the modal can restore focus to the icon the user
   * just clicked instead of an arbitrary element when the modal
   * closes.
   */
  function askDelete(
    collection: Collection,
    event: MouseEvent | KeyboardEvent,
  ): void {
    if (collection.kind !== "user") {
      return;
    }
    if (renamingId !== null) {
      cancelRename();
    }
    pendingDelete = collection;
    pendingDeleteBusy = false;
    lastDeleteError = null;
    pendingDeleteTrigger =
      (event.currentTarget as HTMLElement | null) ?? null;
  }

  function cancelDelete(): void {
    pendingDelete = null;
    pendingDeleteBusy = false;
    lastDeleteError = null;
    pendingDeleteTrigger = null;
  }

  async function confirmDelete(): Promise<void> {
    if (pendingDelete === null || pendingDeleteBusy) return;
    const collectionId = pendingDelete.id;
    pendingDeleteBusy = true;
    lastDeleteError = null;
    // Close the modal optimistically; the parent re-renders the
    // collections list through `refreshOrganization` so the
    // confirmation step never lingers when the round-trip
    // succeeds. A failure surfaces through `organizationError`
    // surfaced by the parent.
    dispatch("delete", { collectionId });
    await tick();
    pendingDelete = null;
    pendingDeleteBusy = false;
    pendingDeleteTrigger = null;
  }

  /**
   * Open the colour-editor modal against the supplied collection.
   * The square's parent button is captured so the modal can restore
   * focus when the user closes the picker without saving.
   *
   * The square is the only interactive surface the row offers for
   * this flow; a single click on a collection row must NOT
   * accidentally open the picker (the row's own click selects the
   * collection), so the helper is wired to the dedicated `dblclick`
   * and `keydown` paths the square installs below.
   */
  function askEditColor(
    collection: Collection,
    event: MouseEvent | KeyboardEvent,
  ): void {
    if (renamingId !== null) {
      cancelRename();
    }
    if (pendingDelete !== null) {
      cancelDelete();
    }
    pendingColorCollection = collection;
    lastColorError = null;
    void event;
  }

  function cancelColorEdit(): void {
    pendingColorCollection = null;
    lastColorError = null;
  }

  function commitColorEdit(colorHex: string): void {
    if (pendingColorCollection === null) return;
    const collectionId = pendingColorCollection.id;
    dispatch("set-color", { collectionId, colorHex });
    pendingColorCollection = null;
    lastColorError = null;
  }

  function isActive(id: number | null): boolean {
    return activeCollectionId === id;
  }

  function isHistory(collection: Collection): boolean {
    return collection.kind === "system";
  }

  /**
   * Accessible label that surfaces the remote origin the
   * `peer-import-collection-visibility` change binds to a user
   * collection. The sidebar reads the metadata-only
   * `peer_display_name` the backend join produces and falls back
   * to a generic safe label when the peer is missing. The
   * sidebar never renders the raw `peer_id` so a regression
   * that forwards the binding key cannot leak through this
   * surface.
   */
  function remoteOriginLabel(collection: Collection): string {
    if (!collection.is_peer_bound) {
      return "";
    }
    const peerName = (collection.peer_display_name ?? "").trim();
    if (peerName.length > 0) {
      return `Importadas de ${peerName}`;
    }
    return "Importadas de equipo remoto";
  }

  function hasRemoteOrigin(collection: Collection): boolean {
    return collection.is_peer_bound === true && collection.kind === "user";
  }

  function onCreateKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      submitCreate();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelCreate();
    }
  }

  function onRenameKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      submitRename();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelRename();
    }
    if (event.key === "F2") {
      event.preventDefault();
    }
  }

  /**
   * Square keyboard handling. Enter and Space open the picker;
   * single click must NOT change the active collection, so the
   * pointer has its own branch. Escape lets the user back out
   * without opening the modal.
   */
  function onColorSquareKeydown(
    event: KeyboardEvent,
    collection: Collection,
  ): void {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      event.stopPropagation();
      askEditColor(collection, event);
      return;
    }
    if (event.key === "Escape" && pendingColorCollection !== null) {
      event.preventDefault();
      cancelColorEdit();
    }
  }

  function onCollectionRowKeydown(
    event: KeyboardEvent,
    collection: Collection,
  ): void {
    if (event.key === "F2" && collection.kind === "user") {
      event.preventDefault();
      startRename(collection);
    }
  }

  // ---------------------------------------------------------------
  // Drag and drop wiring.
  //
  // The sidebar installs a SINGLE delegated drop zone on the
  // scrollable `<ul>` (the `data-collections-drop-viewport`
  // element). The delegated handlers resolve the row under the
  // pointer through the live event target and
  // `document.elementFromPoint`, so a regression in WebKit/Tauri
  // that drops the DataTransfer during `dragover` cannot break the
  // flow. The handlers live in `lib/collectionDropZone.ts`; both
  // the sidebar and the integration test import the same factory
  // so the test exercises the production hand-off rather than a
  // hand-rolled copy.
  //
  // The `dragOverCollectionId` reactive variable is the single
  // source of truth for the visual highlight; the `dragleave`,
  // `drop` and global `dragend` listeners clear it. The sidebar
  // never inspects clipboard content: the only field it reads is
  // the integer entry id encoded by the card.
  // ---------------------------------------------------------------

  /**
   * Live reference to the scrollable viewport. Bound through
   * `bind:this` so the delegated handlers always resolve rows
   * against the element the user can actually see. The viewport
   * carries `data-collections-drop-viewport` so the helper can
   * identify it without relying on class names.
   */
  let viewportEl: HTMLElement | null = null;

  function resetDragOver(): void {
    dragOverCollectionId = null;
  }

  function onCardDropDispatch(entryId: number, collectionId: number): void {
    dispatch("card-drop", { entryId, collectionId });
  }

  function pointerDropZone(node: HTMLElement): { destroy(): void } {
    const onPointerDragOver = (event: Event): void => {
      dropZoneHandlers.onPointerDragOver(
        event as CustomEvent,
      );
    };
    const onPointerDrop = (event: Event): void => {
      dropZoneHandlers.onPointerDrop(event as CustomEvent);
    };
    node.addEventListener("clipvault-pointer-drag-over", onPointerDragOver);
    node.addEventListener("clipvault-pointer-drop", onPointerDrop);
    return {
      destroy(): void {
        node.removeEventListener(
          "clipvault-pointer-drag-over",
          onPointerDragOver,
        );
        node.removeEventListener("clipvault-pointer-drop", onPointerDrop);
      },
    };
  }

  $: dropZoneHandlers = createCollectionDropZoneHandlers({
    collections: orderedCollections,
    getViewport: () => viewportEl,
    getDragOverCollectionId: () => dragOverCollectionId,
    setDragOverCollectionId: (id) => {
      dragOverCollectionId = id;
    },
    onCardDrop: onCardDropDispatch,
  });

  function onWindowDragEnd(): void {
    // `dragend` fires on the source (the card) when the browser
    // cancels the drag (Escape, drop outside a target, …). The
    // sidebar installs the global listener so a cancel that happens
    // outside the viewport still clears the highlight AND closes
    // the in-memory drag session so a later foreign drag cannot
    // impersonate the card. The listener is also uninstalled on
    // `onDestroy`.
    resetDragOver();
    endDragSession();
  }

  $: orderedCollections = (() => {
    const system = collections.filter((c) => c.kind === "system");
    const users = [...collections.filter((c) => c.kind === "user")].sort(
      (a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()),
    );
    return [...system, ...users];
  })();

  onMount(() => {
    // The browser fires `dragend` on the source (the card) when the
    // drag is cancelled (Escape, drop outside a target, modal open,
    // etc). The sidebar installs a single document-level listener so
    // the highlight always clears regardless of where the cancel
    // happens. The listener is removed in `onDestroy` so the
    // sidebar never leaks handlers across remounts.
    document.addEventListener("dragend", onWindowDragEnd);
    document.addEventListener(POINTER_DRAG_END_EVENT, resetDragOver);
  });

  onDestroy(() => {
    document.removeEventListener("dragend", onWindowDragEnd);
    document.removeEventListener(POINTER_DRAG_END_EVENT, resetDragOver);
    resetDragOver();
  });
</script>

<aside
  class="sidebar"
  data-testid="organization-sidebar"
  data-cv-card-rail-height="true"
>
  <header class="sidebar-header">
    <h2>Colecciones</h2>
    <button
      type="button"
      class="icon-only new-icon"
      on:click={startCreate}
      data-testid="sidebar-new-collection"
      aria-label="Nueva colección"
      title="Nueva colección (Enter para confirmar)"
    >
      <svg
        aria-hidden="true"
        focusable="false"
        width="16"
        height="16"
        viewBox="0 0 16 16"
      >
        <path
          d="M8 2.25a.75.75 0 0 1 .75.75v4.25H13a.75.75 0 0 1 0 1.5H8.75V13a.75.75 0 0 1-1.5 0V8.75H3a.75.75 0 0 1 0-1.5h4.25V3A.75.75 0 0 1 8 2.25Z"
          fill="currentColor"
        />
      </svg>
    </button>
  </header>
  {#if creating}
    <form
      class="inline-create-form"
      data-testid="sidebar-create-form"
      on:submit={(event) => {
        event.preventDefault();
        submitCreate();
      }}
    >
      <input
        type="text"
        bind:value={newCollectionName}
        on:keydown={onCreateKeydown}
        placeholder="Nombre de la colección"
        maxlength="80"
        aria-label="Nombre de la colección"
        data-testid="sidebar-create-input"
      />
      <button
        type="submit"
        class="icon-only confirm-icon"
        aria-label="Confirmar nueva colección"
        title="Confirmar (Enter)"
        data-testid="sidebar-create-save"
        disabled={newCollectionName.trim().length === 0}
      >
        <svg
          aria-hidden="true"
          focusable="false"
          width="14"
          height="14"
          viewBox="0 0 14 14"
        >
          <path
            d="M3 7.5l2.6 2.6L11 4.5"
            stroke="currentColor"
            stroke-width="1.6"
            fill="none"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
      <button
        type="button"
        class="icon-only cancel-icon"
        on:click={cancelCreate}
        aria-label="Cancelar nueva colección"
        title="Cancelar (Escape)"
        data-testid="sidebar-create-cancel"
      >
        <svg
          aria-hidden="true"
          focusable="false"
          width="14"
          height="14"
          viewBox="0 0 14 14"
        >
          <path
            d="M3 3l8 8M11 3l-8 8"
            stroke="currentColor"
            stroke-width="1.6"
            fill="none"
            stroke-linecap="round"
          />
        </svg>
      </button>
    </form>
  {/if}
  <ul
    class="collection-list"
    role="list"
    data-collections-drop-viewport={true}
    bind:this={viewportEl}
    on:dragenter={(event) => dropZoneHandlers.onDragEnter(event)}
    on:dragover={(event) => dropZoneHandlers.onDragOver(event)}
    on:dragleave={(event) => dropZoneHandlers.onDragLeave(event)}
    on:drop={(event) => dropZoneHandlers.onDrop(event)}
    use:pointerDropZone
  >
    {#each orderedCollections as collection (collection.id)}
      <li
        class="collection-row"
        class:active={isActive(collection.id)}
        class:drop-target={isDropTarget(collection)}
        class:system-collection={collection.kind === "system"}
        class:drag-over={dragOverCollectionId === collection.id}
        data-testid="sidebar-collection-row"
        data-collection-id={collection.id}
        data-collection-kind={collection.kind}
        data-droppable={isDropTarget(collection) ? "true" : "false"}
        data-drop-target={isDropTarget(collection)
          ? COLLECTION_DROP_TARGET_VALUE
          : null}
        on:keydown={(event) => onCollectionRowKeydown(event, collection)}
        role="presentation"
      >
        {#if renamingId === collection.id}
          <form
            class="inline-rename"
            data-testid="sidebar-rename-form"
            on:submit={(event) => {
              event.preventDefault();
              submitRename();
            }}
          >
            <input
              type="text"
              bind:value={renameDraft}
              on:keydown={onRenameKeydown}
              maxlength="80"
              aria-label="Renombrar colección"
              class="rename-input"
              data-testid="sidebar-rename-input"
            />
            <button
              type="submit"
              class="icon-only confirm-icon"
              aria-label="Confirmar renombrado"
              title="Confirmar (Enter)"
              data-testid="sidebar-rename-save"
              disabled={renameDraft.trim().length === 0}
            >
              <svg
                aria-hidden="true"
                focusable="false"
                width="14"
                height="14"
                viewBox="0 0 14 14"
              >
                <path
                  d="M3 7.5l2.6 2.6L11 4.5"
                  stroke="currentColor"
                  stroke-width="1.6"
                  fill="none"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            </button>
            <button
              type="button"
              class="icon-only cancel-icon"
              on:click={cancelRename}
              aria-label="Cancelar renombrado"
              title="Cancelar (Escape)"
              data-testid="sidebar-rename-cancel"
            >
              <svg
                aria-hidden="true"
                focusable="false"
                width="14"
                height="14"
                viewBox="0 0 14 14"
              >
                <path
                  d="M3 3l8 8M11 3l-8 8"
                  stroke="currentColor"
                  stroke-width="1.6"
                  fill="none"
                  stroke-linecap="round"
                />
              </svg>
            </button>
          </form>
        {:else}
          <button
            type="button"
            class="collection-button"
            class:history={isHistory(collection)}
            on:click={() => selectCollection(collection.id)}
            on:dblclick={() => startRename(collection)}
            aria-pressed={isActive(collection.id)}
            aria-label={isHistory(collection)
              ? "Activar colección del sistema"
              : `Activar y renombrar ${collection.name} (doble clic o F2)`}
            title={isHistory(collection)
              ? collection.name
              : `Doble clic o F2 para renombrar ${collection.name}`}
            data-testid="sidebar-collection-button"
            data-collection-id={collection.id}
          >
            <span class="collection-name">{collection.name}</span>
            {#if isHistory(collection)}
              <span
                class="badge system"
                aria-label="Colección de sistema protegida"
              >
                sistema
              </span>
            {:else if hasRemoteOrigin(collection)}
              <span
                class="badge remote"
                data-testid="sidebar-collection-remote-origin"
                data-peer-bound="true"
                aria-label={remoteOriginLabel(collection)}
                title={remoteOriginLabel(collection)}
              >
                importadas
              </span>
            {/if}
          </button>
          <button
            type="button"
            class="color-square"
            aria-label={`Cambiar color de ${collection.name}`}
            title={`Cambiar color de ${collection.name} (doble clic o Enter)`}
            data-testid="sidebar-collection-color"
            data-collection-id={collection.id}
            data-color={collection.color_hex}
            style="background-color: {collection.color_hex};"
            on:dblclick|stopPropagation={(event) =>
              askEditColor(collection, event)}
            on:keydown={(event) => onColorSquareKeydown(event, collection)}
          ></button>
          {#if collection.kind === "user"}
            <button
              type="button"
              class="icon-only danger delete-icon"
              on:click={(event) => askDelete(collection, event)}
              aria-label={`Eliminar ${collection.name}`}
              title={`Eliminar ${collection.name}`}
              data-testid="sidebar-collection-delete"
              data-collection-id={collection.id}
              data-cv-danger="collection-delete"
            >
              <svg
                aria-hidden="true"
                focusable="false"
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M5 8h14" />
                <path d="M10 8V6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v2" />
                <path d="M7 8l.6 11.2a2 2 0 0 0 2 1.8h4.8a2 2 0 0 0 2-1.8L17 8" />
                <path d="M10 11.5v5" />
                <path d="M14 11.5v5" />
              </svg>
            </button>
          {/if}
        {/if}
      </li>
    {/each}
  </ul>
  <!--
    `Equipos vinculados` lives INSIDE the sidebar so the desktop
    grid keeps exactly two columns (sidebar + main panel). The
    scroller is bounded by `min-height: 0; overflow-y: auto` so a
    long list of trusted peers does not steal height from the
    collection list and vice versa. Selecting a peer re-emits the
    `select-peer` event the parent flips through the main panel;
    selecting a collection (or `Historial`) clears the active
    peer at the parent so the rail local vuelve a renderizar.
  -->
  <LinkedPeers
    snapshot={linkedPeers}
    {activePeerId}
    on:select-peer={forwardPeerSelection}
  />
</aside>

<Modal
  open={pendingDelete !== null}
  titleId="sidebar-delete-collection-modal-title"
  title="Eliminar colección"
  busy={pendingDeleteBusy}
  returnFocusTo={pendingDeleteTrigger}
  onClose={cancelDelete}
>
  <div
    class="sidebar-delete-modal"
    data-testid="sidebar-delete-modal"
    data-collection-id={pendingDelete?.id ?? null}
  >
    <p
      class="sidebar-delete-modal-warning"
      data-testid="sidebar-delete-modal-warning"
    >
      ¿Eliminar “{pendingDelete?.name ?? ""}”? Las capturas no se borran.
    </p>
    {#if lastDeleteError}
      <p
        class="sidebar-delete-modal-error"
        role="alert"
        data-testid="sidebar-delete-modal-error"
      >
        {lastDeleteError}
      </p>
    {/if}
    <div class="sidebar-delete-modal-actions">
      <button
        type="button"
        class="modal-action modal-action-secondary"
        on:click={cancelDelete}
        disabled={pendingDeleteBusy}
        data-testid="sidebar-delete-modal-cancel"
      >
        Cancelar
      </button>
      <button
        type="button"
        class="modal-action modal-action-danger"
        on:click={() => void confirmDelete()}
        disabled={pendingDeleteBusy}
        data-testid="sidebar-delete-modal-confirm"
        data-cv-danger="collection-delete-confirm"
      >
        Eliminar
      </button>
    </div>
  </div>
</Modal>

<CollectionColorModal
  open={pendingColorCollection !== null}
  collection={pendingColorCollection}
  on:save={(event) => commitColorEdit(event.detail.colorHex)}
  on:cancel={cancelColorEdit}
/>

{#if lastColorError}
  <p
    class="sidebar-color-error"
    role="alert"
    data-testid="sidebar-color-error"
  >
    {lastColorError}
  </p>
{/if}

<style>
  .sidebar {
    /* The collection panel no longer carries its own background,
     * border, radius or padding: the unified `.layout` workspace
     * panel is the single bounded surface that owns those tokens
     * (see `App.svelte`'s `.layout` rule). Stripping the visual
     * surface here means zones 1 (collections) and zones 2–3
     * (search/actions/rail) read as one panel instead of two.
     *
     * The internal layout stays untouched:
     *
     *   - `display: flex; flex-direction: column; gap` keeps the
     *     header, inline create form, rename form and list stacked
     *     top-to-bottom.
     *   - `height: 100%` + `min-height: 0` makes the panel stretch
     *     to the row the workspace grid owns (`align-items: stretch`)
     *     while still allowing the internal list to shrink below
     *     its intrinsic content so the row height is driven by the
     *     rail, not by the number of collections. `overflow: hidden`
     *     keeps the rounded workspace corners visually consistent
     *     with the unified panel.
     *   - `flex-shrink: 0` prevents a regression where the panel
     *     shrinks below its content inside a future flex row. */
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    width: 100%;
    height: 100%;
    min-height: 0;
    box-sizing: border-box;
    flex-shrink: 0;
    overflow: hidden;
  }
  .sidebar-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    flex: 0 0 auto;
  }
  .sidebar-header h2 {
    margin: 0;
    font-size: 1rem;
  }
  .collection-list {
    /* Rows remain normal DOM descendants of this fixed viewport, so
     * visible scrolled rows are still valid drag-and-drop targets. */
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
  }
  .collection-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 0.3rem;
    border-radius: 6px;
    padding: 0.15rem 0.4rem;
  }
  .collection-row.active {
    background: rgba(37, 99, 235, 0.15);
  }
  /*
   * Drag-over feedback. The CSS rules are scoped to user-kind rows
   * so the system `Historial` collection never wears the
   * `drag-over` ring (the drop handler refuses it anyway, but the
   * visual hint reinforces the contract). The colour matches the
   * active-collection accent so the user sees the same intent.
   */
  .collection-row.drop-target {
    border: 1px dashed transparent;
    /*
     * The CSS transition lives on the row itself (not only on the
     * `:hover` selector) so entering the row smoothly animates the
     * `background`, `border-color` and `box-shadow` state changes
     * that the `drag-over` modifier introduces. Exiting the row
     * (real `dragleave`, `drop`, `dragend`, error or destroy) runs
     * the reverse transition so the highlight never snaps back.
     * The duration and easing are local to the sidebar so the rest
     * of the desktop keeps the existing motion contract.
     */
    transition:
      background-color 0.18s ease-out,
      border-color 0.18s ease-out,
      box-shadow 0.18s ease-out,
      outline-color 0.18s ease-out;
  }
  .collection-row.drop-target.drag-over {
    /* The drag-over highlight intentionally differs from the
     * `.active` selection (which uses a softer `rgba(37, 99, 235,
     * 0.15)` background). The drop target carries a slightly more
     * saturated background, a hard 1px border in the accent colour
     * and a soft glow so the row reads as a separate, actionable
     * surface. The transition properties on `.drop-target` animate
     * every one of those properties so the user sees a soft
     * 150–250 ms colour change rather than a sudden snap. */
    background: rgba(37, 99, 235, 0.28);
    border-color: var(--cv-accent, #2563eb);
    outline: 1px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.6));
    outline-offset: 1px;
    /* The shadow keeps the contrast-safe accent without the row
     * losing its square shape; it fades with the same easing as
     * the background so the transition feels cohesive. */
    box-shadow: 0 0 0 2px rgba(37, 99, 235, 0.28);
  }
  .collection-row.drop-target.system-collection.drag-over {
    /* `Historial` (system kind) is never a valid drop target; if a
     * drop-target row happens to also be flagged `system` we
     * suppress the highlight so the visual feedback never lies. */
    background: transparent;
    border-color: transparent;
    outline: 0;
    box-shadow: none;
  }
  .collection-button {
    background: transparent;
    border: 0;
    color: inherit;
    text-align: left;
    padding: 0.3rem 0.4rem;
    border-radius: 6px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    font: inherit;
    flex: 1 1 auto;
    min-width: 0;
  }
  .collection-button:hover,
  .collection-button:focus-visible {
    background: rgba(255, 255, 255, 0.04);
    outline: 1px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 0;
  }
  .collection-button.history {
    cursor: default;
  }
  .collection-name {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge.system {
    flex: 0 0 auto;
    background: #1f2937;
    color: #93c5fd;
    border-radius: 999px;
    font-size: 0.65rem;
    padding: 0.05rem 0.45rem;
    border: 1px solid #30363d;
    text-transform: lowercase;
  }
  /*
   * Remote-origin marker rendered next to a user collection
   * bound to a `peer_id`. The badge carries the accessible
   * label `Importadas de <peer>` (with a safe fallback when the
   * persisted display name is empty) so screen readers and
   * tooltips describe the binding without exposing the raw
   * `peer_id`, certificate fingerprint or any other secret.
   */
  .badge.remote {
    flex: 0 0 auto;
    background: #0c2c1f;
    color: #6ee7b7;
    border-radius: 999px;
    font-size: 0.65rem;
    padding: 0.05rem 0.45rem;
    border: 1px solid #115e3a;
    text-transform: lowercase;
  }
  .color-square {
    flex: 0 0 auto;
    width: 1.1rem;
    height: 1.1rem;
    border-radius: 4px;
    border: 1px solid var(--cv-border, #30363d);
    cursor: pointer;
    padding: 0;
    background-clip: padding-box;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.25);
    transition:
      outline-color 0.15s ease-out,
      transform 0.15s ease-out;
  }
  .color-square:hover {
    transform: scale(1.08);
  }
  .color-square:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.85));
    outline-offset: 1px;
  }
  .icon-only {
    flex: 0 0 auto;
    background: transparent;
    color: inherit;
    border: 1px solid transparent;
    border-radius: 6px;
    width: 1.65rem;
    height: 1.65rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
  }
  .icon-only:focus-visible {
    border-color: var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline: none;
  }
  .icon-only:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .icon-only:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .new-icon {
    color: #93c5fd;
  }
  .confirm-icon {
    color: #4ade80;
  }
  .cancel-icon {
    color: #94a3b8;
  }
  /*
   * Collection delete icon: rendered in the documented danger colour
   * at rest, matching the trash button and the per-card delete
   * affordance. The accessible name and the `title` tooltip are the
   * textual cues the user gets; the destructive confirmation flow
   * stays in the parent component and runs only after the user
   * accepts the modal the sidebar renders.
   */
  .icon-only.danger.delete-icon {
    color: var(--cv-danger, #b91c1c);
  }
  .icon-only.danger.delete-icon:hover {
    color: var(--cv-danger-hover, #991b1b);
    background: rgba(185, 28, 28, 0.12);
  }
  .icon-only.danger.delete-icon:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 1px;
  }
  .inline-create-form,
  .inline-rename {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    flex: 1 1 auto;
    width: 100%;
  }
  .inline-create-form input,
  .inline-rename .rename-input {
    flex: 1 1 auto;
    min-width: 0;
    background: #0e1116;
    color: inherit;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 0.3rem 0.45rem;
    font: inherit;
    box-sizing: border-box;
  }
  .inline-rename {
    flex-wrap: nowrap;
  }
  /*
   * Delete-collection modal body. The shared `Modal` shell owns the
   * overlay, the focus trap and the Escape handler — this block
   * styles the body content so the destructive action stays
   * consistent with the rest of the desktop (primary cancel,
   * danger-coloured confirm, accessible focus ring, disabled
   * state while the parent round-trip is in flight).
   */
  :global(.sidebar-delete-modal) {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    min-width: 320px;
  }
  :global(.sidebar-delete-modal-warning) {
    margin: 0;
    font-size: var(--cv-body, 0.9rem);
    line-height: 1.4;
    color: var(--cv-fg, #f0f4f8);
    word-break: break-word;
  }
  :global(.sidebar-delete-modal-error) {
    margin: 0;
    padding: 0.5rem 0.65rem;
    border-radius: var(--cv-radius-sm, 6px);
    background: rgba(248, 113, 113, 0.12);
    border: 1px solid rgba(248, 113, 113, 0.45);
    color: #fee2e2;
    font-size: 0.8rem;
  }
  :global(.sidebar-delete-modal-actions) {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  :global(.modal-action) {
    border: 0;
    padding: 0.45rem 0.95rem;
    border-radius: var(--cv-radius-sm, 6px);
    font-size: var(--cv-control, 0.85rem);
    cursor: pointer;
    font: inherit;
    line-height: 1.1;
  }
  :global(.modal-action:disabled) {
    opacity: 0.55;
    cursor: not-allowed;
  }
  :global(.modal-action-secondary) {
    background: transparent;
    color: var(--cv-fg-muted, #94a3b8);
    border: 1px solid var(--cv-border, #30363d);
  }
  :global(.modal-action-secondary:hover:not(:disabled)) {
    background: rgba(255, 255, 255, 0.05);
    color: var(--cv-fg, #f0f4f8);
  }
  :global(.modal-action-danger) {
    background: var(--cv-danger, #b91c1c);
    color: white;
  }
  :global(.modal-action-danger:hover:not(:disabled)) {
    background: var(--cv-danger-hover, #991b1b);
  }
  :global(.modal-action:focus-visible) {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 2px;
  }
  .sidebar-color-error {
    margin: 0;
    padding: 0.45rem 0.55rem;
    border-radius: var(--cv-radius-sm, 6px);
    background: rgba(248, 113, 113, 0.12);
    border: 1px solid rgba(248, 113, 113, 0.45);
    color: #fee2e2;
    font-size: 0.75rem;
    flex: 0 0 auto;
  }
</style>
