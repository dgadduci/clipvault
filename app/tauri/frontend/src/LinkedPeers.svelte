<script lang="ts">
  /**
   * Read-only list of the trusted, persisted peers the desktop
   * shell renders immediately below the user-defined collections.
   * The component is the desktop equivalent of the historical
   * `Equipos` modal — every row carries the validated display name
   * the remote advertised plus a small circle that mirrors the
   * persisted presence state the discovery runtime observed.
   *
   * The component does NOT manage pairing / revocation / blocking
   * (those flows still live in the `PeerSharingModal`); its only
   * responsibility is the metadata-only `Equipos vinculados` surface
   * the `peer-text-history-browser` change pins. Selecting a row
   * dispatches the `select-peer` event the parent routes into the
   * main panel so a paired, active peer replaces the local history
   * / collection content with a horizontal rail of remote previews.
   *
   * The presence dot tracks the latest snapshot the
   * `peerSnapshotCommand` returned. The dot ONLY goes green when
   * the peer is `trusted && is_present`; every other trusted
   * state surfaces a gray dot. The `RemoteHistoryRail` mirrors the
   * same predicate to decide whether to open the network call.
   *
   * The component is fully read-only — it never opens a network
   * call, never touches SQLite and never opens the pairing modal.
   * The parent fires `peerSnapshotCommand` on every refresh and
   * explicitly when the pairing modal closes so the list updates
   * without the linked list itself driving any snapshot round-trip.
   */
  import { createEventDispatcher } from "svelte";
  import type { PeerSnapshot, PeerSnapshotEntry } from "./types";

  /**
   * Props the parent supplies. The component is a pure adapter
   * over the `peerSnapshotCommand` response, so the parent passes
   * the latest snapshot directly without polling.
   */
  export let snapshot: PeerSnapshot | null = null;
  /**
   * Optional currently-selected peer id (the `peer_id` the parent
   * routed through the main panel). When the parent surfaces the
   * remote previews for the peer, the linked list keeps the row
   * highlighted so the user can see which peer owns the rail
   * currently visible in the main panel.
   */
  export let activePeerId: string | null = null;

  type DispatchEvents = {
    "select-peer": { peerId: string };
  };
  const dispatch = createEventDispatcher<DispatchEvents>();

  /**
   * Filter the snapshot to the peers the desktop should render in
   * the `Equipos vinculados` list. Only peers with a persisted
   * `trust_state` of `trusted` qualify (the discovery-only row
   * stays in the `Equipos` modal where the user can pair /
   * revoke / block). The list is the desktop equivalent of the
   * `Equipos` modal "Vincular / Ver historial" view.
   */
  $: linkedPeers = (snapshot?.entries ?? []).filter(
    (entry) => entry.trust_state === "trusted",
  );

  /**
   * Whether a peer row is currently `Activo`. A peer is Active
   * ONLY when the discovery runtime observed it inside the
   * presence TTL window AND the persisted trust is `trusted`;
   * the predicate is the exact same gate `RemoteHistoryRail`
   * uses to decide whether to open the network call, so the dot
   * the user sees can never disagree with the rail the user
   * lands on. The component does not consult a fresh
   * `health_probe` (the dedicated `peer_pairing_health`
   * command); it relies on the latest snapshot the parent
   * already polls.
   */
  function isActive(entry: PeerSnapshotEntry): boolean {
    return entry.trust_state === "trusted" && entry.is_present;
  }

  /**
   * Whether the row should show the gray "No disponible" pill.
   * Trusted but currently absent peers land here so the
   * renderer never shows a green dot for a peer that lost
   * presence and never shows a gray dot for a peer that has
   * not yet been trusted.
   */
  function isTrustedUnavailable(entry: PeerSnapshotEntry): boolean {
    return entry.trust_state === "trusted" && !entry.is_present;
  }

  function shortDisplayName(entry: PeerSnapshotEntry): string {
    const value = entry.display_name?.trim();
    if (value && value.length > 0) return value;
    return `peer ${shortPeerId(entry.peer_id)}`;
  }

  function shortPeerId(peerId: string): string {
    if (peerId.length <= 12) return peerId;
    return `${peerId.slice(0, 4)}…${peerId.slice(-4)}`;
  }

  function selectPeer(entry: PeerSnapshotEntry): void {
    dispatch("select-peer", { peerId: entry.peer_id });
  }
</script>

<section
  class="linked-peers"
  data-testid="linked-peers-section"
  aria-label="Equipos vinculados"
>
  <h2>Equipos vinculados</h2>
  {#if !snapshot}
    <p class="linked-peers-empty" data-testid="linked-peers-empty">
      Activando la red local…
    </p>
  {:else if linkedPeers.length === 0}
    <p class="linked-peers-empty" data-testid="linked-peers-empty">
      Sin pares vinculados. Vincula uno desde el panel Compartir.
    </p>
  {:else}
    <ul
      class="linked-peers-list"
      data-testid="linked-peers-list"
      role="list"
    >
      {#each linkedPeers as entry (entry.peer_id)}
        <li
          class="linked-peers-row"
          class:active={activePeerId === entry.peer_id}
          data-testid="linked-peers-row"
          data-peer-id={entry.peer_id}
          data-active={isActive(entry) ? "true" : "false"}
        >
          <button
            type="button"
            class="linked-peers-button"
            data-testid="linked-peers-button"
            data-peer-id={entry.peer_id}
            aria-pressed={activePeerId === entry.peer_id}
            on:click={() => selectPeer(entry)}
          >
            <span
              class="linked-peers-dot"
              data-testid="linked-peers-dot"
              data-active={isActive(entry) ? "true" : "false"}
              data-trusted-unavailable={isTrustedUnavailable(entry) ? "true" : "false"}
              aria-hidden="true"
            ></span>
            <span class="linked-peers-name" data-testid="linked-peers-name">
              {shortDisplayName(entry)}
            </span>
            <span class="linked-peers-status" data-testid="linked-peers-status">
              {#if isActive(entry)}
                Activo
              {:else if isTrustedUnavailable(entry)}
                No disponible
              {:else}
                —
              {/if}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .linked-peers {
    /* The section shares the sidebar's vertical space with the
     * collection list. `flex: 1 1 auto; min-height: 0` keeps the
     * scroller bounded by the panel height — a long list of
     * trusted peers does not steal height from the collection
     * list and vice versa. The header (`flex: 0 0 auto`) stays
     * pinned above the list at every list length. */
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding-top: 0.55rem;
    border-top: 1px solid var(--cv-border, #30363d);
    flex: 1 1 0;
    min-height: 4.5rem;
  }
  .linked-peers h2 {
    margin: 0;
    font-size: 0.85rem;
    color: var(--cv-fg-muted, #94a3b8);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    flex: 0 0 auto;
  }
  .linked-peers-empty {
    margin: 0;
    font-size: 0.8rem;
    color: var(--cv-fg-muted, #94a3b8);
    line-height: 1.4;
    flex: 0 0 auto;
  }
  .linked-peers-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
  }
  .linked-peers-row.active .linked-peers-button {
    background: rgba(37, 99, 235, 0.18);
    outline: 1px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.5));
  }
  .linked-peers-button {
    width: 100%;
    background: transparent;
    color: inherit;
    border: 1px solid transparent;
    border-radius: 6px;
    padding: 0.3rem 0.45rem;
    display: flex;
    align-items: center;
    gap: 0.45rem;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .linked-peers-button:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  .linked-peers-button:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.6));
    outline-offset: 1px;
  }
  .linked-peers-dot {
    flex: 0 0 auto;
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 50%;
    background: #6b7280;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.3);
  }
  .linked-peers-dot[data-active="true"] {
    background: #4ade80;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.3),
      0 0 0 2px rgba(74, 222, 128, 0.25);
  }
  .linked-peers-name {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .linked-peers-status {
    flex: 0 0 auto;
    font-size: 0.7rem;
    color: var(--cv-fg-muted, #94a3b8);
    text-transform: lowercase;
  }
</style>
