<script lang="ts">
  /**
   * Modal hosting the privacy controls (blacklist + active-app
   * diagnostics + local peer identity). The retention selector
   * lives in its own **Retención** modal so this view stays focused
   * on privacy-only concerns.
   *
   * The component keeps the exact Tauri contract the previous
   * inline privacy surface used (`clipvault_settings_get`,
   * `clipvault_ignored_app_*`, `clipvault_active_app_*`,
   * `clipvault_local_peer_profile_*`). It is self-contained: every
   * async refresh lives here and the parent only sees the typed
   * `onSettingsChanged` callback the rest of the app uses to keep
   * the rail / cards in sync.
   */
  import { onDestroy, onMount } from "svelte";
  import {
    activeAppDiagnosticsCommand,
    ignoredAppIconCommand,
    ignoredAppLinuxAddCommand,
    ignoredAppLinuxCatalogCommand,
    ignoredAppPickAndAddCommand,
    ignoredAppsListWithMetadataCommand,
    ignoredAppsRemoveCommand,
    localPeerProfileGetCommand,
    localPeerProfileUpdateCommand,
    refreshActiveAppDiagnosticsCommand,
    settingsGetCommand,
    sourceAppIconCommand,
  } from "./lib/tauri";
  import type {
    ActiveAppDiagnostics,
    IgnoredAppEntry,
    LinuxCatalogResponse,
    LinuxPickerCandidate,
    LocalPeerProfile,
    LocalPeerProfileResponse,
    PickAndAddResponse,
    PickErrorReason,
    Settings,
  } from "./types";
  import {
    createIconResolver,
    type IconLoader,
    type IconResolver,
  } from "./lib/iconResolver";
  import { t } from "./lib/localization";

  export let onSettingsChanged: (settings: Settings) => void = () => {};

  let settings: Settings | null = null;
  let ignoredEntries: IgnoredAppEntry[] = [];
  let pickerError: string | null = null;
  let actionMessage: string | null = null;
  let actionMessageParams: Record<string, string | number | Date> = {};
  let loading = true;
  let diagnostics: ActiveAppDiagnostics | null = null;
  let diagnosticsError: string | null = null;
  let refreshingDiagnostics = false;
  let lastBlacklistMatch: boolean | null = null;
  let pickerPending = false;
  let iconUrls: Record<string, string> = {};
  let iconFailures: Record<string, boolean> = {};
  let linuxPicker: LinuxCatalogResponse | null = null;
  let linuxPickerOpen = false;
  let linuxPickerLoading = false;
  let linuxPickerError: string | null = null;
  let linuxPickerIconUrls: Record<string, string> = {};
  let linuxPickerIconFailures: Record<string, boolean> = {};
  let linuxPickerIconRefs: Record<string, string> = {};
  let identityProfile: LocalPeerProfile | null = null;
  let identityUnavailable: string | null = null;
  let identityLoading = false;
  let identityError: string | null = null;
  let identitySaving = false;
  let identityDraft = "";

  const tauriIconLoader: IconLoader = {
    async loadIconBytes(ref) {
      try {
        return await ignoredAppIconCommand({ ref });
      } catch {
        return null;
      }
    },
  };
  // Catalog candidates live in the `application-icons/` namespace
  // (the metadata provider persists them there when it rasterises the
  // resolved icon). `clipvault_ignored_app_icon` only accepts the
  // `ignored-apps/` namespace, so the picker MUST go through
  // `clipvault_source_app_icon` instead — the same bridge the
  // capture pipeline already uses for `source_app_icon_ref`. The
  // blacklist panel keeps the `ignoredAppIconCommand` loader because
  // its rows are persisted under `ignored-apps/`.
  const linuxPickerIconLoader: IconLoader = {
    async loadIconBytes(ref) {
      try {
        return await sourceAppIconCommand({ ref });
      } catch {
        return null;
      }
    },
  };
  const iconResolver: IconResolver = createIconResolver(tauriIconLoader);
  const linuxPickerIconResolver: IconResolver =
    createIconResolver(linuxPickerIconLoader);

  async function refresh(): Promise<void> {
    loading = true;
    try {
      settings = await settingsGetCommand();
      identityDraft = settings.local_peer_display_name ?? "";
      await refreshIgnored();
      await refreshIcons();
      await refreshIdentity();
    } finally {
      loading = false;
    }
  }

  async function refreshIgnored(): Promise<void> {
    try {
      ignoredEntries = await ignoredAppsListWithMetadataCommand();
      await refreshIcons();
    } catch (error) {
      pickerError = describeError(error);
    }
  }

  async function refreshIdentity(): Promise<void> {
    identityLoading = true;
    identityError = null;
    try {
      const response: LocalPeerProfileResponse = await localPeerProfileGetCommand();
      if (response.kind === "available") {
        identityProfile = response.profile;
        identityUnavailable = null;
      } else {
        identityProfile = null;
        identityUnavailable = response.reason;
      }
    } catch (error) {
      identityProfile = null;
      identityUnavailable = null;
      identityError = describeError(error);
    } finally {
      identityLoading = false;
    }
  }

  async function saveIdentityName(): Promise<void> {
    if (!settings) return;
    const trimmed = identityDraft.trim();
    if (trimmed === (settings.local_peer_display_name ?? "")) {
      actionMessage = "privacy.identity.already_updated";
      actionMessageParams = {};
      return;
    }
    identitySaving = true;
    identityError = null;
    try {
      const response = await localPeerProfileUpdateCommand({
        name: trimmed.length > 0 ? trimmed : null,
      });
      if (response.kind === "available") {
        identityProfile = response.profile;
        identityUnavailable = null;
        identityDraft = response.profile.display_name ?? "";
      } else {
        identityProfile = null;
        identityUnavailable = response.reason;
        identityDraft = trimmed.length > 0 ? trimmed : "";
      }
      settings = {
        ...settings,
        local_peer_display_name: trimmed.length > 0 ? trimmed : null,
      };
      onSettingsChanged(settings);
      actionMessage = trimmed.length > 0
        ? "privacy.identity.updated"
        : "privacy.identity.deleted";
      actionMessageParams = trimmed.length > 0 ? { name: trimmed } : {};
    } catch (error) {
      identityError = describeError(error);
    } finally {
      identitySaving = false;
    }
  }

  function describePeerDisplayName(code: string | undefined): string {
    switch (code) {
      case "invalid_peer_display_name":
        return "privacy.identity.error.invalid_name";
      case "peer_display_name_too_long":
        return "privacy.identity.error.name_too_long";
      default:
        return "privacy.identity.error.name_rejected";
    }
  }

  async function refreshDiagnostics(): Promise<void> {
    diagnosticsError = null;
    refreshingDiagnostics = true;
    try {
      diagnostics = await refreshActiveAppDiagnosticsCommand();
    } catch (error) {
      diagnosticsError = describeError(error);
      diagnostics = null;
    } finally {
      refreshingDiagnostics = false;
    }
    recomputeBlacklistMatch();
  }

  async function consultDiagnostics(): Promise<void> {
    diagnosticsError = null;
    refreshingDiagnostics = true;
    try {
      diagnostics = await activeAppDiagnosticsCommand();
    } catch (error) {
      diagnosticsError = describeError(error);
      diagnostics = null;
    } finally {
      refreshingDiagnostics = false;
    }
    recomputeBlacklistMatch();
  }

  function normaliseForMatch(value: string): string {
    return value.trim().toLowerCase();
  }

  function recomputeBlacklistMatch(): void {
    if (!diagnostics || !diagnostics.identifier || !settings) {
      lastBlacklistMatch = null;
      return;
    }
    const observed = normaliseForMatch(diagnostics.identifier);
    lastBlacklistMatch = settings.ignored_apps.some(
      (id: string) => normaliseForMatch(id) === observed,
    );
  }

  async function pickAndAddIgnored(): Promise<void> {
    pickerError = null;
    actionMessage = null;
    pickerPending = true;
    try {
      const linuxCatalog = await loadLinuxCatalog();
      if (linuxCatalog) {
        if (linuxCatalog.kind === "supported") {
          linuxPicker = linuxCatalog;
          linuxPickerOpen = true;
          return;
        }
        // The Linux command answered successfully, but determined that this
        // session cannot provide a deterministic catalog. Do not replace that
        // answer with the legacy picker: on Linux it reports the unrelated
        // `unsupported_session` fallback and masks the actual condition.
        pickerError = describeLinuxCatalogUnavailable(linuxCatalog.reason);
        return;
      }
      const response: PickAndAddResponse = await ignoredAppPickAndAddCommand();
      handlePickResponse(response);
    } catch (error) {
      pickerError = describeError(error);
    } finally {
      pickerPending = false;
    }
  }

  async function loadLinuxCatalog(): Promise<LinuxCatalogResponse | null> {
    linuxPickerLoading = true;
    linuxPickerError = null;
    try {
      const response = await ignoredAppLinuxCatalogCommand();
      if (response.kind === "supported") {
        await refreshLinuxPickerIcons(response.candidates);
      }
      return response;
    } catch (error) {
      // Linux-only commands are absent from non-Linux builds. Keep the
      // existing picker path available there instead of surfacing an IPC
      // command-not-found error as a Linux-specific UI failure.
      linuxPickerError = describeError(error);
      return null;
    } finally {
      linuxPickerLoading = false;
    }
  }

  async function refreshLinuxPickerIcons(
    candidates: readonly LinuxPickerCandidate[],
  ): Promise<void> {
    // Release any previously-displayed Blob URLs before resolving
    // the new catalog: the resolver reuses cached entries, so a ref
    // that disappears from the new catalog would otherwise stay
    // cached until the resolver itself is fully released. Releasing
    // here keeps the Blob URL lifecycle tied to whichever catalog is
    // currently rendered.
    for (const ref of Object.values(linuxPickerIconRefs)) {
      linuxPickerIconResolver.releaseFor(ref);
    }
    const nextUrls: Record<string, string> = {};
    const nextFailures: Record<string, boolean> = {};
    const nextRefs: Record<string, string> = {};
    for (const candidate of candidates) {
      if (!candidate.icon_ref) continue;
      const resolution = await linuxPickerIconResolver.resolve(candidate.icon_ref);
      if (resolution.ok && resolution.url) {
        nextUrls[candidate.identifier] = resolution.url;
        nextRefs[candidate.identifier] = candidate.icon_ref;
      } else {
        nextFailures[candidate.identifier] = true;
      }
    }
    linuxPickerIconUrls = nextUrls;
    linuxPickerIconFailures = nextFailures;
    linuxPickerIconRefs = nextRefs;
  }

  async function confirmLinuxPick(candidate: LinuxPickerCandidate): Promise<void> {
    pickerError = null;
    pickerPending = true;
    try {
      // The backend re-resolves the catalog against the requested
      // identifier and uses its own `display_name` / `icon_ref` as
      // the source of truth. The frontend MUST only forward the
      // opaque identifier so the picker payload never carries
      // application names, icon references or any other metadata
      // that could leak beyond the catalog itself.
      const response = await ignoredAppLinuxAddCommand({
        identifier: candidate.identifier,
      });
      closeLinuxPicker();
      if (response.kind === "added" || response.kind === "updated") {
        syncEntriesWithPicker([response.entry]);
        actionMessage = "privacy.ignored.added";
        actionMessageParams = { name: describeEntryName(response.entry) };
      }
    } catch (error) {
      pickerError = describeError(error);
    } finally {
      pickerPending = false;
    }
  }

  function closeLinuxPicker(): void {
    for (const ref of Object.values(linuxPickerIconRefs)) {
      linuxPickerIconResolver.releaseFor(ref);
    }
    linuxPickerIconUrls = {};
    linuxPickerIconFailures = {};
    linuxPickerIconRefs = {};
    linuxPickerOpen = false;
    linuxPicker = null;
  }

  function handlePickResponse(response: PickAndAddResponse): void {
    switch (response.kind) {
      case "added":
        actionMessage = "privacy.ignored.added";
        actionMessageParams = { name: describeEntryName(response.entry) };
        syncEntriesWithPicker([response.entry]);
        break;
      case "updated":
        actionMessage = "privacy.ignored.already_listed";
        actionMessageParams = { name: describeEntryName(response.entry) };
        syncEntriesWithPicker([response.entry]);
        break;
      case "cancelled":
        break;
      case "error":
        pickerError = describePickError(response.reason);
        break;
    }
  }

  function syncEntriesWithPicker(updated: IgnoredAppEntry[]): void {
    const next = ignoredEntries.slice();
    for (const fresh of updated) {
      const idx = next.findIndex((existing) => existing.id === fresh.id);
      if (idx >= 0) {
        next[idx] = fresh;
      } else {
        next.push(fresh);
      }
    }
    next.sort((a, b) => a.id.localeCompare(b.id));
    ignoredEntries = next;
    if (settings) {
      settings = {
        ...settings,
        ignored_apps: next.map((row) => row.id),
      };
      onSettingsChanged(settings);
      recomputeBlacklistMatch();
    }
    void refreshIcons();
  }

  async function removeIgnored(entry: IgnoredAppEntry): Promise<void> {
    pickerError = null;
    try {
      const next = await ignoredAppsRemoveCommand({ id: entry.id });
      ignoredEntries = ignoredEntries.filter((row) => row.id !== entry.id);
      actionMessage = "privacy.ignored.removed";
      actionMessageParams = { name: describeEntryName(entry) };
      onSettingsChanged(next);
      recomputeBlacklistMatch();
      iconResolver.releaseFor(entry.id);
      iconUrls = { ...iconUrls };
      iconFailures = { ...iconFailures };
      delete iconUrls[entry.id];
      delete iconFailures[entry.id];
    } catch (error) {
      pickerError = describeError(error);
    }
  }

  async function refreshIcons(): Promise<void> {
    const nextUrls: Record<string, string> = { ...iconUrls };
    const nextFailures: Record<string, boolean> = { ...iconFailures };
    for (const entry of ignoredEntries) {
      if (!entry.icon_ref) continue;
      if (nextUrls[entry.id]) continue;
      const resolution = await iconResolver.resolve(entry.icon_ref);
      if (resolution.ok && resolution.url) {
        nextUrls[entry.id] = resolution.url;
        delete nextFailures[entry.id];
      } else {
        nextFailures[entry.id] = true;
        delete nextUrls[entry.id];
      }
    }
    iconUrls = nextUrls;
    iconFailures = nextFailures;
  }

  function describeEntryName(entry: IgnoredAppEntry): string {
    return entry.display_name || entry.id;
  }

  function describePickError(reason: PickErrorReason): string {
    switch (reason) {
      case "cancelled":
        return "privacy.error.unknown";
      case "invalid_selection":
        return "privacy.error.selection_invalid";
      case "missing_identifier":
        return "privacy.error.identifier_missing";
      case "backend_unavailable":
        return "privacy.error.picker_unavailable";
      case "unsupported_session":
        return "privacy.error.picker_unsupported";
      case "persistence_error":
        return "privacy.error.unknown";
      default:
        return "privacy.error.unknown";
    }
  }

  function describeLinuxCatalogUnavailable(reason: string): string {
    if (reason === "linux picker catalog is empty for this session") {
      return "privacy.error.no_linux_apps";
    }
    return "privacy.error.linux_catalog";
  }

  function describeError(error: unknown): string {
    if (!error) return "privacy.error.unknown";
    if (typeof error === "object" && error) {
      const candidate = error as {
        message?: unknown;
        code?: unknown;
        kind?: unknown;
      };
      if (candidate.kind === "validation_error") {
        if (typeof candidate.code === "string") {
          if (
            candidate.code === "invalid_peer_display_name" ||
            candidate.code === "peer_display_name_too_long"
          ) {
            return describePeerDisplayName(candidate.code);
          }
        }
      }
    }
    return "privacy.error.unknown";
  }

  function describeBackend(value: string): string {
    switch (value) {
      case "macos_workspace":
        return "macOS NSWorkspace";
      case "x11_ewmh":
        return "X11 EWMH";
      case "unavailable":
        return "common.unavailable";
      default:
        return value;
    }
  }

  function describeRefresh(
    outcome: ActiveAppDiagnostics["refresh_outcome"],
  ): { key: string; causeKey?: string } {
    switch (outcome.kind) {
      case "pending":
        return { key: "privacy.cache.pending" };
      case "ok":
        return { key: "privacy.cache.ok" };
      case "failed":
        return {
          key: "privacy.cache.failed",
          causeKey: describeFailureKind(outcome.failure_kind),
        };
    }
  }

  function describeFailureKind(
    kind: ActiveAppDiagnostics["failure_kind"],
  ): string {
    switch (kind) {
      case "schedule":
        return "privacy.cache.failure.schedule";
      case "timeout":
        return "privacy.cache.failure.timeout";
      case "unavailable":
        return "privacy.cache.failure.unavailable";
      case "backend":
        return "privacy.cache.failure.backend";
      case null:
        return "privacy.cache.failure.unknown";
      default:
        return "privacy.cache.failure.unknown";
    }
  }

  function describeCache(diag: ActiveAppDiagnostics | null): { key: string; params?: Record<string, string | number | Date> } {
    if (!diag) return { key: "privacy.cache.no_diagnostics" };
    if (!diag.available) {
      return { key: "privacy.cache.unavailable" };
    }
    if (!diag.cache_populated || !diag.identifier) {
      return { key: "privacy.cache.empty" };
    }
    return {
      key: "privacy.cache.observed",
      params: { identifier: diag.identifier, name: diag.name ? ` (${diag.name})` : "" },
    };
  }

  function describeBlacklistMatch(
    match: boolean | null,
    diag: ActiveAppDiagnostics | null,
  ): string {
    if (!diag || !diag.identifier || match === null) {
      return "privacy.cache.match_check";
    }
    if (match) {
      return "privacy.cache.will_ignore";
    }
    return "privacy.cache.will_allow";
  }

  function describeLoop(diag: ActiveAppDiagnostics | null): string {
    if (!diag) return "privacy.cache.no_diagnostics";
    if (!diag.loop_started) return "privacy.cache.loop_not_started";
    return "privacy.cache.loop_running";
  }

  function describeCounters(diag: ActiveAppDiagnostics | null): { key: string; params?: Record<string, string | number | Date> } {
    if (!diag) return { key: "privacy.cache.no_diagnostics" };
    return {
      key: "privacy.cache.counters_value",
      params: { attempts: diag.refresh_attempts, successes: diag.successful_refreshes, failures: diag.failed_refreshes },
    };
  }

  function describeLastDecision(diag: ActiveAppDiagnostics | null): { key: string; params?: Record<string, string | number | Date> } {
    if (!diag) return { key: "privacy.cache.no_diagnostics" };
    if (!diag.last_capture_decision) {
      return { key: "privacy.cache.no_capture_decision" };
    }
    return { key: "privacy.cache.last_capture_decision", params: { decision: diag.last_capture_decision } };
  }

  function describeLastRefreshAt(diag: ActiveAppDiagnostics | null): { key: string; params?: Record<string, string | number | Date> } {
    if (!diag || diag.last_refresh_unix_ms == null) {
      return { key: "privacy.cache.no_refresh_attempt" };
    }
    try {
      const date = new Date(diag.last_refresh_unix_ms);
      return { key: "privacy.cache.last_refresh_attempt", params: { date } };
    } catch {
      return { key: "privacy.cache.last_refresh_epoch", params: { timestamp: diag.last_refresh_unix_ms } };
    }
  }

  onMount(async () => {
    await refresh();
    await refreshDiagnostics();
  });

  onDestroy(() => {
    closeLinuxPicker();
    iconResolver.release();
    linuxPickerIconResolver.release();
    iconUrls = {};
    iconFailures = {};
  });

  $: settings, recomputeBlacklistMatch();
</script>

<section class="privacy" data-testid="privacy-modal">
  <article data-testid="local-identity-card">
    <h3>{$t("privacy.identity.title")}</h3>
    <p class="muted">
      {$t("privacy.identity.description")}
    </p>
    {#if identityLoading}
      <p class="muted" data-testid="local-identity-loading">
        {$t("privacy.identity.loading")}
      </p>
    {:else if identityUnavailable}
      <p
        class="muted"
        role="status"
        aria-live="polite"
        data-testid="local-identity-unavailable"
      >
        {$t("privacy.identity.unavailable", { detail: identityUnavailable })}
      </p>
    {:else if identityProfile}
      <dl class="diagnostics" data-testid="local-identity-profile">
        <dt>{$t("privacy.peer_id")}</dt>
        <dd data-testid="local-identity-peer-id">
          {identityProfile.peer_id}
        </dd>
        <dt>{$t("privacy.identity.fingerprint")}</dt>
        <dd data-testid="local-identity-fingerprint">
          {identityProfile.fingerprint}
        </dd>
      </dl>
    {/if}
    <form
      class="identity-form"
      on:submit|preventDefault={saveIdentityName}
      data-testid="local-identity-form"
    >
      <label for="local-identity-name">{$t("privacy.identity.display_name")}</label>
      <input
        id="local-identity-name"
        name="local-peer-display-name"
        type="text"
        bind:value={identityDraft}
        maxlength="64"
        disabled={identitySaving}
        aria-invalid={identityError !== null}
        aria-describedby={identityError ? "local-identity-error" : undefined}
        data-testid="local-identity-name-input"
      />
      <button
        type="submit"
        disabled={identitySaving}
        aria-busy={identitySaving}
        data-testid="local-identity-save"
      >
        {identitySaving ? $t("privacy.identity.saving") : $t("privacy.identity.save_name")}
      </button>
      <button
        type="button"
        class="secondary"
        on:click={() => {
          identityDraft = "";
          void saveIdentityName();
        }}
        disabled={identitySaving}
        data-testid="local-identity-clear"
      >
        {$t("privacy.identity.clear_name")}
      </button>
    </form>
    {#if identityError}
      <p
        class="error"
        role="alert"
        id="local-identity-error"
        data-testid="local-identity-error"
      >
        {$t(identityError)}
      </p>
    {/if}
  </article>

  <article data-testid="privacy-blacklist-card">
    <h3>{$t("privacy.ignored.title")}</h3>
    <p class="muted">
      {$t("privacy.ignored.description")}
    </p>
    <div class="row">
      <button
        type="button"
        on:click={pickAndAddIgnored}
        disabled={pickerPending}
        aria-busy={pickerPending}
        data-testid="blacklist-pick-button"
      >
        {pickerPending ? $t("privacy.ignored.selecting") : $t("privacy.ignored.select")}
      </button>
      <button
        type="button"
        class="secondary"
        on:click={refreshIgnored}
        data-testid="blacklist-refresh"
      >
        {$t("privacy.diagnostics.refresh")}
      </button>
    </div>
    {#if pickerError}
      <p class="error" role="alert" data-testid="blacklist-error">
        {$t(pickerError)}
      </p>
    {/if}
    {#if ignoredEntries.length === 0}
      <p class="muted" data-testid="blacklist-empty">
        {$t("privacy.ignored.empty")}
      </p>
    {:else}
      <ul class="ignored-list" aria-label={$t("privacy.ignored.list")} data-testid="blacklist-list">
        {#each ignoredEntries as entry (entry.id)}
          {@const iconUrl = iconUrls[entry.id]}
          <li data-testid="blacklist-row">
            <span class="icon-cell" data-testid="blacklist-icon-cell">
              {#if iconUrl}
                <img
                  class="icon-image"
                  src={iconUrl}
                  alt=""
                  aria-hidden="true"
                  data-testid="blacklist-icon-image"
                  on:error={() => {
                    iconResolver.releaseFor(entry.id);
                    iconUrls = { ...iconUrls };
                    delete iconUrls[entry.id];
                    iconFailures = { ...iconFailures, [entry.id]: true };
                  }}
                />
              {:else}
                <span
                  class="icon-fallback"
                  class:muted={!entry.icon_ref || iconFailures[entry.id]}
                  aria-hidden="true"
                  data-testid="blacklist-icon-fallback"
                >
                  {describeEntryName(entry).slice(0, 1).toUpperCase()}
                </span>
              {/if}
            </span>
            <span class="name-cell" data-testid="blacklist-entry">
              {describeEntryName(entry)}
            </span>
            <button
              type="button"
              class="secondary"
              title={entry.id}
              aria-label={$t("privacy.ignored.remove_aria", { name: describeEntryName(entry), id: entry.id })}
              on:click={() => removeIgnored(entry)}
              data-testid="blacklist-remove"
            >
              {$t("privacy.ignored.remove")}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </article>

  {#if linuxPickerOpen && linuxPicker && linuxPicker.kind === "supported"}
    <div
      class="modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="linux-picker-title"
      data-testid="linux-picker-modal"
    >
      <div class="modal" data-testid="linux-picker-modal-content">
        <h3 id="linux-picker-title">{$t("privacy.picker.title")}</h3>
        <p class="muted">
          {#if linuxPicker.strategy === "desktop_file_id"}
            {$t("privacy.picker.strategy_desktop")}
          {:else}
            {$t("privacy.picker.strategy_wmclass")}
          {/if}
        </p>
        {#if linuxPickerLoading}
          <p class="muted">{$t("privacy.picker.loading")}</p>
        {:else if linuxPicker.candidates.length === 0}
          <p class="muted" data-testid="linux-picker-empty">
            {$t("privacy.picker.empty")}
          </p>
        {:else}
          <ul class="ignored-list" data-testid="linux-picker-list">
            {#each linuxPicker.candidates as candidate (candidate.identifier)}
              {@const iconUrl = linuxPickerIconUrls[candidate.identifier]}
              {@const iconFailed = linuxPickerIconFailures[candidate.identifier]}
              <li data-testid="linux-picker-row">
                <button
                  type="button"
                  class="link-button"
                  on:click={() => confirmLinuxPick(candidate)}
                  disabled={pickerPending}
                  aria-busy={pickerPending}
                  data-testid="linux-picker-confirm"
                  aria-label={$t("privacy.picker.add_aria", { name: candidate.display_name ?? candidate.identifier })}
                >
                  <span class="icon-cell">
                    {#if iconUrl}
                      <img
                        class="icon-image"
                        src={iconUrl}
                        alt=""
                        aria-hidden="true"
                        data-testid="linux-picker-icon-image"
                        on:error={() => {
                          if (candidate.icon_ref) {
                            linuxPickerIconResolver.releaseFor(candidate.icon_ref);
                          }
                          linuxPickerIconUrls = { ...linuxPickerIconUrls };
                          delete linuxPickerIconUrls[candidate.identifier];
                          linuxPickerIconRefs = { ...linuxPickerIconRefs };
                          delete linuxPickerIconRefs[candidate.identifier];
                          linuxPickerIconFailures = {
                            ...linuxPickerIconFailures,
                            [candidate.identifier]: true,
                          };
                        }}
                      />
                    {:else}
                      <span
                        class="icon-fallback"
                        class:muted={!candidate.icon_ref || iconFailed}
                        aria-hidden="true"
                        data-testid="linux-picker-fallback"
                      >
                        {(candidate.display_name ?? candidate.identifier)
                          .slice(0, 1)
                          .toUpperCase()}
                      </span>
                    {/if}
                  </span>
                  <span class="name-cell" data-testid="linux-picker-name">
                    {candidate.display_name ?? candidate.identifier}
                  </span>
                  <span class="muted identifier-cell" data-testid="linux-picker-identifier">
                    {candidate.identifier}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        {#if linuxPickerError}
          <p class="error" role="alert" data-testid="linux-picker-error">
            {$t(linuxPickerError)}
          </p>
        {/if}
        <div class="row">
          <button
            type="button"
            class="secondary"
            on:click={closeLinuxPicker}
            data-testid="linux-picker-cancel"
          >
            {$t("privacy.picker.cancel")}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <article data-testid="privacy-diagnostics-card">
    <h3>{$t("privacy.diagnostics.title")}</h3>
    <p class="muted">
      {$t("privacy.diagnostics.description")}
    </p>
    {#if diagnosticsError}
      <p class="error" role="alert" data-testid="diagnostics-error">
        {$t(diagnosticsError)}
      </p>
    {:else if diagnostics}
      {@const cacheCopy = describeCache(diagnostics)}
      {@const refreshCopy = describeRefresh(diagnostics.refresh_outcome)}
      {@const countersCopy = describeCounters(diagnostics)}
      {@const decisionCopy = describeLastDecision(diagnostics)}
      {@const refreshAttemptCopy = describeLastRefreshAt(diagnostics)}
      <dl class="diagnostics">
        <dt>{$t("privacy.diagnostics.adapter")}</dt>
        <dd data-testid="diagnostics-backend">
          {$t(describeBackend(diagnostics.backend))}
        </dd>
        <dt>{$t("privacy.diagnostics.state")}</dt>
        <dd data-testid="diagnostics-cache">
          {$t(cacheCopy.key, cacheCopy.params)}
        </dd>
        <dt>{$t("privacy.diagnostics.refresh")}</dt>
        <dd data-testid="diagnostics-refresh">
          {$t(refreshCopy.key, { cause: refreshCopy.causeKey ? $t(refreshCopy.causeKey) : "" })}
        </dd>
        {#if diagnostics.refresh_outcome.kind === "failed"}
          <dt>{$t("privacy.diagnostics.cause")}</dt>
          <dd data-testid="diagnostics-failure-kind">
            {$t(describeFailureKind(diagnostics.failure_kind))}
          </dd>
        {/if}
        <dt>{$t("privacy.diagnostics.loop")}</dt>
        <dd data-testid="diagnostics-loop">{$t(describeLoop(diagnostics))}</dd>
        <dt>{$t("privacy.diagnostics.counters")}</dt>
        <dd data-testid="diagnostics-counters">
          {$t(countersCopy.key, countersCopy.params)}
        </dd>
        <dt>{$t("privacy.diagnostics.last_decision")}</dt>
        <dd data-testid="diagnostics-decision">
          {$t(decisionCopy.key, decisionCopy.params)}
        </dd>
        <dt>{$t("privacy.diagnostics.last_attempt")}</dt>
        <dd data-testid="diagnostics-last-refresh">
          {$t(refreshAttemptCopy.key, refreshAttemptCopy.params)}
        </dd>
        <dt>{$t("privacy.diagnostics.ignored_list")}</dt>
        <dd data-testid="diagnostics-blacklist-match">
          {$t(describeBlacklistMatch(lastBlacklistMatch, diagnostics))}
        </dd>
      </dl>
    {:else}
      <p class="muted" data-testid="diagnostics-empty">
        {$t("privacy.diagnostics.not_consulted")}
      </p>
    {/if}
    <div class="row">
      <button
        type="button"
        on:click={refreshDiagnostics}
        disabled={refreshingDiagnostics}
        data-testid="diagnostics-refresh-button"
      >
        {refreshingDiagnostics ? $t("privacy.diagnostics.refreshing") : $t("privacy.diagnostics.refresh_action")}
      </button>
      <button
        type="button"
        class="secondary"
        on:click={consultDiagnostics}
        disabled={refreshingDiagnostics}
        data-testid="diagnostics-consult-button"
      >
        {$t("privacy.diagnostics.consult")}
      </button>
    </div>
  </article>

  {#if loading}
    <p class="muted">{$t("privacy.diagnostics.loading_settings")}</p>
  {:else if !settings}
    <p class="error">{$t("privacy.diagnostics.load_error")}</p>
  {/if}
  {#if actionMessage}
    <p class="ok" role="status" data-testid="action-message">{$t(actionMessage, actionMessageParams)}</p>
  {/if}
</section>

<style>
  .privacy {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  .privacy article {
    background: var(--cv-bg-surface, #0e1116);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 0.85rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .privacy h3 {
    margin: 0;
    font-size: var(--cv-title-sm, 0.95rem);
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .muted {
    color: var(--cv-fg-muted, #94a3b8);
  }
  .error {
    color: var(--cv-fg-error, #f87171);
    margin: 0;
  }
  .ok {
    color: var(--cv-fg-ok, #0a7e2c);
    margin: 0;
  }
  .ignored-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .ignored-list li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.25rem 0;
  }
  .icon-cell {
    width: 1.75rem;
    height: 1.75rem;
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.06);
    overflow: hidden;
  }
  .icon-image {
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
  }
  .icon-fallback {
    width: 100%;
    height: 100%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
    color: #cbd5f5;
    background: #1f2937;
  }
  .icon-fallback.muted {
    color: var(--cv-fg-muted, #94a3b8);
  }
  .name-cell {
    flex: 1 1 auto;
    word-break: break-word;
  }
  .diagnostics {
    display: grid;
    grid-template-columns: max-content 1fr;
    column-gap: 0.75rem;
    row-gap: 0.25rem;
    margin: 0;
  }
  .diagnostics dt {
    font-weight: 600;
  }
  .diagnostics dd {
    margin: 0;
    word-break: break-all;
  }
  .identity-form {
    display: flex;
    align-items: flex-end;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .identity-form label {
    flex: 0 0 auto;
    font-weight: 600;
  }
  .identity-form input {
    flex: 1 1 12rem;
    padding: 0.4rem 0.6rem;
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-sm, 6px);
    background: var(--cv-input-bg, #0b0f14);
    color: inherit;
  }
  .identity-form input[aria-invalid="true"] {
    border-color: var(--cv-fg-error, #f87171);
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    background: var(--cv-bg-surface, #0e1116);
    color: var(--cv-fg, #f8fafc);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 1rem;
    width: min(36rem, 90vw);
    max-height: 80vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .link-button {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.25rem 0;
    text-align: left;
  }
  .link-button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }
  .identifier-cell {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85em;
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

  button.secondary {
    background: #1f2937;
  }

  button:disabled {
    background: #374151;
    color: #94a3b8;
    cursor: not-allowed;
  }

  button:hover:not(:disabled) {
    background: var(--cv-accent-hover, #1d4ed8);
  }
  button.secondary:hover:not(:disabled) {
    background: #374151;
  }
</style>
