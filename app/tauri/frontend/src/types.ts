// Type definitions that mirror the Rust types returned by the Tauri
// commands. Keeping them hand-written avoids pulling a Rust-side
// schema generator into the bootstrap.

export interface Diagnostics {
  version: string;
  database_path: string;
  migrations_applied: number;
  started_at: string;
  platform_os: string;
  display_server: string;
  history_entries: number;
  capabilities: Capabilities;
}

export interface DatabasePath {
  path: string;
}

export interface MigrationsApplied {
  count: number;
}

export interface Capabilities {
  clipboard_read: boolean;
  clipboard_write: boolean;
  /**
   * Whether the current session can read a raster image from the
   * clipboard. Reported independently from `clipboard_read`: a host
   * that reads text is not assumed to read images (notably Linux
   * Wayland, where ClipVault has no verifiable image path).
   */
  clipboard_read_image: boolean;
  /**
   * Whether the current session can write a raster image to the
   * clipboard. Reported independently from `clipboard_write` and from
   * `clipboard_read_image`.
   */
  clipboard_write_image: boolean;
  /**
   * Whether the current session can read a rich-text representation
   * alongside plain text. Reported independently from `clipboard_read`:
   * a host that reads text is not assumed to read rich text.
   */
  clipboard_read_rich_text: boolean;
  /**
   * Whether the current session can write a rich-text representation
   * to the clipboard. Reported independently from `clipboard_write` and
   * from `clipboard_read_rich_text`.
   */
  clipboard_write_rich_text: boolean;
  global_hotkey: boolean;
  synthetic_paste: boolean;
  active_application: boolean;
  tray: boolean;
}

export interface CaptureResponse {
  kind: "stored" | "duplicate" | "ignored" | "failed";
  id: number | null;
  message: string | null;
}

export interface WatchTickResponse {
  kind:
    | "captured_stored"
    | "captured_duplicate"
    | "captured_ignored"
    | "captured_failed"
    | "unchanged"
    | "ignored"
    | "failed";
  id: number | null;
  message: string | null;
}

export interface PasteResponse {
  kind: "pasted" | "pasted_plain_fallback" | "failed" | "capability_unavailable";
  id: number | null;
  capability: string | null;
  error_kind: string | null;
  message: string | null;
  guidance: PlatformGuidance | null;
  /**
   * Mode the request resolved to. `null` when the call did not
   * resolve into a paste (the legacy typed error branches).
   */
  mode: string | null;
}

/**
 * Metadata-only response of the copy-only keyboard flow
 * (`clipvault_copy_entry`).
 *
 * The discriminator mirrors the backend [`CopyOutcome`]: a copy flow
 * can never return `pasted` / `pasted_plain_fallback` — those arms
 * are reserved for the paste flow and would conflate "clipboard was
 * written" with "the synthetic paste was triggered". The frontend
 * uses the `copied` / `copied_plain_fallback` arms to hide the
 * window without ever calling the paste controller.
 */
export interface CopyResponse {
  kind: "copied" | "copied_plain_fallback" | "failed" | "capability_unavailable";
  id: number | null;
  capability: string | null;
  error_kind: string | null;
  message: string | null;
  guidance: PlatformGuidance | null;
  /**
   * Mode the request resolved to. `null` when the call did not
   * resolve into a successful write.
   */
  mode: string | null;
}

export type PlatformIssueKind =
  | "permission_required"
  | "unsupported_session"
  | "backend_unavailable"
  | "unknown";

export type PlatformSettingsTarget =
  | "macos_accessibility"
  | "linux_desktop_integration";

export interface PlatformGuidance {
  capability: string;
  kind: PlatformIssueKind;
  title: string;
  summary: string;
  steps: string[];
  retryable: boolean;
  can_open_settings: boolean;
  settings_target: PlatformSettingsTarget | null;
}

export type SettingsOpenResponse =
  | { kind: "opened" }
  | { kind: "fallback_required"; manual_steps: string[] }
  | { kind: "failed"; reason: string };

export interface ActiveApplicationResponse {
  available: boolean;
  name: string | null;
  identifier: string | null;
}

/**
 * Concrete reason a refresh attempt failed. The diagnostics surface
 * exposes this enum so the UI can tell apart `failed:schedule` (Tauri
 * refused to enqueue the closure) from `failed:timeout` (the main
 * thread did not pick it up inside the synchronous timeout) from
 * `failed:unavailable` (the platform probe cannot run on this
 * session — Apple's `NSWorkspace` requires the main thread) from
 * `failed:backend` (the platform probe returned an error).
 */
export type ActiveAppFailureKind =
  | "schedule"
  | "timeout"
  | "unavailable"
  | "backend";

/**
 * Metadata-only snapshot of the cached active-app probe. On macOS the
 * shell installs a `dispatch2` timer on the Cocoa main queue so the
 * cache is updated every second without going through Tauri's
 * `run_on_main_thread` scheduler (the original source of the
 * "Intentos: 214, Correctos: 0, Fallidos: 214" regression). The
 * shape never carries clipboard content, content hashes, snippets or
 * past source-app identifiers — only the platform adapter's current
 * answer, the typed outcome of the most recent refresh, the
 * structured failure cause and the lifecycle counters the shell
 * increments around every refresh attempt.
 */
export type ActiveAppRefreshOutcome =
  | { kind: "pending" }
  | { kind: "ok" }
  | { kind: "failed"; failure_kind: ActiveAppFailureKind; message: string };

export interface ActiveAppDiagnostics {
  available: boolean;
  backend: "macos_workspace" | "x11_ewmh" | "unavailable" | string;
  cache_populated: boolean;
  identifier: string | null;
  name: string | null;
  refresh_outcome: ActiveAppRefreshOutcome;
  /**
   * Snake-case stable string identifying why the most recent
   * refresh failed. Mirrors
   * [`clipvault_core::ActiveAppFailureKind::as_str`] so the UI can
   * render actionable copy without parsing the free-form `message`.
   * `null` for `pending` and `ok` outcomes.
   */
  failure_kind: ActiveAppFailureKind | null;
  loop_started: boolean;
  refresh_attempts: number;
  successful_refreshes: number;
  failed_refreshes: number;
  last_refresh_unix_ms: number | null;
  last_capture_decision: string | null;
}

export interface EntryRecord {
  id: number;
  /**
   * Textual payload. For a `content_type === "image"` row this is the
   * backend's empty sentinel: the card MUST NOT render it as text.
   * Use `isImageEntry` / `hasRenderableImage` from
   * `lib/clipboardAsset.ts` to branch instead of inspecting it.
   */
  content: string;
  content_type: string;
  content_size: number;
  content_hash: string;
  source_app: string | null;
  is_pinned: boolean;
  created_at: string;
  updated_at: string;
  last_seen_at: string;
  /**
   * Optional user-defined card title. `null` (or an empty string)
   * means the card uses the localised label of `content_type` as
   * the title.
   */
  title: string | null;
  /**
   * User-visible source-application name. `null` when the platform
   * adapter could not resolve a name or when the row predates the
   * `history-card-layout` migration.
   */
  source_app_name: string | null;
  /**
   * Opaque, locally-controlled reference to the source-application
   * icon (for example `application-icons/com.apple.textedit.png`).
   * `null` when no icon was extracted.
   */
  source_app_icon_ref: string | null;
  /**
   * Opaque, locally-controlled **relative** reference to the persisted
   * payload asset, always of the form
   * `clipboard/<lowercase-sha256>.png`. `null` for every textual row.
   *
   * This is never an absolute path: the backend rejects anything that
   * is not a relative reference inside its clipboard namespace, and the
   * frontend only ever forwards the value back to
   * `clipvault_clipboard_asset` to obtain bytes. It must never be
   * rendered in the UI.
   */
  asset_ref: string | null;
  /** MIME type of the persisted asset — `"image/png"` in this phase. */
  mime_type: string | null;
  /** Original pixel width of the captured image. */
  payload_width: number | null;
  /** Original pixel height of the captured image. */
  payload_height: number | null;
  /**
   * Deterministic SHA-256 of the canonical rich-text representation
   * the core computed when the row was created. `null` for every plain
   * text and image row. Acts as the secondary dedupe key so two
   * captures with identical plain text but different styles never
   * collapse to a single row. The frontend never echoes this value to
   * the user and never forwards it back to a Tauri command.
   */
  rich_text_hash: string | null;
  /**
   * Opaque, locally-controlled **relative** reference to the original
   * HTML representation of a rich-text capture, of the form
   * `rich-text/<sha256>.html`. `null` when the source did not expose
   * HTML or when the row is not rich. The frontend never echoes this
   * value to the user and never forwards it back to a Tauri command —
   * the rich preview bridge is the only legitimate consumer.
   */
  rich_html_ref: string | null;
  /**
   * Opaque, locally-controlled **relative** reference to the original
   * RTF representation of a rich-text capture, of the form
   * `rich-text/<sha256>.rtf`. `null` when the source did not expose
   * RTF. Same privacy rules as `rich_html_ref`.
   */
  rich_rtf_ref: string | null;
  /**
   * Opaque, locally-controlled **relative** reference to the
   * sanitised rich-text preview of the capture, of the form
   * `rich-text/<sha256>.preview.html`. `null` for every non-rich row.
   * The frontend fetches the bytes through the validated preview
   * bridge; the original HTML and RTF never cross the Tauri boundary
   * through the card surface.
   */
  rich_preview_ref: string | null;
  /** Byte length of the original HTML representation, or `null`. */
  rich_html_size: number | null;
  /** Byte length of the original RTF representation, or `null`. */
  rich_rtf_size: number | null;
  /**
   * Canonical programming-language identifier the
   * `code-language-detection` detector accepted for the capture.
   * `null` for every row predating the change, every textual
   * variant other than `code`, every image / rich-text row and
   * every payload the detector could not classify with sufficient
   * confidence. The value is the canonical, normalised identifier
   * from the allowlist (never an alias) so the SQL index, the
   * label helper and the preview overlay agree byte-for-byte.
   */
  code_language: string | null;
}

export interface SearchHit {
  entry_id: number;
  snippet: string;
  score: number;
  record: EntryRecord;
}

export interface SearchResponse {
  /** Stable note for the UI: "empty_query" | "ok". */
  note: string;
  hits: SearchHit[];
}

export interface EntrySummary {
  id: number;
  is_pinned: boolean;
  updated_at: string;
}

/**
 * Whether a textual history entry is eligible for the
 * `Editar captura` flow. Mirrors the backend predicate:
 * `content_type` is one of the textual variants and the row does
 * not carry image / rich-text metadata that the editor would otherwise
 * destroy. Used by the ellipsis menu to gate the action and by the
 * `EntryTextEditorModal` to fail fast when the entry is stale.
 */
export function isEditableTextEntry(entry: EntryRecord): boolean {
  if (entry.content_type === "image") return false;
  if (entry.asset_ref != null || entry.mime_type != null) return false;
  if (entry.payload_width != null || entry.payload_height != null) {
    return false;
  }
  if (entry.rich_text_hash != null) return false;
  if (entry.rich_html_ref != null) return false;
  if (entry.rich_rtf_ref != null) return false;
  if (entry.rich_preview_ref != null) return false;
  return true;
}

export type SetFavoriteResponse =
  | { kind: "updated"; entry: EntrySummary }
  | { kind: "not_found" };

/**
 * Discriminated response of [`updateTextEntryCommand`].
 *
 * - `updated`: the row was rewritten in place. The bridge returns the
 *   refreshed record so the rail can patch its in-memory list.
 * - `noop`: the submitted text was byte-for-byte equal to the
 *   persisted one. The frontend receives the previous record and
 *   can keep the visible card as it was.
 * - `not_found`: the entry disappeared between the rail read and
 *   the edit request.
 * - `not_editable`: the entry is an image or carries rich-text
 *   metadata the editor must not touch.
 * - `empty_content`: the trimmed input is empty.
 * - `duplicate_content`: another live row already owns the canonical
 *   hash the new text would produce. The previous entry stays
 *   untouched.
 *
 * The bridge never carries clipboard content, hashes, snippets or
 * asset references in either direction; only the typed `kind` and
 * the refreshed record travel across the IPC.
 */
export type UpdateTextEntryResponse =
  | { kind: "updated"; entry: EntryRecord }
  | { kind: "noop"; entry: EntryRecord }
  | { kind: "not_found" }
  | { kind: "not_editable" }
  | { kind: "empty_content" }
  | { kind: "duplicate_content" };

export type DeleteResponse =
  | { kind: "removed"; removed: number }
  | { kind: "not_found" }
  | { kind: "confirmation_required" };

export type ClearResponse =
  | { kind: "removed"; removed: number }
  | { kind: "confirmation_required" };

export type RetentionPolicy = "forever" | "days_7" | "days_30" | "days_90";

export interface RetentionResponse {
  policy: RetentionPolicy;
  removed: number;
}

export interface RetentionPreview {
  policy: RetentionPolicy;
  would_remove: number;
}

export interface HotkeySpec {
  id: string;
  key: string;
  cmd_or_ctrl: boolean;
  shift: boolean;
  alt: boolean;
  meta: boolean;
}

export interface Settings {
  retention: RetentionPolicy;
  ignored_apps: string[];
  quick_paste_hotkey: HotkeySpec | null;
  /**
   * Validated visible device name for the local peer identity
   * foundation. Always present (even when `null`) so the Settings
   * panel renders the Identity section with a stable shape.
   */
  local_peer_display_name: string | null;
}

export interface SettingsUpdate {
  retention?: RetentionPolicy;
  ignored_apps_add?: string[];
  ignored_apps_remove?: string[];
  quick_paste_hotkey?: HotkeySpec | null;
  local_peer_display_name?: string | null;
}

/**
 * Metadata-only projection the frontend reads through
 * `clipvault_local_peer_profile_get`. Carries the cryptographic
 * identity the secure store already minted plus the validated
 * display name. The private key bytes, the raw public key bytes
 * and any other secret material NEVER cross the bridge; the spec
 * keeps the wire surface narrow on purpose so the local identity
 * foundation cannot accidentally expose enough material to attempt
 * impersonation when the future pairing change lands.
 */
export interface LocalPeerProfile {
  peer_id: string;
  fingerprint: string;
  display_name: string | null;
}

/**
 * Discriminated response of `clipvault_local_peer_profile_get`
 * and `clipvault_local_peer_profile_update`. The frontend
 * branches on `kind` to render the right copy (success vs
 * unavailable) without inspecting free-form strings.
 */
export type LocalPeerProfileResponse =
  | { kind: "available"; profile: LocalPeerProfile }
  | { kind: "unavailable"; reason: string };

/**
 * Discriminated response of `clipvault_peer_sharing_toggle_get`
 * and `clipvault_peer_sharing_toggle_set`. The union keeps the
 * runtime state and the persisted flag on the same response so
 * the Settings panel can render the toggle and the status copy
 * without a follow-up round-trip. The `Equipos` view renders
 * the `peer_snapshot` shape; this DTO is metadata-only too.
 *
 * - `active`: sharing is on AND the runtime is browsing the
 *   LAN.
 * - `identity_unavailable`: sharing is on but the secure
 *   identity store is unreachable on this session.
 * - `runtime_stopped`: sharing is on but the platform cannot
 *   expose a multicast / mDNS path right now (firewall,
 *   router, …). The UI shows the documented degraded copy.
 */
export type PeerSharingToggleResponse =
  | { kind: "active"; enabled: true }
  | { kind: "identity_unavailable"; enabled: true }
  | { kind: "runtime_stopped"; enabled: boolean };

/**
 * Stable presence discriminator the `Equipos` view renders.
 * Mirrors `clipvault_core::PeerPresence::as_str` so the UI can
 * branch on the string without parsing free-form text.
 */
export type PeerPresence = "detected" | "not_available" | "unverified";

/**
 * Single metadata-only row the `Equipos` view renders. The
 * struct never carries IP addresses, ports, raw public keys,
 * clipboard content, previews, hashes or source identifiers;
 * only the validated discovery metadata the runtime persists.
 */
export interface PeerSnapshotEntry {
  peer_id: string;
  public_key_fingerprint: string;
  display_name: string;
  protocol_major: number;
  capability: string;
  first_seen_at: string;
  last_discovered_at: string;
  /** True when the runtime observed the peer inside the presence TTL. */
  is_present: boolean;
  presence: PeerPresence;
}

/**
 * Response of `clipvault_peer_snapshot`. The runtime reports
 * `sharing_active = false` whenever it cannot browse; the
 * `sharing_inactive_reason` string carries one of the
 * documented values the frontend branches on
 * (`identity_unavailable`, `runtime_stopped`, `disabled`).
 */
export interface PeerSnapshot {
  entries: PeerSnapshotEntry[];
  sharing_active: boolean;
  sharing_inactive_reason: string | null;
}

/**
 * Stable validation error codes the Identity section reads from
 * `clipvault_local_peer_profile_update`. Mirrors the Rust
 * `clipvault_core::ValidationCode` enum so the panel never has
 * to inspect free-form strings.
 */
export type PeerDisplayNameErrorCode =
  | "invalid_peer_display_name"
  | "peer_display_name_too_long";

/**
 * Trust state the pairing runtime persists in `known_peers`.
 * Mirrors [`clipvault_db::TrustState`] so the frontend can
 * render the matching copy without inspecting free-form strings.
 *
 * - `unverified`: discovery has observed the peer; the user has
 *   not started (or has rejected / cancelled) a pairing session.
 * - `trusted`: the reciprocal SAS exchange completed; health
 *   probes are accepted on the mTLS-pinned channel.
 * - `revoked`: the user disconnected. A fresh pairing is required
 *   to restore trust.
 * - `blocked`: the user blocked the peer. Pairing and health
 *   probes are rejected before any cryptographic work runs.
 */
export type PeerTrustState =
  | "unverified"
  | "trusted"
  | "revoked"
  | "blocked";

/**
 * Discriminated response of every `clipvault_peer_pairing_*`
 * command the bridge exposes. The frontend branches on `kind`
 * to render the matching modal state without inspecting the
 * typed variants on the Rust side.
 */
export type PeerPairingOutcomeResponse =
  | {
      kind: "trusted";
      peer_id: string;
      display_name: string;
      short_fingerprint: string;
      paired_at: string;
    }
  | { kind: "awaiting_remote_approval"; session_id: number }
  | {
      kind: "failed";
      reason:
        | "session_expired"
        | "cancelled"
        | "incompatible_protocol"
        | "unknown_or_key_mismatch"
        | "blocked"
        | "revoked"
        | "rate_limited"
        | "transport_unavailable";
    };

/**
 * Metadata-only snapshot of every in-flight pairing session
 * the runtime holds. The modal renders the SAS code next to the
 * session id; the runtime expires the entries automatically so
 * the snapshot is always in sync with the state machine's hard
 * two-minute timeout.
 *
 * The optional `cert_fingerprint` field carries the SHA-256 of
 * the remote peer's TLS cert DER. The runtime persists this
 * value in `known_peers.tls_cert_fingerprint` after the
 * dual-approval gate promotes the row and forwards it to the
 * productive pairing transport so the next mTLS handshake is
 * pinned against it.
 */
export interface PeerPairingSessionSnapshot {
  session_id: number;
  remote_peer_id: string;
  remote_fingerprint: string;
  remote_display_name: string;
  local_approved: boolean;
  remote_approved: boolean;
  sas: string;
  expires_at: string;
  cert_fingerprint?: string | null;
}

/**
 * Discriminated response of the trust-state transition
 * commands the bridge exposes. The frontend branches on `kind`
 * to render the matching copy and to refresh the snapshot.
 */
export type PeerTrustOperationResponse =
  | { kind: "stored"; trust_state: PeerTrustState }
  | { kind: "conflict"; trust_state: PeerTrustState }
  | { kind: "unknown" };

/**
 * Stable wire representation of the productive health probe.
 * The transport dials the remote listener over mTLS, exchanges
 * the bounded `health` envelope, and returns only the typed
 * response so the bridge never crosses free-form strings. The
 * `failed` variant carries a stable `reason` the renderer can
 * branch on without inspecting free-form messages.
 */
export type PeerPairingHealthResponse =
  | { kind: "ok"; peer_id: string; protocol_major: number }
  | { kind: "failed"; reason: PeerPairingHealthFailureReason };

export type PeerPairingHealthFailureReason =
  | "unknown_or_key_mismatch"
  | "blocked"
  | "revoked"
  | "incompatible_protocol"
  | "transport_unavailable"
  | "cancelled"
  | "session_expired"
  | "rate_limited";

/**
 * Augmented metadata-only row the `Equipos` view renders when
 * the pairing change is enabled. The DTO extends the discovery
 * snapshot entry with the trust state the pairing runtime
 * persists so the modal can branch on the row without an extra
 * round-trip. The `peer_id`, `display_name`,
 * `short_fingerprint`, `paired_at`, `last_discovered_at` and
 * `presence` fields keep the same wire contract the
 * `local-peer-discovery` change exposes.
 */
export interface PeerRow {
  peer_id: string;
  display_name: string;
  short_fingerprint: string;
  trust_state: PeerTrustState;
  presence: "detected" | "not_available" | "unverified";
  paired_at: string | null;
  last_discovered_at: string;
}

/**
 * Metadata-only record the privacy modal renders for every
 * blacklisted application. The frontend MUST treat the shape as
 * metadata-only: no clipboard content, hashes or snippets ever
 * appear in this object.
 */
export interface IgnoredAppEntry {
  /** Normalised identifier used by the matcher. */
  id: string;
  /** Visible application name. `null` for legacy rows without metadata. */
  display_name: string | null;
  /**
   * Opaque, locally-controlled reference for the application icon.
   * `null` when the row has no icon metadata or the extraction
   * failed.
   */
  icon_ref: string | null;
  /** RFC 3339 UTC timestamp. */
  created_at: string;
}

/**
 * Stable picker error reason. Mirrors the Rust
 * `clipvault_core::IgnoredAppError::kind_str` taxonomy so the UI
 * can render the matching copy without parsing free-form messages.
 */
export type PickErrorReason =
  | "cancelled"
  | "invalid_selection"
  | "missing_identifier"
  | "backend_unavailable"
  | "unsupported_session"
  | "persistence_error";

/**
 * Response of [`clipvault_ignored_app_pick_and_add`]. The
 * discriminated union lets the panel branch on `kind` and apply the
 * matching UI without inspecting free-form strings. `Cancelled` is
 * not an error.
 */
export type PickAndAddResponse =
  | { kind: "added"; entry: IgnoredAppEntry }
  | { kind: "updated"; entry: IgnoredAppEntry }
  | { kind: "cancelled" }
  | { kind: "error"; reason: PickErrorReason; message: string };

// ---------------------------------------------------------------------------
// Linux picker (`linux-blacklist-app-picker` capability).
//
// The Linux picker mirrors the macOS flow but presents a list of
// installed `.desktop` files the user can pick from. The
// frontend never executes any helper process: the catalog returns a
// deterministic identifier the active-app adapter publishes, the
// user picks one and the frontend forwards the selection to the
// `clipvault_ignored_app_linux_add` command.
// ---------------------------------------------------------------------------

/**
 * Strategy the catalog used to compute the identifier. Mirrors the
 * Rust enum. The UI surfaces the strategy in the picker header so the
 * user can distinguish X11 / Wayland native (`wm_class`) candidates
 * from GNOME Wayland (`desktop_file_id`) candidates.
 */
export type LinuxPickerStrategy = "wm_class" | "desktop_file_id";

/**
 * One entry the Linux picker catalog exposes. The frontend renders
 * every entry as a row in the picker modal; the backend persists the
 * row the user clicked.
 */
export interface LinuxPickerCandidate {
  /** Deterministic identifier the active-app adapter publishes. */
  identifier: string;
  /** User-visible display name. `null` when the `.desktop` lacks `Name=`. */
  display_name: string | null;
  /** Opaque icon reference. `null` when the icon could not be persisted. */
  icon_ref: string | null;
  strategy: LinuxPickerStrategy;
}

/**
 * Discriminated response of [`clipvault_ignored_app_linux_catalog`].
 *
 * - `supported` carries the deterministic identifier strategy the
 *   catalog applied and the candidate list the frontend renders.
 * - `unsupported` keeps the manual entry surface enabled; the
 *   reason is a metadata-only diagnostic string the UI may show in a
 *   tooltip but never parses as a key.
 */
export type LinuxCatalogResponse =
  | {
      kind: "supported";
      strategy: LinuxPickerStrategy;
      candidates: LinuxPickerCandidate[];
    }
  | { kind: "unsupported"; reason: string };

/**
 * Response of [`clipvault_ignored_app_linux_add`]. Mirrors the
 * macOS picker shape minus the `cancelled` variant (the catalog
 * flow has no equivalent of the user dismissing a native dialog).
 */
export type LinuxPickAndAddResponse =
  | { kind: "added"; entry: IgnoredAppEntry }
  | { kind: "updated"; entry: IgnoredAppEntry };

export interface CommandError {
  kind: string;
  message: string;
}

// ---------------------------------------------------------------------------
// `tags-and-collections` capability.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// `gnome-wayland-integration` capability.
// ---------------------------------------------------------------------------

/**
 * Tauri command name the frontend uses to read the GNOME Shell
 * integration status.
 */
export const GNOME_INTEGRATION_STATUS_COMMAND =
  "clipvault_gnome_integration_status";

/**
 * User-facing GNOME integration consent + technical state. Carries
 * only metadata — never clipboard content, source-app identifiers or
 * environment variables.
 */
export type GnomeConsentDecision =
  | "unknown"
  | "accepted"
  | "declined"
  | "disabled";

/**
 * Stable lifecycle states the GNOME integration publishes through the
 * diagnostics endpoint. The strings mirror
 * [`clipvault_core::GnomeTechnicalState::as_str`] byte-for-byte.
 */
export type GnomeTechnicalState =
  | "not_installed"
  | "disabled"
  | "incompatible"
  | "activation_pending"
  | "connected"
  | "identified"
  | "no_active_application"
  | "disconnected"
  | "communication_error"
  | "unavailable";

/**
 * Outcome of the GNOME integration status command. The
 * discriminated union keeps the per-variant fields stable so the
 * UI can branch on `kind` and follow every transition without
 * having to inspect a free-form message.
 */
export type GnomeIntegrationStatusResponse =
  | { kind: "not_applicable"; session: string; desktop: string }
  | { kind: "ready"; payload: GnomeIntegrationPayload }
  | { kind: "not_configured"; reason: string };

/**
 * Metadata-only payload emitted by the GNOME integration status
 * endpoint. Every field is optional; absent values render the
 * fallback copy the design documents.
 */
/**
 * Metadata-only payload emitted by the GNOME integration status
 * endpoint. Every field is metadata; absolute filesystem paths,
 * socket paths, clipboard content and environment variables never
 * leave the Rust process.
 */
export interface GnomeIntegrationPayload {
  applicable: boolean;
  session: string;
  desktop: string;
  consent: GnomeConsentDecision;
  technical_state: GnomeTechnicalState;
  installed: boolean;
  identifier: string | null;
  detail: string | null;
  uuid: string;
  backend: string;
  protocol_version: number;
}

/**
 * Result of `clipvault_gnome_integration_install`. Mirrors the typed
 * Rust `InstallResult`. The payload only carries metadata: no
 * clipboard content, no source-app identifiers, no environment
 * variables.
 */
export interface GnomeIntegrationInstallResult {
  installation: {
    installed: boolean;
    enabled: boolean;
    uuid: string;
    version: number;
  };
  status: GnomeIntegrationPayload;
  diagnostics: {
    state: string;
    backend: string;
    identifier: string | null;
    detail: string | null;
    last_probe_stage: string;
  };
}

export type CollectionKind = "system" | "user";

export interface Collection {
  id: number;
  /** Stable key reserved for system collections (`history` for `Historial`). */
  stable_key: string | null;
  name: string;
  kind: CollectionKind;
  /**
   * Normalised opaque RGB colour the sidebar square and the
   * per-card collection labels render. The backend rejects any
   * value that does not match `#rrggbb`; the frontend therefore
   * treats the string as metadata-only and never inspects its
   * bytes to drive accessibility or styling decisions.
   */
  color_hex: string;
  created_at: string;
  updated_at: string;
}

export interface Tag {
  id: number;
  /** Case-folded, whitespace-collapsed identity. */
  normalized_name: string;
  /** Readable form preserving the user-supplied casing. */
  display_name: string;
  created_at: string;
  updated_at: string;
}

export interface OrganizationSnapshot {
  collections: Collection[];
  tags: Tag[];
}

// ---------------------------------------------------------------------------
// `source-app-filter` capability.
// ---------------------------------------------------------------------------

/**
 * Wire-level filter the combobox sends on every recents/search request.
 * The discriminator prevents the frontend from collapsing the "Unknown"
 * branch into the "All" branch: both surface a different rail set and the
 * user can target either on purpose.
 *
 * - `all`: no restriction; the recents/search query applies the rest of the
 *   filter set as usual.
 * - `known`: pin the candidate set to a single stable `source_app`
 *   identifier. The identifier is opaque; it MUST NOT be rendered as
 *   visible text by the combobox.
 * - `unknown`: rows whose `source_app` is `null` or empty.
 */
export type SourceAppFilter =
  | { kind: "all" }
  | { kind: "known"; source_app: string }
  | { kind: "unknown" };

/**
 * Single metadata-only entry the combobox renders. `source_app` is the
 * stable internal identifier used by every equality predicate; the
 * frontend treats it as opaque and never displays it. `display_name` is
 * the user-visible label. `icon_ref` is the relative reference under
 * `application-icons/` resolved through the existing icon bridge.
 * `fallback` is `true` when the combobox must render the generic glyph
 * because no persisted icon was found.
 */
export interface SourceApplicationOption {
  source_app: string | null;
  display_name: string;
  icon_ref: string | null;
  fallback: boolean;
}

// ---------------------------------------------------------------------------
// `tag-filter` capability.
// ---------------------------------------------------------------------------

/**
 * Wire-level filter the desktop combobox sends on every recents/search
 * request. The discriminator mirrors the source-app filter shape so
 * the two comboboxes can be composed in one place:
 *
 * - `all`: no restriction; the recents/search query applies the rest
 *   of the filter set as usual.
 * - `tag`: pin the candidate set to a single tag id. The id is the
 *   stable `Tag.id` value the rest of the desktop consumes; the
 *   frontend treats it as opaque and never displays it directly.
 *
 * The combobox keeps `Todas` as the first option so the default view
 * reproduces the pre-tag-filter rail byte-for-byte.
 */
export type TagFilter =
  | { kind: "all" }
  | { kind: "tag"; tagId: number };

/**
 * Single metadata-only entry the tag combobox renders. The frontend
 * reads `display_name` for the visible label; `id` is the stable
 * internal identifier the filter forwards to the recents/search
 * request.
 */
export interface TagFilterOption {
  id: number;
  display_name: string;
}

/**
 * Metadata describing the scope a `SourceApplicationsSnapshot` was
 * computed against. Returned alongside the options so the frontend can
 * detect a stale response (a collection switch that landed while the
 * query was in flight, for example) and drop it without polluting the
 * combobox.
 */
export interface SourceApplicationsScope {
  collection_id: number | null;
  tag_ids: number[];
}

/**
 * Result of `clipvault_source_applications`. `Todas` is always the
 * first option; `Aplicación desconocida` follows when the scope
 * actually contains rows with no source identifier. The remaining
 * entries are the known applications, ordered case-insensitive by
 * `display_name` and using the stable identifier as the deterministic
 * tiebreaker.
 */
export interface SourceApplicationsSnapshot {
  options: SourceApplicationOption[];
  scope: SourceApplicationsScope;
}

export type ClipvaultCommand<T> = () => Promise<T>;
export type ClipvaultCommandArg<T, A> = (arg: A) => Promise<T>;
