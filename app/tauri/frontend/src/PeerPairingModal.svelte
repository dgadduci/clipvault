<script lang="ts">
  /**
   * Local peer pairing modal.
   *
   * Drives the `local-peer-mutual-pairing` reciprocal approval
   * flow. The modal is intentionally accessible:
   *
   * - on open, focus moves to the `Aceptar` button so a screen
   *   reader announces the next action; `Vincular` is only
   *   enabled while the local session is still waiting for the
   *   remote approval;
   * - `Escape` cancels the session without leaking the runtime
   *   state machine;
   * - the modal restores focus to the originating trigger when
   *   it closes;
   * - a two-minute hard timeout mirrors
   *   [`clipvault_core::PAIRING_SESSION_TIMEOUT`] so the modal
   *   cannot stay open indefinitely even if the runtime never
   *   reports a `session_expired` outcome;
   * - the modal is metadata-only: the wire contract never
   *   accepts an IP, a port, a TLS key, a SAS candidate or a
   *   signature from the frontend; the pairing runtime owns
   *   every byte that crosses the trust boundary.
   *
   * The modal also detects an inbound pairing session the
   * remote peer opened through the same mDNS-detected channel.
   * When [`peerPairingSnapshotCommand`] returns a session for
   * the same `peer_id` BEFORE the local user clicks `Vincular`,
   * the modal renders the inbound metadata (SAS, peer_id,
   * fingerprint, display name, expiration) WITHOUT invoking
   * [`peerPairingStartCommand`] — the inbound session was
   * already created by the listener, so starting a new outbound
   * one would duplicate the dialog and race the inbound state
   * machine. The local approval flows through the same
   * [`peerPairingApproveLocalCommand`] the runtime routes to
   * [`PeerTransport::approve_inbound_session`] for inbound
   * sessions.
   *
   * Closing the modal — through Escape, the backdrop, the
   * `Cerrar` / `Cancelar` button, or by unmounting the slot
   * — cancels the active session so a stale inbound listener
   * does not stay pinned to a peer the user dismissed.
   */
  import { onDestroy, onMount, tick } from "svelte";

  import {
    peerPairingApproveLocalCommand,
    peerPairingCancelCommand,
    peerPairingSnapshotCommand,
    peerPairingStartCommand,
  } from "./lib/tauri";
  import type {
    PeerPairingOutcomeResponse,
    PeerPairingSessionSnapshot,
    PeerRow,
  } from "./types";

  export let open = false;
  export let row: PeerRow | null = null;
  /** Element to restore focus to when the modal closes. */
  export let trigger: HTMLElement | null = null;

  let session: PeerPairingSessionSnapshot | null = null;
  let outcome: PeerPairingOutcomeResponse | null = null;
  let lastError: string | null = null;
  let starting = false;
  let approving = false;
  let cancelling = false;
  let mountedAt: number | null = null;
  let refreshInterval: ReturnType<typeof setInterval> | null = null;
  let countdownInterval: ReturnType<typeof setInterval> | null = null;
  let secondsLeft = 0;
  /**
   * True when the modal is rendering an inbound session the
   * listener registered. The flag suppresses the auto-start
   * branch (the listener already owns the session) and routes
   * the local approval through the same `approve_local`
   * command the runtime maps to
   * [`PeerTransport::approve_inbound_session`].
   */
  let inboundMode = false;

  const SESSION_TIMEOUT_SECONDS = 120;

  async function refreshSnapshot(): Promise<void> {
    try {
      const snapshots = await peerPairingSnapshotCommand();
      const match = row
        ? snapshots.find((value) => value.remote_peer_id === row!.peer_id)
        : snapshots[0];
      session = match ?? null;
      secondsLeft = computeSecondsLeft();
    } catch (error) {
      lastError = describeError(error);
    }
  }

  function startRefresh(): void {
    if (refreshInterval !== null) {
      return;
    }
    refreshInterval = setInterval(() => {
      void refreshSnapshot();
    }, 1000);
  }

  function stopRefresh(): void {
    if (refreshInterval !== null) {
      clearInterval(refreshInterval);
      refreshInterval = null;
    }
    if (countdownInterval !== null) {
      clearInterval(countdownInterval);
      countdownInterval = null;
    }
  }

  function computeSecondsLeft(): number {
    if (!mountedAt) {
      return SESSION_TIMEOUT_SECONDS;
    }
    const elapsed = (Date.now() - mountedAt) / 1000;
    return Math.max(0, Math.ceil(SESSION_TIMEOUT_SECONDS - elapsed));
  }

  /**
   * Detect an inbound session for the current row BEFORE
   * auto-starting an outbound one. The runtime already keeps
   * the inbound session in its in-memory map; rendering it
   * avoids a second `start_outbound` round-trip that would
   * collide with the listener's bounded wait and double the
   * connection count on the wire.
   */
  async function detectInboundSession(): Promise<boolean> {
    if (!row) {
      return false;
    }
    try {
      const snapshots = await peerPairingSnapshotCommand();
      const inbound = snapshots.find(
        (value) => value.remote_peer_id === row!.peer_id,
      );
      if (!inbound) {
        return false;
      }
      session = inbound;
      inboundMode = true;
      mountedAt = Date.now();
      startRefresh();
      countdownInterval = setInterval(() => {
        secondsLeft = computeSecondsLeft();
        if (secondsLeft <= 0 && !session?.remote_approved) {
          lastError = "La sesión expiró antes de la aprobación remota.";
        }
      }, 250);
      await focusPrimaryAction();
      return true;
    } catch (error) {
      lastError = describeError(error);
      return false;
    }
  }

  async function startSession(): Promise<void> {
    if (!row) {
      return;
    }
    starting = true;
    lastError = null;
    outcome = null;
    try {
      // Detect an inbound session the listener already opened
      // before we attempt to start a new outbound one. The
      // pairing runtime returns the same canonical metadata the
      // remote peer's modal would render; the modal surfaces the
      // SAS / fingerprint / display name without ever calling
      // `peerPairingStartCommand`.
      if (await detectInboundSession()) {
        return;
      }
      const response = await peerPairingStartCommand({
        peer_id: row.peer_id,
        display_name: row.display_name,
      });
      outcome = response;
      mountedAt = Date.now();
      startRefresh();
      countdownInterval = setInterval(() => {
        secondsLeft = computeSecondsLeft();
        if (secondsLeft <= 0 && session && !session.remote_approved) {
          lastError = "La sesión expiró antes de la aprobación remota.";
        }
      }, 250);
      await refreshSnapshot();
      await focusPrimaryAction();
    } catch (error) {
      lastError = describeError(error);
    } finally {
      starting = false;
    }
  }

  async function approve(): Promise<void> {
    if (!session) {
      return;
    }
    approving = true;
    try {
      // The runtime routes inbound vs outbound internally:
      // [`PairingRuntime::approve_local`] calls
      // [`PeerTransport::approve_inbound_session`] when the
      // session originated in `register_inbound_from_metadata`,
      // and [`PeerTransport::approve_local`] otherwise. The
      // modal calls the same `approve_local` command either way
      // — the bridge is metadata-only and never inspects the
      // direction.
      const response = await peerPairingApproveLocalCommand({
        session_id: session.session_id,
      });
      outcome = response;
      await refreshSnapshot();
    } catch (error) {
      lastError = describeError(error);
    } finally {
      approving = false;
    }
  }

  async function cancel(): Promise<void> {
    if (!session) {
      close();
      return;
    }
    cancelling = true;
    try {
      const response = await peerPairingCancelCommand({
        session_id: session.session_id,
      });
      outcome = response;
      session = null;
      inboundMode = false;
    } catch (error) {
      lastError = describeError(error);
    } finally {
      cancelling = false;
    }
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (!open) {
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      if (session) {
        void cancel();
      } else {
        close();
      }
    }
  }

  function describeError(error: unknown): string {
    if (typeof error === "string") {
      return error;
    }
    if (error && typeof error === "object" && "message" in error) {
      return String((error as { message: unknown }).message);
    }
    return "Error desconocido";
  }

  function close(): void {
    // Always cancel an in-flight session before closing the
    // modal so the listener's bounded wait does not stay
    // pinned to a peer the user dismissed. The helper is
    // idempotent — calling it on a session that already
    // completed collapses to a no-op.
    if (session) {
      void cancel();
    }
    open = false;
    inboundMode = false;
    if (trigger && typeof trigger.focus === "function") {
      trigger.focus();
    }
  }

  onMount(async () => {
    if (open) {
      await startSession();
    }
  });

  onDestroy(() => {
    stopRefresh();
    // Closing the modal slot MUST cancel any active session
    // so a stale inbound listener does not leak a held
    // session across reopens. The bridge command is idempotent
    // when the session is already gone.
    if (session) {
      void peerPairingCancelCommand({ session_id: session.session_id });
    }
  });

  $: if (open && !session && !starting && !inboundMode) {
    void startSession();
  }

  async function focusPrimaryAction(): Promise<void> {
    await tick();
    const button = document.querySelector<HTMLButtonElement>(
      '[data-testid="peer-pairing-approve"]',
    );
    button?.focus();
  }
</script>

<svelte:window on:keydown={handleKeydown} />

{#if open && row}
  <div
    class="peer-pairing-backdrop"
    role="presentation"
    on:click|self={close}
    data-testid="peer-pairing-modal"
  >
    <article
      class="peer-pairing"
      role="dialog"
      aria-modal="true"
      aria-labelledby="peer-pairing-title"
    >
      <header>
        <h3 id="peer-pairing-title">
          {inboundMode
            ? `${row.display_name} solicita vincularse`
            : `Vincular con ${row.display_name}`}
        </h3>
        <p class="muted">
          {#if inboundMode}
            El otro equipo inició la solicitud. Compara el siguiente
            código de seis dígitos con el que aparece en su pantalla.
            Si coincide y ambos confirman, el vínculo se establece y la
            comunicación pasa a verificarse por TLS mutuo.
          {:else}
            Compara el siguiente código de seis dígitos con el que aparece en
            el otro equipo. Si coincide y ambos confirman, el vínculo se
            establece y la comunicación pasa a verificarse por TLS mutuo.
          {/if}
        </p>
      </header>

      {#if session}
        <div class="sas" data-testid="peer-pairing-sas" aria-live="polite">
          {session.sas}
        </div>
        <p class="muted" data-testid="peer-pairing-countdown">
          Expira en {secondsLeft} s
        </p>
        <p class="muted">
          peer_id remoto: <code>{session.remote_peer_id}</code>
        </p>
        <p class="muted">
          Huella remota: <code>{session.remote_fingerprint.slice(0, 8)}</code>
        </p>
        {#if session.remote_display_name}
          <p class="muted">
            Nombre remoto: <code>{session.remote_display_name}</code>
          </p>
        {/if}
        {#if session.local_approved && session.remote_approved}
          <p class="ok" role="status" data-testid="peer-pairing-trusted">
            Vínculo establecido.
          </p>
        {:else if session.local_approved}
          <p class="muted" role="status">
            Esperando aprobación del otro equipo…
          </p>
        {/if}
      {:else if outcome?.kind === "trusted"}
        <p class="ok" role="status" data-testid="peer-pairing-trusted">
          Vínculo establecido con {outcome.display_name}.
        </p>
      {:else if lastError}
        <p class="error" role="alert" data-testid="peer-pairing-error">
          {lastError}
        </p>
      {:else}
        <p class="muted" data-testid="peer-pairing-loading">
          Generando código…
        </p>
      {/if}

      <footer>
        <button
          type="button"
          class="primary"
          data-testid="peer-pairing-approve"
          disabled={!session ||
            session.local_approved ||
            session.remote_approved ||
            approving}
          on:click={approve}
        >
          {session?.local_approved ? "Aprobado" : "Aceptar"}
        </button>
        <button
          type="button"
          class="secondary"
          data-testid="peer-pairing-cancel"
          disabled={cancelling}
          on:click={() => (session ? cancel() : close())}
        >
          {session ? "Cancelar" : "Cerrar"}
        </button>
      </footer>
    </article>
  </div>
{/if}

<style>
  .peer-pairing-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(8, 11, 16, 0.7);
    display: grid;
    place-items: center;
    z-index: 50;
  }
  .peer-pairing {
    background: var(--cv-bg-surface, #0e1116);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: var(--cv-radius-md, 10px);
    padding: 1.1rem 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    min-width: 320px;
    max-width: 480px;
  }
  .peer-pairing h3 {
    margin: 0;
    font-size: var(--cv-title-sm, 0.95rem);
  }
  .peer-pairing header p {
    margin: 0.25rem 0 0;
  }
  .sas {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 2rem;
    font-weight: 700;
    letter-spacing: 0.5rem;
    text-align: center;
    padding: 0.85rem 1rem;
    background: rgba(37, 99, 235, 0.15);
    border-radius: var(--cv-radius-sm, 6px);
  }
  .peer-pairing footer {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }
  .muted {
    color: var(--cv-fg-muted, #94a3b8);
    margin: 0;
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
    border: 0;
    padding: 0.45rem 1rem;
    border-radius: var(--cv-radius-sm, 6px);
    cursor: pointer;
    font-size: 0.9rem;
  }
  button.primary {
    background: var(--cv-accent, #2563eb);
    color: white;
  }
  button.secondary {
    background: #1f2937;
    color: white;
  }
  button:disabled {
    background: #374151;
    color: #94a3b8;
    cursor: not-allowed;
  }
</style>