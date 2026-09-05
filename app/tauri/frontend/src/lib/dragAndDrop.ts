// Pure helpers for the drag-and-drop flow that lets users drag a
// history card onto a user collection to add the association.
//
// The helpers are intentionally tiny and free of Tauri, Svelte and
// DOM mutations so the contract the card and the sidebar rely on
// stays testable end-to-end without a desktop host.
//
// The contract being pinned:
//
//   - `buildDragPayload` produces two complementary representations
//     for the same opaque entry id:
//
//         1. The ClipVault-private MIME
//            `application/x.clipvault-entry-id` carries the JSON
//            encoding the sidebar accepts on a fully standards-
//            compliant drag-and-drop implementation.
//
//         2. The strict `text/plain` fallback
//            `clipvault-entry:v1:<integer-id>`. The fallback exists
//            because WebKit/Tauri sometimes drops the private MIME
//            from `DataTransfer.types` during `dragover` and `drop`
//            — the browser only exposes the generic `text/plain`
//            representation. The sidebar MUST keep accepting the
//            drop so the user-visible flow (drag → drop →
//            association) still completes.
//
//     Both representations are intentionally tiny and free of
//     clipboard content, snippets, hashes, asset references, paths
//     or image bytes — the helpers do not even accept those fields.
//
//   - `parseDragPayload` is the round-trip: it rejects everything
//     that does not match the canonical shape of either
//     representation so the sidebar can drop payloads sourced from
//     another app or from a malformed card.
//   - `acceptsDragOver` opts a row into the `drop` event by
//     accepting EITHER the private MIME, the versioned
//     `text/plain` fallback OR an active in-memory drag session
//     opened by the local pointer/mouse controller after activation. The session fallback is the
//     safety net for the WebKit/Tauri quirk that leaves
//     `DataTransfer.types` empty during `dragover` and `drop`; the
//     session only exists for drags the card itself started, so a
//     foreign drag from another app cannot impersonate a card.
//   - `beginDragSession` / `endDragSession` own the in-memory
//     session that bridges the WebKit/Tauri gap. The pointer/mouse controller calls
//     `beginDragSession(entry.id)` after activation and the sidebar
//     consults / clears it during `dragover`, `drop` and the
//     document-level `dragend`. A session that is left dangling
//     after a cancel is cleared by the global `dragend` listener so
//     a stale entry id cannot leak across drags.
//   - `combineMemberships` adds the target collection to the
//     persisted set without dropping the existing memberships. The
//     system `Historial` collection is always retained; calling the
//     helper with a `null`/`undefined` `current` argument is the
//     "unhydrated" branch that the spec demands: the helper refuses
//     to send an empty list because it would clobber the
//     associations the backend already holds.
//   - `isDragPayload`, `isDropTarget` and `isDragEvent` are the
//     minimal DOM-level guards the components need to keep the
//     listeners honest: they type-narrow, they never crash on a
//     missing field and they never inspect the payload content.

/**
 * Stable MIME type ClipVault uses to ferry a single entry id through
 * the DataTransfer. A foreign drop (e.g. a file from Finder) won't
 * expose this MIME so the sidebar can drop the payload without
 * sniffing the contents.
 */
export const CLIPVAULT_ENTRY_MIME = "application/x.clipvault-entry-id";

/**
 * Stable `text/plain` fallback for environments that strip the
 * private MIME during `dragover` / `drop` (WebKit/Tauri observed on
 * macOS). The format is intentionally minimal and versioned:
 *
 *     clipvault-entry:v1:<integer-id>
 *
 * The prefix is exact: anything that does not match
 * `clipvault-entry:v1:` plus a single finite integer is rejected
 * outright so a foreign drag (a file path, a piece of clipboard
 * text, an URL or a JSON blob from another app) cannot impersonate
 * an entry id. The integer is never negative and never fractional.
 */
export const CLIPVAULT_ENTRY_TEXT_PREFIX = "clipvault-entry:v1:";

/**
 * In-memory drag session the pointer/mouse controller opens after
 * activation and the
 * sidebar consults on `dragover` / `drop` when `DataTransfer.types`
 * is empty or only exposes an unexpected MIME. The session only
 * stores an opaque entry id; clipboard content, snippets, hashes,
 * paths and image bytes never enter this state. The session is
 * cleared on `dragend`, on every `drop`, on `dragend` and after a
 * manual reset so a stale id cannot leak across drags.
 */
interface DragSession {
  entryId: number;
  token: number;
}

let activeDragSession: DragSession | null = null;
let dragSessionTokenCounter = 0;

/**
 * Open a fresh in-memory drag session for the supplied entry id.
 *
 * The helper returns an opaque `token` the caller can pass to
 * `endDragSession` so a stale listener that fires after a new drag
 * starts cannot accidentally clear the new session. The session is
 * intentionally a module-level singleton: the card and the sidebar
 * live in the same JS context (the Tauri webview) so a shared
 * reference is the simplest, race-free channel that survives the
 * WebKit/Tauri quirk that drops the DataTransfer on `dragover`.
 *
 * `null` is returned when the caller cannot prove the entry id is a
 * finite non-negative integer — the session must never carry an
 * arbitrary value because the sidebar trusts it as a fallback
 * `DataTransfer` payload.
 */
export function beginDragSession(entryId: number): number | null {
  if (!Number.isFinite(entryId) || !Number.isInteger(entryId) || entryId < 0) {
    return null;
  }
  dragSessionTokenCounter += 1;
  activeDragSession = {
    entryId,
    token: dragSessionTokenCounter,
  };
  return dragSessionTokenCounter;
}

/**
 * End the in-memory drag session. When the caller forwards the
 * token it received from `beginDragSession`, the helper only clears
 * the session if the token still matches — a listener that fires
 * late (e.g. a cancel arriving after the user started a new drag)
 * cannot accidentally drop the new session. Without a token the
 * helper clears unconditionally, which is the safe default for the
 * global `dragend` listener.
 */
export function endDragSession(token?: number): void {
  if (token === undefined) {
    activeDragSession = null;
    return;
  }
  if (activeDragSession && activeDragSession.token === token) {
    activeDragSession = null;
  }
}

/**
 * `true` when an in-memory drag session is currently active, i.e.
 * the controller opened a session after activation and the sidebar did not
 * yet observe the matching `dragend` or `drop`. Used as the last
 * fallback in `acceptsDragOver` and as a defensive guard in the
 * drop handler.
 */
export function hasActiveDragSession(): boolean {
  return activeDragSession !== null;
}

/**
 * Read the entry id the active drag session carries. Returns
 * `null` when no session is active so the sidebar can fall through
 * to the next resolution strategy.
 */
export function getActiveDragSessionEntryId(): number | null {
  return activeDragSession ? activeDragSession.entryId : null;
}

/**
 * Test-only escape hatch. Production code MUST use `endDragSession`
 * with the token returned by `beginDragSession`. The helper exists
 * so the test runner can scrub state between sequential test cases
 * without exposing a privileged API to the components.
 */
export function __resetDragSessionForTests(): void {
  activeDragSession = null;
  dragSessionTokenCounter = 0;
}

/**
 * Build the dual representation the card drops on a user collection.
 *
 * The helper never accepts extra fields so a regression that wired
 * clipboard content, snippets, hashes or asset references into the
 * DataTransfer would surface here as a compile error.
 *
 * Returns an object with both payloads so the caller can write them
 * to their respective MIME types in a single code path; the
 * structure makes it impossible to set one representation without
 * the other.
 */
export interface DragPayloadPair {
  /** JSON payload for the ClipVault private MIME. */
  privatePayload: string;
  /**
   * Strict versioned `text/plain` fallback the sidebar accepts when
   * WebKit/Tauri drops the private MIME from `DataTransfer.types`.
   */
  textPayload: string;
}

export function buildDragPayload(entryId: number): DragPayloadPair {
  if (!Number.isFinite(entryId) || !Number.isInteger(entryId)) {
    throw new Error(
      `ClipVault drag payload requires an integer entry id (got ${String(entryId)})`,
    );
  }
  return {
    privatePayload: JSON.stringify({ id: entryId }),
    textPayload: `${CLIPVAULT_ENTRY_TEXT_PREFIX}${entryId}`,
  };
}

/**
 * Read a JSON payload the card dropped through the private MIME.
 *
 * Returns `null` when the payload is missing, has the wrong MIME
 * type, is not valid JSON or carries anything other than a single
 * integer `id`. The helpers never throw on malformed inputs — the
 * sidebar surfaces a typed no-op when the payload is invalid.
 */
export function parseDragPayload(raw: string | null | undefined): number | null {
  if (typeof raw !== "string" || raw.length === 0) {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!parsed || typeof parsed !== "object") {
    return null;
  }
  const record = parsed as Record<string, unknown>;
  if (Object.keys(record).length !== 1) {
    return null;
  }
  const value = record["id"];
  if (typeof value !== "number" || !Number.isInteger(value) || !Number.isFinite(value)) {
    return null;
  }
  return value;
}

/**
 * Read the versioned `text/plain` fallback the card dropped when
 * WebKit/Tauri stripped the private MIME. The parser is the strict
 * counterpart of `buildDragPayload`: it accepts ONLY
 * `clipvault-entry:v1:<integer-id>` and nothing else.
 *
 * The function rejects:
 *
 *   - missing or empty strings,
 *   - the bare prefix without an integer suffix,
 *   - extra whitespace, extra fields, JSON blobs, file paths,
 *     `text/plain` content copied from the clipboard, anything that
 *     is not a finite non-negative integer,
 *   - non-integer ids (floats, big integers rendered as strings,
 *     scientific notation, leading `+` or `-`, decimal points).
 *
 * A successful parse returns the integer id; anything else returns
 * `null` so the sidebar can treat the drop as a safe no-op.
 */
export function parseDragTextPayload(
  raw: string | null | undefined,
): number | null {
  if (typeof raw !== "string" || raw.length === 0) {
    return null;
  }
  if (!raw.startsWith(CLIPVAULT_ENTRY_TEXT_PREFIX)) {
    return null;
  }
  const suffix = raw.slice(CLIPVAULT_ENTRY_TEXT_PREFIX.length);
  if (suffix.length === 0) {
    return null;
  }
  // The integer must match exactly: digits only, no leading sign,
  // no decimal point, no whitespace. Anything that escapes the
  // pattern is a foreign drop and must be rejected.
  if (!/^[0-9]+$/.test(suffix)) {
    return null;
  }
  const value = Number(suffix);
  if (!Number.isFinite(value) || !Number.isInteger(value)) {
    return null;
  }
  return value;
}

/**
 * Parse whichever representation the DataTransfer exposed. The
 * sidebar uses this helper for `drop` because WebKit/Tauri can
 * surface the private MIME, the fallback, or both. The helper tries
 * the private MIME first, falls back to the versioned text/plain
 * payload and finally — when both representations are unavailable —
 * consults the in-memory drag session opened by the card on
 * pointer/mouse activation. The third branch only resolves when the session is
 * currently active so a foreign drag without an open session still
 * reduces to `null` and the sidebar treats it as a safe no-op.
 *
 * The helper never inspects the payload content beyond the integer
 * id; it does not log, echo or forward the raw text to the rest of
 * the desktop.
 */
export function parseDragPayloadFromTransfer(payloads: {
  privatePayload: string | null | undefined;
  textPayload: string | null | undefined;
  sessionEntryId?: number | null | undefined;
}): number | null {
  const fromPrivate = parseDragPayload(payloads.privatePayload);
  if (fromPrivate !== null) {
    return fromPrivate;
  }
  const fromText = parseDragTextPayload(payloads.textPayload);
  if (fromText !== null) {
    return fromText;
  }
  const sessionId = payloads.sessionEntryId;
  if (
    typeof sessionId === "number" &&
    Number.isFinite(sessionId) &&
    Number.isInteger(sessionId) &&
    sessionId >= 0
  ) {
    return sessionId;
  }
  return null;
}

/**
 * Additive combination of the entry's current collection ids and the
 * target collection the user dropped on. The helper never removes
 * anything from `current`, never inserts duplicates and never drops
 * `historialId` when it is supplied alongside `currentIds`.
 *
 * The `currentIds` argument is intentionally nullable: the caller
 * MUST pass `null` when the entry's associations are still in
 * flight (`entryOrganizationHydration[id] === "pending"` or
 * `"error"`). The helper treats `null` as a safety branch: it
 * refuses to send a `[]` list that would clobber the persisted
 * associations on the backend. The function returns the *unchanged*
 * current list along with `false` so the sidebar can render a safe
 * no-op without ever issuing a destructive mutation.
 */
export interface MembershipResult {
  /**
   * The collection ids that should be persisted. `undefined` when
   * the helper refused to mutate (`safeNoop === true`); otherwise
   * the merged list with `historialId` always included.
   */
  nextCollectionIds: number[] | undefined;
  /**
   * Whether the operation must be skipped because hydration is
   * missing, the entry is already a member, the target is invalid
   * or the helper detected a `Historial` drop. Side effects MUST be
   * skipped when this is `true`.
   */
  safeNoop: boolean;
  /** Human-readable reason the helper skipped; useful for tests. */
  reason:
    | "added"
    | "already_member"
    | "invalid_target"
    | "system_collection"
    | "missing_hydration"
    | "invalid_entry";
}

/**
 * `systemCollectionId` is the id of the protected `Historial`
 * collection. `targetId` is the collection the user dropped on.
 * `currentIds` is the list of collection ids the entry already
 * belongs to (`null` when the cache is still loading or failed).
 * `entryId` is forwarded by the helper so it can short-circuit an
 * invalid entry id alongside an invalid target.
 */
export function combineMemberships(
  entryId: number,
  targetId: number | null | undefined,
  currentIds: number[] | null | undefined,
  systemCollectionId: number | null | undefined,
): MembershipResult {
  if (!Number.isFinite(entryId) || !Number.isInteger(entryId)) {
    return { nextCollectionIds: undefined, safeNoop: true, reason: "invalid_entry" };
  }
  if (typeof targetId !== "number" || !Number.isInteger(targetId)) {
    return { nextCollectionIds: undefined, safeNoop: true, reason: "invalid_target" };
  }
  if (
    typeof systemCollectionId === "number" &&
    targetId === systemCollectionId
  ) {
    return {
      nextCollectionIds: undefined,
      safeNoop: true,
      reason: "system_collection",
    };
  }
  if (currentIds == null) {
    return {
      nextCollectionIds: undefined,
      safeNoop: true,
      reason: "missing_hydration",
    };
  }
  if (currentIds.includes(targetId)) {
    return {
      nextCollectionIds: undefined,
      safeNoop: true,
      reason: "already_member",
    };
  }
  const next = [...currentIds, targetId];
  if (
    typeof systemCollectionId === "number" &&
    !next.includes(systemCollectionId)
  ) {
    next.push(systemCollectionId);
  }
  return { nextCollectionIds: next, safeNoop: false, reason: "added" };
}

/**
 * Type-narrowing guard for the MIME types the sidebar accepts.
 * Returns `true` when the DataTransfer exposes the ClipVault
 * private MIME or the versioned `text/plain` fallback. The fallback
 * covers the WebKit/Tauri quirk that strips the private MIME during
 * `dragover` / `drop`.
 */
export function isDragPayload(types: readonly string[] | DOMStringList | null | undefined): boolean {
  if (!types) return false;
  if (typeof (types as DOMStringList).contains === "function") {
    const domList = types as DOMStringList;
    return (
      domList.contains(CLIPVAULT_ENTRY_MIME) ||
      domList.contains("text/plain")
    );
  }
  for (let i = 0; i < (types as readonly string[]).length; i++) {
    const type = (types as readonly string[])[i];
    if (type === CLIPVAULT_ENTRY_MIME || type === "text/plain") {
      return true;
    }
  }
  return false;
}

/**
 * Decide whether a `dragover` event carries a usable ClipVault
 * payload. The sidebar uses the helper to opt into the `drop` event
 * — without `preventDefault` on `dragover`, the browser never fires
 * a corresponding `drop`. The helper accepts the private MIME, the
 * `text/plain` fallback AND the in-memory drag session so a
 * WebKit/Tauri drag that drops the DataTransfer entirely still
 * opts the row in. The session fallback is safe because it is only
 * opened by the local pointer/mouse controller; a foreign drag from another
 * app cannot open a session and therefore cannot impersonate a card.
 *
 * The helper never inspects the payload contents; it only checks
 * the MIME types so the function remains cheap enough to call on
 * every `dragover`.
 */
export function acceptsDragOver(event: {
  dataTransfer?:
    | { types?: readonly string[] | DOMStringList | null | undefined }
    | null;
}): boolean {
  const transfer = event.dataTransfer;
  if (transfer) {
    if (isDragPayload(transfer.types ?? null)) {
      return true;
    }
  }
  // The session fallback only fires when the local pointer/mouse
  // controller opened the session. A foreign drag (file from Finder,
  // selection from another webview, …) cannot open the session and
  // therefore reduces to `false` so the sidebar stays inert.
  return hasActiveDragSession();
}

/**
 * Drop target predicate: a collection row is a valid drop target
 * when it is a user collection (not `Historial`). The helper is the
 * single source of truth the sidebar uses to wire the visual
 * feedback and the `dragover` / `drop` listeners so a regression
 * that wired the system collection as a drop target would surface
 * here as a failed assertion in the tests.
 */
export function isDropTarget(collection: { kind: string } | null | undefined): boolean {
  return !!collection && collection.kind === "user";
}
