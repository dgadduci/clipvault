/** Capability gate shared by the remote card and focused tests. */
export function supportsRemoteImageThumbnails(
  capability: string | null,
): boolean {
  if (capability === null) return false;
  const tokens = capability
    .split(",")
    .map((token) => token.trim())
    .filter((token) => token.length > 0);
  return (
    tokens.includes("image_import") &&
    tokens.includes("image_preview_thumbnail")
  );
}

export type RemoteImageThumbnailPhase = "idle" | "loading" | "ready" | "error";

export interface RemoteImageThumbnailRequestState {
  isImageRow: boolean;
  isIntersecting: boolean;
  peerId: string | null;
  peerStateReady: boolean;
  peerCapability: string | null;
  requestInFlight: boolean;
  phase: RemoteImageThumbnailPhase;
}

/** Pure request gate shared by the viewport callback and the card fetcher. */
export function shouldRequestRemoteImageThumbnail(
  state: RemoteImageThumbnailRequestState,
): boolean {
  return (
    state.isImageRow &&
    state.isIntersecting &&
    state.peerId !== null &&
    state.peerStateReady &&
    supportsRemoteImageThumbnails(state.peerCapability) &&
    !state.requestInFlight &&
    state.phase !== "loading" &&
    state.phase !== "ready"
  );
}

/** Composite each-key: remote IDs are local to a host, not global. */
export function remoteImageThumbnailCardKey(
  peerId: string | null,
  remoteEntryId: string,
): string {
  return JSON.stringify([peerId, remoteEntryId]) ?? "";
}

/** Full card identity used to invalidate in-flight and rendered previews. */
export function remoteImageThumbnailCardIdentity(
  peerId: string | null,
  remoteEntryId: string,
  createdAt: string,
  title: string | null,
  preview: string,
  isImageRow: boolean,
  capability: string | null,
): string {
  return (
    JSON.stringify([
      peerId,
      remoteEntryId,
      createdAt,
      title,
      preview,
      isImageRow,
      capability,
    ]) ?? ""
  );
}

export function isCurrentThumbnailRequest(
  requestToken: number,
  currentToken: number,
): boolean {
  return requestToken === currentToken;
}
