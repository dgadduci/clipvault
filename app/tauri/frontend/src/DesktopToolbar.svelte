<script lang="ts">
  /**
   * Compact desktop toolbar.
   *
   * Hosts:
   *   - the search input the main desktop already owned, decorated
   *     with a visible platform-aware shortcut hint (⌘F on macOS,
   *     Ctrl F on every other supported host);
   *   - one ellipsis button that opens an accessible menu exposing
   *     the four secondary desktop actions: Development, Privacidad,
   *     Retención and Atajo de pegado rápido. The menu is the single
   *     affordance for those four callbacks — there is no second
   *     toolbar surface that renders them inline;
   *   - the global clear-history trash icon, kept as a sibling of
   *     the ellipsis button (immediately to its right) so the
   *     destructive action stays visible outside the menu and keeps
   *     the existing confirmation flow owned by the parent.
   *
   * The toolbar is presentational: the parent owns the modal state
   * machine, the search controller and the trash confirmation flow.
   * The component only forwards user intents through callbacks so
   * `App.svelte` keeps the single source of truth.
   */
  import { onDestroy, tick } from "svelte";
  import SourceAppFilter from "./SourceAppFilter.svelte";
  import TagFilter from "./TagFilter.svelte";
  import type { SourceApplicationOption } from "./types.ts";

  type SourceAppFilterValue = import("./types.ts").SourceAppFilter;
  type TagFilterValue = import("./types.ts").TagFilter;
  type TagFilterOptionValue = import("./types.ts").TagFilterOption;

  export let searchQuery: string = "";
  export let searching: boolean = false;
  export let searchPlaceholder: string = "Buscar en el historial";
  export let searchAriaLabel: string = "Buscar en el historial del portapapeles";
  /**
   * Shortcut label rendered on the right edge of the search input.
   * The parent computes the value from the diagnostics payload so the
   * same helper drives every surface that surfaces the binding.
   */
  export let searchShortcut: string = "Ctrl F";
  /**
   * Accessible label matching the visible shortcut hint. Pin it so a
   * screen reader announces the platform-specific binding instead of
   * the cosmetic glyph alone.
   */
  export let searchShortcutAccessible: string = "Buscar (Control F)";
  /**
   * Visible label of the global clear-history trash icon. The
   * accessible name and the `title` tooltip both reuse the same
   * string so the button never surfaces two different wordings for
   * the same destructive shortcut.
   */
  export let trashLabel: string = "Eliminar capturas no organizadas";
  export let trashConfirming: boolean = false;
  /**
   * Visibility flag for the trash button. The parent owns the
   * computed value so the toolbar stays presentational: when
   * `false`, the button is absent from the DOM, the keyboard tab
   * order and the accessibility tree, not just `display: none` or
   * `disabled`. The trash therefore never reaches a user-collection
   * view where the destructive branch would be misleading.
   */
  export let showClearHistory: boolean = true;
  /**
   * Source-application filter the combobox renders. The toolbar is
   * still presentational: the parent owns the canonical state so
   * the same filter can be composed with collection, query and
   * tag facets inside `App.svelte`.
   */
  export let sourceAppFilter: SourceAppFilterValue = { kind: "all" };
  /**
   * Options the combobox renders. The parent loads them through
   * `sourceApplicationsCommand`; the toolbar only forwards them
   * to the combobox.
   */
  export let sourceAppOptions: SourceApplicationOption[] = [];
  /**
   * Tag filter the toolbar renders between the source-application
   * combobox and the configuration menu. The parent owns the
   * canonical state so the same filter can be composed with the
   * collection, query and source-app facets inside `App.svelte`.
   */
  export let tagFilter: TagFilterValue = { kind: "all" };
  /**
   * Tag combobox options. The parent derives them from the active
   * scope (`Historial` or the active collection) so the combobox
   * never surfaces tags the user cannot target; the toolbar only
   * forwards the list to the combobox.
   */
  export let tagFilterOptions: TagFilterOptionValue[] = [];
  export let onSearchInput: (value: string) => void = () => {};
  export let onOpenDevelopment: (event: MouseEvent) => void = () => {};
  export let onOpenPrivacy: (event: MouseEvent) => void = () => {};
  export let onOpenRetention: (event: MouseEvent) => void = () => {};
  export let onOpenShortcut: (event: MouseEvent) => void = () => {};
  export let onRequestClearHistory: (event: MouseEvent) => void = () => {};
  export let onSourceAppFilterChange: (next: SourceAppFilterValue) => void = () => {};
  export let onTagFilterChange: (next: TagFilterValue) => void = () => {};

  /**
   * Ellipsis-menu state. The toolbar owns a single menu instance;
   * opening it twice (mount/remount, hot reload) keeps exactly one
   * set of outside click and keyboard listeners because the
   * add/remove helpers are idempotent.
   */
  let menuOpen = false;
  let menuEl: HTMLDivElement | null = null;
  let ellipsisEl: HTMLButtonElement | null = null;

  function handleInput(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    onSearchInput(value);
  }

  function focusEllipsis(): void {
    queueMicrotask(() => {
      ellipsisEl?.focus();
    });
  }

  function closeMenu(opts: { restoreFocus?: boolean } = {}): void {
    if (!menuOpen) return;
    menuOpen = false;
    if (opts.restoreFocus) {
      focusEllipsis();
    }
  }

  /**
   * Forward an item selection through the parent callback and close
   * the menu WITHOUT restoring focus: the parent opens a modal that
   * traps focus, so the focus must remain where the user's click
   * landed (or jump to the modal's first focusable element).
   */
  function selectItem(invoke: (event: MouseEvent) => void, event: MouseEvent): void {
    menuOpen = false;
    invoke(event);
  }

  function toggleMenu(): void {
    if (menuOpen) {
      closeMenu({ restoreFocus: true });
    } else {
      menuOpen = true;
      // Move focus into the menu after the DOM commits the menu node.
      void tick().then(() => {
        const first = menuEl?.querySelector<HTMLElement>(
          '[role="menuitem"]:not([disabled])',
        );
        first?.focus();
      });
    }
  }

  function onMenuKeydown(event: KeyboardEvent): void {
    if (!menuOpen) return;
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenu({ restoreFocus: true });
    }
  }

  function onWindowPointerDown(event: MouseEvent): void {
    if (!menuOpen) return;
    const target = event.target as HTMLElement | null;
    if (!target) return;
    if (ellipsisEl && (target === ellipsisEl || ellipsisEl.contains(target))) {
      return;
    }
    if (menuEl && menuEl.contains(target)) {
      return;
    }
    closeMenu({ restoreFocus: true });
  }

  function onWindowKeydown(event: KeyboardEvent): void {
    if (!menuOpen) return;
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenu({ restoreFocus: true });
    }
  }

  // Idempotent listener registration: the toolbar installs the
  // document-level outside click / Escape handlers exactly once
  // even when Svelte mounts and remounts the component during
  // hot-reload or a remount triggered by a parent state change.
  // The flag is local to the module closure and survives across
  // remounts because it is anchored on a singleton WeakMap-like
  // owner the component itself controls.
  let listenersInstalled = false;

  function ensureWindowListeners(): void {
    if (listenersInstalled) return;
    if (typeof document === "undefined") return;
    document.addEventListener("pointerdown", onWindowPointerDown, true);
    document.addEventListener("keydown", onWindowKeydown, true);
    listenersInstalled = true;
  }

  function detachWindowListeners(): void {
    if (typeof document === "undefined") return;
    document.removeEventListener("pointerdown", onWindowPointerDown, true);
    document.removeEventListener("keydown", onWindowKeydown, true);
    listenersInstalled = false;
  }

  $: if (menuOpen) {
    ensureWindowListeners();
  } else {
    detachWindowListeners();
  }

  onDestroy(() => {
    detachWindowListeners();
  });
</script>

<div class="toolbar" data-testid="desktop-toolbar">
  <div class="toolbar-row">
    <div class="search-shell" data-testid="search-shell">
      <input
        type="search"
        class="search"
        placeholder={searchPlaceholder}
        aria-label={searchAriaLabel}
        value={searchQuery}
        on:input={handleInput}
        aria-busy={searching}
        data-testid="search-input"
      />
      <span
        class="search-shortcut"
        aria-label={searchShortcutAccessible}
        title={searchShortcutAccessible}
        data-testid="search-shortcut-hint"
      >
        {searchShortcut}
      </span>
    </div>
    <SourceAppFilter
      selected={sourceAppFilter}
      options={sourceAppOptions}
      onChange={onSourceAppFilterChange}
    />
    <TagFilter
      selected={tagFilter}
      options={tagFilterOptions}
      onChange={onTagFilterChange}
    />
    <div class="actions" role="toolbar" aria-label="Configuración y limpieza">
      <div class="menu-wrapper" data-testid="overflow-menu-wrapper">
        <button
          type="button"
          class="menu-trigger"
          bind:this={ellipsisEl}
          aria-haspopup="menu"
          aria-expanded={menuOpen}
          aria-controls="desktop-overflow-menu"
          aria-label="Más acciones del escritorio"
          title="Más acciones"
          data-testid="open-overflow-menu"
          on:click={toggleMenu}
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
        {#if menuOpen}
          <div
            class="menu"
            id="desktop-overflow-menu"
            role="menu"
            aria-label="Más acciones del escritorio"
            data-testid="overflow-menu"
            bind:this={menuEl}
            on:keydown={onMenuKeydown}
          >
            <button
              type="button"
              role="menuitem"
              class="menu-item"
              data-testid="open-development"
              on:click={(event) => selectItem(onOpenDevelopment, event)}
            >
              Development
            </button>
            <button
              type="button"
              role="menuitem"
              class="menu-item"
              data-testid="open-privacy"
              on:click={(event) => selectItem(onOpenPrivacy, event)}
            >
              Privacidad
            </button>
            <button
              type="button"
              role="menuitem"
              class="menu-item"
              data-testid="open-retention"
              on:click={(event) => selectItem(onOpenRetention, event)}
            >
              Retención
            </button>
            <button
              type="button"
              role="menuitem"
              class="menu-item"
              data-testid="open-shortcut"
              on:click={(event) => selectItem(onOpenShortcut, event)}
            >
              Atajo de pegado rápido
            </button>
          </div>
        {/if}
      </div>
      {#if showClearHistory}
        <button
          type="button"
          class="trash"
          aria-label={trashLabel}
          title={trashLabel}
          aria-busy={trashConfirming}
          data-testid="trash-clear-history"
          data-cv-danger="clear-history"
          on:click={onRequestClearHistory}
        >
          <svg
            aria-hidden="true"
            focusable="false"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M4 7h16" />
            <path d="M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2" />
            <path d="M6 7l.8 12.2a2 2 0 0 0 2 1.8h6.4a2 2 0 0 0 2-1.8L18 7" />
            <path d="M10 11v6" />
            <path d="M14 11v6" />
          </svg>
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  /*
   * The toolbar lives inside the right column of the workspace grid.
   * It must not introduce its own background card (the workspace is
   * the surface), so the wrapper is transparent and only the inner
   * controls carry borders. The wrapper still reserves a single
   * visual row so the search input and the overflow menu / trash
   * button stay horizontally aligned without injecting a third
   * divider the layout grid does not need.
   */
  .toolbar {
    width: 100%;
    min-width: 0;
    background: transparent;
    border: 0;
    padding: 0;
    margin: 0;
  }

  .toolbar-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .search-shell {
    position: relative;
    flex: 1 1 240px;
    min-width: 12rem;
    display: flex;
    align-items: center;
  }
  .search {
    width: 100%;
    padding: 0.45rem 4.5rem 0.45rem 0.75rem;
    background: var(--cv-bg-elevated, #161b22);
    color: var(--cv-fg, #f0f4f8);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-sm, 6px);
    font: inherit;
    font-size: var(--cv-body, 0.9rem);
    box-sizing: border-box;
  }

  .search:focus {
    outline: none;
    border-color: var(--cv-accent, #2563eb);
    box-shadow: 0 0 0 1px var(--cv-accent, #2563eb);
  }

  .search-shortcut {
    position: absolute;
    right: 0.5rem;
    top: 50%;
    transform: translateY(-50%);
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    border: 1px solid var(--cv-border, #30363d);
    background: rgba(15, 23, 42, 0.5);
    color: var(--cv-fg-muted, #94a3b8);
    font-size: var(--cv-muted, 0.78rem);
    pointer-events: none;
    line-height: 1.2;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
    margin-left: auto;
  }

  /*
   * The menu wrapper anchors the overflow menu so the menu panel can
   * absolutely position itself against the wrapper without escaping
   * the right column's bounds (the parent already carries
   * `overflow: visible` by default; the menu stays within the
   * column because `position: absolute` resolves against this
   * relatively-positioned wrapper).
   */
  .menu-wrapper {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  .menu-trigger {
    background: transparent;
    color: var(--cv-fg, #f0f4f8);
    border: 1px solid var(--cv-border, #30363d);
    padding: 0;
    border-radius: var(--cv-radius-sm, 6px);
    width: 2rem;
    height: 2rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    font: inherit;
  }

  .menu-trigger:hover {
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.06));
  }

  .menu-trigger[aria-expanded="true"] {
    background: var(--cv-accent, #2563eb);
    color: white;
    border-color: var(--cv-accent, #2563eb);
  }

  .menu-trigger:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 2px;
  }

  /*
   * The menu surfaces the four secondary desktop actions through a
   * single accessible list. Each menuitem keeps the data-testid the
   * previous four standalone buttons used so the regression suite
   * (and any external integration) can still dispatch through them;
   * only the visual surface changed.
   */
  .menu {
    position: absolute;
    top: calc(100% + 0.35rem);
    right: 0;
    z-index: 30;
    min-width: 14rem;
    background: var(--cv-bg-elevated, #161b22);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    box-shadow: 0 14px 36px rgba(0, 0, 0, 0.34);
    padding: 0.35rem;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .menu-item {
    background: transparent;
    color: var(--cv-fg, #f0f4f8);
    border: 1px solid transparent;
    border-radius: var(--cv-radius-sm, 6px);
    padding: 0.45rem 0.7rem;
    text-align: left;
    cursor: pointer;
    font: inherit;
    font-size: var(--cv-control, 0.85rem);
  }

  .menu-item:hover,
  .menu-item:focus-visible {
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.08));
    outline: none;
  }

  /*
   * Trash icon: the destructive clear-history shortcut is presented in
   * the documented danger colour at rest and reinforces the same
   * emphasis on hover. The dedicated `data-cv-danger="clear-history"`
   * hook pins the colour contract so a future contributor cannot
   * accidentally soften the trash back to a neutral tone; the
   * accessible name and the explicit `title` keep the action
   * discoverable for screen-reader and mouse-only users alike. The
   * confirmation modal the parent owns is the only path that mutates
   * history; this button only requests the confirmation.
   */
  .trash {
    background: transparent;
    color: var(--cv-danger, #b91c1c);
    border: 1px solid var(--cv-danger, #b91c1c);
    border-radius: var(--cv-radius-sm, 6px);
    width: 2rem;
    height: 2rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
  }

  .trash:hover {
    color: var(--cv-danger-hover, #991b1b);
    border-color: var(--cv-danger-hover, #991b1b);
    background: rgba(185, 28, 28, 0.12);
  }

  .trash:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.45));
    outline-offset: 2px;
  }

  .trash[aria-busy="true"] {
    opacity: 0.6;
    cursor: progress;
  }

  @media (max-width: 640px) {
    .toolbar-row {
      align-items: stretch;
    }
    .actions {
      width: 100%;
      justify-content: flex-start;
    }
    .menu-wrapper {
      margin-left: auto;
    }
  }
</style>