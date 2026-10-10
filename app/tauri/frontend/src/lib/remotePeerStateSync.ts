/**
 * Metadata-only state the renderer mirrors into the runtime caches that gate
 * remote browsing, importing and thumbnails. It deliberately contains no row
 * content or transport data.
 */
export interface RemotePeerState {
  peer_id: string;
  trusted: boolean;
  active: boolean;
}

/** The snapshot shape needed to project a peer's runtime eligibility state. */
export interface RemotePeerStateSnapshot {
  entries: ReadonlyArray<{
    peer_id: string;
    trust_state: string;
    is_present: boolean;
  }>;
}

export type RemotePeerStateWriter = (state: RemotePeerState) => Promise<void>;

/**
 * Projects a received snapshot into runtime state. `null` means the complete
 * snapshot is not known yet, while a missing peer in a known snapshot revokes
 * the peer by returning an inactive state.
 */
export function remotePeerStateFromSnapshot(
  peerId: string,
  snapshot: RemotePeerStateSnapshot | null,
): RemotePeerState | null {
  if (snapshot === null) return null;

  const entry = snapshot.entries.find((candidate) => candidate.peer_id === peerId);
  return {
    peer_id: peerId,
    trusted: entry?.trust_state === "trusted",
    active: entry?.is_present ?? false,
  };
}

/**
 * Serializes bridge writes for each peer in observation order. Tauri invokes
 * are independent, so a newer snapshot must wait for the previous write to
 * finish; otherwise a late inactive write can overwrite a current active one.
 *
 * Equal consecutive states reuse their queued write. A failed write remains
 * observable by its caller, while a later state can still proceed and a retry
 * of the same state is allowed.
 */
export class RemotePeerStateSynchronizer {
  private readonly chains = new Map<string, Promise<void>>();
  private readonly latestStateKeys = new Map<string, string>();

  synchronize(
    state: RemotePeerState,
    write: RemotePeerStateWriter,
  ): Promise<void> {
    const stateKey = `${state.trusted}\u0000${state.active}`;
    const previous = this.chains.get(state.peer_id);
    if (
      previous !== undefined &&
      this.latestStateKeys.get(state.peer_id) === stateKey
    ) {
      return previous;
    }

    const next = (previous ?? Promise.resolve())
      .catch(() => undefined)
      .then(() => write(state));
    this.chains.set(state.peer_id, next);
    this.latestStateKeys.set(state.peer_id, stateKey);

    void next.catch(() => {
      if (
        this.chains.get(state.peer_id) === next &&
        this.latestStateKeys.get(state.peer_id) === stateKey
      ) {
        this.latestStateKeys.delete(state.peer_id);
      }
    });

    return next;
  }
}
