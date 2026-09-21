<script lang="ts">
  /**
   * Reciprocal local-peer pairing UI. Its state is deliberately
   * epoch-scoped: dismissing the dialog invalidates outstanding IPC
   * calls, so a late start response cannot re-open a cancelled session.
   */
  import { createEventDispatcher, onDestroy, tick } from "svelte";

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
  export let trigger: HTMLElement | null = null;

  const dispatch = createEventDispatcher<{ close: { sessionId: number | null } }>();
  const SESSION_TIMEOUT_SECONDS = 120;

  let session: PeerPairingSessionSnapshot | null = null;
  let outcome: PeerPairingOutcomeResponse | null = null;
  let lastError: string | null = null;
  let approving = false;
  let mountedAt: number | null = null;
  let refreshInterval: ReturnType<typeof setInterval> | null = null;
  let countdownInterval: ReturnType<typeof setInterval> | null = null;
  let secondsLeft = 0;
  let inboundMode = false;
  let openEpoch = 0;
  let wasOpen = false;

  function isCurrentOpen(epoch: number): boolean {
    return open && epoch === openEpoch;
  }

  function computeSecondsLeft(): number {
    if (!mountedAt) return SESSION_TIMEOUT_SECONDS;
    const elapsed = (Date.now() - mountedAt) / 1000;
    return Math.max(0, Math.ceil(SESSION_TIMEOUT_SECONDS - elapsed));
  }

  async function refreshSnapshot(epoch = openEpoch): Promise<void> {
    try {
      const snapshots = await peerPairingSnapshotCommand();
      if (!isCurrentOpen(epoch)) return;
      session = row
        ? snapshots.find((value) => value.remote_peer_id === row!.peer_id) ?? null
        : snapshots[0] ?? null;
      secondsLeft = computeSecondsLeft();
    } catch (error) {
      if (isCurrentOpen(epoch)) lastError = describeError(error);
    }
  }

  function startRefresh(): void {
    if (refreshInterval !== null) return;
    refreshInterval = setInterval(() => void refreshSnapshot(), 1000);
  }

  function startCountdown(epoch: number): void {
    if (countdownInterval !== null) return;
    countdownInterval = setInterval(() => {
      if (!isCurrentOpen(epoch)) return;
      secondsLeft = computeSecondsLeft();
      if (secondsLeft <= 0 && !session?.remote_approved) {
        lastError = "La sesión expiró antes de la aprobación remota.";
      }
    }, 250);
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

  /**
   * Reuse the existing session, whether it is inbound or the user
   * reopened a still-active outbound attempt. Starting again would
   * create a competing mTLS connection.
   */
  async function detectInboundSession(epoch: number): Promise<boolean> {
    if (!row) return false;
    try {
      const snapshots = await peerPairingSnapshotCommand();
      if (!isCurrentOpen(epoch)) return false;
      const existing = snapshots.find(
        (value) => value.remote_peer_id === row!.peer_id,
      );
      if (!existing) return false;
      session = existing;
      inboundMode = existing.is_inbound;
      mountedAt = Date.now();
      startRefresh();
      startCountdown(epoch);
      await focusPrimaryAction();
      return true;
    } catch (error) {
      if (isCurrentOpen(epoch)) lastError = describeError(error);
      return false;
    }
  }

  function failedOutcomeMessage(response: PeerPairingOutcomeResponse): string | null {
    if (response.kind !== "failed") return null;
    switch (response.reason) {
      case "transport_unavailable":
        return "No se pudo abrir una conexión segura con el otro equipo. Verifica que siga disponible e inténtalo otra vez.";
      case "unknown_or_key_mismatch":
        return "La identidad del otro equipo cambió o ya no coincide con el anuncio de red.";
      case "rate_limited":
        return "Hay demasiados intentos de vínculo. Espera un minuto antes de reintentar.";
      case "blocked":
        return "El otro equipo está bloqueado.";
      case "revoked":
        return "El vínculo fue revocado. Revisa el estado del equipo antes de reintentar.";
      case "incompatible_protocol":
        return "El otro equipo usa una versión de vínculo incompatible.";
      case "session_expired":
        return "La sesión de vínculo expiró.";
      case "cancelled":
        return "La sesión de vínculo se canceló.";
    }
  }

  async function cancelOutcomeSession(response: PeerPairingOutcomeResponse): Promise<void> {
    if (response.kind !== "awaiting_remote_approval") return;
    try {
      await peerPairingCancelCommand({ session_id: response.session_id });
    } catch {
      // The runtime may already have released it; cancellation is idempotent.
    }
  }

  async function startSession(epoch: number): Promise<void> {
    if (!row) return;
    lastError = null;
    outcome = null;
    try {
      if (await detectInboundSession(epoch)) return;
      if (!isCurrentOpen(epoch)) return;
      const response = await peerPairingStartCommand({
        peer_id: row.peer_id,
        display_name: row.display_name,
      });
      if (!isCurrentOpen(epoch)) {
        await cancelOutcomeSession(response);
        return;
      }
      outcome = response;
      const failure = failedOutcomeMessage(response);
      if (failure) {
        lastError = failure;
        return;
      }
      mountedAt = Date.now();
      startRefresh();
      startCountdown(epoch);
      await refreshSnapshot(epoch);
      if (!isCurrentOpen(epoch)) {
        await cancelOutcomeSession(response);
        return;
      }
      if (!session && response.kind === "awaiting_remote_approval") {
        lastError = "La sesión de vínculo no quedó disponible. Cierra e inténtalo otra vez.";
        return;
      }
      await focusPrimaryAction();
    } catch (error) {
      if (isCurrentOpen(epoch)) lastError = describeError(error);
    }
  }

  async function approve(): Promise<void> {
    if (!session) return;
    approving = true;
    try {
      outcome = await peerPairingApproveLocalCommand({
        session_id: session.session_id,
      });
      await refreshSnapshot();
    } catch (error) {
      lastError = describeError(error);
    } finally {
      approving = false;
    }
  }

  function describeError(error: unknown): string {
    if (typeof error === "string") return error;
    if (error && typeof error === "object" && "message" in error) {
      return String((error as { message: unknown }).message);
    }
    return "Error desconocido";
  }

  function close(): void {
    const sessionId = session?.session_id;
    // Invalidate first: a late start response will cancel its own
    // reserved session instead of bringing this dialog back.
    openEpoch += 1;
    session = null;
    outcome = null;
    inboundMode = false;
    stopRefresh();
    open = false;
    dispatch("close", { sessionId: sessionId ?? null });
    if (sessionId !== undefined) {
      void peerPairingCancelCommand({ session_id: sessionId });
    }
    if (trigger && typeof trigger.focus === "function") trigger.focus();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (open && event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }

  onDestroy(() => {
    openEpoch += 1;
    stopRefresh();
    if (session) {
      void peerPairingCancelCommand({ session_id: session.session_id });
    }
  });

  $: if (open && !wasOpen) {
    wasOpen = true;
    openEpoch += 1;
    session = null;
    outcome = null;
    lastError = null;
    inboundMode = false;
    mountedAt = null;
    secondsLeft = SESSION_TIMEOUT_SECONDS;
    stopRefresh();
    void startSession(openEpoch);
  } else if (!open && wasOpen) {
    wasOpen = false;
    openEpoch += 1;
    stopRefresh();
  }

  async function focusPrimaryAction(): Promise<void> {
    await tick();
    document
      .querySelector<HTMLButtonElement>('[data-testid="peer-pairing-approve"]')
      ?.focus();
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
          on:click={close}
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
    z-index: 80;
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
