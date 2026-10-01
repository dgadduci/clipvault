<script lang="ts">
  /**
   * Reciprocal local-peer pairing UI. Its state is deliberately
   * epoch-scoped: dismissing the dialog invalidates outstanding IPC
   * calls, so a late start response cannot re-open a cancelled session.
   */
  import { createEventDispatcher, onDestroy, tick } from "svelte";
  import { t } from "./lib/localization.ts";

  import {
    peerPairingApproveLocalCommand,
    peerPairingCancelCommand,
    peerPairingSnapshotCommand,
    peerPairingStartCommand,
    peerSnapshotCommand,
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
      const previousSession = session;
      const nextSession = row
        ? snapshots.find((value) => value.remote_peer_id === row!.peer_id) ?? null
        : snapshots[0] ?? null;
      // Pairing completes asynchronously in the transport task, so
      // the command that accepted the local SAS normally returns
      // `awaiting_remote_approval`. Once the task persists trust it
      // removes the in-memory session. Reconcile that disappearance
      // with the metadata-only peer snapshot rather than continuing
      // to render a stale “Esperando aprobación” state.
      if (
        !nextSession &&
        row &&
        (previousSession?.local_approved || outcome?.kind === "awaiting_remote_approval")
      ) {
        const peerSnapshot = await peerSnapshotCommand();
        if (!isCurrentOpen(epoch)) return;
        const peer = peerSnapshot.entries.find((entry) => entry.peer_id === row!.peer_id);
        if (peer?.trust_state === "trusted") {
          outcome = {
            kind: "trusted",
            peer_id: peer.peer_id,
            display_name: peer.display_name,
            short_fingerprint: peer.public_key_fingerprint.slice(0, 8),
            paired_at: peer.paired_at ?? "",
          };
          lastError = null;
          stopRefresh();
        } else {
          outcome = null;
          lastError = "peers.pairing.failed";
          stopRefresh();
        }
      }
      session = nextSession;
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
        lastError = "peers.pairing.expired_before_approval";
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
        return "peers.pairing.error.transport";
      case "unknown_or_key_mismatch":
        return "peers.pairing.error.identity_changed";
      case "rate_limited":
        return "peers.pairing.error.rate_limited";
      case "blocked":
        return "peers.pairing.error.blocked";
      case "revoked":
        return "peers.pairing.error.revoked";
      case "incompatible_protocol":
        return "peers.pairing.error.incompatible";
      case "session_expired":
        return "peers.pairing.error.expired";
      case "cancelled":
        return "peers.pairing.error.cancelled";
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
        lastError = "peers.pairing.error.session_missing";
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
    lastError = null;
    try {
      const response = await peerPairingApproveLocalCommand({
        session_id: session.session_id,
      });
      outcome = response;
      lastError = failedOutcomeMessage(response);
      await refreshSnapshot();
    } catch (error) {
      lastError = describeError(error);
    } finally {
      approving = false;
    }
  }

  function describeError(error: unknown): string {
    void error;
    return "peers.error.generic";
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
            ? $t("peers.pairing.requested_by", { name: row.display_name })
            : $t("peers.pairing.title", { name: row.display_name })}
        </h3>
        <p class="muted">
          {#if inboundMode}
            {$t("peers.pairing.inbound_help")}
          {:else}
            {$t("peers.pairing.outbound_help")}
          {/if}
        </p>
      </header>

      {#if session}
        <div class="sas" data-testid="peer-pairing-sas" aria-live="polite">
          {session.sas}
        </div>
        <p class="muted" data-testid="peer-pairing-countdown">
          {$t("peers.pairing.expires", { count: secondsLeft })}
        </p>
        <p class="muted">
          {$t("peers.pairing.remote_peer_id")}: <code>{session.remote_peer_id}</code>
        </p>
        <p class="muted">
          {$t("peers.pairing.remote_fingerprint")}: <code>{session.remote_fingerprint.slice(0, 8)}</code>
        </p>
        {#if session.remote_display_name}
          <p class="muted">
            {$t("peers.pairing.remote_name")}: <code>{session.remote_display_name}</code>
          </p>
        {/if}
        {#if lastError}
          <p class="error" role="alert" data-testid="peer-pairing-error">
            {$t(lastError)}
          </p>
        {:else if session.local_approved && session.remote_approved}
          <p class="ok" role="status" data-testid="peer-pairing-trusted">
            {$t("peers.pairing.trusted")}
          </p>
        {:else if session.local_approved}
          <p class="muted" role="status">
            {$t("peers.pairing.waiting_approval")}
          </p>
        {/if}
      {:else if outcome?.kind === "trusted"}
        <p class="ok" role="status" data-testid="peer-pairing-trusted">
          {$t("peers.pairing.trusted_with", { name: outcome.display_name })}
        </p>
      {:else if lastError}
        <p class="error" role="alert" data-testid="peer-pairing-error">
          {$t(lastError)}
        </p>
      {:else}
        <p class="muted" data-testid="peer-pairing-loading">
          {$t("peers.pairing.generating")}
        </p>
      {/if}

      <footer>
        <button
          type="button"
          class="primary"
          data-testid="peer-pairing-approve"
          disabled={!session ||
            session.local_approved ||
            approving}
          on:click={approve}
        >
          {session?.local_approved ? $t("peers.pairing.approved") : $t("common.accept")}
        </button>
        <button
          type="button"
          class="secondary"
          data-testid="peer-pairing-cancel"
          on:click={close}
        >
          {session ? $t("common.cancel") : $t("common.close")}
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
