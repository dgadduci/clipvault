<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { get } from "svelte/store";
  import {
    captureControlGetCommand,
    captureControlSetCommand,
    gnomeIntegrationStatusCommand,
    kdeKwinIntegrationStatusCommand,
    settingsGetCommand,
    settingsSetCommand,
  } from "./lib/tauri.ts";
  import {
    listenCaptureControlChanged,
    listenCaptureControlError,
  } from "./lib/captureControlUpdates.ts";
  import { captureToggleShortcutLabel } from "./lib/clipboardCaptureControl.ts";
  import { keyboardShortcuts } from "./lib/keyboardShortcuts.ts";
  import { hasManageableDesktopIntegration } from "./lib/desktopIntegrationSettings.ts";
  import { createEventDispatcher } from "svelte";
  import {
    localeStore,
    setLocale,
    t,
    type Locale,
  } from "./lib/localization.ts";

  export let open = false;
  export let platformOs: string | null = null;
  export let displayServer: string | null = null;

  const dispatch = createEventDispatcher<{
    keyboardShortcutsRequested: void;
    desktopIntegrationsRequested: void;
  }>();

  let captureEnabled: boolean | null = null;
  let shareNotesEnabled: boolean | null = null;
  let language: Locale = "en";
  let loading = false;
  let saving = false;
  let errorKey: string | null = null;
  let requestId = 0;
  let integrationAvailabilityRequestId = 0;
  let desktopIntegrationsAvailable = false;
  let desktopIntegrationsAvailabilityError = false;
  let disposed = false;
  let unlisten: (() => void) | null = null;
  let unlistenError: (() => void) | null = null;

  $: shortcut = $keyboardShortcuts && captureToggleShortcutLabel(platformOs);
  $: if (open) void loadSettings();
  $: if (open) void loadDesktopIntegrationAvailability();

  async function loadDesktopIntegrationAvailability(): Promise<void> {
    const currentRequest = ++integrationAvailabilityRequestId;
    desktopIntegrationsAvailable = false;
    desktopIntegrationsAvailabilityError = false;

    const [gnomeResult, kdeResult] = await Promise.allSettled([
      gnomeIntegrationStatusCommand(),
      kdeKwinIntegrationStatusCommand(),
    ]);
    if (currentRequest !== integrationAvailabilityRequestId) return;

    const gnome = gnomeResult.status === "fulfilled" ? gnomeResult.value : null;
    const kde = kdeResult.status === "fulfilled" ? kdeResult.value : null;
    desktopIntegrationsAvailable = hasManageableDesktopIntegration(gnome, kde);

    if (gnomeResult.status === "rejected" || kdeResult.status === "rejected") {
      desktopIntegrationsAvailabilityError = true;
    }
  }

  async function loadSettings(): Promise<void> {
    const currentRequest = ++requestId;
    loading = true;
    errorKey = null;
    try {
      const [enabled, settings] = await Promise.all([
        captureControlGetCommand(),
        settingsGetCommand(),
      ]);
      if (currentRequest === requestId) {
        captureEnabled = enabled;
        shareNotesEnabled = settings.capture_notes_sharing_enabled ?? false;
        language = settings.language;
      }
    } catch (loadError) {
      if (currentRequest === requestId) {
        errorKey = "settings.error.loading";
      }
    } finally {
      if (currentRequest === requestId) loading = false;
    }
  }

  async function toggleShareNotes(): Promise<void> {
    if (shareNotesEnabled === null || saving) return;
    const previous = shareNotesEnabled;
    const next = !previous;
    shareNotesEnabled = next;
    saving = true;
    errorKey = null;
    try {
      const settings = await settingsSetCommand({ capture_notes_sharing_enabled: next });
      shareNotesEnabled = settings.capture_notes_sharing_enabled ?? false;
    } catch (saveError) {
      shareNotesEnabled = previous;
      errorKey = "settings.error.saving";
    } finally {
      saving = false;
    }
  }

  async function toggleCapture(): Promise<void> {
    if (captureEnabled === null || saving) return;
    saving = true;
    try {
      captureEnabled = await captureControlSetCommand({
        enabled: !captureEnabled,
      });
    } catch (saveError) {
      errorKey = "settings.error.capture_toggle";
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    listenCaptureControlChanged((enabled) => {
      captureEnabled = enabled;
      errorKey = null;
    })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        errorKey = "settings.error.capture_toggle";
      });
    listenCaptureControlError(() => {
      errorKey = "settings.error.capture_toggle";
    })
      .then((stop) => {
        if (disposed) stop();
        else unlistenError = stop;
      })
      .catch(() => {
        errorKey = "settings.error.capture_toggle";
      });
    return () => {
      disposed = true;
      unlisten?.();
      unlisten = null;
      unlistenError?.();
      unlistenError = null;
    };
  });

  async function saveLanguage(event: Event): Promise<void> {
    const requested = (event.currentTarget as HTMLSelectElement).value as Locale;
    const previous = get(localeStore);
    language = requested;
    saving = true;
    errorKey = null;
    try {
      const settings = await settingsSetCommand({ language: requested });
      language = settings.language;
      setLocale(settings.language);
    } catch {
      language = previous;
      errorKey = "settings.language.save_error";
    } finally {
      saving = false;
    }
  }

  onDestroy(() => {
    disposed = true;
    integrationAvailabilityRequestId += 1;
    unlisten?.();
    unlisten = null;
    unlistenError?.();
    unlistenError = null;
  });
</script>

<section class="general-settings" data-testid="general-settings-modal">
  <article data-testid="keyboard-shortcuts-setting">
    <h3>{$t("keyboard_shortcuts.title")}</h3>
    <p class="muted">{$t("keyboard_shortcuts.description")}</p>
    <div class="controls">
      <button
        type="button"
        data-testid="keyboard-shortcuts-open"
        on:click={() => dispatch("keyboardShortcutsRequested")}
      >{$t("keyboard_shortcuts.open")}</button>
    </div>
  </article>
  {#if desktopIntegrationsAvailable}
    <article data-testid="desktop-integrations-setting">
      <h3>{$t("settings.desktop_integrations.title")}</h3>
      <p class="muted">{$t("settings.desktop_integrations.entry_description")}</p>
      {#if desktopIntegrationsAvailabilityError}
        <p class="error" role="status">{$t("settings.desktop_integrations.error")}</p>
      {/if}
      <div class="controls">
        <button
          type="button"
          on:click={() => dispatch("desktopIntegrationsRequested")}
          data-testid="desktop-integrations-open"
        >
          {$t("settings.desktop_integrations.open")}
        </button>
      </div>
    </article>
  {/if}
  <article data-testid="language-setting">
    <h3>{$t("settings.language.title")}</h3>
    <p class="muted">{$t("settings.language.description")}</p>
    <label for="clipvault-language">{$t("settings.language.label")}</label>
    <select
      id="clipvault-language"
      bind:value={language}
      on:change={saveLanguage}
      disabled={saving}
      data-testid="language-selector"
      aria-label={$t("settings.language.label")}
    >
      <option value="en">{$t("language.name.en")}</option>
      <option value="es">{$t("language.name.es")}</option>
      <option value="pt">{$t("language.name.pt")}</option>
      <option value="de">{$t("language.name.de")}</option>
      <option value="fr">{$t("language.name.fr")}</option>
    </select>
  </article>
  <article data-testid="clipboard-capture-setting">
    <h3>{$t("settings.capture.title")}</h3>
    <p class="muted">
      {$t("settings.capture.description")}
    </p>

    {#if loading && captureEnabled === null}
      <p class="muted" role="status" data-testid="capture-control-loading">
        {$t("settings.loading")}
      </p>
    {:else if captureEnabled !== null}
      <p class="state" data-testid="capture-control-state" aria-live="polite">
        {$t("settings.state", {
          state: captureEnabled
            ? $t("settings.state.active")
            : $t("settings.state.paused"),
        })}
      </p>
      <div class="controls">
        <button
          type="button"
          class="secondary"
          on:click={toggleCapture}
          disabled={saving}
          aria-busy={saving}
          aria-pressed={captureEnabled}
          data-testid="capture-control-toggle"
        >
          {saving
            ? $t("settings.saving")
            : captureEnabled
              ? $t("settings.capture.pause")
              : $t("settings.capture.resume")}
        </button>
        <span class="shortcut" data-testid="capture-control-shortcut">
          {$t("settings.capture.shortcut", { shortcut })}
        </span>
      </div>
      {#if platformOs === "linux" && displayServer === "wayland"}
        <p class="muted" data-testid="capture-control-wayland-note">
          {$t("settings.capture.wayland")}
        </p>
      {/if}
    {/if}

  </article>
  <article data-testid="capture-note-sharing-setting">
    <h3>{$t("settings.notes.title")}</h3>
    <p class="muted">
      {$t("settings.notes.description")}
    </p>
    {#if shareNotesEnabled === null}
      <p class="muted" role="status" data-testid="capture-note-sharing-loading">{$t("settings.loading")}</p>
    {:else}
      <label class="note-sharing-control">
        <input
          type="checkbox"
          checked={shareNotesEnabled}
          disabled={saving}
          on:change={toggleShareNotes}
          data-testid="capture-note-sharing-toggle"
        />
        {$t("settings.notes.include")}
      </label>
    {/if}
  </article>
  {#if errorKey}
    <p class="error" role="alert" data-testid="general-settings-error">
      {$t(errorKey)}
    </p>
  {/if}
</section>

<style>
  .general-settings {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  article {
    background: var(--cv-bg-surface, #0e1116);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 0.85rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  h3 {
    margin: 0;
    font-size: var(--cv-title-sm, 0.95rem);
    font-weight: 600;
  }
  p {
    margin: 0;
  }
  .muted,
  .shortcut {
    color: var(--cv-fg-muted, #94a3b8);
    font-size: var(--cv-muted, 0.82rem);
  }
  .state {
    font-weight: 600;
  }
  .controls {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.75rem;
  }
  .error {
    color: var(--cv-danger, #f87171);
  }
</style>
