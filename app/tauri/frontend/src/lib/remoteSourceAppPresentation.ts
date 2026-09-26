/** Bounded, on-demand source-app presentation rules for remote cards. */

export const MAX_REMOTE_SOURCE_APP_ICON_BYTES = 512 * 1024;
export const MAX_REMOTE_SOURCE_APP_NAME_CHARS = 128;

export type RemoteSourceAppPhase = "idle" | "loading" | "ready" | "unavailable";

export interface RemoteSourceAppRequestState {
  peerId: string | null;
  peerStateReady: boolean;
  capability: string | null;
  isVisible: boolean;
  requestInFlight: boolean;
  phase: RemoteSourceAppPhase;
}

export function supportsRemoteSourceAppPresentation(
  capability: string | null,
): boolean {
  return (
    capability
      ?.split(",")
      .some((token) => token.trim() === "source_app_presentation") ?? false
  );
}

/** Fetch only for a visible row in the active, trusted peer rail. */
export function shouldRequestRemoteSourceAppPresentation(
  state: RemoteSourceAppRequestState,
): boolean {
  return (
    state.peerId !== null &&
    state.peerStateReady &&
    state.isVisible &&
    supportsRemoteSourceAppPresentation(state.capability) &&
    !state.requestInFlight &&
    state.phase === "idle"
  );
}

/** Reject malformed host names; never silently strip control characters. */
export function validatedRemoteSourceAppName(
  input: string | null,
): string | null {
  if (input === null) return null;
  const name = input.trim();
  if (
    name.length === 0 ||
    Array.from(name).length > MAX_REMOTE_SOURCE_APP_NAME_CHARS ||
    /[\u0000-\u001f\u007f-\u009f]/u.test(name)
  ) {
    return null;
  }
  return name;
}

export function isCurrentRemoteSourceAppRequest(
  requestToken: number,
  currentToken: number,
): boolean {
  return requestToken === currentToken;
}
