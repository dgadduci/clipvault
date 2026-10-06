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
    pasteEntryCommand,
    platformCapabilitiesCommand,
    recentEntriesCommand,
    refreshCapabilitiesCommand,
  } from "./lib/tauri";
  import { shouldEnablePasteButton } from "./lib/guidance";
  import { t } from "./lib/localization";
  import { createEventDispatcher } from "svelte";
  import type {
    ActiveApplicationResponse,
    Capabilities,
    Diagnostics,
    EntryRecord,
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

  const dispatch = createEventDispatcher<{
    refresh: void;
    capabilitiesChanged: Capabilities;
    entriesChanged: EntryRecord[];
    pasteFailed: PasteResponse;
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
      refreshError = "development.error.refresh";
    }
  }

  async function refreshCapabilities(): Promise<void> {
    refreshError = null;
    capRefreshBusy = true;
    try {
      capabilities = await refreshCapabilitiesCommand();
      dispatch("capabilitiesChanged", capabilities);
    } catch (err) {
      refreshError = "development.error.refresh";
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
      tickResult = `development.tick.result.${result.kind}`;
      const next = await recentEntriesCommand({ limit: 10 });
      entries = next;
      dispatch("entriesChanged", next);
    } catch (err) {
      tickResult = "development.error.tick";
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
      tickResult = `development.paste.result.${response.kind}`;
      if (response.kind !== "pasted") {
        // The parent App owns the guidance surface so the platform
        // modal can render the platform-specific instructions.
        dispatch("pasteFailed", response);
      }
    } catch (err) {
      tickResult = "development.error.paste";
    } finally {
      pasteBusy = false;
    }
  }
</script>

<section class="dev-section" data-testid="development-modal">
  <article class="card-block">
    <h3 class="block-title">{$t("development.architecture")}</h3>
    {#if diagnostics}
      <dl class="diag-list">
        <dt>{$t("development.field.version")}</dt>
        <dd><code>{diagnostics.version}</code></dd>
        <dt>{$t("development.field.database_path")}</dt>
        <dd><code>{diagnostics.database_path}</code></dd>
        <dt>{$t("development.field.migrations")}</dt>
        <dd><code>{diagnostics.migrations_applied}</code></dd>
        <dt>{$t("development.field.started_at")}</dt>
        <dd><code>{diagnostics.started_at}</code></dd>
        <dt>{$t("development.field.host_os")}</dt>
        <dd><code>{diagnostics.platform_os}</code></dd>
        <dt>{$t("development.field.display_server")}</dt>
        <dd><code>{diagnostics.display_server}</code></dd>
        <dt>{$t("development.field.history_entries")}</dt>
        <dd><code>{diagnostics.history_entries}</code></dd>
      </dl>
    {:else}
      <p class="muted">{$t("development.diagnostics.unavailable")}</p>
    {/if}
    {#if refreshError}
      <p class="status error" role="alert" data-testid="development-refresh-error">
        {$t(refreshError)}
      </p>
    {/if}
    <div class="row">
      <button type="button" on:click={refreshDiagnostics} data-testid="development-refresh">
        {$t("common.refresh")}
      </button>
      <button
        type="button"
        on:click={refreshCapabilities}
        disabled={capRefreshBusy}
        data-testid="development-refresh-capabilities"
      >
        {capRefreshBusy ? $t("development.refreshing") : $t("development.refresh_capabilities")}
      </button>
    </div>
  </article>

  {#if capabilities}
    <article class="card-block">
      <h3 class="block-title">{$t("development.capabilities")}</h3>
      <ul class="caps">
        <li class:off={!capabilities.clipboard_read}>
          clipboard_read: {capabilities.clipboard_read ? $t("common.yes") : $t("common.no")}
        </li>
        <li class:off={!capabilities.clipboard_write}>
          clipboard_write: {capabilities.clipboard_write ? $t("common.yes") : $t("common.no")}
        </li>
        <li class:off={!capabilities.global_hotkey}>
          global_hotkey: {capabilities.global_hotkey ? $t("common.yes") : $t("common.no")}
        </li>
        <li class:off={!capabilities.synthetic_paste}>
          synthetic_paste: {capabilities.synthetic_paste ? $t("common.yes") : $t("common.no")}
        </li>
        <li class:off={!capabilities.active_application}>
          active_application: {capabilities.active_application ? $t("common.yes") : $t("common.no")}
        </li>
        <li class:off={!capabilities.tray}>
          tray: {capabilities.tray ? $t("common.yes") : $t("common.no")}
        </li>
      </ul>
    </article>
  {/if}

  {#if activeApp}
    <article class="card-block">
      <h3 class="block-title">{$t("development.active_application")}</h3>
      {#if activeApp.available}
        <p>
          <code>{activeApp.name ?? $t("development.unnamed")}</code>
          {#if activeApp.identifier}
            <span class="muted">({activeApp.identifier})</span>
          {/if}
        </p>
      {:else}
        <p class="muted">{$t("development.active_app_unavailable")}</p>
      {/if}
    </article>
  {/if}

  <article class="card-block" data-testid="quick-paste-card">
    <h3 class="block-title">{$t("development.quick_paste")}</h3>
    <p class="muted">
      {$t("development.quick_paste.description")}
    </p>
    <div class="row">
      <button
        type="button"
        on:click={tickCapture}
        disabled={tickBusy || !capabilities?.clipboard_read}
        data-testid="tick-capture"
      >
        {tickBusy ? $t("development.tick.running") : $t("development.tick.capture")}
      </button>
      <button
        type="button"
        on:click={pasteLatest}
        disabled={pasteBusy || !shouldEnablePasteButton(entries.length)}
        data-testid="paste-latest"
        title={!capabilities?.synthetic_paste
          ? $t("development.paste.unavailable_tooltip")
          : $t("development.paste.latest_tooltip")}
      >
        {pasteBusy ? $t("development.paste.running") : $t("development.paste.latest")}
      </button>
      {#if tickResult}
        <span class="muted" data-testid="tick-result">{$t(tickResult)}</span>
      {/if}
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
