<script lang="ts">
  /**
   * Modal hosting the privacy controls (blacklist + active-app
   * diagnostics). The original settings panel also surfaced the
   * retention selector; that control has moved to the **Retención**
   * modal, so this view is the privacy-only slice of the old panel.
   *
   * The component keeps the exact Tauri contract the inline panel
   * used (`clipvault_settings_get`, `clipvault_ignored_app_*`,
   * `clipvault_active_app_*`). It is self-contained: every
   * async refresh lives here and the parent only sees the typed
   * `onSettingsChanged` callback the rest of the app uses to keep
   * the rail / cards in sync.
   */
  import { onDestroy, onMount } from "svelte";
  import {
    activeAppDiagnosticsCommand,
    ignoredAppIconCommand,
    ignoredAppPickAndAddCommand,
    ignoredAppsListWithMetadataCommand,
    ignoredAppsRemoveCommand,
    refreshActiveAppDiagnosticsCommand,
    settingsGetCommand,
  } from "./lib/tauri";
  import type {
    ActiveAppDiagnostics,
    IgnoredAppEntry,
    PickAndAddResponse,
    PickErrorReason,
    Settings,
  } from "./types";
  import {
    createIconResolver,
    type IconLoader,
    type IconResolver,
  } from "./lib/iconResolver";

  export let onSettingsChanged: (settings: Settings) => void = () => {};

  let settings: Settings | null = null;
  let ignoredEntries: IgnoredAppEntry[] = [];
  let pickerError: string | null = null;
  let actionMessage: string | null = null;
  let loading = true;
  let diagnostics: ActiveAppDiagnostics | null = null;
  let diagnosticsError: string | null = null;
  let refreshingDiagnostics = false;
  let lastBlacklistMatch: boolean | null = null;
  let pickerPending = false;
  let iconUrls: Record<string, string> = {};
  let iconFailures: Record<string, boolean> = {};

  const tauriIconLoader: IconLoader = {
    async loadIconBytes(ref) {
      try {
        return await ignoredAppIconCommand({ ref });
      } catch {
        return null;
      }
    },
  };
  const iconResolver: IconResolver = createIconResolver(tauriIconLoader);

  async function refresh(): Promise<void> {
    loading = true;
    try {
      settings = await settingsGetCommand();
      await refreshIgnored();
      await refreshIcons();
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
      const response: PickAndAddResponse = await ignoredAppPickAndAddCommand();
      handlePickResponse(response);
    } catch (error) {
      pickerError = describeError(error);
    } finally {
      pickerPending = false;
    }
  }

  function handlePickResponse(response: PickAndAddResponse): void {
    switch (response.kind) {
      case "added":
        actionMessage = `Aplicación añadida: ${describeEntryName(response.entry)}.`;
        syncEntriesWithPicker([response.entry]);
        break;
      case "updated":
        actionMessage = `Aplicación ya estaba en la lista: ${describeEntryName(response.entry)}.`;
        syncEntriesWithPicker([response.entry]);
        break;
      case "cancelled":
        break;
      case "error":
        pickerError = describePickError(response.reason, response.message);
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
      actionMessage = `Aplicación eliminada: ${describeEntryName(entry)}.`;
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

  function describePickError(reason: PickErrorReason, fallback: string): string {
    switch (reason) {
      case "cancelled":
        return fallback;
      case "invalid_selection":
        return "La selección no es un bundle válido. Elige una aplicación instalada.";
      case "missing_identifier":
        return "El bundle no expone un identificador utilizable. Selecciona otra aplicación.";
      case "backend_unavailable":
        return "El selector nativo no está disponible en esta sesión.";
      case "unsupported_session":
        return "La selección visual no está disponible todavía en esta plataforma. Puedes seguir añadiendo identificadores manualmente.";
      case "persistence_error":
        return `No se pudo guardar la selección: ${fallback}`;
      default:
        return fallback;
    }
  }

  function describeError(error: unknown): string {
    if (!error) return "Error desconocido.";
    if (typeof error === "string") return error;
    if (typeof error === "object" && error && "message" in error) {
      return String((error as { message: unknown }).message);
    }
    return "Error desconocido.";
  }

  function describeBackend(value: string): string {
    switch (value) {
      case "macos_workspace":
        return "macOS NSWorkspace";
      case "x11_ewmh":
        return "X11 EWMH";
      case "unavailable":
        return "no disponible";
      default:
        return value;
    }
  }

  function describeRefresh(
    outcome: ActiveAppDiagnostics["refresh_outcome"],
  ): { headline: string; detail: string } {
    switch (outcome.kind) {
      case "pending":
        return { headline: "Aún no se ha refrescado la caché.", detail: "" };
      case "ok":
        return { headline: "Último refresco correcto.", detail: "" };
      case "failed":
        return {
          headline: `Último refresco falló: ${describeFailureKind(
            outcome.failure_kind,
          )}`,
          detail: outcome.message,
        };
    }
  }

  function describeFailureKind(
    kind: ActiveAppDiagnostics["failure_kind"],
  ): string {
    switch (kind) {
      case "schedule":
        return "El planificador de Tauri rechazó encolar el cierre.";
      case "timeout":
        return "El hilo principal no ejecutó el cierre dentro del tiempo límite.";
      case "unavailable":
        return "La sonda de plataforma no puede ejecutarse en esta sesión.";
      case "backend":
        return "La sonda de plataforma devolvió un error.";
      case null:
        return "Causa desconocida.";
      default:
        return `Causa desconocida (${kind}).`;
    }
  }

  function describeCache(diag: ActiveAppDiagnostics | null): string {
    if (!diag) return "Sin diagnóstico.";
    if (!diag.available) {
      return "El adaptador de aplicación activa no está disponible en esta sesión.";
    }
    if (!diag.cache_populated || !diag.identifier) {
      return "Caché vacía: el bucle todavía no ha visto una aplicación activa.";
    }
    return `Identificador observado: ${diag.identifier}${diag.name ? ` (${diag.name})` : ""}.`;
  }

  function describeBlacklistMatch(
    match: boolean | null,
    diag: ActiveAppDiagnostics | null,
  ): string {
    if (!diag || !diag.identifier || match === null) {
      return "Comprobará la lista ignorada cuando haya un identificador disponible.";
    }
    if (match) {
      return "El bucle va a descartar capturas de esta aplicación.";
    }
    return "El bucle va a permitir capturas de esta aplicación.";
  }

  function describeLoop(diag: ActiveAppDiagnostics | null): string {
    if (!diag) return "Sin diagnóstico.";
    if (!diag.loop_started) return "El bucle todavía no ha arrancado.";
    return "El bucle está en marcha.";
  }

  function describeCounters(diag: ActiveAppDiagnostics | null): string {
    if (!diag) return "Sin diagnóstico.";
    return `Intentos: ${diag.refresh_attempts} · Correctos: ${diag.successful_refreshes} · Fallidos: ${diag.failed_refreshes}`;
  }

  function describeLastDecision(diag: ActiveAppDiagnostics | null): string {
    if (!diag) return "Sin diagnóstico.";
    if (!diag.last_capture_decision) {
      return "El bucle todavía no ha evaluado una captura.";
    }
    return `Última decisión del bucle: ${diag.last_capture_decision}`;
  }

  function describeLastRefreshAt(diag: ActiveAppDiagnostics | null): string {
    if (!diag || diag.last_refresh_unix_ms == null) {
      return "El bucle todavía no ha intentado refrescar.";
    }
    try {
      const date = new Date(diag.last_refresh_unix_ms);
      return `Último intento de refresco: ${date.toLocaleString()}`;
    } catch {
      return `Último intento de refresco (epoch ms): ${diag.last_refresh_unix_ms}`;
    }
  }

  onMount(async () => {
    await refresh();
    await refreshDiagnostics();
  });

  onDestroy(() => {
    iconResolver.release();
    iconUrls = {};
    iconFailures = {};
  });

  $: settings, recomputeBlacklistMatch();
</script>

<section class="privacy" data-testid="privacy-modal">
  <article data-testid="privacy-blacklist-card">
    <h3>Aplicaciones ignoradas</h3>
    <p class="muted">
      Selecciona visualmente las aplicaciones cuyo contenido del portapapeles
      nunca se almacena. La lista es insensible a mayúsculas; el panel
      compara el identificador exacto que devuelve el adaptador antes de
      aceptar una captura.
    </p>
    <div class="row">
      <button
        type="button"
        on:click={pickAndAddIgnored}
        disabled={pickerPending}
        aria-busy={pickerPending}
        data-testid="blacklist-pick-button"
      >
        {pickerPending ? "Seleccionando…" : "Seleccionar aplicación"}
      </button>
      <button
        type="button"
        class="secondary"
        on:click={refreshIgnored}
        data-testid="blacklist-refresh"
      >
        Refrescar
      </button>
    </div>
    {#if pickerError}
      <p class="error" role="alert" data-testid="blacklist-error">
        {pickerError}
      </p>
    {/if}
    {#if ignoredEntries.length === 0}
      <p class="muted" data-testid="blacklist-empty">
        Aún no has añadido aplicaciones ignoradas.
      </p>
    {:else}
      <ul class="ignored-list" aria-label="Aplicaciones ignoradas" data-testid="blacklist-list">
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
              aria-label={`Eliminar ${describeEntryName(entry)} (${entry.id})`}
              on:click={() => removeIgnored(entry)}
              data-testid="blacklist-remove"
            >
              Eliminar
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </article>

  <article data-testid="privacy-diagnostics-card">
    <h3>Diagnóstico de la caché</h3>
    <p class="muted">
      ClipVault refresca la caché de la aplicación activa de forma
      síncrona antes de evaluar cada captura. Si la última prueba
      falló, la captura siguiente se trata como "origen
      desconocido" según el contrato del blacklist.
    </p>
    {#if diagnosticsError}
      <p class="error" role="alert" data-testid="diagnostics-error">
        {diagnosticsError}
      </p>
    {:else if diagnostics}
      <dl class="diagnostics">
        <dt>Adaptador</dt>
        <dd data-testid="diagnostics-backend">
          {describeBackend(diagnostics.backend)}
        </dd>
        <dt>Estado</dt>
        <dd data-testid="diagnostics-cache">{describeCache(diagnostics)}</dd>
        <dt>Refresco</dt>
        <dd data-testid="diagnostics-refresh">
          {(() => {
            const r = describeRefresh(diagnostics.refresh_outcome);
            return r.detail ? `${r.headline} (${r.detail})` : r.headline;
          })()}
        </dd>
        {#if diagnostics.refresh_outcome.kind === "failed"}
          <dt>Causa</dt>
          <dd data-testid="diagnostics-failure-kind">
            {describeFailureKind(diagnostics.failure_kind)}
          </dd>
        {/if}
        <dt>Bucle</dt>
        <dd data-testid="diagnostics-loop">{describeLoop(diagnostics)}</dd>
        <dt>Contadores</dt>
        <dd data-testid="diagnostics-counters">{describeCounters(diagnostics)}</dd>
        <dt>Última decisión</dt>
        <dd data-testid="diagnostics-decision">{describeLastDecision(diagnostics)}</dd>
        <dt>Último intento</dt>
        <dd data-testid="diagnostics-last-refresh">{describeLastRefreshAt(diagnostics)}</dd>
        <dt>Lista ignorada</dt>
        <dd data-testid="diagnostics-blacklist-match">
          {describeBlacklistMatch(lastBlacklistMatch, diagnostics)}
        </dd>
      </dl>
    {:else}
      <p class="muted" data-testid="diagnostics-empty">
        Aún no se consultó el diagnóstico.
      </p>
    {/if}
    <div class="row">
      <button
        type="button"
        on:click={refreshDiagnostics}
        disabled={refreshingDiagnostics}
        data-testid="diagnostics-refresh-button"
      >
        {refreshingDiagnostics ? "Refrescando…" : "Refrescar diagnóstico"}
      </button>
      <button
        type="button"
        class="secondary"
        on:click={consultDiagnostics}
        disabled={refreshingDiagnostics}
        data-testid="diagnostics-consult-button"
      >
        Consultar diagnóstico
      </button>
    </div>
  </article>

  {#if loading}
    <p class="muted">Cargando ajustes…</p>
  {:else if !settings}
    <p class="error">No se pudieron cargar los ajustes.</p>
  {/if}
  {#if actionMessage}
    <p class="ok" role="status" data-testid="action-message">{actionMessage}</p>
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