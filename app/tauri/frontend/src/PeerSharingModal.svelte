<script lang="ts">
  /**
   * Local peer sharing + Equipos modal.
   *
   * The modal hosts the opt-in `Compartir en red local` toggle and
   * the read-only `Equipos` view that lists every other
   * ClipVault installation the runtime has observed. The runtime
   * is metadata-only: no clipboard content, no IP, no port, no
   * raw public key, no preview — only the validated wire shape
   * the runtime persists in `known_peers`.
   *
   * The modal keeps the exact Tauri contract the previous privacy
   * surface used (`clipvault_peer_sharing_toggle_*`,
   * `clipvault_peer_snapshot`,
   * `clipvault_peer_sharing_refresh_identity`). It is
   * self-contained: every async refresh lives here and the
   * parent only sees the typed `onSettingsChanged` callback the
   * rest of the app uses to keep the rail / cards in sync.
   */
  import { onDestroy, onMount } from "svelte";
  import {
    peerSharingRefreshIdentityCommand,
    peerSharingToggleGetCommand,
    peerSharingToggleSetCommand,
    peerSnapshotCommand,
  } from "./lib/tauri";
  import type {
    PeerPresence,
    PeerSharingToggleResponse,
    PeerSnapshot,
    PeerSnapshotEntry,
    Settings,
  } from "./types";

  export let onSettingsChanged: (settings: Settings) => void = () => {};

  let loading = true;
  let toggle: PeerSharingToggleResponse | null = null;
  let snapshot: PeerSnapshot | null = null;
  let toggleBusy = false;
  let refreshingIdentity = false;
  let actionMessage: string | null = null;
  let errorMessage: string | null = null;

  async function refresh(): Promise<void> {
    loading = true;
    try {
      const [toggleValue, snapshotValue] = await Promise.all([
        peerSharingToggleGetCommand(),
        peerSnapshotCommand(),
      ]);
      toggle = toggleValue;
      snapshot = snapshotValue;
    } catch (error) {
      errorMessage = describeError(error);
    } finally {
      loading = false;
    }
  }

  async function refreshSnapshot(): Promise<void> {
    try {
      snapshot = await peerSnapshotCommand();
    } catch (error) {
      errorMessage = describeError(error);
    }
  }

  async function toggleSharing(target: boolean): Promise<void> {
    if (toggleBusy) return;
    toggleBusy = true;
    errorMessage = null;
    actionMessage = null;
    try {
      const response = await peerSharingToggleSetCommand({ enabled: target });
      toggle = response;
      // The runtime drives start / stop in lockstep with the
      // toggle; refresh the snapshot so the `Equipos` view
      // reflects the new active / inactive state immediately.
      await refreshSnapshot();
      actionMessage = target
        ? "Compartir en red local activado."
        : "Compartir en red local desactivado.";
    } catch (error) {
      errorMessage = describeError(error);
    } finally {
      toggleBusy = false;
    }
  }

  async function refreshIdentity(): Promise<void> {
    if (refreshingIdentity) return;
    refreshingIdentity = true;
    errorMessage = null;
    try {
      const response = await peerSharingRefreshIdentityCommand();
      toggle = response;
      await refreshSnapshot();
      actionMessage = "Identidad local reintentada.";
    } catch (error) {
      errorMessage = describeError(error);
    } finally {
      refreshingIdentity = false;
    }
  }

  function describeBackendKind(kind: PeerSharingToggleResponse["kind"]): string {
    switch (kind) {
      case "active":
        return "Navegando";
      case "identity_unavailable":
        return "Identidad no disponible";
      case "runtime_stopped":
        return "Sin red local";
      default:
        return `Estado desconocido (${kind})`;
    }
  }

  function describePresence(presence: PeerPresence): string {
    switch (presence) {
      case "detected":
        return "Detectado";
      case "not_available":
        return "No disponible";
      case "unverified":
        return "No verificado";
      default:
        return `Estado desconocido (${presence})`;
    }
  }

  function describeEntry(entry: PeerSnapshotEntry): string {
    return entry.display_name || entry.peer_id;
  }

  function describeError(error: unknown): string {
    if (!error) return "Error desconocido.";
    if (typeof error === "string") return error;
    if (typeof error === "object" && error) {
      const candidate = error as { message?: unknown; kind?: unknown };
      if (typeof candidate.message === "string") {
        return candidate.message;
      }
      if (typeof candidate.kind === "string") {
        return candidate.kind;
      }
    }
    return "Error desconocido.";
  }

  function isToggleOn(value: PeerSharingToggleResponse | null): boolean {
    if (!value) return false;
    // Only report the toggle as "on" when the runtime actually
    // started browsing the LAN. The `enabled` flag mirrors the
    // persisted preference but the runtime may refuse to start
    // (identity unavailable, multicast blocked, …) — the toggle
    // MUST stay visually off in that case so the user sees the
    // typed status copy the runtime surfaces.
    if (value.kind === "active") return true;
    return false;
  }

  function describeBackend(value: PeerSharingToggleResponse["kind"]): string {
    return describeBackendKind(value);
  }

  function shortFingerprint(fingerprint: string): string {
    if (fingerprint.length <= 8) return fingerprint;
    return `${fingerprint.slice(0, 4)}…${fingerprint.slice(-4)}`;
  }

  onMount(async () => {
    await refresh();
  });

  onDestroy(() => {
    toggleBusy = false;
    refreshingIdentity = false;
  });

  $: toggle, onSettingsChanged?.({} as Settings);
</script>

<section class="peer-sharing" data-testid="peer-sharing-modal">
  <article data-testid="peer-sharing-toggle-card">
    <h3>Compartir en red local</h3>
    <p class="muted">
      Activa el descubrimiento opcional de equipos en la misma red.
      ClipVault solo intercambia metadatos públicos (identificador,
      huella, nombre visible, versión). Nunca comparte el contenido
      del portapapeles ni abre una conexión TCP de aplicación; el
      pareado, la importación y el historial remoto vendrán en
      próximos cambios.
    </p>
    <div class="row">
      <label class="toggle">
        <input
          type="checkbox"
          data-testid="peer-sharing-toggle"
          checked={isToggleOn(toggle)}
          disabled={toggleBusy}
          on:change={(event) => {
            const target = (event.currentTarget as HTMLInputElement).checked;
            void toggleSharing(target);
          }}
        />
        <span data-testid="peer-sharing-toggle-state">
          {isToggleOn(toggle) ? "Activado" : "Desactivado"}
        </span>
      </label>
      <button
        type="button"
        class="secondary"
        data-testid="peer-sharing-refresh-identity"
        on:click={refreshIdentity}
        disabled={refreshingIdentity}
      >
        {refreshingIdentity ? "Reintentando…" : "Reintentar identidad"}
      </button>
    </div>
    {#if toggle}
      <p
        class="muted"
        data-testid="peer-sharing-backend"
        data-presence={toggle.kind}
      >
        Estado del runtime: {describeBackend(toggle.kind)}.
      </p>
    {/if}
    {#if errorMessage}
      <p class="error" role="alert" data-testid="peer-sharing-error">
        {errorMessage}
      </p>
    {/if}
    {#if actionMessage}
      <p class="ok" role="status" data-testid="peer-sharing-action">
        {actionMessage}
      </p>
    {/if}
  </article>

  <article data-testid="peer-equipos-card">
    <h3>Equipos</h3>
    <p class="muted">
      Lista de solo lectura de los equipos ClipVault observados en
      este segmento. La vista no ofrece acciones de pareado ni de
      historial; sólo metadatos públicos.
    </p>
    {#if loading}
      <p class="muted" data-testid="peer-equipos-loading">Cargando…</p>
    {:else if !snapshot}
      <p class="error">No se pudo cargar la lista.</p>
    {:else if snapshot.entries.length === 0}
      <p class="muted" data-testid="peer-equipos-empty">
        Aún no se han observado equipos. Activa el descubrimiento y
        espera a que otra instalación de ClipVault se anuncie.
      </p>
    {:else}
      <p class="muted" data-testid="peer-equipos-count">
        {snapshot.entries.length} pares observados.
      </p>
      <ul class="peer-list" aria-label="Equipos detectados" data-testid="peer-equipos-list">
        {#each snapshot.entries as entry (entry.peer_id)}
          <li data-testid="peer-equipos-row">
            <span class="name-cell" data-testid="peer-equipos-name">
              {describeEntry(entry)}
            </span>
            <span class="muted" data-testid="peer-equipos-peer-id">
              peer_id: {entry.peer_id}
            </span>
            <span class="muted" data-testid="peer-equipos-fingerprint">
              Huella: {shortFingerprint(entry.public_key_fingerprint)}
            </span>
            <span
              class="presence"
              data-testid="peer-equipos-presence"
              data-presence={entry.presence}
            >
              {describePresence(entry.presence)}
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  </article>
</section>

<style>
  .peer-sharing {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  .peer-sharing article {
    background: var(--cv-bg-surface, #0e1116);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 0.85rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .peer-sharing h3 {
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
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  .peer-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .peer-list li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.25rem 0.75rem;
    padding: 0.5rem 0.65rem;
    background: rgba(255, 255, 255, 0.04);
    border-radius: var(--cv-radius-sm, 6px);
  }
  .peer-list .name-cell {
    font-weight: 600;
  }
  .peer-list .muted {
    font-size: 0.85em;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    color: var(--cv-fg-muted, #94a3b8);
    word-break: break-all;
  }
  .peer-list .presence {
    align-self: center;
    font-size: 0.85em;
    padding: 0.15rem 0.5rem;
    border-radius: 999px;
    background: rgba(37, 99, 235, 0.25);
    color: var(--cv-fg, #f8fafc);
  }
  .peer-list .presence[data-presence="not_available"] {
    background: rgba(148, 163, 184, 0.25);
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