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

export type SetFavoriteResponse =
  | { kind: "updated"; entry: EntrySummary }
  | { kind: "not_found" };

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
}

export interface SettingsUpdate {
  retention?: RetentionPolicy;
  ignored_apps_add?: string[];
  ignored_apps_remove?: string[];
  quick_paste_hotkey?: HotkeySpec | null;
}

/**
 * Metadata-only record the settings panel renders for every
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

export interface CommandError {
  kind: string;
  message: string;
}

// ---------------------------------------------------------------------------
// `tags-and-collections` capability.
// ---------------------------------------------------------------------------

export type CollectionKind = "system" | "user";

export interface Collection {
  id: number;
  /** Stable key reserved for system collections (`history` for `Historial`). */
  stable_key: string | null;
  name: string;
  kind: CollectionKind;
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

export type ClipvaultCommand<T> = () => Promise<T>;
export type ClipvaultCommandArg<T, A> = (arg: A) => Promise<T>;
