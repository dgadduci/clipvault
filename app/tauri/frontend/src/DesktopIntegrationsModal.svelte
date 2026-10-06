<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";
  import GnomeIntegrationModal from "./GnomeIntegrationModal.svelte";
  import {
    gnomeIntegrationStatusCommand,
    kdeKwinIntegrationActivateCommand,
    kdeKwinIntegrationDeclineCommand,
    kdeKwinIntegrationDisableCommand,
    kdeKwinIntegrationRetryCommand,
    kdeKwinIntegrationStatusCommand,
    kdeKwinIntegrationUninstallCommand,
  } from "./lib/tauri";
  import { kdeIntegrationStatusKind } from "./lib/desktopIntegrationSettings";
  import { t } from "./lib/localization";
  import type {
    GnomeIntegrationStatusResponse,
    KdeKwinIntegrationPayload,
  } from "./types";

  export let open = false;
  export let initialGnomeStatus: GnomeIntegrationStatusResponse | null = null;

  let gnomeStatus = initialGnomeStatus;
  let kdeStatus: KdeKwinIntegrationPayload | null = null;
  let loading = false;
  let gnomeError: string | null = null;
  let kdeError: string | null = null;
  let gnomeBusy = false;
  let kdeBusy = false;
  let requestId = 0;

  const dispatch = createEventDispatcher<{
    gnomeStatusChanged: GnomeIntegrationStatusResponse;
  }>();

  $: if (initialGnomeStatus) gnomeStatus = initialGnomeStatus;
  $: if (open) void refreshStatus();

  $: showGnome =
    gnomeStatus?.kind === "ready" && gnomeStatus.payload.applicable;
  $: showKde = kdeStatus?.applicable === true;
  $: kdeStatusKey = kdeStatus
    ? kdeIntegrationStatusKind(kdeStatus) === "pending"
      ? "settings.desktop_integrations.status.pending.kde"
      : `settings.desktop_integrations.status.${kdeIntegrationStatusKind(kdeStatus)}`
    : "settings.desktop_integrations.status.unsupported";

  async function refreshStatus(): Promise<void> {
    const currentRequest = ++requestId;
    loading = true;
    gnomeError = null;
    kdeError = null;

    const [gnomeResult, kdeResult] = await Promise.allSettled([
      gnomeIntegrationStatusCommand(),
      kdeKwinIntegrationStatusCommand(),
    ]);

    if (currentRequest !== requestId) return;

    if (gnomeResult.status === "fulfilled") {
      gnomeStatus = gnomeResult.value;
      dispatch("gnomeStatusChanged", gnomeStatus);
    } else {
      gnomeError = "settings.desktop_integrations.error";
    }

    if (kdeResult.status === "fulfilled") {
      kdeStatus = kdeResult.value;
    } else {
      kdeError = "settings.desktop_integrations.error";
    }
    loading = false;
  }

  async function runKdeAction(
    action: () => Promise<KdeKwinIntegrationPayload>,
  ): Promise<void> {
    if (kdeBusy) return;
    kdeBusy = true;
    kdeError = null;
    try {
      kdeStatus = await action();
    } catch {
      kdeError = "settings.desktop_integrations.error";
    } finally {
      kdeBusy = false;
    }
  }

  onDestroy(() => {
    requestId += 1;
  });
</script>

<section class="desktop-integrations" data-testid="desktop-integrations-modal">
  <p class="muted">{$t("settings.desktop_integrations.description")}</p>

  <div class="toolbar">
    {#if loading}
      <span class="muted" role="status">{$t("settings.desktop_integrations.checking")}</span>
    {/if}
    <button
      type="button"
      class="secondary"
      on:click={refreshStatus}
      disabled={loading || gnomeBusy || kdeBusy}
      data-testid="desktop-integrations-refresh"
    >
      {$t("settings.desktop_integrations.refresh")}
    </button>
  </div>

  {#if showGnome && gnomeStatus?.kind === "ready" && !loading}
    <div data-testid="desktop-integrations-gnome">
      <GnomeIntegrationModal
        initial={gnomeStatus}
        on:statusChanged={(event) => dispatch("gnomeStatusChanged", event.detail)}
        on:actionStarted={() => (gnomeBusy = true)}
        on:actionFinished={() => (gnomeBusy = false)}
      />
    </div>
  {/if}

  {#if showKde && kdeStatus}
    <article class="integration-card" data-testid="desktop-integrations-kde">
      <h3>{$t("development.kde.title")}</h3>
      <p class="muted">{$t("settings.desktop_integrations.kde.description")}</p>
      <p class="integration-status" role="status" data-testid="kde-friendly-status">
        {$t(kdeStatusKey)}
      </p>

      {#if kdeError}
        <p class="error" role="alert" data-testid="desktop-integrations-kde-error">
          {$t(kdeError)}
        </p>
      {/if}

      {#if kdeStatus.applicable}
        <div class="actions">
          {#if !kdeStatus.enabled || kdeStatus.consent !== "accepted"}
            <button
              type="button"
              class="primary"
              on:click={() => runKdeAction(kdeKwinIntegrationActivateCommand)}
              disabled={kdeBusy || loading}
              data-testid="desktop-integrations-kde-activate"
            >
              {kdeBusy ? $t("development.processing") : $t("development.kde.activate")}
            </button>
          {/if}
          {#if kdeStatus.consent === "unknown"}
            <button
              type="button"
              class="secondary"
              on:click={() => runKdeAction(kdeKwinIntegrationDeclineCommand)}
              disabled={kdeBusy || loading}
              data-testid="desktop-integrations-kde-decline"
            >
              {$t("development.kde.decline")}
            </button>
          {/if}
          {#if kdeStatus.installed && kdeStatus.enabled}
            <button
              type="button"
              class="secondary"
              on:click={() => runKdeAction(kdeKwinIntegrationDisableCommand)}
              disabled={kdeBusy || loading}
              data-testid="desktop-integrations-kde-disable"
            >
              {$t("development.kde.disable")}
            </button>
            <button
              type="button"
              class="danger"
              on:click={() => runKdeAction(kdeKwinIntegrationUninstallCommand)}
              disabled={kdeBusy || loading}
              data-testid="desktop-integrations-kde-uninstall"
            >
              {$t("development.kde.uninstall")}
            </button>
          {/if}
          {#if kdeStatus.consent === "accepted" && kdeStatus.installed && kdeStatus.enabled && kdeStatus.technical_state !== "identified"}
            <button
              type="button"
              class="secondary"
              on:click={() => runKdeAction(kdeKwinIntegrationRetryCommand)}
              disabled={kdeBusy || loading}
              data-testid="desktop-integrations-kde-retry"
            >
              {$t("development.kde.retry")}
            </button>
          {/if}
        </div>
      {/if}
    </article>
  {/if}

  {#if !loading && !showGnome && !showKde}
    <p class="muted" role="status" data-testid="desktop-integrations-unavailable">
      {$t("settings.desktop_integrations.unavailable")}
    </p>
  {/if}

  {#if gnomeError}
    <p class="error" role="alert" data-testid="desktop-integrations-gnome-error">
      {$t(gnomeError)}
    </p>
  {/if}
</section>

<style>
  .desktop-integrations {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  .integration-card {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    padding: 0.85rem 1rem;
    background: var(--cv-bg-surface, #0e1116);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
  }

  h3,
  p {
    margin: 0;
  }

  h3 {
    font-size: var(--cv-title-sm, 0.95rem);
    font-weight: 600;
  }

  .toolbar,
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  .toolbar {
    justify-content: space-between;
  }

  .integration-status {
    font-weight: 600;
  }

  .muted {
    color: var(--cv-fg-muted, #94a3b8);
    font-size: var(--cv-muted, 0.82rem);
  }

  .error {
    color: var(--cv-danger, #f87171);
  }
</style>
