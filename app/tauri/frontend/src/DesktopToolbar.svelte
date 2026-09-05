<script lang="ts">
  /**
   * Compact desktop toolbar.
   *
   * Hosts:
   *   - the search input the main desktop already owned, decorated
   *     with a visible platform-aware shortcut hint (⌘F on macOS,
   *     Ctrl F on every other supported host);
   *   - the four modal triggers (Development, Privacidad, Retención,
   *     Atajo de pegado rápido);
   *   - the global clear-history trash icon.
   *
   * The toolbar is presentational: the parent owns the modal state
   * machine, the search controller and the trash confirmation flow.
   * The component only forwards user intents through callbacks so
   * `App.svelte` keeps the single source of truth.
   */
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
  export let trashLabel: string = "Limpiar historial no favorito";
  export let trashConfirming: boolean = false;
  export let openModal:
    | "development"
    | "privacy"
    | "retention"
    | "quick_paste_shortcut"
    | null = null;
  export let onSearchInput: (value: string) => void = () => {};
  export let onOpenDevelopment: (event: MouseEvent) => void = () => {};
  export let onOpenPrivacy: (event: MouseEvent) => void = () => {};
  export let onOpenRetention: (event: MouseEvent) => void = () => {};
  export let onOpenShortcut: (event: MouseEvent) => void = () => {};
  export let onRequestClearHistory: (event: MouseEvent) => void = () => {};

  function handleInput(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    onSearchInput(value);
  }
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
    <div class="actions" role="toolbar" aria-label="Configuración y limpieza">
      <button
        type="button"
        class="action"
        aria-haspopup="dialog"
        aria-expanded={openModal === "development"}
        title="Diagnóstico y controles de desarrollo"
        data-testid="open-development"
        on:click={onOpenDevelopment}
      >
        Development
      </button>
      <button
        type="button"
        class="action"
        aria-haspopup="dialog"
        aria-expanded={openModal === "privacy"}
        title="Privacidad y blacklist"
        data-testid="open-privacy"
        on:click={onOpenPrivacy}
      >
        Privacidad
      </button>
      <button
        type="button"
        class="action"
        aria-haspopup="dialog"
        aria-expanded={openModal === "retention"}
        title="Retención del historial"
        data-testid="open-retention"
        on:click={onOpenRetention}
      >
        Retención
      </button>
      <button
        type="button"
        class="action"
        aria-haspopup="dialog"
        aria-expanded={openModal === "quick_paste_shortcut"}
        title="Atajo de pegado rápido"
        data-testid="open-shortcut"
        on:click={onOpenShortcut}
      >
        Atajo
      </button>
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
    </div>
  </div>
</div>

<style>
  .toolbar {
    width: 100%;
    background: var(--cv-bg-elevated, #161b22);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 0.6rem 0.75rem;
    margin-bottom: 1rem;
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
    background: var(--cv-bg-surface, #0e1116);
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

  .action {
    background: transparent;
    color: var(--cv-fg, #f0f4f8);
    border: 1px solid var(--cv-border, #30363d);
    padding: 0.35rem 0.7rem;
    border-radius: var(--cv-radius-sm, 6px);
    cursor: pointer;
    font: inherit;
    font-size: var(--cv-control, 0.85rem);
  }

  .action:hover {
    background: var(--cv-bg-hover, rgba(255, 255, 255, 0.06));
  }

  .action[aria-expanded="true"] {
    background: var(--cv-accent, #2563eb);
    color: white;
    border-color: var(--cv-accent, #2563eb);
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
  }
</style>
