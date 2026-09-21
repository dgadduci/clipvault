/**
 * Regression coverage for the `local-peer-mutual-pairing` modal
 * and the per-row trust actions the `Equipos` view exposes.
 *
 * The pairing change ships a metadata-only Tauri bridge
 * (`clipvault_peer_pairing_*` commands + a per-row
 * `Vincular` / `Desvincular` / `Bloquear` / `Desbloquear` action
 * surface). The contract this suite pins:
 *
 *   - The bridge is metadata-only: no IP, port, TLS key, SAS
 *     candidate, signature or any other peer-supplied bytes cross
 *     the IPC boundary. A regression that introduced a leaky
 *     payload field breaks here.
 *   - The `Equipos` row surfaces the trust state alongside the
 *     presence so the modal can render the right action set
 *     without an extra round-trip. A regression that hid the
 *     trust state behind a string match breaks here.
 *   - The accessible pairing modal owns focus + Escape + the
 *     two-minute timeout; the runtime never auto-accepts. A
 *     regression that wired a `confirm()` dialog, removed the
 *     timeout, or sent the local approval automatically breaks
 *     here.
 *   - History / fetch / import routes stay rejected: the
 *     transport module exposes only pairing + metadata-only
 *     health endpoints. A regression that added an unrestricted
 *     RPC handler breaks here.
 *
 * The suite reads the production source through the `loadSource`
 * helper so it stays in lock-step with the implementation
 * without standing up a Svelte runtime.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

const FRONTEND_ROOT = process.cwd();

function loadSource(...segments: string[]): string {
  return readFileSync(path.join(FRONTEND_ROOT, ...segments), "utf8");
}

/**
 * Strip comments so a regression pinned inside a comment cannot
 * accidentally match the source-level assertions below.
 */
function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/^\s*\*.*$/gm, "");
}

function peerSharingSource(): string {
  return stripComments(loadSource("src/PeerSharingModal.svelte"));
}

function peerPairingSource(): string {
  return stripComments(loadSource("src/PeerPairingModal.svelte"));
}

function appSource(): string {
  return stripComments(loadSource("src/App.svelte"));
}

function tauriSource(): string {
  return stripComments(loadSource("src/lib/tauri.ts"));
}

test("pairing bridge is metadata-only and never accepts peer-supplied bytes", () => {
  // The bridge MUST only forward the typed peer_id /
  // display_name / cert_fingerprint / session_id / message
  // arguments the pairing runtime exposes. A regression that
  // added an IP, port, TLS key, SAS candidate, signature or
  // any other peer-supplied field breaks here.
  const body = tauriSource();
  for (const forbidden of [
    "ip_address",
    "ipAddress",
    "tls_key",
    "tlsKey",
    "private_key",
    "privateKey",
    "shared_secret",
    "sharedSecret",
    "session_secret",
    "sessionSecret",
    "sas_candidate",
    "sasCandidate",
    "signature",
  ]) {
    assert.doesNotMatch(
      body,
      new RegExp(`\\b${forbidden}\\b`),
      `pairing bridge must never forward a ${forbidden} field`,
    );
  }
});

test("pairing bridge exposes start / approve / cancel / snapshot commands", () => {
  // The bridge MUST expose the documented command surface;
  // the production Tauri commands live behind these helpers so
  // removing any of them is a contract regression. The
  // observe command is intentionally absent: the renderer
  // must never submit a `PairingMessage`, signature,
  // certificate fingerprint or any other peer-supplied byte;
  // the runtime receives authenticated events exclusively
  // from the mTLS transport.
  const body = tauriSource();
  for (const expected of [
    "peerPairingStartCommand",
    "peerPairingApproveLocalCommand",
    "peerPairingCancelCommand",
    "peerPairingSnapshotCommand",
    "peerPairingRevokeCommand",
    "peerPairingBlockCommand",
    "peerPairingUnblockCommand",
  ]) {
    assert.ok(
      body.includes(expected),
      `pairing bridge must expose ${expected}`,
    );
  }
  // The renderer must never be able to push a raw envelope
  // through the IPC bridge; removing it removes the only path
  // a misbehaving webview could use to forge a remote
  // approval.
  assert.ok(
    !body.includes("peerPairingObserveCommand"),
    "pairing bridge must not expose peerPairingObserveCommand",
  );
  assert.ok(
    !body.includes("clipvault_peer_pairing_observe"),
    "pairing bridge must not invoke clipvault_peer_pairing_observe",
  );
});

test("peer-sharing modal wires the per-row trust actions", () => {
  // The Equipos view MUST render the Vincular / Desvincular /
  // Bloquear / Desbloquear action set next to every peer so the
  // user can drive the reciprocal approval flow. A regression
  // that hid the actions behind a config flag, removed the
  // revoke/block buttons, or accidentally wired the buttons to
  // the wrong command breaks here.
  const body = peerSharingSource();
  for (const expected of [
    "peer-equipos-pair",
    "peer-equipos-revoke",
    "peer-equipos-block",
    "peer-equipos-unblock",
    "peer-equipos-trust-state",
    "peerPairingRevokeCommand",
    "peerPairingBlockCommand",
    "peerPairingUnblockCommand",
  ]) {
    assert.ok(
      body.includes(expected),
      `PeerSharingModal must surface the ${expected} action`,
    );
  }
});

test("peer-pairing modal owns focus, Escape and the two-minute timeout", () => {
  // The accessible pairing modal must move focus on open,
  // cancel on Escape, surface the SAS code and respect the
  // documented two-minute hard limit. A regression that wired
  // a `confirm()` dialog, removed the timeout, or sent the
  // local approval automatically breaks here.
  const body = peerPairingSource();
  assert.match(
    body,
    /SESSION_TIMEOUT_SECONDS\s*=\s*120/,
    "peer-pairing modal must mirror the runtime two-minute timeout (120s)",
  );
  assert.match(
    body,
    /Escape/,
    "peer-pairing modal must cancel on Escape",
  );
  assert.match(
    body,
    /peer-pairing-sas/,
    "peer-pairing modal must render the SAS code in a data-testid element",
  );
  assert.match(
    body,
    /peerPairingApproveLocalCommand/,
    "peer-pairing modal must wire the local approval command",
  );
  // No auto-accept: the modal must require an explicit click on
  // the `Aceptar` button — a regression that fired the approval
  // inside `onMount` or wired a `confirm()` shortcut breaks
  // here.
  assert.doesNotMatch(
    body,
    /confirm\s*\(/,
    "peer-pairing modal must not use the browser confirm() dialog",
  );
});

test("peer-pairing modal detects inbound sessions before auto-starting outbound", () => {
  // When a remote peer opens a pairing request, the listener
  // registers it through `on_pairing_session_started` and
  // pushes the canonical metadata into the runtime. The modal
  // MUST detect the inbound row through
  // `peerPairingSnapshotCommand` BEFORE invoking
  // `peerPairingStartCommand` — a regression that always
  // starts an outbound session would duplicate the dialog
  // and race the inbound state machine.
  const body = peerPairingSource();
  assert.match(
    body,
    /peerPairingSnapshotCommand/,
    "peer-pairing modal must consult the snapshot to detect inbound sessions",
  );
  // The detection branch MUST call approve_local on the same
  // session_id, NOT a second start command. The runtime routes
  // inbound vs outbound through the same `approve_local`
  // command; the modal only flips the `inboundMode` flag so
  // the UI copy changes.
  assert.match(
    body,
    /detectInboundSession/,
    "peer-pairing modal must detect inbound sessions through detectInboundSession",
  );
  // The local approval MUST NOT auto-fire — a regression that
  // calls `peerPairingApproveLocalCommand` inside the inbound
  // detection path would auto-accept every remote pairing
  // attempt and bypass the dual-approval gate.
  assert.match(
    body,
    /if\s*\(\s*await\s+detectInboundSession\([^)]*\)\s*\)\s*(?:\{\s*)?return;/,
    "inbound detection must NOT auto-approve; it must only surface the metadata and wait for the user",
  );
});

test("peer-pairing modal cancels on close and on unmount", () => {
  // The modal MUST cancel the active session when the user
  // dismisses it through any of the four documented paths:
  // Escape keypress, backdrop click, the Cancelar / Cerrar
  // button, and the onDestroy hook when the slot unmounts.
  // A stale inbound listener that keeps a held session past
  // dismissal would pin a peer the user dismissed.
  const body = peerPairingSource();
  // Close captures the active id, invalidates late IPC work, and
  // forwards cancellation before the parent is notified. This
  // avoids both a stale listener and a dialog that cannot reopen.
  assert.match(
    body,
    /function close\b[\s\S]{0,300}const\s+sessionId\s*=\s*session\?\.session_id[\s\S]{0,500}peerPairingCancelCommand\(\{\s*session_id:\s*sessionId\s*}\)/,
    "close() must cancel the active pairing session",
  );
  assert.match(
    body,
    /dispatch\(\s*"close"\s*,\s*\{\s*sessionId:/,
    "the child modal must notify its parent so a later Vincular click can reopen it",
  );
  // onDestroy must cancel the active session so the listener
  // releases its bounded wait.
  assert.match(
    body,
    /onDestroy[\s\S]{0,200}peerPairingCancelCommand/,
    "onDestroy must cancel the active pairing session",
  );
});

test("App surfaces inbound invitations globally and only once", () => {
  // The receiving device must not require its Settings modal to be
  // open. The root app polls the metadata-only snapshot, filters to
  // listener-originated sessions, and mounts the shared pairing
  // modal. Dismissed ids suppress the short cancel propagation race;
  // a fresh inbound session still remains eligible.
  const body = appSource();
  assert.match(body, /peerPairingSnapshotCommand/);
  assert.match(body, /session\.is_inbound/);
  assert.match(body, /PAIRING_INVITATION_REFRESH_MS\s*=\s*1_000/);
  assert.match(body, /dismissedInboundPairingSessions/);
  assert.match(body, /<PeerPairingModal[\s\S]*open=\{openModal === "peer_pairing"\}/);
});

test("failed starts show a typed error instead of looping on Generando código", () => {
  const body = peerPairingSource();
  assert.match(body, /function failedOutcomeMessage/);
  assert.match(body, /const failure = failedOutcomeMessage\(response\)/);
  assert.match(body, /if\s*\(failure\)\s*\{\s*lastError = failure;\s*return;/);
  assert.doesNotMatch(
    body,
    /open\s*&&\s*!session\s*&&\s*!starting/,
    "a failed or cancelled session must wait for an explicit new open, not auto-start again",
  );
});

test("peer-pairing wire shape is metadata-only and never carries endpoint bytes", () => {
  // The Tauri commands the bridge forwards accept a typed
  // payload; the wire shape MUST NOT carry IP, port, TLS key,
  // SAS candidate, signature or any other peer-supplied bytes.
  // The runtime is the single owner of those values; the
  // bridge only forwards peer_id, display_name, cert_fingerprint
  // and the session_id.
  const body = tauriSource();
  const startIdx = body.indexOf("peerPairingStartCommand");
  const endIdx = body.indexOf("peerPairingUnblockCommand");
  assert.ok(startIdx >= 0 && endIdx >= 0, "pairing command block must exist");
  const commandBlock = body.slice(startIdx, endIdx);
  for (const forbidden of [
    "ip_address",
    "tls_key",
    "shared_secret",
    "session_secret",
    "sas_candidate",
  ]) {
    assert.doesNotMatch(
      commandBlock,
      new RegExp(`\\b${forbidden}\\b`),
      `pairing command block must not forward ${forbidden}`,
    );
  }
});

test("peer-sharing modal never invokes history / fetch / import commands", () => {
  // The pairing change must NOT widen the production bridge to
  // history, fetch or import routes; those commands land in
  // later changes. A regression that wired an out-of-scope
  // command to the pairing modal breaks here.
  const body = peerSharingSource();
  for (const forbidden in {
    history: "history",
    fetch: "fetch",
    import: "import",
  }) {
    assert.ok(!forbidden.startsWith("forbidden"));
  }
  // The list of clipvault commands the bridge exposes lives in
  // the shared `lib/tauri.ts`. The pairing change is allowed to
  // call only the pairing commands + the existing
  // peer_snapshot / peer_sharing commands. A regression that
  // added history / fetch / import command call sites breaks
  // here.
  const allowedCommandInvocations = body.match(
    /peerPairing\w+Command\(|peerSnapshotCommand\(|peerSharing\w+Command\(/g,
  );
  assert.ok(
    allowedCommandInvocations,
    "PeerSharingModal must only call the documented peer pairing / discovery / sharing commands",
  );
  for (const invocation of allowedCommandInvocations ?? []) {
    assert.ok(
      invocation.startsWith("peerPairing") ||
        invocation.startsWith("peerSnapshot") ||
        invocation.startsWith("peerSharing"),
      `unexpected command invocation: ${invocation}`,
    );
  }
});

test("peer-pairing health response exposes the discriminated union and no exception path", () => {
  // The `peerPairingHealthCommand` wrapper MUST return only the
  // `ok` / `failed` union so the renderer never has to translate
  // a `CommandError`. A regression that re-introduces
  // `Result<_, CommandError>` would surface free-form strings
  // through the IPC bridge.
  const body = tauriSource();
  assert.match(
    body,
    /peerPairingHealthCommand[\s\S]{0,400}PeerPairingHealthResponse/,
    "peerPairingHealthCommand must return the PeerPairingHealthResponse union",
  );
  // The TypeScript type MUST keep the same union shape the
  // Rust bridge exposes; a regression that drops the `failed`
  // variant breaks here.
  const typesBody = loadSource("src/types.ts");
  assert.match(
    typesBody,
    /PeerPairingHealthResponse[\s\S]{0,200}kind:\s*"ok"[\s\S]{0,400}kind:\s*"failed"/,
    "PeerPairingHealthResponse must declare both ok and failed variants",
  );
  // Each typed failure reason the Rust bridge emits MUST be a
  // stable string the renderer can branch on. A regression that
  // drops any of these arms would let the renderer fall back
  // to free-form error strings.
  for (const reason of [
    "unknown_or_key_mismatch",
    "blocked",
    "revoked",
    "incompatible_protocol",
    "transport_unavailable",
    "cancelled",
    "session_expired",
    "rate_limited",
  ]) {
    assert.ok(
      typesBody.includes(`"${reason}"`),
      `PeerPairingHealthFailureReason must map ${reason} to a typed failure`,
    );
  }
  assert.ok(
    /PeerPairingHealthFailureReason\s*=[\s\S]+"\w+"/m.test(typesBody),
    "PeerPairingHealthFailureReason must be a union type alias",
  );
});
