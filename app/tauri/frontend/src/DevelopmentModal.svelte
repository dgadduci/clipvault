<script lang="ts">
  /**
   * Modal hosting the developer-facing diagnostics. The shell keeps
   * these views off the main desktop to reduce visual noise; they
   * remain the canonical place to inspect the boot diagnostics, the
   * platform capabilities, the active application probe and the
   * quick-paste diagnostic controls.
   *
   * No new Tauri command is introduced: every interaction goes
   * through the same bridges the original inline section already
   * used. The modal just owns its own busy/error states so the
   * rest of the desktop stays usable while a tick or a paste
   * command is in flight.
   */
  import {
    activeApplicationCommand,
    captureTickCommand,
    diagnosticsCommand,
    gnomeIntegrationStatusCommand,
    pasteEntryCommand,
    platformCapabilitiesCommand,
    recentEntriesCommand,
    refreshCapabilitiesCommand,
  } from "./lib/tauri";
  import { shouldEnablePasteButton } from "./lib/guidance";
  import { createEventDispatcher } from "svelte";
  import type {
    ActiveApplicationResponse,
    Capabilities,
    Diagnostics,
    EntryRecord,
    GnomeIntegrationStatusResponse,
    PasteResponse,
  } from "./types";

  export let diagnostics: Diagnostics | null = null;
  export let capabilities: Capabilities | null = null;
  export let activeApp: ActiveApplicationResponse | null = null;
  export let entries: EntryRecord[] = [];

  let tickBusy = false;
  let pasteBusy = false;
  let tickResult: string | null = null;
  let capRefreshBusy = false;
  let refreshError: string | null = null;
  let gnomeStatus: GnomeIntegrationStatusResponse | null = null;
  let gnomeBusy = false;
  let gnomeError: string | null = null;

  const dispatch = createEventDispatcher<{
    refresh: void;
    capabilitiesChanged: Capabilities;
    entriesChanged: EntryRecord[];
    pasteFailed: PasteResponse;
    gnomeStatusChanged: GnomeIntegrationStatusResponse;
  }>();

  async function refreshDiagnostics(): Promise<void> {
    refreshError = null;
    try {
      const [diag, caps, app] = await Promise.all([
        diagnosticsCommand(),
        platformCapabilitiesCommand(),
        activeApplicationCommand(),
      ]);
      diagnostics = diag;
      capabilities = caps;
      activeApp = app;
      dispatch("refresh");
    } catch (err) {
      refreshError = err instanceof Error ? err.message : String(err);
    }
  }

  async function refreshGnomeStatus(): Promise<void> {
    gnomeError = null;
    gnomeBusy = true;
    try {
      gnomeStatus = await gnomeIntegrationStatusCommand();
      dispatch("gnomeStatusChanged", gnomeStatus);
    } catch (err) {
      gnomeError = err instanceof Error ? err.message : String(err);
    } finally {
      gnomeBusy = false;
    }
  }

  async function refreshCapabilities(): Promise<void> {
    refreshError = null;
    capRefreshBusy = true;
    try {
      capabilities = await refreshCapabilitiesCommand();
      dispatch("capabilitiesChanged", capabilities);
    } catch (err) {
      refreshError = err instanceof Error ? err.message : String(err);
    } finally {
      capRefreshBusy = false;
    }
  }

  async function tickCapture(): Promise<void> {
    if (tickBusy) return;
    tickBusy = true;
    tickResult = null;
    try {
      const result = await captureTickCommand({ sourceApp: null });
      tickResult = result.kind;
      const next = await recentEntriesCommand({ limit: 10 });
      entries = next;
      dispatch("entriesChanged", next);
    } catch (err) {
      tickResult = `error: ${err instanceof Error ? err.message : String(err)}`;
    } finally {
      tickBusy = false;
    }
  }

  async function pasteLatest(): Promise<void> {
    if (pasteBusy) return;
    if (entries.length === 0) return;
    pasteBusy = true;
    try {
      const response = await pasteEntryCommand({ id: entries[0].id });
      tickResult = `paste: ${response.kind}`;
      if (response.kind !== "pasted") {
        // The parent App owns the guidance surface so the platform
        // modal can render the platform-specific instructions.
        dispatch("pasteFailed", response);
      }
    } catch (err) {
      tickResult = `paste error: ${
        err instanceof Error ? err.message : String(err)
      }`;
    } finally {
      pasteBusy = false;
    }
  }
</script>

<section class="dev-section" data-testid="development-modal">
  <article class="card-block">
    <h3 class="block-title">Frontend ↔ Tauri ↔ Rust ↔ SQLite</h3>
    {#if diagnostics}
      <dl class="diag-list">
        <dt>Application version</dt>
        <dd><code>{diagnostics.version}</code></dd>
        <dt>Database path</dt>
        <dd><code>{diagnostics.database_path}</code></dd>
        <dt>Migrations applied</dt>
        <dd><code>{diagnostics.migrations_applied}</code></dd>
        <dt>Bootstrap timestamp (UTC)</dt>
        <dd><code>{diagnostics.started_at}</code></dd>
        <dt>Host OS</dt>
        <dd><code>{diagnostics.platform_os}</code></dd>
        <dt>Linux display server</dt>
        <dd><code>{diagnostics.display_server}</code></dd>
        <dt>History entries</dt>
        <dd><code>{diagnostics.history_entries}</code></dd>
      </dl>
    {:else}
      <p class="muted">Diagnostics not available yet.</p>
    {/if}
    {#if refreshError}
      <p class="status error" role="alert" data-testid="development-refresh-error">
        {refreshError}
      </p>
    {/if}
    <div class="row">
      <button type="button" on:click={refreshDiagnostics} data-testid="development-refresh">
        Refresh
      </button>
      <button
        type="button"
        on:click={refreshCapabilities}
        disabled={capRefreshBusy}
        data-testid="development-refresh-capabilities"
      >
        {capRefreshBusy ? "Refrescando…" : "Refresh capabilities"}
      </button>
    </div>
  </article>

  {#if capabilities}
    <article class="card-block">
      <h3 class="block-title">Capabilities</h3>
      <ul class="caps">
        <li class:off={!capabilities.clipboard_read}>
          clipboard_read: {capabilities.clipboard_read ? "yes" : "no"}
        </li>
        <li class:off={!capabilities.clipboard_write}>
          clipboard_write: {capabilities.clipboard_write ? "yes" : "no"}
        </li>
        <li class:off={!capabilities.global_hotkey}>
          global_hotkey: {capabilities.global_hotkey ? "yes" : "no"}
        </li>
        <li class:off={!capabilities.synthetic_paste}>
          synthetic_paste: {capabilities.synthetic_paste ? "yes" : "no"}
        </li>
        <li class:off={!capabilities.active_application}>
          active_application: {capabilities.active_application ? "yes" : "no"}
        </li>
        <li class:off={!capabilities.tray}>
          tray: {capabilities.tray ? "yes" : "no"}
        </li>
      </ul>
    </article>
  {/if}

  {#if activeApp}
    <article class="card-block">
      <h3 class="block-title">Active application</h3>
      {#if activeApp.available}
        <p>
          <code>{activeApp.name ?? "(unnamed)"}</code>
          {#if activeApp.identifier}
            <span class="muted">({activeApp.identifier})</span>
          {/if}
        </p>
      {:else}
        <p class="muted">Active-app probe is unavailable on this session.</p>
      {/if}
    </article>
  {/if}

  <article class="card-block" data-testid="quick-paste-card">
    <h3 class="block-title">Quick paste</h3>
    <p class="muted">
      Trigger a watcher tick (the shell would normally run this on a
      timer). If the synthetic-paste capability is unavailable the
      action is reported and a guidance modal explains what to do.
    </p>
    <div class="row">
      <button
        type="button"
        on:click={tickCapture}
        disabled={tickBusy || !capabilities?.clipboard_read}
        data-testid="tick-capture"
      >
        {tickBusy ? "Tick…" : "Tick capture"}
      </button>
      <button
        type="button"
        on:click={pasteLatest}
        disabled={pasteBusy || !shouldEnablePasteButton(entries.length)}
        data-testid="paste-latest"
        title={!capabilities?.synthetic_paste
          ? "Synthetic paste is unavailable; pressing Paste latest will open the guidance modal."
          : "Paste the most recent entry."}
      >
        {pasteBusy ? "Pasting…" : "Paste latest"}
      </button>
      {#if tickResult}
        <span class="muted" data-testid="tick-result">{tickResult}</span>
      {/if}
    </div>
  </article>

  <article class="card-block" data-testid="gnome-integration-card">
    <h3 class="block-title">Integración GNOME Wayland</h3>
    <p class="muted">
      Estado de la integración opcional con GNOME Shell. Sólo se
      aplica a sesiones GNOME Wayland y sólo transporta el
      identificador de la aplicación enfocada.
    </p>
    {#if gnomeStatus === null}
      <p class="muted">Recuperar estado bajo demanda.</p>
    {:else if gnomeStatus.kind === "not_applicable"}
      <p><strong>No aplicable</strong> ({gnomeStatus.session}, {gnomeStatus.desktop}).</p>
    {:else if gnomeStatus.kind === "not_configured"}
      <p class="muted">Integración no disponible ({gnomeStatus.reason}).</p>
    {:else if gnomeStatus.kind === "ready"}
      <dl class="diag-list">
        <dt>Sesión</dt>
        <dd><code>{gnomeStatus.payload.session}</code></dd>
        <dt>Consentimiento</dt>
        <dd><code>{gnomeStatus.payload.consent}</code></dd>
        <dt>Estado técnico</dt>
        <dd><code>{gnomeStatus.payload.technical_state}</code></dd>
        <dt>Identificador publicado</dt>
        <dd>
          <code>{gnomeStatus.payload.identifier ?? "(none)"}</code>
        </dd>
        {#if gnomeStatus.payload.detail}
          <dt>Detalle</dt>
          <dd><code>{gnomeStatus.payload.detail}</code></dd>
        {/if}
      </dl>
    {/if}
    {#if gnomeError}
      <p class="status error" role="alert" data-testid="gnome-debug-error">
        {gnomeError}
      </p>
    {/if}
    <div class="row">
      <button
        type="button"
        on:click={refreshGnomeStatus}
        disabled={gnomeBusy}
        data-testid="gnome-debug-refresh"
      >
        {gnomeBusy ? "Actualizando…" : "Refrescar integración"}
      </button>
    </div>
  </article>
</section>

<style>
  .dev-section {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  .card-block {
    background: var(--cv-bg-surface, #0e1116);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 0.85rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .block-title {
    margin: 0;
    font-size: var(--cv-title-sm, 0.95rem);
    font-weight: 600;
  }

  .diag-list {
    display: grid;
    grid-template-columns: max-content 1fr;
    column-gap: 0.85rem;
    row-gap: 0.25rem;
    margin: 0;
  }

  .diag-list dt {
    color: var(--cv-fg-muted, #94a3b8);
    font-weight: 500;
  }

  .diag-list dd {
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    word-break: break-all;
  }

  .caps {
    list-style: none;
    padding: 0;
    margin: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 0.85rem;
  }

  .caps li {
    padding: 0.1rem 0;
  }

  .caps li.off {
    color: #6b7280;
    text-decoration: line-through;
  }

  .row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .muted {
    color: var(--cv-fg-muted, #94a3b8);
  }

  .status.error {
    color: var(--cv-fg-error, #f87171);
  }

  button {
    background: var(--cv-accent, #2563eb);
    color: white;
    border: 0;
    padding: 0.4rem 0.85rem;
    border-radius: var(--cv-radius-sm, 6px);
    cursor: pointer;
    font-size: 0.85rem;
  }

  button:disabled {
    background: #374151;
    color: #94a3b8;
    cursor: not-allowed;
  }

  button:hover:not(:disabled) {
    background: var(--cv-accent-hover, #1d4ed8);
  }

  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
</style>