<script lang="ts">
  /**
   * Read-only metadata-only preview card the
   * `peer-text-history-browser` change renders for the host's
   * transferable text rows. The component shares the visual
   * skeleton (typography, density, hierarchy) of the local
   * `HistoryCard` but uses a dedicated read-only surface and
   * state; it never registers a drag handler, never starts a
   * pointer drag, never copies content to the clipboard and
   * never opens the local edit / pin / menu flows the local
   * card controls. The card carries a single menu entry —
   * `Importar (próximamente)` — that is permanently disabled:
   * the menu exists only to communicate that the future import
   * flow will plug into this surface, never to mutate state.
   *
   * The payload is metadata-only by construction. The card
   * NEVER receives the entry body or any field the spec /
   * design forbid (tags, collections, favourites, source-app
   * metadata, content hash, asset references). The component
   * simply does not have a slot for those fields; the bridge
   * refuses to forward them.
   */
  import type { PeerHistoryRow } from "./types";

  export let row: PeerHistoryRow;
  /**
   * Stable row id the test suite / debug overlay can use to
   * attribute the card without inspecting the opaque
   * `remote_entry_id` value. The component never exposes the
   * remote id outside the dedicated `data-remote-entry-id`
   * attribute so the test surface stays consistent across
   * hosts that mint the id with a different prefix.
   */
  export let rowTestId: string | null = null;

  /**
   * Localised content-type label. The remote rows arrive with
   * the canonical snake_case string the local SQLite layer
   * persists; the helper mirrors the local card so the
   * typography / label stay aligned. The component intentionally
   * does NOT inspect the `entry` payload for the type — the
   * label is the only metadata the renderer needs to render the
   * badge.
   */
  function describeContentType(value: string): string {
    switch (value) {
      case "text":
        return "Texto";
      case "url":
        return "URL";
      case "email":
        return "Email";
      case "json":
        return "JSON";
      case "jwt":
        return "JWT";
      case "uuid":
        return "UUID";
      case "ipv4":
        return "IPv4";
      case "ipv6":
        return "IPv6";
      case "hex_color":
        return "Color";
      case "html":
        return "HTML";
      case "file_path":
        return "Ruta";
      case "shell_command":
        return "Shell";
      case "sql":
        return "SQL";
      case "code":
        return "Código";
      case "image":
        return "Imagen";
      default:
        return value;
    }
  }

  /**
   * The menu is permanently disabled. The future `peer-text-import`
   * change replaces the disabled button with a productive one;
   * for now the placeholder is the only menu entry so the
   * renderer can communicate the upcoming flow without
   * mutating local state.
   */
  let menuOpen = false;
  function toggleMenu(): void {
    menuOpen = !menuOpen;
  }
  function closeMenu(): void {
    menuOpen = false;
  }
</script>

<article
  class="remote-preview-card"
  data-testid="remote-preview-card"
  data-remote-entry-id={row.remote_entry_id}
  data-row-test-id={rowTestId}
  draggable="false"
  aria-label={row.title ?? "Vista previa remota"}
>
  <header class="remote-preview-card-header">
    {#if row.title}
      <h3 class="remote-preview-card-title" data-testid="remote-preview-card-title">
        {row.title}
      </h3>
    {:else}
      <span class="remote-preview-card-title muted" data-testid="remote-preview-card-title">
        {describeContentType(row.content_type)}
      </span>
    {/if}
    <span
      class="remote-preview-card-type"
      data-testid="remote-preview-card-type"
      data-content-type={row.content_type}
    >
      {describeContentType(row.content_type)}
    </span>
  </header>
  <p
    class="remote-preview-card-preview"
    data-testid="remote-preview-card-preview"
  >
    {row.preview}
  </p>
  <footer class="remote-preview-card-footer">
    <time
      class="remote-preview-card-date"
      data-testid="remote-preview-card-date"
      datetime={row.created_at}
    >
      {row.created_at}
    </time>
    <div class="remote-preview-card-menu">
      <button
        type="button"
        class="remote-preview-card-menu-button"
        data-testid="remote-preview-card-menu-button"
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        on:click={toggleMenu}
      >
        ⋯
      </button>
      {#if menuOpen}
        <ul
          class="remote-preview-card-menu-list"
          data-testid="remote-preview-card-menu-list"
          role="menu"
        >
          <li role="none">
            <button
              type="button"
              role="menuitem"
              class="remote-preview-card-menu-item"
              data-testid="remote-preview-card-import-placeholder"
              disabled
              aria-disabled="true"
              on:click={closeMenu}
              title="Disponible próximamente"
            >
              Importar (próximamente)
            </button>
          </li>
        </ul>
      {/if}
    </div>
  </footer>
</article>

<style>
  .remote-preview-card {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    width: 18rem;
    flex: 0 0 auto;
    padding: 0.65rem 0.8rem;
    border-radius: 8px;
    border: 1px solid var(--cv-border, #30363d);
    background: var(--cv-bg-elev, rgba(255, 255, 255, 0.02));
    color: inherit;
    user-select: none;
    pointer-events: auto;
  }
  .remote-preview-card-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .remote-preview-card-title {
    margin: 0;
    font-size: 0.85rem;
    font-weight: 600;
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .remote-preview-card-title.muted {
    color: var(--cv-fg-muted, #94a3b8);
    font-weight: 500;
  }
  .remote-preview-card-type {
    flex: 0 0 auto;
    font-size: 0.7rem;
    text-transform: lowercase;
    color: var(--cv-fg-muted, #94a3b8);
    padding: 0.05rem 0.45rem;
    border-radius: 999px;
    border: 1px solid var(--cv-border, #30363d);
  }
  .remote-preview-card-preview {
    margin: 0;
    font-size: 0.8rem;
    line-height: 1.4;
    color: var(--cv-fg, #f0f4f8);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-word;
  }
  .remote-preview-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .remote-preview-card-date {
    font-size: 0.7rem;
    color: var(--cv-fg-muted, #94a3b8);
    flex: 1 1 auto;
  }
  .remote-preview-card-menu {
    position: relative;
    flex: 0 0 auto;
  }
  .remote-preview-card-menu-button {
    background: transparent;
    color: inherit;
    border: 1px solid transparent;
    border-radius: 6px;
    width: 1.65rem;
    height: 1.65rem;
    cursor: pointer;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .remote-preview-card-menu-button:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .remote-preview-card-menu-button:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.5));
    outline-offset: 1px;
  }
  .remote-preview-card-menu-list {
    list-style: none;
    margin: 0;
    padding: 0.25rem;
    position: absolute;
    right: 0;
    bottom: calc(100% + 0.25rem);
    background: var(--cv-bg-elev, #1f2937);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
    z-index: 10;
    min-width: 12rem;
  }
  .remote-preview-card-menu-item {
    width: 100%;
    background: transparent;
    border: 0;
    color: inherit;
    padding: 0.45rem 0.65rem;
    text-align: left;
    border-radius: 4px;
    cursor: not-allowed;
    font: inherit;
    opacity: 0.55;
  }
  .remote-preview-card-menu-item:disabled {
    cursor: not-allowed;
  }
</style>
