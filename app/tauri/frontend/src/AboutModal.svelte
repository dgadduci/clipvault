<script lang="ts">
  /**
   * Modal exposing the canonical product identity for ClipVault.
   *
   * The desktop only ever opens this modal from the global ellipsis
   * menu on the toolbar — every other surface (the HistoryCard rail
   * menu, the Quick Paste preview, the per-card actions) MUST NOT
   * surface a parallel "Acerca de" entry. Keeping the affordance on
   * a single menu keeps the version string and the project metadata
   * honest: the user reads the same number the canonical Tauri
   * command (`clipvault_diagnostics`) reports, never a hard-coded
   * constant that the maintainer forgot to bump.
   *
   * The modal is metadata-only: it never echoes clipboard content,
   * source identifiers, asset paths or hashes. Closing the dialog
   * with Escape, the backdrop click or the explicit close button all
   * route through the shared `Modal` shell so the focus and keydown
   * listeners stay symmetric with every other modal the desktop
   * opens.
   */
  import type { Diagnostics } from "./types";

  export let diagnostics: Diagnostics | null = null;
  export let onClose: () => void;

  /**
   * Canonical version string. Always read from the diagnostics
   * payload (`Cargo.toml`'s `[workspace.package].version`, surfaced
   * through the `clipvault_diagnostics` Tauri command) — never from
   * a hard-coded Svelte constant. The accessor falls back to a
   * placeholder so the modal stays usable while the bootstrap call
   * has not resolved yet; the moment `diagnostics` populates the
   * real version replaces the placeholder so the modal never shows
   * a stale value.
   */
  $: displayVersion = (() => {
    const raw = diagnostics?.version?.trim();
    if (raw && raw.length > 0) return `v${raw}`;
    return "v—";
  })();
</script>

<section class="about-section" data-testid="about-modal">
  <article class="card-block">
    <h3 class="block-title">ClipVault</h3>
    <p class="muted">
      ClipVault es un workspace local para el portapapeles, snippets y
      utilidades de productividad en macOS y Linux. La información
      permanece en tu equipo, sin cuentas ni telemetría obligatoria.
    </p>
    <dl class="meta-list">
      <dt>Producto</dt>
      <dd>ClipVault</dd>
      <dt>Versión</dt>
      <dd>
        <code data-testid="about-version">{displayVersion}</code>
      </dd>
    </dl>
    <p class="muted small">
      La versión se lee desde el manifiesto canónico de la
      aplicación (Cargo workspace + Tauri config). El cierre del
      modal con <kbd>Esc</kbd>, el botón de cierre o un click sobre
      el fondo regresa el foco al menú de puntos suspensivos.
    </p>
  </article>
  <div class="row" data-testid="about-actions">
    <button
      type="button"
      class="close"
      data-testid="about-close"
      on:click={onClose}
    >
      Cerrar
    </button>
  </div>
</section>

<style>
  .about-section {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .card-block {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }

  .block-title {
    margin: 0;
    font-size: var(--cv-title-md, 1rem);
    font-weight: 600;
  }

  .muted {
    color: var(--cv-fg-muted, #94a3b8);
  }

  .muted.small {
    font-size: var(--cv-muted, 0.78rem);
  }

  .meta-list {
    display: grid;
    grid-template-columns: max-content 1fr;
    column-gap: 0.85rem;
    row-gap: 0.25rem;
    margin: 0;
  }

  .meta-list dt {
    color: var(--cv-fg-muted, #94a3b8);
    font-weight: 500;
  }

  .meta-list dd {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    word-break: break-all;
  }

  .row {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }

  button.close {
    background: var(--cv-accent, #2563eb);
    color: white;
    border: 0;
    padding: 0.45rem 0.85rem;
    border-radius: var(--cv-radius-sm, 6px);
    cursor: pointer;
    font: inherit;
  }

  button.close:hover {
    background: var(--cv-accent-hover, #1d4ed8);
  }

  kbd {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    background: rgba(148, 163, 184, 0.18);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: 4px;
    padding: 0 0.35rem;
  }
</style>