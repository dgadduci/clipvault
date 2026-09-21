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
   *
   * While the modal is mounted and the runtime is browsing the
   * LAN (`toggle.kind === "active"`) the modal polls the
   * metadata-only snapshot on a short interval so a fresh
   * `Observed` or `Removed` event the discovery worker drained
   * reaches the `Equipos` view without reopening the modal. The
   * frontend never touches SQLite, sockets, IPs, ports or mDNS
   * logic: the refresh always routes through
   * `peerSnapshotCommand`. The interval is cleared in
   * `onDestroy` so closing the modal halts polling immediately,
   * and `refreshSnapshot` is the single shared helper that owns
   * the round-trip plus a single-flight guard: every snapshot
   * read (mount-time refresh, post-toggle refresh, identity
   * retry and the polling tick) coalesces through it so a
   * previous in-flight request can never overlap a new one.
   *
   * Pairing actions (`Vincular` / `Desvincular` / `Bloquear` /
   * `Desbloquear`) live next to each `Equipos` row. The modal
   * never accepts an IP, a port, a TLS key, a SAS candidate or a
   * signature; the pairing runtime owns every byte that crosses
   * the trust boundary and the bridge exposes only the typed
   * `PeerPairingOutcomeResponse` variants. The accessible
   * pairing modal ([`PeerPairingModal.svelte`]) handles focus,
   * Escape and the two-minute timeout the runtime enforces.
   */
  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import {
    peerPairingBlockCommand,
    peerPairingRevokeCommand,
    peerPairingUnblockCommand,
    peerSharingRefreshIdentityCommand,
    peerSharingToggleGetCommand,
    peerSharingToggleSetCommand,
    peerSnapshotCommand,
  } from "./lib/tauri";
  import type {
    PeerPresence,
    PeerRow,
    PeerSharingToggleResponse,
    PeerSnapshot,
    PeerSnapshotEntry,
    PeerTrustState,
    Settings,
  } from "./types";

  export let onSettingsChanged: (settings: Settings) => void = () => {};

  const dispatch = createEventDispatcher<{ pairingRequested: PeerRow }>();

  /** Local trust-state mirror used to drive the per-row actions. */
  let trustStates: Record<string, PeerTrustState> = {};
  let pairingActionMessage: string | null = null;
  let pairingActionError: string | null = null;
  let pairingBusy = false;

  let loading = true;
  let toggle: PeerSharingToggleResponse | null = null;
  let snapshot: PeerSnapshot | null = null;
  let toggleBusy = false;
  let refreshingIdentity = false;
  let actionMessage: string | null = null;
  let errorMessage: string | null = null;

  // Polling cadence for the metadata-only snapshot. Kept short so
  // a fresh `Observed` / `Removed` event the worker drained from
  // the queue reflects on the `Equipos` view without reopening
  // the modal, and short enough to feel near-real-time without
  // burning the bridge.
  const PEER_SNAPSHOT_REFRESH_MS = 2_000;
  let snapshotTimer: ReturnType<typeof setInterval> | null = null;
  // Single-flight latch for the snapshot round-trip. The polling
  // tick, the post-toggle reload, the identity retry and the
  // mount-time refresh all funnel through `refreshSnapshot`; this
  // promise is shared while a request is in flight so the next
  // caller reuses it instead of opening a second
  // `peerSnapshotCommand()` while the previous one is still
  // pending.
  let snapshotRefreshPromise: Promise<void> | null = null;

  function startSnapshotRefresh(): void {
    if (snapshotTimer !== null) return;
    snapshotTimer = setInterval(() => {
      void refreshSnapshot();
    }, PEER_SNAPSHOT_REFRESH_MS);
  }

  function stopSnapshotRefresh(): void {
    if (snapshotTimer === null) return;
    clearInterval(snapshotTimer);
    snapshotTimer = null;
  }

  async function refresh(): Promise<void> {
    loading = true;
    try {
      // Toggle and snapshot are independent reads; run them in
      // parallel but always route the snapshot through the
      // shared single-flight helper so it cannot overlap a
      // tick that fired just before mount.
      const [, ] = await Promise.all([
        peerSharingToggleGetCommand().then((value) => {
          toggle = value;
        }),
        refreshSnapshot(),
      ]);
    } catch (error) {
      errorMessage = describeError(error);
    } finally {
      loading = false;
    }
  }

  async function refreshSnapshot(): Promise<void> {
    // Single-flight: if a previous round-trip is still pending,
    // return its promise so every snapshot read coalesces into
    // one bridge call. The polling tick, the post-toggle
    // reload, the identity retry and the mount-time refresh
    // all funnel through this helper — only this function may
    // call `peerSnapshotCommand()`. The IIFE clears the latch
    // in `finally` so a rejected refresh cannot lock it
    // forever and starve the next caller.
    if (snapshotRefreshPromise !== null) {
      return snapshotRefreshPromise;
    }
    snapshotRefreshPromise = (async () => {
      try {
        snapshot = await peerSnapshotCommand();
      } catch (error) {
        errorMessage = describeError(error);
      } finally {
        snapshotRefreshPromise = null;
      }
    })();
    return snapshotRefreshPromise;
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
      // Only claim the discovery actually started when the
      // runtime confirmed it is browsing. `identity_unavailable`
      // and `runtime_stopped` mean the toggle is persisted but
      // the runtime did NOT come up (secure store unreachable,
      // multicast blocked, …); a triumphant "activado" message
      // would lie to the user and hide the typed status copy
      // the runtime already surfaces.
      if (!target) {
        actionMessage = "Compartir en red local desactivado.";
      } else if (response.kind === "active") {
        actionMessage = "Compartir en red local activado.";
      } else if (response.kind === "identity_unavailable") {
        actionMessage =
          "Preferencia guardada, pero la identidad segura no está disponible: el descubrimiento no arrancó. Vuelve a intentarlo cuando el llavero esté accesible.";
      } else {
        actionMessage =
          "Preferencia guardada, pero el descubrimiento no pudo iniciar (multicast bloqueado o ruta no disponible). El estado del runtime se muestra arriba.";
      }
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

  function trustStateOf(peerId: string): PeerTrustState {
    return trustStates[peerId] ?? "unverified";
  }

  function describeTrustState(state: PeerTrustState): string {
    switch (state) {
      case "trusted":
        return "Activo";
      case "revoked":
        return "Desvinculado";
      case "blocked":
        return "Bloqueado";
      default:
        return "Sin pareado";
    }
  }

  function openPairingModal(entry: PeerSnapshotEntry): void {
    pairingActionMessage = null;
    pairingActionError = null;
    dispatch("pairingRequested", {
      peer_id: entry.peer_id,
      display_name: describeEntry(entry),
      short_fingerprint: shortFingerprint(entry.public_key_fingerprint),
      trust_state: trustStateOf(entry.peer_id),
      presence: entry.presence,
      paired_at: null,
      last_discovered_at: entry.last_discovered_at,
    });
  }

  async function revoke(entry: PeerSnapshotEntry): Promise<void> {
    if (pairingBusy) return;
    pairingBusy = true;
    pairingActionError = null;
    try {
      await peerPairingRevokeCommand({ peer_id: entry.peer_id });
      trustStates = { ...trustStates, [entry.peer_id]: "revoked" };
      pairingActionMessage = "Vínculo deshecho.";
    } catch (error) {
      pairingActionError = describeError(error);
    } finally {
      pairingBusy = false;
    }
  }

  async function block(entry: PeerSnapshotEntry): Promise<void> {
    if (pairingBusy) return;
    pairingBusy = true;
    pairingActionError = null;
    try {
      await peerPairingBlockCommand({ peer_id: entry.peer_id });
      trustStates = { ...trustStates, [entry.peer_id]: "blocked" };
      pairingActionMessage = "Equipo bloqueado.";
    } catch (error) {
      pairingActionError = describeError(error);
    } finally {
      pairingBusy = false;
    }
  }

  async function unblock(entry: PeerSnapshotEntry): Promise<void> {
    if (pairingBusy) return;
    pairingBusy = true;
    pairingActionError = null;
    try {
      await peerPairingUnblockCommand({ peer_id: entry.peer_id });
      trustStates = { ...trustStates, [entry.peer_id]: "unverified" };
      pairingActionMessage = "Equipo desbloqueado; el vínculo debe restablecerse manualmente.";
    } catch (error) {
      pairingActionError = describeError(error);
    } finally {
      pairingBusy = false;
    }
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
    // Halting the polling here is required: leaving the modal
    // open / closed without clearing the interval would keep the
    // bridge round-trip alive in the background. The shared
    // `Modal` shell only mounts the slot while `open === true`,
    // so the timer MUST stop when the modal unmounts.
    stopSnapshotRefresh();
  });

  $: toggle, onSettingsChanged?.({} as Settings);
  $: {
    // Only poll while the runtime is actually browsing the LAN.
    // `identity_unavailable` / `runtime_stopped` mean the toggle
    // is persisted but the worker is not surfacing new events,
    // so polling would only re-fetch a frozen snapshot.
    if (toggle !== null && toggle.kind === "active") {
      startSnapshotRefresh();
    } else {
      stopSnapshotRefresh();
    }
  }
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
            <span
              class="presence"
              data-testid="peer-equipos-trust-state"
              data-trust={trustStateOf(entry.peer_id)}
            >
              {describeTrustState(trustStateOf(entry.peer_id))}
            </span>
            <div class="peer-actions" data-testid="peer-equipos-actions">
              {#if trustStateOf(entry.peer_id) === "unverified"}
                <button
                  type="button"
                  class="secondary"
                  data-testid="peer-equipos-pair"
                  disabled={pairingBusy}
                  on:click={() => openPairingModal(entry)}
                >
                  Vincular
                </button>
                <button
                  type="button"
                  class="secondary"
                  data-testid="peer-equipos-block"
                  disabled={pairingBusy}
                  on:click={() => block(entry)}
                >
                  Bloquear
                </button>
              {:else if trustStateOf(entry.peer_id) === "trusted"}
                <button
                  type="button"
                  class="secondary"
                  data-testid="peer-equipos-revoke"
                  disabled={pairingBusy}
                  on:click={() => revoke(entry)}
                >
                  Desvincular
                </button>
                <button
                  type="button"
                  class="secondary"
                  data-testid="peer-equipos-block"
                  disabled={pairingBusy}
                  on:click={() => block(entry)}
                >
                  Bloquear
                </button>
              {:else if trustStateOf(entry.peer_id) === "revoked"}
                <button
                  type="button"
                  class="secondary"
                  data-testid="peer-equipos-pair"
                  disabled={pairingBusy}
                  on:click={() => openPairingModal(entry)}
                >
                  Volver a parear
                </button>
                <button
                  type="button"
                  class="secondary"
                  data-testid="peer-equipos-block"
                  disabled={pairingBusy}
                  on:click={() => block(entry)}
                >
                  Bloquear
                </button>
              {:else if trustStateOf(entry.peer_id) === "blocked"}
                <button
                  type="button"
                  class="secondary"
                  data-testid="peer-equipos-unblock"
                  disabled={pairingBusy}
                  on:click={() => unblock(entry)}
                >
                  Desbloquear
                </button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
      {#if pairingActionMessage}
        <p class="ok" role="status" data-testid="peer-pairing-action">
          {pairingActionMessage}
        </p>
      {/if}
      {#if pairingActionError}
        <p class="error" role="alert" data-testid="peer-pairing-error">
          {pairingActionError}
        </p>
      {/if}
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
