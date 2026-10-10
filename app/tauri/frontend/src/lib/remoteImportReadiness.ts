/**
 * UI-only gate for explicit remote imports. Preview rows are metadata-only and
 * may render before the import services receive the selected peer's current
 * trust/presence snapshot; imports must wait for that synchronization.
 */
export interface RemoteImportReadinessState {
  peerId: string | null;
  peerStateReady: boolean;
  isImageRow: boolean;
  peerSupportsImageImport: boolean;
  busy: boolean;
}

/**
 * Returns true only when the current card may invoke its existing import
 * bridge. The core rechecks peer eligibility as defence in depth.
 */
export function canImportRemoteEntry(
  state: RemoteImportReadinessState,
): boolean {
  return (
    state.peerId !== null &&
    state.peerStateReady &&
    !state.busy &&
    (!state.isImageRow || state.peerSupportsImageImport)
  );
}
