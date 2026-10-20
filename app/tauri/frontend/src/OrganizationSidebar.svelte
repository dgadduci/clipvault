<script lang="ts">
  /**
   * Sidebar that exposes `Historial` first and user collections as
   * one-level parent/child rows below.
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
   * The parent owns collection deletion and its scoped confirmation
   * so the operation stays next to the history-management bridge.
   */
  import { createEventDispatcher, onDestroy, onMount, tick } from "svelte";
  import type { Collection, PeerSnapshot } from "./types";
  import { anchorPopupToViewport } from "./lib/anchorPopupToViewport";
  import { collectionColor, collectionColorTint } from "./lib/collectionColor";
  import { collectionChildren, visibleCollectionRows } from "./lib/collectionTree";
  import {
    COLLECTION_DROP_TARGET_VALUE,
    createCollectionDropZoneHandlers,
  } from "./lib/collectionDropZone";
  import { endDragSession, isDropTarget } from "./lib/dragAndDrop";
  import { POINTER_DRAG_END_EVENT } from "./lib/pointerDragAndDrop";
  import CollectionColorModal from "./CollectionColorModal.svelte";
  import TextNoteModal from "./TextNoteModal.svelte";
  import LinkedPeers from "./LinkedPeers.svelte";
  import { t, tPlural } from "./lib/localization.ts";

  export let collections: Collection[] = [];
  export let activeCollectionId: number | null = null;
  /** Principal collection ids whose shared child captures are hidden. */
  export let hiddenSubcollectionCaptureCollectionIds: Set<number> = new Set();
  export let historyShortcutAccessible: string = "";
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
    "create-subcollection": { parentId: number; name: string };
    rename: { collectionId: number; name: string };
    delete: { collectionId: number };
    "card-drop": {
      entryId: number;
      collectionId: number;
      entryIds?: number[];
    };
    "peer-bound-drop": Record<string, never>;
    "toggle-subcollection-captures": {
      collectionId: number;
      show: boolean;
    };
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
  let creatingParentId: number | null = null;
  let expandedCollectionIds = new Set<number>();
  let newCollectionName = "";
  let renamingId: number | null = null;
  let renameDraft = "";
  /** Modal state for collection actions and their return-focus targets. */
  let pendingColorCollection: Collection | null = null;
  let pendingColorReturnFocusTarget: HTMLElement | null = null;
  let pendingNoteCollection: Collection | null = null;
  let noteReturnFocusTarget: HTMLElement | null = null;
  let lastColorError: string | null = null;
  let actionsMenuCollectionId: number | null = null;
  let actionsMenuTriggerEl: HTMLButtonElement | null = null;
  let actionsMenuEl: HTMLDivElement | null = null;
  /**
   * Id of the collection row currently receiving a drag. The visual
   * feedback is purely decorative: the helper clears the highlight
   * on `dragleave`, `drop`, `dragend` and the `pointercancel`
   * fallback so the row can never stay highlighted while a drag
   * was cancelled outside the sidebar.
   */
  let dragOverCollectionId: number | null = null;

  function selectCollection(id: number | null): void {
    closeActionsMenu();
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
    closeActionsMenu();
    if (renamingId !== null) {
      cancelRename();
    }
    if (pendingColorCollection !== null) {
      cancelColorEdit();
    }
    creating = true;
    creatingParentId = null;
    newCollectionName = "";
  }

  function startCreateSubcollection(collection: Collection): void {
    if (
      collection.kind !== "user" ||
      collection.parent_id != null ||
      collection.is_peer_bound
    ) return;
    closeActionsMenu();
    if (renamingId !== null) cancelRename();
    if (pendingColorCollection !== null) cancelColorEdit();
    expandedCollectionIds = new Set(expandedCollectionIds).add(collection.id);
    creating = true;
    creatingParentId = collection.id;
    newCollectionName = "";
  }

  function cancelCreate(): void {
    creating = false;
    creatingParentId = null;
    newCollectionName = "";
  }

  function submitCreate(): void {
    const trimmed = newCollectionName.trim();
    if (!trimmed) {
      cancelCreate();
      return;
    }
    if (creatingParentId === null) {
      dispatch("create", { name: trimmed });
    } else {
      dispatch("create-subcollection", {
        parentId: creatingParentId,
        name: trimmed,
      });
    }
    cancelCreate();
  }

  function toggleExpanded(collectionId: number): void {
    const next = new Set(expandedCollectionIds);
    if (next.has(collectionId)) {
      next.delete(collectionId);
      const active = collections.find(
        (collection) => collection.id === activeCollectionId,
      );
      if (active?.parent_id === collectionId) {
        // Keep the active collection visible after its parent collapses.
        selectCollection(collectionId);
      } else if (creatingParentId === collectionId) {
        cancelCreate();
      }
    } else {
      next.add(collectionId);
    }
    expandedCollectionIds = next;
  }

  function startRename(collection: Collection): void {
    if (collection.kind !== "user") {
      return;
    }
    closeActionsMenu();
    if (creating) {
      cancelCreate();
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
   * Ask the parent to open the collection deletion flow. The
   * parent owns the preview and confirmation because it calls the
   * history-management command surface.
   */
  function askDelete(
    collection: Collection,
    _event: MouseEvent | KeyboardEvent,
  ): void {
    if (collection.kind !== "user") {
      return;
    }
    closeActionsMenu();
    if (renamingId !== null) {
      cancelRename();
    }
    dispatch("delete", { collectionId: collection.id });
  }

  function askEditColor(collection: Collection): void {
    if (renamingId !== null) {
      cancelRename();
    }
    pendingColorCollection = collection;
    pendingColorReturnFocusTarget = actionsMenuTriggerEl;
    lastColorError = null;
  }

  function cancelColorEdit(): void {
    pendingColorCollection = null;
    const returnFocusTarget = pendingColorReturnFocusTarget;
    void tick().then(() => {
      if (
        pendingColorCollection === null &&
        pendingColorReturnFocusTarget === returnFocusTarget
      ) {
        pendingColorReturnFocusTarget = null;
      }
    });
    lastColorError = null;
  }

  function commitColorEdit(colorHex: string): void {
    if (pendingColorCollection === null) return;
    const collectionId = pendingColorCollection.id;
    dispatch("set-color", { collectionId, colorHex });
    pendingColorCollection = null;
    const returnFocusTarget = pendingColorReturnFocusTarget;
    void tick().then(() => {
      if (
        pendingColorCollection === null &&
        pendingColorReturnFocusTarget === returnFocusTarget
      ) {
        pendingColorReturnFocusTarget = null;
      }
    });
    lastColorError = null;
  }

  function openCollectionNote(collection: Collection, event: MouseEvent): void {
    pendingNoteCollection = collection;
    noteReturnFocusTarget = event.currentTarget as HTMLButtonElement;
  }

  function toggleSubcollectionCaptures(
    collection: Collection,
    event: MouseEvent,
  ): void {
    event.stopPropagation();
    dispatch("toggle-subcollection-captures", {
      collectionId: collection.id,
      show: hiddenSubcollectionCaptureCollectionIds.has(collection.id),
    });
  }

  function closeCollectionNote(): void {
    pendingNoteCollection = null;
  }

  function closeActionsMenu(restoreFocus: boolean = false): void {
    if (actionsMenuCollectionId === null) return;
    const trigger = actionsMenuTriggerEl;
    actionsMenuCollectionId = null;
    actionsMenuEl = null;
    if (restoreFocus) {
      void tick().then(() => trigger?.focus());
    }
  }

  function showActionsMenu(
    collection: Collection,
    trigger: HTMLButtonElement,
    focusLast: boolean = false,
  ): void {
    if (actionsMenuCollectionId !== null) {
      closeActionsMenu();
    }
    actionsMenuTriggerEl = trigger;
    actionsMenuCollectionId = collection.id;
    void tick().then(() => {
      const items = Array.from(
        actionsMenuEl?.querySelectorAll<HTMLButtonElement>(
          '[role="menuitem"]:not(:disabled)',
        ) ?? [],
      );
      const target = focusLast ? items[items.length - 1] : items[0];
      target?.focus();
    });
  }

  function toggleActionsMenu(
    collection: Collection,
    event: MouseEvent,
  ): void {
    event.stopPropagation();
    const trigger = event.currentTarget as HTMLButtonElement;
    if (actionsMenuCollectionId === collection.id) {
      closeActionsMenu(true);
      return;
    }
    showActionsMenu(collection, trigger);
  }

  function onActionsMenuTriggerKeydown(
    collection: Collection,
    event: KeyboardEvent,
  ): void {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    showActionsMenu(
      collection,
      event.currentTarget as HTMLButtonElement,
      event.key === "ArrowUp",
    );
  }

  function onActionsMenuKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      closeActionsMenu(true);
      return;
    }
    if (
      event.key !== "ArrowDown" &&
      event.key !== "ArrowUp" &&
      event.key !== "Home" &&
      event.key !== "End"
    ) {
      return;
    }
    event.preventDefault();
    const items = Array.from(
      actionsMenuEl?.querySelectorAll<HTMLButtonElement>(
        '[role="menuitem"]:not(:disabled)',
      ) ?? [],
    );
    if (items.length === 0) return;
    if (event.key === "Home") {
      items[0].focus();
      return;
    }
    if (event.key === "End") {
      items[items.length - 1].focus();
      return;
    }
    const currentIndex = items.indexOf(
      document.activeElement as HTMLButtonElement,
    );
    const direction = event.key === "ArrowDown" ? 1 : -1;
    const nextIndex = (currentIndex + direction + items.length) % items.length;
    items[nextIndex].focus();
  }

  function onWindowPointerDown(event: PointerEvent): void {
    if (actionsMenuCollectionId === null) return;
    const target = event.target;
    if (!(target instanceof Node)) return;
    if (actionsMenuTriggerEl?.contains(target) || actionsMenuEl?.contains(target)) {
      return;
    }
    closeActionsMenu();
  }

  function onWindowFocusIn(event: FocusEvent): void {
    if (actionsMenuCollectionId === null) return;
    const target = event.target;
    if (!(target instanceof Node)) return;
    if (actionsMenuTriggerEl?.contains(target) || actionsMenuEl?.contains(target)) {
      return;
    }
    closeActionsMenu();
  }

  function onWindowKeydown(event: KeyboardEvent): void {
    if (actionsMenuCollectionId === null || event.key !== "Escape") return;
    event.preventDefault();
    closeActionsMenu(true);
  }

  function selectActionsMenuItem(
    action: (collection: Collection) => void,
    collection: Collection,
  ): void {
    closeActionsMenu();
    action(collection);
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
  function remoteOriginLabel(
    collection: Collection,
    translateText: (key: string, params?: Record<string, string | number | Date>) => string,
  ): string {
    if (!collection.is_peer_bound) {
      return "";
    }
    const peerName = (collection.peer_display_name ?? "").trim();
    if (peerName.length > 0) {
      return translateText("collections.imported_from", { peer: peerName });
    }
    return translateText("collections.imported_remote");
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

  function onCollectionRowKeydown(
    event: KeyboardEvent,
    collection: Collection,
  ): void {
    if (event.key === "F2" && collection.kind === "user") {
      event.preventDefault();
      startRename(collection);
    }
  }

  function onCollectionRowClick(
    event: MouseEvent,
    collection: Collection,
  ): void {
    const target = event.target;
    if (
      !(target instanceof Element) ||
      target.closest("button, [role='menu']") ||
      !target.closest(".badge")
    ) {
      return;
    }
    selectCollection(collection.id);
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

  function onCardDropDispatch(
    entryId: number,
    collectionId: number,
    entryIds?: number[],
  ): void {
    if (!entryIds || entryIds.length <= 1) {
      dispatch("card-drop", { entryId, collectionId });
      return;
    }
    dispatch("card-drop", {
      entryId,
      collectionId,
      entryIds,
    });
  }

  function onPeerBoundDropDispatch(): void {
    dispatch("peer-bound-drop", {});
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
    collections,
    getViewport: () => viewportEl,
    getDragOverCollectionId: () => dragOverCollectionId,
    setDragOverCollectionId: (id) => {
      dragOverCollectionId = id;
    },
    onCardDrop: onCardDropDispatch,
    onPeerBoundDrop: onPeerBoundDropDispatch,
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

  $: visibleRows = visibleCollectionRows(collections, expandedCollectionIds);

  onMount(() => {
    // The browser fires `dragend` on the source (the card) when the
    // drag is cancelled (Escape, drop outside a target, modal open,
    // etc). The sidebar installs a single document-level listener so
    // the highlight always clears regardless of where the cancel
    // happens. The listener is removed in `onDestroy` so the
    // sidebar never leaks handlers across remounts.
    document.addEventListener("dragend", onWindowDragEnd);
    document.addEventListener(POINTER_DRAG_END_EVENT, resetDragOver);
    document.addEventListener("pointerdown", onWindowPointerDown);
    document.addEventListener("focusin", onWindowFocusIn);
    document.addEventListener("keydown", onWindowKeydown);
  });

  onDestroy(() => {
    document.removeEventListener("dragend", onWindowDragEnd);
    document.removeEventListener(POINTER_DRAG_END_EVENT, resetDragOver);
    document.removeEventListener("pointerdown", onWindowPointerDown);
    document.removeEventListener("focusin", onWindowFocusIn);
    document.removeEventListener("keydown", onWindowKeydown);
    closeActionsMenu();
    resetDragOver();
  });
</script>

<aside
  class="sidebar"
  data-testid="organization-sidebar"
  data-cv-card-rail-height="true"
>
  <header class="sidebar-header">
    <h2>{$t("collections.title")}</h2>
    <button
      type="button"
      class="icon-only new-icon"
      on:click={startCreate}
      data-testid="sidebar-new-collection"
      aria-label={$t("collections.create")}
      title={$t("collections.create_hint")}
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
        placeholder={creatingParentId === null
          ? $t("collections.name")
          : $t("collections.create_subcollection_for", {
              name: collections.find((item) => item.id === creatingParentId)?.name ?? "",
            })}
        maxlength="80"
        aria-label={creatingParentId === null
          ? $t("collections.name")
          : $t("collections.create_subcollection_for", {
              name: collections.find((item) => item.id === creatingParentId)?.name ?? "",
            })}
        data-testid="sidebar-create-input"
      />
      <button
        type="submit"
        class="icon-only confirm-icon"
        aria-label={$t("collections.confirm_create")}
        title={$t("common.confirm_enter")}
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
        aria-label={$t("collections.cancel_create")}
        title={$t("common.cancel_escape")}
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
    {#each visibleRows as row (row.collection.id)}
      {@const collection = row.collection}
      <li
        class="collection-row"
        class:subcollection-row={collection.parent_id != null}
        style={`--collection-row-tint: ${collectionColorTint(
          collection.color_hex,
        )};`}
        class:active={isActive(collection.id)}
        class:drop-target={isDropTarget(collection)}
        class:system-collection={collection.kind === "system"}
        class:drag-over={dragOverCollectionId === collection.id}
        data-testid="sidebar-collection-row"
        data-collection-id={collection.id}
        data-collection-kind={collection.kind}
        data-collection-parent-id={collection.parent_id ?? null}
        data-tree-level={row.level}
        aria-level={row.level}
        data-droppable={isDropTarget(collection) ? "true" : "false"}
        data-drop-target={isDropTarget(collection)
          ? COLLECTION_DROP_TARGET_VALUE
          : null}
        data-collection-drop-row={collection.kind === "user" ? "true" : null}
        on:keydown={(event) => onCollectionRowKeydown(event, collection)}
        on:click={(event) => onCollectionRowClick(event, collection)}
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
              aria-label={$t("collections.rename")}
              class="rename-input"
              data-testid="sidebar-rename-input"
            />
            <button
              type="submit"
              class="icon-only confirm-icon"
              aria-label={$t("collections.confirm_rename")}
              title={$t("common.confirm_enter")}
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
              aria-label={$t("collections.cancel_rename")}
              title={$t("common.cancel_escape")}
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
          {@const collectionDisplayName = isHistory(collection)
            ? $t("collections.history")
            : collection.name}
          {#if collection.parent_id == null && collection.kind === "user" && collectionChildren(collections, collection.id).length > 0}
            <button
              type="button"
              class="icon-only collection-disclosure"
              aria-expanded={expandedCollectionIds.has(collection.id)}
              aria-label={$t(expandedCollectionIds.has(collection.id)
                ? "collections.collapse"
                : "collections.expand", { name: collectionDisplayName })}
              title={$t(expandedCollectionIds.has(collection.id)
                ? "collections.collapse"
                : "collections.expand", { name: collectionDisplayName })}
              data-testid="sidebar-collection-disclosure"
              data-collection-id={collection.id}
              on:click={(event) => {
                event.stopPropagation();
                toggleExpanded(collection.id);
              }}
            >
              <svg aria-hidden="true" focusable="false" width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
                {#if expandedCollectionIds.has(collection.id)}
                  <path d="M3.5 6l4.5 4.5L12.5 6z" />
                {:else}
                  <path d="M6 3.5 10.5 8 6 12.5z" />
                {/if}
              </svg>
            </button>
          {/if}
          <button
            type="button"
            class="collection-button"
            class:history={isHistory(collection)}
            on:click={() => selectCollection(collection.id)}
            on:dblclick={() => startRename(collection)}
            aria-pressed={isActive(collection.id)}
            aria-keyshortcuts={isHistory(collection) ? historyShortcutAccessible || undefined : undefined}
            aria-label={isHistory(collection)
              ? `${$t("collections.select_system")}, ${$tPlural("collections.capture_count", collection.capture_count ?? 0)}`
              : `${$t("collections.rename_hint", { name: collection.name })}, ${$tPlural("collections.capture_count", collection.capture_count ?? 0)}`}
            title={isHistory(collection)
              ? $t("collections.history")
              : $t("collections.rename_hint", { name: collection.name })}
            data-testid="sidebar-collection-button"
            data-collection-id={collection.id}
          >
            <span class="collection-name">{collectionDisplayName}</span>
          </button>
          {#if collection.parent_id == null && collection.kind === "user" && collectionChildren(collections, collection.id).length > 0}
            {@const showSubcollectionCaptures = !hiddenSubcollectionCaptureCollectionIds.has(collection.id)}
            <button
              type="button"
              class="icon-only subcollection-visibility-icon"
              aria-pressed={showSubcollectionCaptures}
              aria-label={$t(showSubcollectionCaptures
                ? "collections.hide_subcollection_captures_for"
                : "collections.show_subcollection_captures_for", {
                  name: collectionDisplayName,
                })}
              title={$t(showSubcollectionCaptures
                ? "collections.hide_subcollection_captures_for"
                : "collections.show_subcollection_captures_for", {
                  name: collectionDisplayName,
                })}
              data-testid="sidebar-collection-subcollection-visibility"
              data-collection-id={collection.id}
              data-show-subcollection-captures={showSubcollectionCaptures ? "true" : "false"}
              on:click={(event) => toggleSubcollectionCaptures(collection, event)}
            >
              <svg aria-hidden="true" focusable="false" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                <path d="M2.5 12s3.4-6.25 9.5-6.25S21.5 12 21.5 12 18.1 18.25 12 18.25 2.5 12 2.5 12Z" />
                <circle cx="12" cy="12" r="2.75" />
                {#if !showSubcollectionCaptures}
                  <path d="m4 4 16 16" />
                {/if}
              </svg>
            </button>
          {/if}
          {#if isHistory(collection)}
            <span
              class="badge system"
              aria-label={$t("collections.system_protected")}
            >
              {$t("collections.system_badge")}
            </span>
          {:else if hasRemoteOrigin(collection)}
            <span
              class="badge remote"
              data-testid="sidebar-collection-remote-origin"
              data-peer-bound="true"
              aria-label={remoteOriginLabel(collection, $t)}
              title={remoteOriginLabel(collection, $t)}
            >
              {$t("collections.imported_badge")}
            </span>
          {/if}
          <span
            class="badge capture-count"
            aria-hidden="true"
            data-testid="sidebar-collection-count"
            data-count={collection.capture_count ?? 0}
          >
            {collection.capture_count ?? 0}
          </span>
          <button
            type="button"
            class="icon-only note-icon"
            aria-haspopup="dialog"
            aria-expanded={pendingNoteCollection?.id === collection.id}
            aria-label={collection.has_note
              ? $t("collections.edit_note_for", { name: collectionDisplayName })
              : $t("collections.add_note_for", { name: collectionDisplayName })}
            title={collection.has_note ? $t("collections.edit_note") : $t("collections.add_note")}
            data-testid="sidebar-collection-note"
            data-collection-id={collection.id}
            data-has-note={collection.has_note ? "true" : "false"}
            on:click={(event) => openCollectionNote(collection, event)}
          >
            <svg aria-hidden="true" focusable="false" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
              <path d="M7 3.75h7l4 4v11.5A1.75 1.75 0 0 1 16.25 21h-9.5A1.75 1.75 0 0 1 5 19.25v-13.5A2 2 0 0 1 7 3.75Z" />
              <path d="M14 4v4h4" />
              {#if collection.has_note}
                <path d="M8 12h8M8 16h6" />
              {:else}
                <path d="M11 12v5M8.5 14.5h5" />
              {/if}
            </svg>
          </button>
          <button
            type="button"
            class="icon-only actions-menu-trigger"
            aria-haspopup="menu"
            aria-expanded={actionsMenuCollectionId === collection.id}
            aria-controls={`collection-actions-menu-${collection.id}`}
            aria-label={$t("collections.actions_menu", {
              name: collectionDisplayName,
            })}
            title={$t("collections.actions_menu", {
              name: collectionDisplayName,
            })}
            data-testid="sidebar-collection-menu-trigger"
            data-collection-id={collection.id}
            on:click={(event) => toggleActionsMenu(collection, event)}
            on:keydown={(event) => onActionsMenuTriggerKeydown(collection, event)}
          >
            <svg
              aria-hidden="true"
              focusable="false"
              width="18"
              height="18"
              viewBox="0 0 18 18"
              fill="currentColor"
            >
              <circle cx="4" cy="9" r="1.6" />
              <circle cx="9" cy="9" r="1.6" />
              <circle cx="14" cy="9" r="1.6" />
            </svg>
          </button>
          {#if actionsMenuCollectionId === collection.id}
            <div
              id={`collection-actions-menu-${collection.id}`}
              class="collection-actions-menu"
              role="menu"
              tabindex="-1"
              aria-label={$t("collections.actions_menu", {
                name: collectionDisplayName,
              })}
              data-testid="sidebar-collection-actions-menu"
              data-collection-id={collection.id}
              bind:this={actionsMenuEl}
              use:anchorPopupToViewport={{
                anchor: () => actionsMenuTriggerEl,
                align: "end",
              }}
              on:keydown={onActionsMenuKeydown}
            >
              {#if collection.parent_id == null && collection.kind === "user" && !hasRemoteOrigin(collection)}
                <button
                  type="button"
                  role="menuitem"
                  class="collection-action-menu-item"
                  aria-label={$t("collections.create_subcollection_for", { name: collectionDisplayName })}
                  title={$t("collections.create_subcollection_for", { name: collectionDisplayName })}
                  data-testid="sidebar-collection-create-subcollection"
                  data-collection-id={collection.id}
                  on:click={() => selectActionsMenuItem(startCreateSubcollection, collection)}
                >
                  <svg aria-hidden="true" focusable="false" width="18" height="18" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M7.25 2.5a.75.75 0 0 1 1.5 0v4.75h4.75a.75.75 0 0 1 0 1.5H8.75v4.75a.75.75 0 0 1-1.5 0V8.75H2.5a.75.75 0 0 1 0-1.5h4.75z" />
                  </svg>
                  <span>{$t("collections.create_subcollection")}</span>
                </button>
              {/if}
              <button
                type="button"
                role="menuitem"
                class="collection-action-menu-item"
                aria-haspopup="dialog"
                aria-label={$t("collections.change_color", { name: collectionDisplayName })}
                title={$t("collections.change_color", { name: collectionDisplayName })}
                data-testid="sidebar-collection-color"
                data-collection-id={collection.id}
                data-color={collection.color_hex}
                on:click={() => selectActionsMenuItem(askEditColor, collection)}
              >
                <span
                  class="collection-action-color-swatch"
                  style={`background-color: ${collectionColor(collection.color_hex)};`}
                  aria-hidden="true"
                ></span>
                <span>{$t("collections.change_color", { name: collectionDisplayName })}</span>
              </button>
              {#if collection.kind === "user"}
                <button
                  type="button"
                  role="menuitem"
                  class="collection-action-menu-item danger delete-icon"
                  on:click={(event) => askDelete(collection, event)}
                  aria-label={$t("collections.delete", { name: collection.name })}
                  title={$t("collections.delete", { name: collection.name })}
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
                  <span>{$t("collections.delete", { name: collection.name })}</span>
                </button>
              {/if}
            </div>
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

<CollectionColorModal
  open={pendingColorCollection !== null}
  collection={pendingColorCollection}
  returnFocusTo={pendingColorReturnFocusTarget}
  on:save={(event) => commitColorEdit(event.detail.colorHex)}
  on:cancel={cancelColorEdit}
/>

{#if pendingNoteCollection}
  <TextNoteModal
    open={true}
    targetKind="collection"
    targetId={pendingNoteCollection.id}
    targetLabel={pendingNoteCollection.kind === "system"
      ? $t("collections.history")
      : pendingNoteCollection.name}
    returnFocusTo={noteReturnFocusTarget}
    on:close={closeCollectionNote}
  />
{/if}

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
    color: var(--cv-fg, #f0f4f8);
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
    flex: 1 1 0;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
  }
  .collection-row {
    box-sizing: border-box;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 0.3rem;
    border-radius: 6px;
    height: 2rem;
    min-height: 2rem;
    max-height: 2rem;
    padding: 0 0.4rem;
    background-color: var(--collection-row-tint, transparent);
  }
  .collection-row.subcollection-row {
    width: calc(100% - 1.35rem);
    margin-inline-start: 1.35rem;
  }
  .collection-disclosure {
    flex: 0 0 1rem;
    width: 1rem;
    height: 1.5rem;
    padding: 0;
    border-radius: 4px;
    color: var(--cv-muted, #8b949e);
  }
  .collection-disclosure:hover,
  .collection-disclosure:focus-visible {
    color: var(--cv-fg, #f0f4f8);
  }
  .collection-row.active {
    box-shadow: inset 0 0 0 1px var(--cv-accent, #2563eb);
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
     * `.active` selection (which keeps the assigned collection tint
     * and adds an inset accent border). The drop target overlays a
     * translucent accent, a hard 1px border in the accent colour
     * and a soft glow so the row reads as a separate, actionable
     * surface. The transition properties on `.drop-target` animate
     * every one of those properties so the user sees a soft
     * 150–250 ms colour change rather than a sudden snap. */
    background-color: var(--collection-row-tint, transparent);
    border-color: var(--cv-accent, #2563eb);
    outline: 1px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.6));
    outline-offset: 1px;
    /* The shadow keeps the contrast-safe accent without the row
     * losing its square shape; it fades with the same easing as
     * the background so the transition feels cohesive. */
    box-shadow:
      inset 0 0 0 999px rgba(37, 99, 235, 0.16),
      0 0 0 2px rgba(37, 99, 235, 0.28);
  }
  .collection-row.drop-target.system-collection.drag-over {
    /* `Historial` (system kind) is never a valid drop target; if a
     * drop-target row happens to also be flagged `system` we
     * suppress the highlight so the visual feedback never lies. */
    background-color: var(--collection-row-tint, transparent);
    border-color: transparent;
    outline: 0;
    box-shadow: none;
  }
  .collection-button {
    box-sizing: border-box;
    background: transparent;
    border: 0;
    color: var(--cv-fg, #f0f4f8);
    text-align: left;
    height: 100%;
    padding: 0.1rem 0.4rem;
    border-radius: 6px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    font: inherit;
    line-height: 1.2;
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
  .badge.capture-count {
    flex: 0 0 auto;
    margin-left: auto;
    background: var(--cv-bg-elevated, #161b22);
    color: var(--cv-muted, #8b949e);
    border-radius: 999px;
    font-size: 0.65rem;
    font-variant-numeric: tabular-nums;
    line-height: 1;
    min-width: 1.1rem;
    padding: 0.05rem 0.35rem;
    text-align: center;
    border: 1px solid var(--cv-border, #30363d);
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
  .note-icon {
    color: #93c5fd;
  }
  .note-icon[data-has-note="true"] {
    color: #bfdbfe;
    background: rgba(59, 130, 246, 0.12);
  }
  .subcollection-visibility-icon {
    color: #93c5fd;
  }
  .subcollection-visibility-icon[aria-pressed="false"] {
    color: var(--cv-muted, #94a3b8);
  }
  .actions-menu-trigger {
    color: var(--cv-muted, #94a3b8);
  }
  .actions-menu-trigger[aria-expanded="true"] {
    color: var(--cv-fg, #f0f4f8);
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.06));
  }
  .actions-menu-trigger:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 1px;
  }
  .confirm-icon {
    color: #4ade80;
  }
  .cancel-icon {
    color: #94a3b8;
  }
  .collection-actions-menu {
    position: fixed;
    z-index: 40;
    min-width: 14rem;
    padding: 0.35rem;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    color: var(--cv-fg, #f0f4f8);
    background: var(--cv-bg-elevated, #161b22);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    box-shadow: 0 14px 36px rgba(0, 0, 0, 0.34);
  }
  .collection-action-menu-item {
    box-sizing: border-box;
    width: 100%;
    min-height: 2rem;
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.4rem 0.65rem;
    color: var(--cv-fg, #f0f4f8);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--cv-radius-sm, 6px);
    text-align: left;
    cursor: pointer;
    font: inherit;
    font-size: var(--cv-control, 0.85rem);
  }
  .collection-action-menu-item:hover,
  .collection-action-menu-item:focus-visible {
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.06));
    outline: none;
  }
  .collection-action-color-swatch {
    flex: 0 0 0.9rem;
    width: 0.9rem;
    height: 0.9rem;
    border: 1px solid rgba(255, 255, 255, 0.3);
    border-radius: 3px;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.2);
  }
  .collection-action-menu-item.danger.delete-icon {
    margin-top: 0.15rem;
    padding-top: 0.5rem;
    border-top-color: var(--cv-border, #30363d);
    color: var(--cv-danger, #b91c1c);
  }
  .collection-action-menu-item.danger.delete-icon:hover {
    color: var(--cv-danger-hover, #991b1b);
    background: rgba(185, 28, 28, 0.12);
  }
  .collection-action-menu-item.danger.delete-icon:focus-visible {
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
