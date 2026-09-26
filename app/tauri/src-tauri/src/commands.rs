//! Thin Tauri commands. Each command is a one-liner over
//! `clipvault-core` and forwards the typed outcome to the frontend.

use std::sync::Arc;

#[cfg(all(
    feature = "local-peer-pairing-tls",
    feature = "local-peer-discovery-mdns"
))]
use clipvault_core::{
    peer_pairing::TransportError,
    peer_pairing::{PairingAdvertisement, TransportSink},
};
use clipvault_core::{
    ActiveAppDiagnostics, Capabilities, ClearOutcome, ClipboardAssetStore,
    CodeLanguageServiceError, CopyOutcome, DeleteOutcome, IgnoredAppEntry, IgnoredAppError,
    LocalSettingsReader, PasteMode, PasteOutcome, PickAndAddOutcome, PlatformGuidance,
    PlatformSettingsTarget, RetentionOutcome, RetentionPolicy, RetentionPreview,
    RichTextAssetStore, SetFavoriteResult, SetTitleOutcome, Settings, SettingsNavigator,
    SettingsOpenOutcome, SettingsServiceError, SettingsUpdate, TitleValidationError,
    UpdateTextHistoryOutcome, ValidationCode, ValidationError, WatchTickOutcome,
};
use clipvault_platform::{
    read_icon_bytes, read_source_app_icon_bytes, ActiveAppError, IconReadError,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime, State};
use tracing::warn;

use crate::state::SharedState;

/// Synchronise the process-local GNOME lifecycle state before a Linux picker
/// operation. The GNOME bridge may be connected while the persisted fallback
/// still says `activation_pending`; the picker must use the live enum without
/// writing each focus transition to SQLite.
#[cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]
fn sync_linux_picker_gnome_runtime_state(state: &SharedState) {
    if let Some(gnome) = state.app_state().gnome_integration.as_ref() {
        gnome.sync_runtime_technical_state();
    }
}

#[cfg(not(all(target_os = "linux", feature = "linux-gnome-shell-integration")))]
fn sync_linux_picker_gnome_runtime_state(_state: &SharedState) {}

/// Metadata-only event the shell fires after every successful
/// organization mutation. The payload is `()` — the frontend never
/// inspects the event details, it only re-reads the organization
/// snapshot and the per-entry association list. Keeping the payload
/// metadata-only is the contract that prevents clipboard content,
/// tag bodies, source identifiers or hashes from ever crossing the
/// event channel.
///
/// The constant is pinned across releases so a backend rename
/// surfaces as a frontend test failure instead of silently dropping
/// the notification.
pub const ORGANIZATION_UPDATED_EVENT: &str = "clipvault://organization-updated";

/// Emit [`ORGANIZATION_UPDATED_EVENT`] after a successful
/// organization mutation. Failures are logged and swallowed: an
/// emitted event is best-effort telemetry, not a contract the
/// storage layer relies on. The frontend already requests an
/// authoritative refresh through the corresponding Tauri command
/// response, so a missed event only delays the visible refresh.
fn emit_organization_updated<R: Runtime>(handle: &AppHandle<R>) {
    if let Err(error) = handle.emit(ORGANIZATION_UPDATED_EVENT, ()) {
        warn!(error = %error, "failed to emit organization-updated event");
    }
}

/// Emit [`crate::bootstrap::HISTORY_UPDATED_EVENT`] after a
/// successful history mutation (capture, edit, import). The
/// metadata-only payload keeps the contract stable; the bridge
/// re-reads the snapshot through the corresponding Tauri
/// command so a missed event only delays the visible refresh.
fn emit_history_updated<R: Runtime>(handle: &AppHandle<R>) {
    if let Err(error) = handle.emit(crate::bootstrap::HISTORY_UPDATED_EVENT, ()) {
        warn!(error = %error, "failed to emit history-updated event");
    }
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub kind: &'static str,
    pub message: String,
}

impl CommandError {
    pub fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl From<clipvault_core::BootstrapError> for CommandError {
    fn from(err: clipvault_core::BootstrapError) -> Self {
        CommandError::new("bootstrap_error", err.to_string())
    }
}

#[derive(Debug, Serialize)]
pub struct CaptureResponse {
    pub kind: &'static str,
    pub id: Option<i64>,
    pub message: Option<String>,
}

impl CaptureResponse {
    fn from_outcome(outcome: clipvault_core::HistoryOutcome) -> Self {
        let (kind, id, message) = match outcome {
            clipvault_core::HistoryOutcome::Stored { id } => ("stored", Some(id), None),
            clipvault_core::HistoryOutcome::Duplicate { id } => ("duplicate", Some(id), None),
            clipvault_core::HistoryOutcome::Ignored => ("ignored", None, None),
            clipvault_core::HistoryOutcome::Failed { message } => ("failed", None, Some(message)),
        };
        Self { kind, id, message }
    }
}

#[derive(Debug, Serialize)]
pub struct WatchTickResponse {
    pub kind: &'static str,
    pub id: Option<i64>,
    pub message: Option<String>,
}

impl WatchTickResponse {
    fn from_outcome(outcome: WatchTickOutcome) -> Self {
        match outcome {
            WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Stored { id }) => Self {
                kind: "captured_stored",
                id: Some(id),
                message: None,
            },
            WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Duplicate { id }) => Self {
                kind: "captured_duplicate",
                id: Some(id),
                message: None,
            },
            WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Ignored) => Self {
                kind: "captured_ignored",
                id: None,
                message: None,
            },
            WatchTickOutcome::Captured(clipvault_core::HistoryOutcome::Failed { message }) => {
                Self {
                    kind: "captured_failed",
                    id: None,
                    message: Some(message),
                }
            }
            WatchTickOutcome::Unchanged => Self {
                kind: "unchanged",
                id: None,
                message: None,
            },
            WatchTickOutcome::Suppressed => Self {
                // Surface the suppression as a distinct kind so the
                // frontend can react (or simply ignore) it without
                // mistaking it for a `captured_ignored` that came
                // from the privacy gate. The response is otherwise
                // payload-free.
                kind: "suppressed",
                id: None,
                message: None,
            },
            WatchTickOutcome::Ignored => Self {
                kind: "ignored",
                id: None,
                message: None,
            },
            WatchTickOutcome::Failed { message } => Self {
                kind: "failed",
                id: None,
                message: Some(message),
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PasteResponse {
    /// Stable discriminator. Includes `pasted_plain_fallback` so the
    /// frontend can recognise an explicit downgrade without parsing
    /// free-form strings.
    pub kind: &'static str,
    pub id: Option<i64>,
    pub capability: Option<&'static str>,
    pub error_kind: Option<&'static str>,
    pub message: Option<String>,
    pub guidance: Option<PlatformGuidance>,
    /// Mode the request resolved to. `None` when the call did not
    /// resolve into a paste (the legacy typed error branches).
    pub mode: Option<&'static str>,
}

impl PasteResponse {
    fn from_outcome(outcome: PasteOutcome) -> Self {
        match outcome {
            PasteOutcome::Pasted { id } => Self {
                kind: "pasted",
                id: Some(id),
                capability: None,
                error_kind: None,
                message: None,
                guidance: None,
                mode: None,
            },
            PasteOutcome::PastedPlainFallback { id } => Self {
                kind: "pasted_plain_fallback",
                id: Some(id),
                capability: None,
                error_kind: None,
                message: None,
                guidance: None,
                mode: Some("plain"),
            },
            PasteOutcome::Failed {
                kind,
                message,
                guidance,
            } => Self {
                kind: "failed",
                id: None,
                capability: None,
                error_kind: Some(kind),
                message: Some(message),
                guidance,
                mode: None,
            },
            PasteOutcome::CapabilityUnavailable {
                capability,
                guidance,
            } => Self {
                kind: "capability_unavailable",
                id: None,
                capability: Some(capability),
                error_kind: None,
                message: None,
                guidance,
                mode: None,
            },
        }
    }
}

/// Response of [`clipvault_copy_entry`].
///
/// Mirrors [`PasteResponse`] minus the `pasted` / `pasted_plain_fallback`
/// variants — the copy-only flow never triggers a synthetic paste and
/// must never pretend it did. The discriminator is the stable snake
/// case `kind` value the frontend branches on.
#[derive(Debug, Serialize)]
pub struct CopyResponse {
    /// Stable discriminator: `copied`, `copied_plain_fallback`,
    /// `failed` or `capability_unavailable`.
    pub kind: &'static str,
    pub id: Option<i64>,
    pub capability: Option<&'static str>,
    pub error_kind: Option<&'static str>,
    pub message: Option<String>,
    pub guidance: Option<PlatformGuidance>,
    /// Mode the request resolved to. `None` when the call did not
    /// resolve into a successful write.
    pub mode: Option<&'static str>,
}

impl CopyResponse {
    fn from_outcome(outcome: CopyOutcome) -> Self {
        match outcome {
            CopyOutcome::Copied { id } => Self {
                kind: "copied",
                id: Some(id),
                capability: None,
                error_kind: None,
                message: None,
                guidance: None,
                mode: None,
            },
            CopyOutcome::CopiedPlainFallback { id } => Self {
                kind: "copied_plain_fallback",
                id: Some(id),
                capability: None,
                error_kind: None,
                message: None,
                guidance: None,
                mode: Some("plain"),
            },
            CopyOutcome::Failed {
                kind,
                message,
                guidance,
            } => Self {
                kind: "failed",
                id: None,
                capability: None,
                error_kind: Some(kind),
                message: Some(message),
                guidance,
                mode: None,
            },
            CopyOutcome::CapabilityUnavailable {
                capability,
                guidance,
            } => Self {
                kind: "capability_unavailable",
                id: None,
                capability: Some(capability),
                error_kind: None,
                message: None,
                guidance,
                mode: None,
            },
        }
    }
}

/// Response of [`clipvault_open_platform_settings`]. Mirrors
/// [`SettingsOpenOutcome`] so the frontend can render the
/// opened/fallback/failed tri-state without parsing free-form strings.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingsOpenResponse {
    Opened,
    FallbackRequired { manual_steps: Vec<String> },
    Failed { reason: String },
}

impl From<SettingsOpenOutcome> for SettingsOpenResponse {
    fn from(outcome: SettingsOpenOutcome) -> Self {
        match outcome {
            SettingsOpenOutcome::Opened => SettingsOpenResponse::Opened,
            SettingsOpenOutcome::FallbackRequired { manual_steps } => {
                SettingsOpenResponse::FallbackRequired { manual_steps }
            }
            SettingsOpenOutcome::Failed { reason } => SettingsOpenResponse::Failed { reason },
        }
    }
}

/// Error returned when the frontend asks for an unknown settings target.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct InvalidTargetError {
    pub kind: &'static str,
    pub target: String,
}

#[derive(Debug, Serialize)]
pub struct ActiveApplicationResponse {
    pub available: bool,
    pub name: Option<String>,
    pub identifier: Option<String>,
}

#[tauri::command]
pub fn clipvault_diagnostics(
    state: State<'_, SharedState>,
) -> Result<clipvault_core::Diagnostics, CommandError> {
    Ok(clipvault_core::DiagnosticsService::snapshot(
        state.context(),
    ))
}

#[tauri::command]
pub fn clipvault_database_path(
    state: State<'_, SharedState>,
) -> Result<clipvault_core::DatabasePath, CommandError> {
    Ok(clipvault_core::DiagnosticsService::database_path(
        state.context(),
    ))
}

#[tauri::command]
pub fn clipvault_migrations_applied(
    state: State<'_, SharedState>,
) -> Result<clipvault_core::MigrationsApplied, CommandError> {
    Ok(clipvault_core::DiagnosticsService::migrations_applied(
        state.context(),
    ))
}

/// Direct capture command. Resolves the source-application
/// identifier from the cached active-app probe (kept fresh on
/// macOS by the main-queue refresher and refreshed on demand by the
/// **Refrescar diagnóstico** button elsewhere) instead of trusting
/// the caller-supplied argument. Centralising the resolution in the
/// backend keeps the `PrivacyGate` and the metadata enrichment
/// consistent across every entry point and prevents the frontend
/// from attributing a capture to ClipVault by convenience.
#[tauri::command]
pub fn clipvault_capture_text(
    state: State<'_, SharedState>,
    source_app: Option<String>,
) -> Result<CaptureResponse, CommandError> {
    let _ = source_app;
    let identifier = crate::bootstrap::resolved_source_identifier(state.context());
    let outcome = state
        .context()
        .history()
        .record_text(state.context(), identifier.as_deref());
    Ok(CaptureResponse::from_outcome(outcome))
}

#[tauri::command]
pub fn clipvault_recent_entries(
    state: State<'_, SharedState>,
    limit: Option<usize>,
) -> Result<Vec<clipvault_db::EntryRecord>, CommandError> {
    let limit = limit.unwrap_or(50).min(500);
    let records = state
        .context()
        .history()
        .recent_entries(state.context(), limit)
        .map_err(|err| CommandError::new("history_error", err.to_string()))?;
    Ok(records)
}

/// Recent entries restricted by collection and/or tags. The arguments
/// mirror the search filter shape: `collection_id == None` and an
/// empty `tag_ids` reproduce the unfiltered "Historial" view. The
/// optional `source_app` argument applies the source-application
/// filter on top of the existing facets without changing ranking,
/// limits or ordering. `None` is treated as [`SourceAppFilter::All`]
/// so existing callers (which never passed a value) keep producing
/// the same row set bit-for-bit.
#[tauri::command]
pub fn clipvault_recent_entries_filtered(
    state: State<'_, SharedState>,
    limit: Option<usize>,
    collection_id: Option<i64>,
    tag_ids: Option<Vec<i64>>,
    source_app: Option<clipvault_core::SourceAppFilter>,
) -> Result<Vec<clipvault_db::EntryRecord>, CommandError> {
    let limit = limit.unwrap_or(50).min(500);
    let tag_ids = tag_ids.unwrap_or_default();
    let source_app = source_app.unwrap_or_default();
    let records = state
        .context()
        .history()
        .recent_entries_with_filter(state.context(), collection_id, &tag_ids, &source_app, limit)
        .map_err(|err| CommandError::new("history_error", err.to_string()))?;
    Ok(records)
}

/// Response of [`clipvault_search_entries`]. Mirrors the service
/// outcome so the frontend can render results and reuse the embedded
/// record once `quick-paste` lands.
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub note: String,
    pub hits: Vec<clipvault_core::SearchEntryHit>,
}

/// Run a search against the local history. The filter arguments are
/// optional and additive: a `collection_id` restricts the candidate
/// set to the entries associated with that collection (passing the
/// `Historial` collection id has no effect because every entry
/// belongs to it), `tag_ids` is AND-combined so the entry must
/// carry every supplied tag, and `source_app` applies the
/// `source-app-filter` facet. All three filters are silently
/// ignored when the corresponding argument is `None` / empty, so a
/// `None` source-app reproduces the pre-extension behaviour
/// bit-for-bit.
#[tauri::command]
pub fn clipvault_search_entries(
    state: State<'_, SharedState>,
    query: String,
    limit: Option<usize>,
    collection_id: Option<i64>,
    tag_ids: Option<Vec<i64>>,
    source_app: Option<clipvault_core::SourceAppFilter>,
) -> Result<SearchResponse, CommandError> {
    let limit = limit.unwrap_or(clipvault_core::SEARCH_DEFAULT_LIMIT);
    let engine_query = clipvault_core::SearchQuery { text: query, limit };
    let filter = clipvault_core::SearchFilter {
        collection_id,
        tag_ids: tag_ids.unwrap_or_default(),
        source_app: source_app.unwrap_or_default(),
    };
    let outcome = state
        .context()
        .search()
        .search_with_filter(state.context(), &engine_query, &filter)
        .map_err(|err| CommandError::new("history_error", err.to_string()))?;
    Ok(SearchResponse {
        note: outcome.note,
        hits: outcome.hits,
    })
}

#[tauri::command]
pub fn clipvault_history_count(
    state: State<'_, SharedState>,
) -> Result<clipvault_core::HistoryCount, CommandError> {
    Ok(clipvault_core::DiagnosticsService::history_count(
        state.context(),
    ))
}

/// List the source applications represented in the active
/// collection scope. The result feeds the combobox the desktop
/// toolbar renders between the search input and the configuration
/// menu: the frontend never inspects clipboard content, hashes or
/// asset references — the snapshot only carries metadata
/// (display name, optional icon reference, fallback flag).
///
/// `collection_id == None` matches the `Historial` system
/// collection: every entry belongs to it by construction, so the
/// query returns the union of all applications the user has ever
/// seen. `tag_ids` is AND-combined, mirroring the recents/search
/// filters so the combobox can mirror whichever secondary facet
/// the user has applied.
#[tauri::command]
pub fn clipvault_source_applications(
    state: State<'_, SharedState>,
    collection_id: Option<i64>,
    tag_ids: Option<Vec<i64>>,
) -> Result<clipvault_core::SourceApplicationsSnapshot, CommandError> {
    let tag_ids = tag_ids.unwrap_or_default();
    let scope = clipvault_core::SourceApplicationsScope::new(collection_id, &tag_ids);
    clipvault_core::SourceApplicationsQuery::new()
        .load(state.context(), &scope)
        .map_err(|err| CommandError::new("history_error", err.to_string()))
}

/// Manual watcher tick driven by the **Tick capture** button. The
/// command shares the same [`CaptureWatcher`] instance as the
/// background capture loop so the dedupe state stays consistent
/// across both entry points.
///
/// The `source_app` argument is intentionally ignored: the shell
/// resolves the identifier from the cached active-app probe so the
/// manual button can never attribute a capture to ClipVault (whose
/// own window is focused when the user presses the button) and the
/// `PrivacyGate` observes the same identifier the background loop
/// would have consulted on the next tick.
#[tauri::command]
pub fn clipvault_capture_tick(
    state: State<'_, SharedState>,
    source_app: Option<String>,
) -> Result<WatchTickResponse, CommandError> {
    let _ = source_app;
    let outcome = state.tick(None);
    Ok(WatchTickResponse::from_outcome(outcome))
}

#[tauri::command]
pub fn clipvault_paste_entry(
    state: State<'_, SharedState>,
    entry_id: i64,
    mode: Option<String>,
) -> Result<PasteResponse, CommandError> {
    let mode = PasteMode::from_wire(mode.as_deref());
    let outcome = state
        .context()
        .paste()
        .paste_entry(state.context(), entry_id, mode);
    Ok(PasteResponse::from_outcome(outcome))
}

/// Copy-only keyboard command used by Quick Paste.
///
/// Writes the type-appropriate representation of `entry_id` to the
/// system clipboard **without** invoking any synthetic paste
/// controller. The user keeps the captured representation available
/// for a later manual `Cmd/Ctrl+V`. The command never modifies the
/// history row and never creates a new card: the suppression arm
/// the paste service runs before the write keeps the watcher quiet.
#[tauri::command]
pub fn clipvault_copy_entry(
    state: State<'_, SharedState>,
    entry_id: i64,
    mode: Option<String>,
) -> Result<CopyResponse, CommandError> {
    let mode = PasteMode::from_wire(mode.as_deref());
    let outcome = state
        .context()
        .paste()
        .copy_entry(state.context(), entry_id, mode);
    Ok(CopyResponse::from_outcome(outcome))
}

#[tauri::command]
pub fn clipvault_platform_capabilities(
    state: State<'_, SharedState>,
) -> Result<Capabilities, CommandError> {
    Ok(state.context().capabilities())
}

#[tauri::command]
pub fn clipvault_active_application(
    state: State<'_, SharedState>,
) -> Result<ActiveApplicationResponse, CommandError> {
    let probe = state.app_state().adapters.active_app();
    match probe.active_application() {
        Ok(Some(app)) => Ok(ActiveApplicationResponse {
            available: true,
            name: Some(app.name),
            identifier: Some(app.identifier),
        }),
        Ok(None) => Ok(ActiveApplicationResponse {
            available: false,
            name: None,
            identifier: None,
        }),
        Err(ActiveAppError::Unavailable) => Ok(ActiveApplicationResponse {
            available: false,
            name: None,
            identifier: None,
        }),
        Err(error) => Err(CommandError::new("active_app_error", error.to_string())),
    }
}

/// Metadata-only snapshot of the cached active-app probe. The shell's
/// background capture loop refreshes this cache synchronously before
/// every poll, so the diagnostics mirror what the `PrivacyGate`
/// observed on the most recent tick. The command never returns
/// clipboard content, hashes, snippets or past source identifiers —
/// only the platform adapter's current answer and the outcome of the
/// most recent refresh.
#[tauri::command]
pub fn clipvault_active_app_diagnostics(
    state: State<'_, SharedState>,
) -> Result<ActiveAppDiagnostics, CommandError> {
    Ok(state.context().active_app_diagnostics())
}

/// Force a synchronous refresh of the cached active-app probe on the
/// Tauri main thread, then return the resulting diagnostics
/// snapshot. Used by the **Refrescar diagnóstico** button in the
/// settings panel so the user can request an on-demand refresh
/// without waiting for the next background-loop tick. Every
/// outcome (`ok`, `failed:schedule`, `failed:timeout`,
/// `failed:backend`) is recorded on the diagnostics state so the
/// returned snapshot reflects the same surface the background loop
/// sees. The command never returns clipboard content.
#[tauri::command]
pub fn clipvault_refresh_active_app_diagnostics(
    state: State<'_, SharedState>,
    handle: tauri::AppHandle<tauri::Wry>,
) -> Result<ActiveAppDiagnostics, CommandError> {
    let diag = crate::bootstrap::refresh_active_app_cached(state.context(), Some(&handle));
    Ok(diag)
}

#[tauri::command]
pub fn clipvault_register_hotkey(
    state: State<'_, SharedState>,
    binding: clipvault_platform::HotkeyBinding,
) -> Result<String, CommandError> {
    let outcome = state
        .app_state()
        .adapters
        .hotkey()
        .register(&binding, Box::new(|| {}))
        .map_err(|err| CommandError::new("hotkey_error", err.to_string()))?;
    Ok(outcome.kind().to_string())
}

#[tauri::command]
pub fn clipvault_shutdown(app: tauri::AppHandle) -> Result<(), CommandError> {
    app.exit(0);
    Ok(())
}

/// Open a known, vetted system settings destination. The frontend can
/// only pick from the [`PlatformSettingsTarget`] enum; the navigator
/// itself rejects unknown targets locally so the shell never launches
/// an arbitrary URL.
#[tauri::command]
pub fn clipvault_open_platform_settings(
    state: State<'_, SharedState>,
    target: PlatformSettingsTarget,
) -> SettingsOpenResponse {
    let navigator: Arc<dyn SettingsNavigator> = state.app_state().adapters.settings_navigator();
    navigator.open(target).into()
}

/// Recompute the capability matrix from the cached platform info. Used
/// after the user returns from the system settings pane so the UI can
/// show the updated support without restarting ClipVault.
#[tauri::command]
pub fn clipvault_refresh_capabilities(
    state: State<'_, SharedState>,
) -> Result<Capabilities, CommandError> {
    state.refresh_capabilities();
    Ok(state.context().capabilities())
}

/// Lightweight summary returned to the frontend so it can render a row
/// without re-fetching the full [`EntryRecord`]. The struct is
/// intentionally metadata-only: no clipboard content, no hash.
#[derive(Debug, Serialize)]
pub struct EntrySummary {
    pub id: i64,
    pub is_pinned: bool,
    pub updated_at: String,
}

impl From<clipvault_db::EntryRecord> for EntrySummary {
    fn from(record: clipvault_db::EntryRecord) -> Self {
        Self {
            id: record.id,
            is_pinned: record.is_pinned,
            updated_at: record.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetFavoriteResponse {
    Updated { entry: EntrySummary },
    NotFound,
}

impl From<SetFavoriteResult> for SetFavoriteResponse {
    fn from(result: SetFavoriteResult) -> Self {
        match result.entry {
            Some(record) => SetFavoriteResponse::Updated {
                entry: record.into(),
            },
            None => SetFavoriteResponse::NotFound,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeleteResponse {
    Removed { removed: usize },
    NotFound,
    ConfirmationRequired,
}

impl From<DeleteOutcome> for DeleteResponse {
    fn from(outcome: DeleteOutcome) -> Self {
        match outcome {
            DeleteOutcome::Removed { removed } => DeleteResponse::Removed { removed },
            DeleteOutcome::NotFound => DeleteResponse::NotFound,
            DeleteOutcome::ConfirmationRequired => DeleteResponse::ConfirmationRequired,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClearResponse {
    Removed { removed: usize },
    ConfirmationRequired,
}

impl From<ClearOutcome> for ClearResponse {
    fn from(outcome: ClearOutcome) -> Self {
        match outcome {
            ClearOutcome::Removed { removed } => ClearResponse::Removed { removed },
            ClearOutcome::ConfirmationRequired => ClearResponse::ConfirmationRequired,
        }
    }
}

#[tauri::command]
pub fn clipvault_set_favorite(
    state: State<'_, SharedState>,
    entry_id: i64,
    pinned: bool,
) -> Result<SetFavoriteResponse, CommandError> {
    let outcome = state
        .context()
        .management()
        .set_favorite(state.context(), entry_id, pinned)
        .map_err(|err| CommandError::new("management_error", err.to_string()))?;
    Ok(outcome.into())
}

#[tauri::command]
pub fn clipvault_delete_entry(
    state: State<'_, SharedState>,
    entry_id: i64,
    confirm: bool,
) -> Result<DeleteResponse, CommandError> {
    let outcome = state
        .context()
        .management()
        .delete_entry(state.context(), entry_id, confirm)
        .map_err(|err| CommandError::new("management_error", err.to_string()))?;
    Ok(outcome.into())
}

#[tauri::command]
pub fn clipvault_clear_history(
    state: State<'_, SharedState>,
    confirm: bool,
) -> Result<ClearResponse, CommandError> {
    let outcome = state
        .context()
        .management()
        .clear_non_favorites(state.context(), confirm)
        .map_err(|err| CommandError::new("management_error", err.to_string()))?;
    Ok(outcome.into())
}

/// Count the non-favorite entries that would be removed by
/// [`clipvault_clear_unorganized_history`]. Lets the frontend render
/// a confirmation message that matches the actual outcome before
/// triggering the destructive operation.
#[tauri::command]
pub fn clipvault_unorganized_clearable_count(
    state: State<'_, SharedState>,
) -> Result<i64, CommandError> {
    state
        .context()
        .management()
        .count_unorganized_clearable(state.context())
        .map_err(|err| CommandError::new("management_error", err.to_string()))
}

/// Remove the "unorganized" history: every non-favorite entry whose
/// only association is the system `Historial` collection. Entries in
/// a user-defined secondary collection, favorite entries and their
/// payload assets are preserved. Requires `confirm: true`; the
/// `ConfirmationRequired` branch keeps the destructive-action
/// contract the rest of the management commands already use.
#[tauri::command]
pub fn clipvault_clear_unorganized_history(
    state: State<'_, SharedState>,
    confirm: bool,
) -> Result<ClearResponse, CommandError> {
    let outcome = state
        .context()
        .management()
        .clear_unorganized_history(state.context(), confirm)
        .map_err(|err| CommandError::new("management_error", err.to_string()))?;
    Ok(outcome.into())
}

#[tauri::command]
pub fn clipvault_apply_retention(
    state: State<'_, SharedState>,
) -> Result<RetentionResponse, CommandError> {
    let reader = LocalSettingsReader::new(state.context().clone(), state.context().clock());
    let outcome = state
        .context()
        .management()
        .apply_retention(state.context(), &reader)
        .map_err(|err| CommandError::new("management_error", err.to_string()))?;
    Ok(outcome.into())
}

#[tauri::command]
pub fn clipvault_retention_preview(
    state: State<'_, SharedState>,
) -> Result<RetentionPreview, CommandError> {
    let reader = LocalSettingsReader::new(state.context().clone(), state.context().clock());
    let preview = state
        .context()
        .management()
        .preview_retention(state.context(), &reader)
        .map_err(|err| CommandError::new("management_error", err.to_string()))?;
    Ok(preview)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RetentionResponse {
    pub policy: RetentionPolicy,
    pub removed: usize,
}

impl From<RetentionOutcome> for RetentionResponse {
    fn from(outcome: RetentionOutcome) -> Self {
        Self {
            policy: outcome.policy,
            removed: outcome.removed,
        }
    }
}

/// Convenience used by the bootstrap to run a retention pass without
/// going through the Tauri runtime. Exposed at the module level so the
/// setup hook can call it directly.
pub fn run_retention(context: &clipvault_core::AppContext) {
    let reader = LocalSettingsReader::new(context.clone(), context.clock());
    if let Err(error) = context.management().apply_retention(context, &reader) {
        tracing::warn!(error = %error, "retention pass failed");
    }
}

// ---------------------------------------------------------------------------
// `privacy-settings` capability commands.
// ---------------------------------------------------------------------------

/// Return the current settings aggregate. The response carries every
/// field the GUI needs, including the persisted retention policy, the
/// ignored-app identifiers and the optional hotkey binding.
#[tauri::command]
pub fn clipvault_settings_get(state: State<'_, SharedState>) -> Result<Settings, CommandError> {
    Ok(state.context().settings().load(state.context()))
}

#[derive(Debug, Serialize)]
pub struct ValidationCommandError {
    pub kind: &'static str,
    pub code: &'static str,
    pub field: &'static str,
    pub message: String,
}

impl ValidationCommandError {
    fn from_validation(error: ValidationError) -> Self {
        let kind = match error.code {
            ValidationCode::Empty => "settings_empty",
            ValidationCode::InvalidRetention => "validation_error",
            ValidationCode::InvalidHotkey => "validation_error",
            ValidationCode::InvalidIdentifier => "validation_error",
            ValidationCode::IdentifierTooLong => "validation_error",
            ValidationCode::InvalidPeerDisplayName => "validation_error",
            ValidationCode::PeerDisplayNameTooLong => "validation_error",
        };
        Self {
            kind,
            code: error.code.as_str(),
            field: error.field,
            message: error.message,
        }
    }
}

impl From<SettingsServiceError> for CommandError {
    fn from(err: SettingsServiceError) -> Self {
        match err {
            SettingsServiceError::Validation(error) => {
                let mapped = ValidationCommandError::from_validation(error);
                CommandError::new(mapped.kind, mapped.message)
            }
            other => CommandError::new("settings_error", other.to_string()),
        }
    }
}

/// Apply a partial settings update. Returns the post-update aggregate so
/// the frontend can refresh without an extra round-trip.
#[tauri::command]
pub fn clipvault_settings_set(
    state: State<'_, SharedState>,
    update: SettingsUpdate,
) -> Result<Settings, CommandError> {
    let outcome = state.context().settings().apply(state.context(), &update)?;
    Ok(outcome)
}

#[tauri::command]
pub fn clipvault_ignored_apps_list(
    state: State<'_, SharedState>,
) -> Result<Vec<String>, CommandError> {
    let settings = state.context().settings().load(state.context());
    Ok(settings.ignored_apps)
}

/// Add an application identifier to the blacklist. Returns the updated
/// list of identifiers.
#[tauri::command]
pub fn clipvault_ignored_apps_add(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Settings, CommandError> {
    let outcome = state
        .context()
        .settings()
        .add_ignored(state.context(), &id)?;
    Ok(outcome)
}

/// Remove an identifier from the blacklist. Returns the updated list.
#[tauri::command]
pub fn clipvault_ignored_apps_remove(
    state: State<'_, SharedState>,
    id: String,
) -> Result<Settings, CommandError> {
    let outcome = state
        .context()
        .settings()
        .remove_ignored(state.context(), &id)?;
    Ok(outcome)
}

/// Typed response of [`clipvault_ignored_app_pick_and_add`]. The
/// frontend never has to inspect the [`CommandError`] for the picker
/// flow: the `kind` field is the stable string the picker surface
/// uses to pick the matching localised copy.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PickAndAddResponse {
    Added { entry: IgnoredAppEntry },
    Updated { entry: IgnoredAppEntry },
    Cancelled,
    Error { reason: String, message: String },
}

impl From<PickAndAddOutcome> for PickAndAddResponse {
    fn from(outcome: PickAndAddOutcome) -> Self {
        match outcome {
            PickAndAddOutcome::Added(entry) => PickAndAddResponse::Added { entry },
            PickAndAddOutcome::Updated(entry) => PickAndAddResponse::Updated { entry },
            PickAndAddOutcome::Cancelled => PickAndAddResponse::Cancelled,
        }
    }
}

impl From<IgnoredAppError> for PickAndAddResponse {
    fn from(error: IgnoredAppError) -> Self {
        PickAndAddResponse::Error {
            reason: error.kind_str().to_string(),
            message: error.to_string(),
        }
    }
}

/// Drive the platform application picker, persist the selection
/// idempotently and update the privacy gate. The command is a thin
/// adapter over [`IgnoredAppsService`]: no platform logic, no Info.plist
/// parsing and no icon handling live here.
///
/// The helper routes the picker through Tauri's main-thread
/// scheduler so `NSOpenPanel` always runs on the thread Apple
/// requires, even when the command body is itself dispatched on a
/// background thread (asynchronous command bodies, threaded runtimes,
/// tests). When the command already runs on the main thread the
/// helper takes the inline path to avoid waiting on itself.
#[tauri::command]
pub fn clipvault_ignored_app_pick_and_add(
    state: State<'_, SharedState>,
    handle: tauri::AppHandle<tauri::Wry>,
) -> PickAndAddResponse {
    match crate::bootstrap::pick_and_add_ignored_app(&handle, state.context()) {
        Ok(outcome) => outcome.into(),
        Err(error) => error.into(),
    }
}

/// List every blacklisted application with the picker metadata.
/// Legacy rows that only carry an identifier are returned with
/// `display_name` and `icon_ref` set to `null` so the frontend can
/// apply its fallback copy.
#[tauri::command]
pub fn clipvault_ignored_apps_list_with_metadata(
    state: State<'_, SharedState>,
) -> Result<Vec<IgnoredAppEntry>, CommandError> {
    let entries = state
        .context()
        .ignored_apps()
        .list(state.context())
        .map_err(|error| CommandError::new("ignored_apps_error", error.to_string()))?;
    Ok(entries)
}

/// Resolve a persisted `icon_ref` to the PNG bytes for the matching
/// application icon. The command is the single bridge that turns the
/// opaque, locally-controlled reference into renderable image data:
/// the frontend never receives an absolute path, never sees the
/// contents of the assets directory and cannot trick the backend into
/// reading an arbitrary location.
///
/// The validation pipeline lives in
/// [`clipvault_platform::resolve_icon_path`] and enforces:
///
/// - the reference must be relative,
/// - it must not contain any `..` component,
/// - it must start with the `ignored-apps/` prefix the picker writes
///   to,
/// - the canonicalised resolved path must stay inside
///   `<data_dir>/assets/ignored-apps/`.
/// - the file must contain the PNG magic header and stay under
///   [`clipvault_platform::MAX_ICON_BYTES`].
///
/// Failures return a [`CommandError`] with the stable
/// [`IconReadError::kind_str`](clipvault_platform::IconReadError)
/// identifier so the frontend can render the matching fallback copy
/// without inspecting the free-form message. The bytes never include
/// clipboard content, hashes or snippets.
#[tauri::command]
pub fn clipvault_ignored_app_icon(
    state: State<'_, SharedState>,
    icon_ref: String,
) -> Result<Vec<u8>, CommandError> {
    let data_dir = state.context().platform().data_dir.clone();
    match read_icon_bytes(&icon_ref, &data_dir) {
        Ok(bytes) => Ok(bytes),
        Err(IconReadError::Ref(reference_error)) => Err(CommandError::new(
            "invalid_icon_ref",
            reference_error.kind_str(),
        )),
        Err(other) => Err(CommandError::new("icon_read_error", other.to_string())),
    }
}

/// Test-friendly handle for [`clipvault_ignored_app_icon`]. The
/// integration suite exercises the same validation pipeline the
/// Tauri command runs without standing up a Tauri runtime, by
/// passing the `data_dir` explicitly instead of reaching into the
/// managed state. Marked `#[allow(dead_code)]` because the binary
/// target does not consume the helper.
#[allow(dead_code)]
pub fn clipvault_ignored_app_icon_for_test(
    data_dir: &std::path::Path,
    icon_ref: String,
) -> Result<Vec<u8>, CommandError> {
    match read_icon_bytes(&icon_ref, data_dir) {
        Ok(bytes) => Ok(bytes),
        Err(IconReadError::Ref(reference_error)) => Err(CommandError::new(
            "invalid_icon_ref",
            reference_error.kind_str(),
        )),
        Err(other) => Err(CommandError::new("icon_read_error", other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Linux visual blacklist picker (`linux-blacklist-app-picker` change).
//
// The Linux picker mirrors the macOS flow but bypasses the
// synchronous `ApplicationPicker::pick()` because Linux sessions
// cannot offer a native `.desktop` chooser without executing an
// arbitrary helper. The catalog presents a list of installed
// `.desktop` files with a deterministic identifier the active-app
// adapter publishes; the frontend shows the list and forwards the
// user's selection to `clipvault_ignored_app_linux_add`.
//
// All the commands below are `#[cfg(target_os = "linux")]` so macOS
// and Windows builds never see the new surface.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LinuxCatalogResponse {
    /// The picker catalog is supported on this session and produced
    /// a list of candidates the frontend can render.
    Supported {
        /// Backend the resolver selected. The frontend surfaces the
        /// label so the user can tell apart X11/XWayland, native
        /// Wayland and GNOME Wayland selections.
        backend: &'static str,
        strategy: &'static str,
        candidates: Vec<clipvault_platform::CandidateApplication>,
    },
    /// The current Linux session cannot guarantee a deterministic
    /// mapping between an installed `.desktop` file and the
    /// identifier the active-app adapter publishes (for example a
    /// Wayland session without any compositors that publish a
    /// stable `app_id`). The frontend keeps the manual-entry
    /// surface enabled.
    Unsupported { reason: String },
}

#[cfg(target_os = "linux")]
#[tauri::command]
pub fn clipvault_ignored_app_linux_catalog(
    state: State<'_, SharedState>,
) -> Result<LinuxCatalogResponse, CommandError> {
    let info = state.context().platform();
    if !matches!(info.os_family, clipvault_platform::OsFamily::Linux) {
        return Ok(LinuxCatalogResponse::Unsupported {
            reason: "linux picker is only available on linux hosts".into(),
        });
    }

    sync_linux_picker_gnome_runtime_state(state.inner());

    // Resolve the picker backend from the runtime state the
    // capture loop is using. The diagnostic surface reports the
    // active-app backend on every refresh; the GNOME integration
    // service reports the consent decision and the technical
    // state. The resolver refuses to invent an identifier when
    // the session cannot guarantee a deterministic mapping.
    let picker_backend = state.context().linux_picker_backend();
    let Some(strategy) = picker_backend.strategy() else {
        tracing::warn!(
            backend = picker_backend.as_str(),
            "linux application picker is unavailable for the active session"
        );
        return Ok(LinuxCatalogResponse::Unsupported {
            reason: format!(
                "linux picker session is not supported ({})",
                picker_backend.as_str()
            ),
        });
    };

    clipvault_ignored_app_linux_catalog_for_test(
        state.context(),
        &info.data_dir.join("assets"),
        strategy,
    )
}

/// Test-friendly handle for [`clipvault_ignored_app_linux_catalog`]
/// so the integration suite can drive the catalog against a
/// deterministic `.desktop` fixture without standing up a Tauri
/// runtime. The `assets_dir` parameter points at the per-session
/// `<data_dir>/assets/` directory the icon writer consumes.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn clipvault_ignored_app_linux_catalog_for_test(
    context: &clipvault_core::AppContext,
    assets_dir: &std::path::Path,
    strategy: clipvault_platform::IdentifierStrategy,
) -> Result<LinuxCatalogResponse, CommandError> {
    use clipvault_platform::runtime::linux_app_catalog::LinuxApplicationCatalog;

    if !matches!(
        context.platform().os_family,
        clipvault_platform::OsFamily::Linux
    ) {
        return Ok(LinuxCatalogResponse::Unsupported {
            reason: "linux picker is only available on linux hosts".into(),
        });
    }

    let catalog = LinuxApplicationCatalog::new(assets_dir, strategy);
    // The catalog command resolves the picker backend independently
    // of the `for_test` helper so the test surface can keep the
    // caller-supplied strategy while the production command only
    // ever runs through the resolver.
    let picker_backend = context.linux_picker_backend();
    match catalog.list() {
        Ok(candidates) if !candidates.is_empty() => Ok(LinuxCatalogResponse::Supported {
            backend: picker_backend.as_str(),
            strategy: strategy.as_str(),
            candidates,
        }),
        Ok(_) => {
            // An empty catalog is not a usable picker surface:
            // the user would see an empty modal. Fall back to
            // `Unsupported` so the manual-entry surface stays the
            // documented fallback.
            tracing::warn!(
                backend = picker_backend.as_str(),
                "linux application picker catalog is empty"
            );
            Ok(LinuxCatalogResponse::Unsupported {
                reason: "linux picker catalog is empty for this session".into(),
            })
        }
        Err(error) => {
            tracing::warn!(error = %error, "linux picker catalog enumeration failed");
            Ok(LinuxCatalogResponse::Unsupported {
                reason: error.to_string(),
            })
        }
    }
}

/// Test-friendly handle that drives the catalog against an
/// in-memory filesystem fixture so the integration suite can pin
/// the candidate list without depending on the host's installed
/// `.desktop` files. The integration tests for the picker
/// resolution matrix use this surface so they can assert the
/// `Supported` / `Unsupported` boundary deterministically instead
/// of riding on whatever the host happens to install.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn clipvault_ignored_app_linux_catalog_for_test_with_fs(
    assets_dir: &std::path::Path,
    strategy: clipvault_platform::IdentifierStrategy,
    backend: clipvault_platform::LinuxPickerBackend,
    fs: std::sync::Arc<dyn clipvault_platform::runtime::linux_app_metadata::DesktopFilesystem>,
) -> Result<LinuxCatalogResponse, CommandError> {
    use clipvault_platform::runtime::linux_app_catalog::LinuxApplicationCatalog;

    let catalog = LinuxApplicationCatalog::with_filesystem(assets_dir, fs, strategy);
    match catalog.list() {
        Ok(candidates) if !candidates.is_empty() => Ok(LinuxCatalogResponse::Supported {
            backend: backend.as_str(),
            strategy: strategy.as_str(),
            candidates,
        }),
        Ok(_) => Ok(LinuxCatalogResponse::Unsupported {
            reason: "linux picker catalog is empty for this session".into(),
        }),
        Err(error) => {
            tracing::warn!(error = %error, "linux picker catalog enumeration failed");
            Ok(LinuxCatalogResponse::Unsupported {
                reason: error.to_string(),
            })
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LinuxPickAndAddResponse {
    Added { entry: IgnoredAppEntry },
    Updated { entry: IgnoredAppEntry },
}

#[cfg(target_os = "linux")]
#[tauri::command]
pub fn clipvault_ignored_app_linux_add(
    state: State<'_, SharedState>,
    identifier: String,
    display_name: Option<String>,
    icon_ref: Option<String>,
) -> Result<LinuxPickAndAddResponse, CommandError> {
    sync_linux_picker_gnome_runtime_state(state.inner());
    clipvault_ignored_app_linux_add_for_test(
        state.context(),
        &identifier,
        display_name.as_deref(),
        icon_ref.as_deref(),
    )
}

/// Test-friendly handle for [`clipvault_ignored_app_linux_add`]. The
/// helper exists so the integration suite can exercise the same
/// validation pipeline the Tauri command runs without standing up a
/// Tauri runtime.
///
/// The helper is the single point of truth for the Linux catalog-add
/// validation pipeline:
///
/// 1. Re-resolve the picker backend from the current runtime
///    state. A session that flipped to `Unsupported` after the user
///    opened the catalog (for example the GNOME extension crashed
///    or the X server disconnected) MUST reject the add.
/// 2. Re-run the catalog against the resolved strategy and look up
///    the requested identifier. The catalog is the only authority
///    that knows which identifiers are safe to persist.
/// 3. Use the catalog's `display_name` and `icon_ref` instead of
///    whatever the frontend supplied. The frontend may have cached
///    stale metadata or be running an older build that ships
///    different icons; the catalog is the live source of truth.
/// 4. Persist through [`IgnoredAppsService::add_with_metadata`]
///    which already updates the privacy gate.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn clipvault_ignored_app_linux_add_for_test(
    context: &clipvault_core::AppContext,
    identifier: &str,
    _display_name: Option<&str>,
    _icon_ref: Option<&str>,
) -> Result<LinuxPickAndAddResponse, CommandError> {
    use clipvault_platform::runtime::linux_app_catalog::LinuxApplicationCatalog;

    let info = context.platform();
    if !matches!(info.os_family, clipvault_platform::OsFamily::Linux) {
        return Err(CommandError::new(
            "unsupported_session",
            "linux picker is only available on linux hosts",
        ));
    }

    // 1. Re-resolve the picker backend from the runtime state.
    //    The catalog was built when the user opened the modal; a
    //    session that flipped since then MUST reject the add.
    let picker_backend = context.linux_picker_backend();
    let Some(strategy) = picker_backend.strategy() else {
        return Err(CommandError::new(
            "unsupported_session",
            format!(
                "linux picker session is not supported ({})",
                picker_backend.as_str()
            ),
        ));
    };

    // 2. Re-run the catalog and look up the requested identifier.
    let assets_dir = info.data_dir.join("assets");
    let catalog = LinuxApplicationCatalog::new(&assets_dir, strategy);
    let candidate = match catalog.find(identifier) {
        Ok(Some(candidate)) => candidate,
        Ok(None) => {
            // The identifier does not belong to the catalog for the
            // current session. Refuse the add — the catalog command is
            // the single authority that knows which identifiers are
            // safe to persist.
            return Err(CommandError::new(
                "unsupported_session",
                format!(
                    "identifier {:?} is not present in the linux picker catalog",
                    identifier
                ),
            ));
        }
        Err(error) => {
            return Err(CommandError::new(
                "backend_unavailable",
                format!("linux catalog enumeration failed: {error}"),
            ));
        }
    };

    // 3. Use the catalog's metadata verbatim. The frontend-supplied
    //    `display_name` and `icon_ref` are ignored on purpose.
    let outcome = context
        .ignored_apps()
        .add_with_metadata(
            context,
            candidate.identifier.as_str(),
            candidate.display_name.as_deref(),
            candidate.icon_ref.as_deref(),
        )
        .map_err(|error| match error {
            clipvault_core::IgnoredAppsServiceError::Domain(
                clipvault_core::IgnoredAppError::MissingIdentifier,
            ) => CommandError::new("missing_identifier", "missing_identifier"),
            clipvault_core::IgnoredAppsServiceError::Domain(other) => {
                CommandError::new(other.kind_str(), other.to_string())
            }
            clipvault_core::IgnoredAppsServiceError::Persistence(reason) => {
                CommandError::new("persistence_error", reason.to_string())
            }
        })?;
    Ok(match outcome {
        PickAndAddOutcome::Added(entry) => LinuxPickAndAddResponse::Added { entry },
        PickAndAddOutcome::Updated(entry) => LinuxPickAndAddResponse::Updated { entry },
        PickAndAddOutcome::Cancelled => unreachable!(
            "add_with_metadata never returns Cancelled; the catalog flow has no picker \
             dismissal event, the frontend never invokes clipvault_ignored_app_linux_add \
             without a chosen identifier"
        ),
    })
}

/// Test-friendly handle for [`clipvault_ignored_app_linux_add`]
/// that drives the catalog against an in-memory filesystem
/// fixture. Mirrors the production helper but lets the integration
/// suite pin the candidate list without touching the host's
/// installed `.desktop` files.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub fn clipvault_ignored_app_linux_add_for_test_with_fs(
    context: &clipvault_core::AppContext,
    identifier: &str,
    _display_name: Option<&str>,
    _icon_ref: Option<&str>,
    fs: std::sync::Arc<dyn clipvault_platform::runtime::linux_app_metadata::DesktopFilesystem>,
) -> Result<LinuxPickAndAddResponse, CommandError> {
    use clipvault_platform::runtime::linux_app_catalog::LinuxApplicationCatalog;

    let info = context.platform();
    if !matches!(info.os_family, clipvault_platform::OsFamily::Linux) {
        return Err(CommandError::new(
            "unsupported_session",
            "linux picker is only available on linux hosts",
        ));
    }

    let picker_backend = context.linux_picker_backend();
    let Some(strategy) = picker_backend.strategy() else {
        return Err(CommandError::new(
            "unsupported_session",
            format!(
                "linux picker session is not supported ({})",
                picker_backend.as_str()
            ),
        ));
    };

    let assets_dir = info.data_dir.join("assets");
    let catalog = LinuxApplicationCatalog::with_filesystem(&assets_dir, fs, strategy);
    let candidate = match catalog.find(identifier) {
        Ok(Some(candidate)) => candidate,
        Ok(None) => {
            return Err(CommandError::new(
                "unsupported_session",
                format!(
                    "identifier {:?} is not present in the linux picker catalog",
                    identifier
                ),
            ));
        }
        Err(error) => {
            return Err(CommandError::new(
                "backend_unavailable",
                format!("linux catalog enumeration failed: {error}"),
            ));
        }
    };

    let outcome = context
        .ignored_apps()
        .add_with_metadata(
            context,
            candidate.identifier.as_str(),
            candidate.display_name.as_deref(),
            candidate.icon_ref.as_deref(),
        )
        .map_err(|error| match error {
            clipvault_core::IgnoredAppsServiceError::Domain(
                clipvault_core::IgnoredAppError::MissingIdentifier,
            ) => CommandError::new("missing_identifier", "missing_identifier"),
            clipvault_core::IgnoredAppsServiceError::Domain(other) => {
                CommandError::new(other.kind_str(), other.to_string())
            }
            clipvault_core::IgnoredAppsServiceError::Persistence(reason) => {
                CommandError::new("persistence_error", reason.to_string())
            }
        })?;
    Ok(match outcome {
        PickAndAddOutcome::Added(entry) => LinuxPickAndAddResponse::Added { entry },
        PickAndAddOutcome::Updated(entry) => LinuxPickAndAddResponse::Updated { entry },
        PickAndAddOutcome::Cancelled => unreachable!(
            "add_with_metadata never returns Cancelled; the catalog flow has no picker \
             dismissal event, the frontend never invokes clipvault_ignored_app_linux_add \
             without a chosen identifier"
        ),
    })
}

// ---------------------------------------------------------------------------
// `history-card-layout` capability commands.
// ---------------------------------------------------------------------------

/// Response of [`clipvault_set_entry_title`]. Mirrors the service-layer
/// outcome so the frontend can branch on the `kind` discriminator
/// without parsing free-form messages.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum SetEntryTitleResponse {
    Updated { entry: clipvault_db::EntryRecord },
    NotFound,
}

impl From<SetTitleOutcome> for SetEntryTitleResponse {
    fn from(outcome: SetTitleOutcome) -> Self {
        match outcome {
            SetTitleOutcome::Updated { record } => SetEntryTitleResponse::Updated { entry: record },
            SetTitleOutcome::NotFound => SetEntryTitleResponse::NotFound,
        }
    }
}

/// Set or restore the card title for a single entry. The command is
/// thin over [`clipvault_core::TextHistoryService::set_title`]: the
/// shell never inspects the clipboard payload, the validation rules
/// live in the core service and the frontend only sees the typed
/// response.
#[tauri::command]
pub fn clipvault_set_entry_title(
    state: State<'_, SharedState>,
    entry_id: i64,
    title: Option<String>,
) -> Result<SetEntryTitleResponse, CommandError> {
    let outcome = state
        .context()
        .history()
        .set_title(state.context(), entry_id, title.as_deref())
        .map_err(|err| match err {
            clipvault_core::HistoryServiceError::InvalidTitle(TitleValidationError::TooLong(
                max,
            )) => CommandError::new("title_too_long", format!("exceeds {max} characters")),
            other => CommandError::new("history_error", other.to_string()),
        })?;
    Ok(outcome.into())
}

/// Test-friendly handle for [`clipvault_set_entry_title`] so the
/// integration suite can exercise the validation pipeline without
/// standing up a Tauri runtime.
#[allow(dead_code)]
pub fn clipvault_set_entry_title_for_test(
    context: &clipvault_core::AppContext,
    entry_id: i64,
    title: Option<String>,
) -> Result<SetEntryTitleResponse, CommandError> {
    let outcome = context
        .history()
        .set_title(context, entry_id, title.as_deref())
        .map_err(|err| match err {
            clipvault_core::HistoryServiceError::InvalidTitle(TitleValidationError::TooLong(
                max,
            )) => CommandError::new("title_too_long", format!("exceeds {max} characters")),
            other => CommandError::new("history_error", other.to_string()),
        })?;
    Ok(outcome.into())
}

// ---------------------------------------------------------------------------
// `editable-text-captures` capability commands.
//
// The `clipvault_update_text_entry` command is the single bridge the
// desktop editor uses to overwrite the textual payload of an
// existing history entry in place. The command is a thin adapter
// over [`clipvault_core::TextHistoryService::update_text`]; no
// payload bytes, hashes or snippets ever leave the persistence
// layer through the response or any emitted event.
// ------------------------------------------------------------------------

/// Response of [`clipvault_update_text_entry`]. The discriminator
/// lets the GUI branch on `kind` without parsing free-form strings;
/// every variant other than the persistence-failure branches carries
/// the refreshed record (when relevant) so the rail can update its
/// state in one round-trip.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum UpdateTextEntryResponse {
    Updated { entry: clipvault_db::EntryRecord },
    Noop { entry: clipvault_db::EntryRecord },
    NotFound,
    NotEditable,
    EmptyContent,
    DuplicateContent,
}

impl From<UpdateTextHistoryOutcome> for UpdateTextEntryResponse {
    fn from(outcome: UpdateTextHistoryOutcome) -> Self {
        match outcome {
            UpdateTextHistoryOutcome::Updated { record } => {
                UpdateTextEntryResponse::Updated { entry: record }
            }
            UpdateTextHistoryOutcome::Noop { record } => {
                UpdateTextEntryResponse::Noop { entry: record }
            }
            UpdateTextHistoryOutcome::NotFound => UpdateTextEntryResponse::NotFound,
            UpdateTextHistoryOutcome::NotEditable => UpdateTextEntryResponse::NotEditable,
            UpdateTextHistoryOutcome::EmptyContent => UpdateTextEntryResponse::EmptyContent,
            UpdateTextHistoryOutcome::DuplicateContent => UpdateTextEntryResponse::DuplicateContent,
        }
    }
}

/// Replace the textual payload of an existing history entry in
/// place. The command is a thin adapter over
/// [`clipvault_core::TextHistoryService::update_text`] and emits the
/// metadata-only `clipvault://history-updated` event after a
/// successful commit so the rail, search and Quick Paste refresh
/// from the same source of truth the persistence layer just
/// updated.
///
/// The command carries **only** the user-entered draft because the
/// edit requires it; the response and the emitted event stay
/// metadata-only — clipboard content, content hashes and snippets
/// never cross the bridge.
#[tauri::command]
pub fn clipvault_update_text_entry(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    entry_id: i64,
    content: String,
) -> Result<UpdateTextEntryResponse, CommandError> {
    let outcome = state
        .context()
        .history()
        .update_text(state.context(), entry_id, content)
        .map_err(|err| CommandError::new("history_error", err.to_string()))?;
    if matches!(outcome, UpdateTextHistoryOutcome::Updated { .. }) {
        if let Err(error) = handle.emit(crate::bootstrap::HISTORY_UPDATED_EVENT, ()) {
            warn!(error = %error, "failed to emit history-updated event");
        }
    }
    Ok(outcome.into())
}

/// Test-friendly handle for [`clipvault_update_text_entry`] so the
/// integration suite can exercise the validation pipeline without
/// standing up a Tauri runtime. The helper does NOT emit the
/// metadata-only refresh event because the test runs outside the
/// shell; production callers must use the Tauri command.
#[allow(dead_code)]
pub fn clipvault_update_text_entry_for_test(
    context: &clipvault_core::AppContext,
    entry_id: i64,
    content: String,
) -> Result<UpdateTextEntryResponse, CommandError> {
    let outcome = context
        .history()
        .update_text(context, entry_id, content)
        .map_err(|err| CommandError::new("history_error", err.to_string()))?;
    Ok(outcome.into())
}

/// Resolve a persisted `source_app_icon_ref` to the PNG bytes the
/// history card can render. The validation pipeline mirrors
/// [`clipvault_ignored_app_icon`] but resolves references under the
/// `application-icons/` namespace.
#[tauri::command]
pub fn clipvault_source_app_icon(
    state: State<'_, SharedState>,
    icon_ref: String,
) -> Result<Vec<u8>, CommandError> {
    let data_dir = state.context().platform().data_dir.clone();
    match read_source_app_icon_bytes(&icon_ref, &data_dir) {
        Ok(bytes) => Ok(bytes),
        Err(IconReadError::Ref(reference_error)) => Err(CommandError::new(
            "invalid_icon_ref",
            reference_error.kind_str(),
        )),
        Err(other) => Err(CommandError::new("icon_read_error", other.to_string())),
    }
}

/// Test-friendly handle for [`clipvault_source_app_icon`] so the
/// integration suite can exercise the validation pipeline without
/// standing up a Tauri runtime.
#[allow(dead_code)]
pub fn clipvault_source_app_icon_for_test(
    data_dir: &std::path::Path,
    icon_ref: String,
) -> Result<Vec<u8>, CommandError> {
    match read_source_app_icon_bytes(&icon_ref, data_dir) {
        Ok(bytes) => Ok(bytes),
        Err(IconReadError::Ref(reference_error)) => Err(CommandError::new(
            "invalid_icon_ref",
            reference_error.kind_str(),
        )),
        Err(other) => Err(CommandError::new("icon_read_error", other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// `clipboard-rich-content` capability commands.
// ---------------------------------------------------------------------------

/// Resolve a persisted `asset_ref` to the PNG bytes of a captured
/// image.
///
/// This is the single bridge between the opaque, relative reference the
/// database stores and the renderable bytes the webview needs. The
/// frontend never receives an absolute path, never learns where the
/// data directory lives and cannot trick the backend into reading an
/// arbitrary location: the command takes **only** the reference and
/// delegates every rule to [`ClipboardAssetStore::read_bytes`], which
/// enforces
///
/// - the reference must be relative and free of `..` components;
/// - it must live in the `clipboard/` namespace — an `ignored-apps/`
///   or `application-icons/` reference is rejected;
/// - the resolved path must not be a symlink and must canonicalise
///   inside `<data_dir>/assets/clipboard/`;
/// - the file must be a decodable PNG within the documented size and
///   dimension caps.
///
/// Failures return a [`CommandError`] whose `message` is the stable
/// [`AssetError::kind_str`] identifier so the card can pick its
/// fallback copy without parsing prose. No branch returns bytes for a
/// rejected reference, and no branch echoes the reference, a hash or a
/// path back to the caller.
#[tauri::command]
pub fn clipvault_clipboard_asset(
    state: State<'_, SharedState>,
    asset_ref: String,
) -> Result<Vec<u8>, CommandError> {
    let data_dir = state.context().platform().data_dir.clone();
    read_clipboard_asset(&data_dir, &asset_ref)
}

/// Test-friendly handle for [`clipvault_clipboard_asset`]. Lets the
/// integration suite drive the exact validation pipeline the command
/// runs by passing `data_dir` explicitly instead of reaching into the
/// managed Tauri state.
#[allow(dead_code)]
pub fn clipvault_clipboard_asset_for_test(
    data_dir: &std::path::Path,
    asset_ref: String,
) -> Result<Vec<u8>, CommandError> {
    read_clipboard_asset(data_dir, &asset_ref)
}

fn read_clipboard_asset(
    data_dir: &std::path::Path,
    asset_ref: &str,
) -> Result<Vec<u8>, CommandError> {
    let store = ClipboardAssetStore::new(data_dir);
    match store.read_bytes(asset_ref) {
        Ok(bytes) => Ok(bytes),
        // Only the stable kind travels back: the free-form Display of
        // an `AssetError` is never surfaced, so no reference, hash or
        // path can leak through the error channel.
        Err(error) => Err(CommandError::new("invalid_asset_ref", error.kind_str())),
    }
}

/// Read the sanitised rich-text preview for an entry. Mirrors the
/// validation guarantees of [`clipvault_clipboard_asset`]: the
/// frontend never sees the original HTML / RTF bytes, only the
/// preview fragment the core produced and sanitised on disk.
#[tauri::command]
pub fn clipvault_rich_text_preview(
    state: State<'_, SharedState>,
    preview_ref: String,
) -> Result<Vec<u8>, CommandError> {
    let data_dir = state.context().platform().data_dir.clone();
    read_rich_text_preview(&data_dir, &preview_ref)
}

/// Test-friendly handle for [`clipvault_rich_text_preview`].
#[allow(dead_code)]
pub fn clipvault_rich_text_preview_for_test(
    data_dir: &std::path::Path,
    preview_ref: String,
) -> Result<Vec<u8>, CommandError> {
    read_rich_text_preview(data_dir, &preview_ref)
}

fn read_rich_text_preview(
    data_dir: &std::path::Path,
    preview_ref: &str,
) -> Result<Vec<u8>, CommandError> {
    let store = RichTextAssetStore::new(data_dir);
    match store.read_bytes(preview_ref) {
        Ok(bytes) => Ok(bytes),
        // Stable kind only; the reference, hash or path never reach
        // the frontend through the error channel.
        Err(error) => Err(CommandError::new("invalid_preview_ref", error.kind_str())),
    }
}

// ---------------------------------------------------------------------------
// `tags-and-collections` capability commands.
// ---------------------------------------------------------------------------

use clipvault_core::{Collection, OrganizationServiceError, OrganizationSidebarSnapshot, Tag};

/// Convert an organization-layer error into the same stable
/// discriminator the rest of the Tauri shell exposes. The error
/// kind never carries clipboard content, hashes or paths.
fn organization_command_error(error: OrganizationServiceError) -> CommandError {
    let kind = match &error {
        OrganizationServiceError::Repository(repository_error) => match repository_error {
            clipvault_db::OrganizationError::Sqlite(_) => "organization_error",
            clipvault_db::OrganizationError::EmptyCollectionName => "empty_collection_name",
            clipvault_db::OrganizationError::CollectionNameTooLong(_) => "collection_name_too_long",
            clipvault_db::OrganizationError::CollectionNameTaken(_) => "collection_name_taken",
            clipvault_db::OrganizationError::EmptyTagName => "empty_tag_name",
            clipvault_db::OrganizationError::TagNameTooLong(_) => "tag_name_too_long",
            clipvault_db::OrganizationError::TagNameTaken(_) => "tag_name_taken",
            clipvault_db::OrganizationError::CollectionNotFound(_) => "collection_not_found",
            clipvault_db::OrganizationError::TagNotFound(_) => "tag_not_found",
            clipvault_db::OrganizationError::EntryNotFound(_) => "entry_not_found",
            clipvault_db::OrganizationError::SystemCollectionProtected(_) => {
                "system_collection_protected"
            }
            clipvault_db::OrganizationError::InvalidCollectionColor(_) => {
                "invalid_collection_color"
            }
        },
    };
    CommandError::new(kind, error.to_string())
}

#[tauri::command]
pub fn clipvault_organization_snapshot(
    state: State<'_, SharedState>,
) -> Result<OrganizationSidebarSnapshot, CommandError> {
    // Read-only query — never mutates, never emits.
    OrganizationSidebarSnapshot::load(state.context()).map_err(organization_command_error)
}

#[tauri::command]
pub fn clipvault_collections_create(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    name: String,
) -> Result<Collection, CommandError> {
    let collection = state
        .context()
        .organization()
        .create_collection(state.context(), &name)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(collection)
}

#[tauri::command]
pub fn clipvault_collections_rename(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    collection_id: i64,
    name: String,
) -> Result<Collection, CommandError> {
    let collection = state
        .context()
        .organization()
        .rename_collection(state.context(), collection_id, &name)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(collection)
}

#[tauri::command]
pub fn clipvault_collections_delete(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    collection_id: i64,
) -> Result<bool, CommandError> {
    let removed = state
        .context()
        .organization()
        .delete_collection(state.context(), collection_id)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(removed)
}

/// Update the persistent `#rrggbb` colour of any collection (system or
/// user). The command is a thin adapter over
/// [`clipvault_core::OrganizationService::set_collection_color`]:
/// it validates the value through the repository, refreshes the
/// returned row and emits the existing metadata-only
/// `clipvault://organization-updated` event so the sidebar and every
/// visible card re-render. The command never touches clipboard
/// content, snippets, hashes or asset references and never echoes
/// the value back through the error channel.
#[tauri::command]
pub fn clipvault_collections_set_color(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    collection_id: i64,
    color_hex: String,
) -> Result<Collection, CommandError> {
    let collection = state
        .context()
        .organization()
        .set_collection_color(state.context(), collection_id, &color_hex)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(collection)
}

#[tauri::command]
pub fn clipvault_tags_create(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    name: String,
) -> Result<Tag, CommandError> {
    let tag = state
        .context()
        .organization()
        .upsert_tag(state.context(), &name)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(tag)
}

#[tauri::command]
pub fn clipvault_tags_rename(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    tag_id: i64,
    name: String,
) -> Result<Tag, CommandError> {
    let tag = state
        .context()
        .organization()
        .rename_tag(state.context(), tag_id, &name)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(tag)
}

#[tauri::command]
pub fn clipvault_tags_delete(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    tag_id: i64,
) -> Result<bool, CommandError> {
    let removed = state
        .context()
        .organization()
        .delete_tag(state.context(), tag_id)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(removed)
}

#[tauri::command]
pub fn clipvault_entry_collections(
    state: State<'_, SharedState>,
    entry_id: i64,
) -> Result<Vec<i64>, CommandError> {
    // Read-only — never emits.
    state
        .context()
        .organization()
        .entry_collection_ids(state.context(), entry_id)
        .map_err(organization_command_error)
}

#[tauri::command]
pub fn clipvault_entry_tags(
    state: State<'_, SharedState>,
    entry_id: i64,
) -> Result<Vec<i64>, CommandError> {
    // Read-only — never emits.
    state
        .context()
        .organization()
        .entry_tag_ids(state.context(), entry_id)
        .map_err(organization_command_error)
}

#[tauri::command]
pub fn clipvault_entry_collections_set(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    entry_id: i64,
    collection_ids: Vec<i64>,
) -> Result<Vec<i64>, CommandError> {
    let resolved = state
        .context()
        .organization()
        .replace_entry_collections(state.context(), entry_id, &collection_ids)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(resolved)
}

#[tauri::command]
pub fn clipvault_entry_tags_set(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    entry_id: i64,
    tag_ids: Vec<i64>,
) -> Result<Vec<i64>, CommandError> {
    let resolved = state
        .context()
        .organization()
        .replace_entry_tags(state.context(), entry_id, &tag_ids)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(resolved)
}

/// Atomically upsert a tag by normalised name and attach it to
/// `entry_id`. The command is the canonical bridge for the
/// **Agregar tag** flow on a history card: the modal lets the user
/// type a new name (or pick an existing tag) and the shell routes
/// every new name through this command so the core owns the
/// normalisation, the idempotency and the join-table insert in a
/// single transaction.
///
/// The response carries the refreshed [`Tag`] row regardless of
/// whether the tag was newly inserted or already existed. The
/// association itself is idempotent: re-running the command for the
/// same `(entry_id, normalised_name)` pair leaves the database
/// untouched. The command returns a typed
/// [`OrganizationServiceError`] (mapped to a stable
/// `CommandError.kind`) when the entry id is stale or the name
/// fails validation; the response never carries clipboard content.
#[tauri::command]
pub fn clipvault_entry_upsert_tag(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    entry_id: i64,
    name: String,
) -> Result<Tag, CommandError> {
    let tag = state
        .context()
        .organization()
        .upsert_and_assign_tag(state.context(), entry_id, &name)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(tag)
}

#[tauri::command]
pub fn clipvault_entry_remove_from_collection(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    entry_id: i64,
    collection_id: i64,
) -> Result<bool, CommandError> {
    let removed = state
        .context()
        .organization()
        .remove_entry_from_collection(state.context(), entry_id, collection_id)
        .map_err(organization_command_error)?;
    emit_organization_updated(&handle);
    Ok(removed)
}

#[tauri::command]
pub fn clipvault_history_collection_id(state: State<'_, SharedState>) -> Result<i64, CommandError> {
    // Read-only.
    state
        .context()
        .organization()
        .history_collection_id(state.context())
        .map_err(organization_command_error)
}

// ---------------------------------------------------------------------------
// `code-language-detection` capability commands.
// ---------------------------------------------------------------------------

/// Response of [`clipvault_code_language_set`]. Mirrors the
/// [`clipvault_core::CodeLanguageServiceOutcome`] discriminated union so
/// the frontend can branch on `kind` without inspecting the free-form
/// `message` string. The response is metadata-only: no clipboard
/// payload, no hash, no snippet and no asset reference.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetCodeLanguageResponse {
    /// The classification was written (or already matched) and the
    /// refreshed record is returned for the rail to update.
    Updated { entry: clipvault_db::EntryRecord },
    /// The repository deliberately refused to overwrite a stored
    /// classification with `null`. The record is still returned so
    /// the frontend can render the previous state.
    Noop { entry: clipvault_db::EntryRecord },
    /// The target entry does not exist (anymore).
    NotFound,
}

impl From<clipvault_core::CodeLanguageServiceOutcome> for SetCodeLanguageResponse {
    fn from(outcome: clipvault_core::CodeLanguageServiceOutcome) -> Self {
        match outcome {
            clipvault_core::CodeLanguageServiceOutcome {
                updated: Some(record),
                noop: true,
            } => SetCodeLanguageResponse::Noop { entry: record },
            clipvault_core::CodeLanguageServiceOutcome {
                updated: Some(record),
                noop: false,
            } => SetCodeLanguageResponse::Updated { entry: record },
            clipvault_core::CodeLanguageServiceOutcome {
                updated: None,
                noop: _,
            } => SetCodeLanguageResponse::NotFound,
        }
    }
}

fn code_language_command_error(error: CodeLanguageServiceError) -> CommandError {
    let kind = error.kind_str();
    CommandError::new(kind, error.to_string())
}

/// Persist the canonical `code_language` the frontend detector accepted
/// for `entry_id`. The command carries only metadata: the
/// canonical language identifier and the entry id. The bridge never
/// inspects clipboard content, hashes, snippets or absolute paths,
/// and the response carries only the refreshed record.
#[tauri::command]
pub fn clipvault_code_language_set(
    state: State<'_, SharedState>,
    entry_id: i64,
    code_language: Option<String>,
) -> Result<SetCodeLanguageResponse, CommandError> {
    let raw_language = code_language.as_deref();
    let mut db = state.context().database().lock();
    let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
    let outcome = state
        .context()
        .code_language()
        .set_code_language(
            &mut repo,
            entry_id,
            raw_language,
            state.context().clock().now(),
        )
        .map_err(code_language_command_error)?;
    Ok(outcome.into())
}

/// Test-friendly handle for [`clipvault_code_language_set`].
#[allow(dead_code)]
pub fn clipvault_code_language_set_for_test(
    context: &clipvault_core::AppContext,
    entry_id: i64,
    code_language: Option<String>,
) -> Result<SetCodeLanguageResponse, CommandError> {
    let raw_language = code_language.as_deref();
    let mut db = context.database().lock();
    let mut repo = clipvault_db::EntryRepository::new(db.connection_mut());
    let outcome = context
        .code_language()
        .set_code_language(&mut repo, entry_id, raw_language, context.clock().now())
        .map_err(code_language_command_error)?;
    Ok(outcome.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the metadata-only event channel identifier so an accidental
    /// rename surfaces as a unit-test failure on the backend instead
    /// of silently dropping notifications in production. The frontend
    /// pins the same constant in
    /// `app/tauri/frontend/src/lib/organizationUpdates.ts`; both ends
    /// of the bridge MUST agree.
    #[test]
    fn organization_updated_event_name_is_stable() {
        assert_eq!(
            ORGANIZATION_UPDATED_EVENT,
            "clipvault://organization-updated",
        );
    }

    /// Pin the wire-format discriminator for the code-language bridge
    /// so a frontend rename surfaces as a unit-test failure instead of
    /// silently dropping notifications.
    #[test]
    fn set_code_language_response_discriminator_is_stable() {
        let entry = clipvault_db::EntryRecord {
            id: 1,
            content: "x".into(),
            content_type: clipvault_db::ContentType::Code,
            content_size: 1,
            content_hash: "hash".into(),
            source_app: None,
            is_pinned: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            last_seen_at: "2026-01-01T00:00:00Z".into(),
            title: None,
            source_app_name: None,
            source_app_icon_ref: None,
            asset_ref: None,
            mime_type: None,
            payload_width: None,
            payload_height: None,
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: Some("python".into()),
        };
        let updated: SetCodeLanguageResponse = clipvault_core::CodeLanguageServiceOutcome {
            updated: Some(entry.clone()),
            noop: false,
        }
        .into();
        match updated {
            SetCodeLanguageResponse::Updated { entry: record } => {
                assert_eq!(record.code_language.as_deref(), Some("python"));
            }
            other => panic!("expected Updated, got {other:?}"),
        }
        let noop: SetCodeLanguageResponse = clipvault_core::CodeLanguageServiceOutcome {
            updated: Some(entry),
            noop: true,
        }
        .into();
        assert!(matches!(noop, SetCodeLanguageResponse::Noop { .. }));

        let missing: SetCodeLanguageResponse = clipvault_core::CodeLanguageServiceOutcome {
            updated: None,
            noop: false,
        }
        .into();
        assert!(matches!(missing, SetCodeLanguageResponse::NotFound));
    }
}

// ---------------------------------------------------------------------------
// `gnome-wayland-integration` capability commands.
// ---------------------------------------------------------------------------

#[cfg(all(target_os = "linux", feature = "linux-gnome-shell-integration"))]
mod gnome_commands {
    use super::*;

    /// Status snapshot of the optional GNOME Shell integration. The
    /// payload is metadata-only: the fields never carry clipboard
    /// content, source-app identifiers or environment variables.
    #[derive(Debug, Serialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    pub enum GnomeIntegrationStatusResponse {
        NotApplicable {
            session: String,
            desktop: String,
        },
        Ready {
            payload: crate::gnome_integration::GnomeIntegrationPayload,
        },
        NotConfigured {
            reason: String,
        },
    }

    /// Convert the shell payload into the discriminated union the
    /// frontend uses. Keeps the per-variant fields stable so the UI
    /// can branch on `kind`.
    pub fn gnome_status_response(
        state_opt: Option<&Arc<crate::gnome_integration::GnomeIntegrationState>>,
    ) -> GnomeIntegrationStatusResponse {
        let Some(state) = state_opt else {
            return GnomeIntegrationStatusResponse::NotConfigured {
                reason: "feature_disabled".to_string(),
            };
        };
        let payload = state.payload();
        if !payload.applicable {
            GnomeIntegrationStatusResponse::NotApplicable {
                session: payload.session,
                desktop: payload.desktop,
            }
        } else {
            GnomeIntegrationStatusResponse::Ready { payload }
        }
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct GnomeConsentUpdate {
        pub decision: String,
    }

    /// Persist the user's consent decision. Recording `declined` or
    /// `disabled` keeps the prompt from reappearing on the next
    /// launch; recording `accepted` allows the next `install` call
    /// to run.
    #[tauri::command]
    pub fn clipvault_gnome_integration_status(
        state: State<'_, SharedState>,
    ) -> Result<GnomeIntegrationStatusResponse, CommandError> {
        Ok(gnome_status_response(
            state.app_state().gnome_integration.as_ref(),
        ))
    }

    #[tauri::command]
    pub fn clipvault_gnome_integration_set_consent(
        state: State<'_, SharedState>,
        update: GnomeConsentUpdate,
    ) -> Result<crate::gnome_integration::GnomeIntegrationPayload, CommandError> {
        let gnome_state = state
            .app_state()
            .gnome_integration
            .as_ref()
            .ok_or_else(|| CommandError::new("feature_disabled", "feature disabled"))?;
        let decision = match update.decision.as_str() {
            "accepted" => clipvault_core::GnomeConsentDecision::Accepted,
            "declined" => clipvault_core::GnomeConsentDecision::Declined,
            "disabled" => clipvault_core::GnomeConsentDecision::Disabled,
            "unknown" => clipvault_core::GnomeConsentDecision::Unknown,
            _ => return Err(CommandError::new("invalid_consent", "unknown decision")),
        };
        gnome_state
            .record_consent(state.context(), decision)
            .map_err(|error| CommandError::new("consent_error", error.to_string()))?;
        Ok(gnome_state.payload())
    }

    #[tauri::command]
    pub fn clipvault_gnome_integration_install(
        state: State<'_, SharedState>,
        handle: tauri::AppHandle<tauri::Wry>,
    ) -> Result<crate::gnome_integration::InstallResult, CommandError> {
        let gnome_state = state
            .app_state()
            .gnome_integration
            .as_ref()
            .ok_or_else(|| CommandError::new("feature_disabled", "feature disabled"))?
            .clone();
        let bundled = crate::gnome_integration::read_bundled_extension(&handle)
            .map_err(|error| CommandError::new("bundled_missing", error.to_string()))?;
        let result = gnome_state
            .install(state.context(), &bundled)
            .map_err(|error| CommandError::new("install_error", error.to_string()))?;
        if let Err(error) = gnome_state.start_listener() {
            let _ = gnome_state.uninstall(state.context());
            return Err(CommandError::new("listener_error", error));
        }
        Ok(result)
    }

    #[tauri::command]
    pub fn clipvault_gnome_integration_uninstall(
        state: State<'_, SharedState>,
    ) -> Result<crate::gnome_integration::GnomeIntegrationPayload, CommandError> {
        let gnome_state = state
            .app_state()
            .gnome_integration
            .as_ref()
            .ok_or_else(|| CommandError::new("feature_disabled", "feature disabled"))?
            .clone();
        let installation = gnome_state
            .uninstall(state.context())
            .map_err(|error| CommandError::new("uninstall_error", error.to_string()))?;
        gnome_state.stop_listener();
        let _ = installation;
        Ok(gnome_state.payload())
    }

    #[tauri::command]
    pub fn clipvault_gnome_integration_retry(
        state: State<'_, SharedState>,
    ) -> Result<crate::gnome_integration::GnomeIntegrationPayload, CommandError> {
        let gnome_state = state
            .app_state()
            .gnome_integration
            .as_ref()
            .ok_or_else(|| CommandError::new("feature_disabled", "feature disabled"))?
            .clone();
        gnome_state
            .start_listener()
            .map_err(|error| CommandError::new("listener_error", error))?;
        Ok(gnome_state.payload())
    }
}

#[cfg(not(all(target_os = "linux", feature = "linux-gnome-shell-integration")))]
mod gnome_commands {
    use super::*;

    /// Stub status response used on hosts where the integration is
    /// not compiled in. The frontend treats every variant of the
    /// discriminated union identically so the absence stays
    /// invisible to the consumer.
    #[derive(Debug, Serialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    #[allow(dead_code)]
    pub enum GnomeIntegrationStatusResponse {
        NotApplicable { session: String, desktop: String },
        NotConfigured { reason: String },
    }

    #[tauri::command]
    pub fn clipvault_gnome_integration_status(
        _state: State<'_, SharedState>,
    ) -> Result<GnomeIntegrationStatusResponse, CommandError> {
        Ok(GnomeIntegrationStatusResponse::NotConfigured {
            reason: "feature_disabled".to_string(),
        })
    }

    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    pub struct GnomeConsentUpdate {
        pub decision: String,
    }

    #[tauri::command]
    pub fn clipvault_gnome_integration_set_consent(
        _state: State<'_, SharedState>,
        _update: GnomeConsentUpdate,
    ) -> Result<(), CommandError> {
        Err(CommandError::new("feature_disabled", "feature disabled"))
    }

    #[tauri::command]
    pub fn clipvault_gnome_integration_install(
        _state: State<'_, SharedState>,
        _handle: tauri::AppHandle<tauri::Wry>,
    ) -> Result<(), CommandError> {
        Err(CommandError::new("feature_disabled", "feature disabled"))
    }

    #[tauri::command]
    pub fn clipvault_gnome_integration_uninstall(
        _state: State<'_, SharedState>,
    ) -> Result<(), CommandError> {
        Err(CommandError::new("feature_disabled", "feature disabled"))
    }

    #[tauri::command]
    pub fn clipvault_gnome_integration_retry(
        _state: State<'_, SharedState>,
    ) -> Result<(), CommandError> {
        Err(CommandError::new("feature_disabled", "feature disabled"))
    }
}

pub use gnome_commands::*;

// ---------------------------------------------------------------------------
// Local peer identity commands.
//
// The shell exposes only metadata-only DTOs to the frontend; the
// private Ed25519 key never crosses the Tauri boundary, the
// platform detail never leaks into a typed error variant, and the
// commands are forbidden from doing anything beyond delegating to
// the `SettingsService` / `PeerIdentityService` already wired into
// the `AppContext`.
// ---------------------------------------------------------------------------

/// Stable user-facing copy returned with the `Unavailable` arm of
/// [`LocalPeerProfileResponse`]. The string lives here so the
/// frontend cannot drift from the typed outcome the backend
/// surfaces; it never carries platform detail, keychain error
/// messages or session identifiers.
pub const LOCAL_PEER_UNAVAILABLE_REASON: &str = "secure identity store unavailable";

/// Tagged response for both
/// [`clipvault_local_peer_profile_get`] and
/// [`clipvault_local_peer_profile_update`]. The frontend branches
/// on `kind` so it can render the right copy (success /
/// unavailable) without inspecting free-form strings.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalPeerProfileResponse {
    /// Identity loaded successfully. `profile` carries only the
    /// metadata the user is allowed to see; no private key bytes
    /// are ever serialised.
    Available {
        profile: clipvault_core::LocalPeerProfile,
    },
    /// Secure credential store is not reachable on this session.
    /// The shell renders the stable guidance copy returned via
    /// [`LOCAL_PEER_UNAVAILABLE_REASON`]; the platform detail
    /// never leaks past this point.
    Unavailable { reason: String },
}

impl LocalPeerProfileResponse {
    /// Build the `Available` arm from a typed
    /// [`clipvault_core::PeerIdentityOutcome`]. The function is the
    /// single point that maps the core outcome onto the wire
    /// contract, so the GET and UPDATE commands cannot drift.
    fn from_outcome(
        outcome: clipvault_core::PeerIdentityOutcome<clipvault_core::LocalPeerProfile>,
    ) -> Self {
        match outcome {
            clipvault_core::PeerIdentityOutcome::Ok(profile) => {
                LocalPeerProfileResponse::Available { profile }
            }
            clipvault_core::PeerIdentityOutcome::Unavailable => {
                LocalPeerProfileResponse::Unavailable {
                    reason: LOCAL_PEER_UNAVAILABLE_REASON.to_string(),
                }
            }
        }
    }
}

#[tauri::command]
pub fn clipvault_local_peer_profile_get(
    state: State<'_, SharedState>,
) -> Result<LocalPeerProfileResponse, CommandError> {
    let context = state.context();
    let outcome = context.settings().local_peer_profile(context);
    Ok(LocalPeerProfileResponse::from_outcome(outcome))
}

/// Update payload the frontend submits when the user edits the
/// visible device name. The `name` field accepts `null` to clear
/// the value (the UI clears the input box); any other value goes
/// through the validator before persistence.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalPeerDisplayNameUpdate {
    pub name: Option<String>,
}

/// Update the visible device name and return the post-update
/// [`LocalPeerProfileResponse`].
///
/// The name is persisted independently from the secure store
/// status: even when the platform keychain is temporarily
/// unavailable, the validated name is written to `app_settings` so
/// the user can keep editing without waiting for the keychain to
/// come back. The response carries the refreshed
/// [`clipvault_core::LocalPeerProfile`] when the secure store is
/// reachable, which lets the frontend update its cached `peer_id`
/// and `fingerprint` from a single round-trip without a follow-up
/// GET (the spec scenario "User changes visible name" pins this
/// shape: peer_id and fingerprint MUST stay unchanged).
#[tauri::command]
pub fn clipvault_local_peer_profile_update(
    state: State<'_, SharedState>,
    update: LocalPeerDisplayNameUpdate,
) -> Result<LocalPeerProfileResponse, CommandError> {
    let context = state.context();
    // Persist the validated name first. The persistence step does
    // not depend on the secure store status — a temporarily
    // unavailable keychain MUST NOT block the user from editing
    // their visible name.
    context
        .settings()
        .set_local_peer_display_name(context, update.name.as_deref())?;
    // Re-load the profile so the response carries the updated
    // `display_name` together with the stable `peer_id` and
    // `fingerprint`. When the secure store is unavailable the
    // command surfaces `Unavailable`; the frontend already
    // rendered the secure-store-unavailable copy and the name
    // has still been persisted.
    let outcome = context.settings().local_peer_profile(context);
    Ok(LocalPeerProfileResponse::from_outcome(outcome))
}

// ---------------------------------------------------------------------------
// `local-peer-discovery` commands.
//
// The shell exposes only metadata-only DTOs to the frontend:
// the toggle, the runtime snapshot, and a read-only refresh hook.
// The commands never return IP addresses, ports, raw public keys,
// clipboard content, previews, hashes or any other secret. The
// runtime itself is platform-agnostic: `clipvault-core` owns the
// event sink, the presence TTL and the persistence wiring, while
// `clipvault-platform` provides the productive
// `MdnsPeerDiscoveryAdapter` behind the optional
// `local-peer-discovery-mdns` feature. The shell's `bootstrap`
// installs that adapter by default on macOS / Linux builds that
// enable the feature and falls back to `NoopPeerDiscoveryAdapter`
// only on Windows builds or cross-compiles where the runtime
// cannot bind mDNS. The `service_fullname -> peer_id` association
// that turns the `_clipvault._tcp.local.` instance descriptor
// into the validated `peer_id` lives entirely inside the mDNS
// adapter (see `crates/clipvault-platform/src/peer_discovery/mdns.rs`)
// and is intentionally opaque to the shell, the core and the
// frontend.
// ---------------------------------------------------------------------------

/// Response of [`clipvault_peer_sharing_toggle_get`] /
/// [`clipvault_peer_sharing_toggle_set`]. The discriminated union
/// keeps the wire contract stable across changes: the frontend
/// branches on `kind` to render the right copy (active / inactive
/// / identity unavailable / runtime stopped) without parsing
/// free-form strings.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerSharingToggleResponse {
    /// The toggle is persisted and the runtime is currently
    /// browsing. `enabled` mirrors the persisted boolean.
    Active { enabled: bool },
    /// The toggle is persisted but the runtime cannot browse
    /// because the secure identity store is unavailable.
    IdentityUnavailable { enabled: bool },
    /// The toggle is persisted but the runtime is stopped
    /// (mDNS multicast is blocked, the host has no multicast
    /// path, …). The shell renders the documented degraded copy.
    RuntimeStopped { enabled: bool },
}

impl PeerSharingToggleResponse {
    fn from_runtime(runtime: &clipvault_core::PeerDiscoveryRuntime, enabled: bool) -> Self {
        Self::from_pairing(
            runtime,
            enabled,
            Err(clipvault_core::peer_pairing::TransportOutcome::Unavailable),
        )
    }

    /// Build the typed discriminated union from the discovery
    /// runtime state and the typed pairing transport outcome.
    /// The pairing outcome is what prevents the regression the
    /// 2026-09-20 review flagged: when the productive TLS install
    /// fails, the toggle must NOT collapse into `active` even
    /// though discovery is browsing. The function folds both
    /// signals into the union so the UI renders the documented
    /// `identity_unavailable` / `runtime_stopped` copy.
    fn from_pairing(
        runtime: &clipvault_core::PeerDiscoveryRuntime,
        enabled: bool,
        pairing_result: Result<u16, clipvault_core::peer_pairing::TransportOutcome>,
    ) -> Self {
        if pairing_result.is_err() {
            // The productive pairing transport refused to bind
            // the listener (or the shell deliberately turned it
            // off). The runtime may still be browsing but the
            // pairing endpoint is not actually dialable; the UI
            // collapses this into `runtime_stopped` so the user
            // sees the same degraded copy regardless of the
            // typed reason.
            return PeerSharingToggleResponse::RuntimeStopped { enabled };
        }
        if !runtime.is_running() {
            // Distinguish `identity_unavailable` (the secure store
            // is unreachable on this session) from `runtime_stopped`
            // (the store minted an identity but the runtime cannot
            // start). The runtime's `snapshot` already does this
            // through `sharing_inactive_reason`.
            let snapshot = runtime.snapshot(Vec::new, std::time::Instant::now());
            if !enabled {
                PeerSharingToggleResponse::RuntimeStopped { enabled }
            } else if snapshot.sharing_inactive_reason.as_deref()
                == Some(clipvault_core::RUNTIME_INACTIVE_REASON_IDENTITY_UNAVAILABLE)
            {
                PeerSharingToggleResponse::IdentityUnavailable { enabled }
            } else {
                PeerSharingToggleResponse::RuntimeStopped { enabled }
            }
        } else {
            PeerSharingToggleResponse::Active { enabled }
        }
    }
}

/// Read the persisted `Compartir en red local` toggle and the
/// runtime's current state. The response is the typed union
/// [`PeerSharingToggleResponse`]; the frontend uses it to render
/// the toggle and the status copy without inspecting free-form
/// strings.
///
/// The function consults BOTH the discovery runtime AND the
/// productive pairing transport: after a restart with the toggle
/// active, the bootstrap re-installs the pairing listener; if
/// the install failed the toggle MUST surface as
/// `RuntimeStopped` rather than collapsing into `active` only
/// because discovery is browsing. The pairing result is
/// synthetic here (we don't re-install on every poll) — the
/// runtime reports the production pairing state the install
/// path cached in [`AppContext`].
#[tauri::command]
pub fn clipvault_peer_sharing_toggle_get(
    state: State<'_, SharedState>,
) -> Result<PeerSharingToggleResponse, CommandError> {
    let context = state.context();
    let settings = context.settings().load(context);
    let runtime = context.peer_discovery();
    let pairing_running = context.pairing_transport_is_running();
    let pairing_result = if settings.local_peer_sharing_enabled && !pairing_running {
        // The toggle is on but the pairing listener is not bound.
        // Surface a typed `Unavailable` so the union collapses to
        // `RuntimeStopped` instead of reporting `active`.
        Err(clipvault_core::peer_pairing::TransportOutcome::Unavailable)
    } else if !settings.local_peer_sharing_enabled {
        // Toggle is off: pairing is intentionally stopped.
        Err(clipvault_core::peer_pairing::TransportOutcome::Unavailable)
    } else {
        // Toggle is on AND the pairing listener is bound.
        Ok(context.pairing_bound_port().unwrap_or(0))
    };
    Ok(PeerSharingToggleResponse::from_pairing(
        &runtime,
        settings.local_peer_sharing_enabled,
        pairing_result,
    ))
}

/// Persist the opt-in toggle and drive the runtime in lockstep.
/// Turning the toggle off always succeeds: the runtime stops and
/// the UI shows the off state immediately. Turning it on is
/// accepted even when the secure store is unavailable — the
/// persisted flag flips to `true` and the runtime surfaces
/// `identity_unavailable` so the user can retry once the
/// keychain is back.
#[tauri::command]
pub fn clipvault_peer_sharing_toggle_set(
    state: State<'_, SharedState>,
    enabled: bool,
) -> Result<PeerSharingToggleResponse, CommandError> {
    let context = state.context();
    let settings = context
        .settings()
        .set_local_peer_sharing_enabled(context, enabled)?;
    let runtime = context.peer_discovery();
    // The runtime is driven in lockstep with the persisted toggle
    // so the snapshot stays in sync without a follow-up refresh.
    context
        .settings()
        .sync_runtime_with_settings(context, &runtime, &settings);
    // The productive pairing transport follows the same toggle.
    // When sharing flips off we stop the listener and withdraw the
    // mDNS advertisement so a remote browser sees the goodbye
    // packet immediately; when it flips on we install the listener
    // and publish the pairing capability alongside the real
    // ephemeral port. The transport itself owns the cert pinning
    // and the mTLS handshake; the toggle is a thin adapter over
    // the productive install path the bootstrap wired. The
    // returned `Result<u16, TransportOutcome>` is forwarded into
    // the discriminated union so a failed TLS install can never
    // collapse into `active`.
    let pairing_result = sync_pairing_transport_with_toggle(context, enabled);
    Ok(PeerSharingToggleResponse::from_pairing(
        &runtime,
        settings.local_peer_sharing_enabled,
        pairing_result,
    ))
}

/// Drive the productive pairing transport in lockstep with the
/// `Compartir en red local` toggle. The shell keeps this helper
/// inline so the toggle command stays a thin adapter. The
/// function returns the typed outcome the install path produced
/// so the toggle can fold it into the documented discriminated
/// union (a failed TLS install MUST NOT collapse into `active`).
#[cfg(all(
    feature = "local-peer-pairing-tls",
    feature = "local-peer-discovery-mdns"
))]
pub(crate) fn sync_pairing_transport_with_toggle(
    context: &clipvault_core::AppContext,
    enabled: bool,
) -> Result<u16, clipvault_core::peer_pairing::TransportOutcome> {
    if !enabled {
        let _ = context.stop_pairing_transport();
        return Err(clipvault_core::peer_pairing::TransportOutcome::Unavailable);
    }
    // Sharing is on: resolve the local identity so the productive
    // install path can mint the cert and publish the real
    // ephemeral port. A missing / unloaded identity surfaces the
    // same `identity_unavailable` reason the discovery runtime
    // already publishes; the toggle keeps the documented degraded
    // copy instead of falling back to a half-broken listener.
    let identity = match context.settings().peer_identity().load_identity() {
        clipvault_core::PeerIdentityOutcome::Ok(identity) => identity,
        clipvault_core::PeerIdentityOutcome::Unavailable => {
            return Err(clipvault_core::peer_pairing::TransportOutcome::Unavailable);
        }
    };
    let display_name = context
        .settings()
        .load(context)
        .local_peer_display_name
        .clone()
        .unwrap_or_default();
    context.install_pairing_local_identity(Some(&identity));
    let Some(mdns_adapter) = context.peer_discovery_mdns_adapter() else {
        return Err(clipvault_core::peer_pairing::TransportOutcome::Unavailable);
    };
    let advertisement_sink =
        clipvault_platform::peer_transport::tls::MdnsPairingAdvertisementSink::new(
            mdns_adapter.clone(),
            &identity,
            display_name.clone(),
        );
    let advertisement: Arc<dyn PairingAdvertisement> = Arc::new(PairingAdvertisementAdapter::new(
        Arc::new(advertisement_sink),
    ));
    let transport_sink: Arc<dyn TransportSink> = Arc::new(context.peer_pairing());
    let resolver: Arc<dyn clipvault_platform::peer_transport::RemotePeerResolver> = Arc::new(
        clipvault_platform::peer_transport::tls::MdnsRemotePeerResolver::new(mdns_adapter),
    );
    context.install_pairing_transport_with_resolver(
        transport_sink,
        advertisement,
        resolver,
        &display_name,
    )
}

#[cfg(not(all(
    feature = "local-peer-pairing-tls",
    feature = "local-peer-discovery-mdns"
)))]
pub(crate) fn sync_pairing_transport_with_toggle(
    context: &clipvault_core::AppContext,
    enabled: bool,
) -> Result<u16, clipvault_core::peer_pairing::TransportOutcome> {
    // Without the productive feature pair the pairing transport
    // cannot install a real listener; a `false` toggle short-
    // circuits to the noop stop so a previous install attempt
    // (built when the feature was enabled) does not leak.
    if !enabled {
        let _ = context.stop_pairing_transport();
    }
    Err(clipvault_core::peer_pairing::TransportOutcome::Unavailable)
}

/// Adapter the toggle uses to bridge the platform-level
/// [`PairingAdvertisementSink`] (the type the
/// [`MdnsPairingAdvertisementSink`] implements) into the
/// runtime-level [`PairingAdvertisement`] trait the bootstrap
/// re-exports. The adapter is feature-gated to the same cfg the
/// pairing transport install path lives under; without the
/// productive feature pair the toggle does not compile the
/// adapter and the runtime falls back to its typed `Unavailable`
/// path.
#[cfg(all(
    feature = "local-peer-pairing-tls",
    feature = "local-peer-discovery-mdns"
))]
struct PairingAdvertisementAdapter {
    inner: Arc<dyn clipvault_platform::peer_transport::PairingAdvertisementSink>,
}

#[cfg(all(
    feature = "local-peer-pairing-tls",
    feature = "local-peer-discovery-mdns"
))]
impl PairingAdvertisementAdapter {
    fn new(inner: Arc<dyn clipvault_platform::peer_transport::PairingAdvertisementSink>) -> Self {
        Self { inner }
    }
}

#[cfg(all(
    feature = "local-peer-pairing-tls",
    feature = "local-peer-discovery-mdns"
))]
impl PairingAdvertisement for PairingAdvertisementAdapter {
    fn publish(&self, bound_port: u16) -> Result<(), TransportError> {
        self.inner.publish(bound_port)
    }
    fn withdraw(&self) -> Result<(), TransportError> {
        self.inner.withdraw()
    }
}

/// Metadata-only snapshot the `Equipos` view renders. The
/// response carries only the persisted `known_peers` rows merged
/// with the in-memory presence state; it NEVER carries IP
/// addresses, ports, raw public keys or clipboard content.
#[tauri::command]
pub fn clipvault_peer_snapshot(
    state: State<'_, SharedState>,
) -> Result<clipvault_core::PeerSnapshot, CommandError> {
    let context = state.context();
    let runtime = context.peer_discovery();
    // The runtime reads every persisted row through the supplied
    // closure so the snapshot stays in sync with `known_peers`
    // without holding the database lock across the runtime
    // callback. The closure maps the database-shaped rows into
    // the metadata-only DTO the frontend renders.
    let rows = {
        let mut db = context.database().lock();
        let conn = db.connection_mut();
        let repo = clipvault_db::KnownPeerRepository::new(conn);
        repo.list().unwrap_or_default()
    };
    Ok(runtime.snapshot(move || rows.clone(), std::time::Instant::now()))
}

/// Trigger an on-demand re-load of the local identity the runtime
/// uses for self-filtering. The shell calls this from the
/// `Refrescar` button next to the identity section so the user
/// can recover without restarting the app when the keychain is
/// temporarily unavailable.
#[tauri::command]
pub fn clipvault_peer_sharing_refresh_identity(
    state: State<'_, SharedState>,
) -> Result<PeerSharingToggleResponse, CommandError> {
    let context = state.context();
    let runtime = context.peer_discovery();
    let outcome = context.settings().peer_identity().load_identity();
    let identity = match outcome {
        clipvault_core::PeerIdentityOutcome::Ok(identity) => Some(identity),
        clipvault_core::PeerIdentityOutcome::Unavailable => None,
    };
    let settings = context.settings().load(context);
    context.refresh_peer_discovery_local_identity(
        identity.as_ref(),
        settings.local_peer_display_name.as_deref(),
    );
    context
        .settings()
        .sync_runtime_with_settings(context, &runtime, &settings);
    Ok(PeerSharingToggleResponse::from_runtime(
        &runtime,
        settings.local_peer_sharing_enabled,
    ))
}

// ---------------------------------------------------------------------------
// Pairing commands
//
// The `local-peer-mutual-pairing` change adds a metadata-only
// bridge for the reciprocal pairing flow. The commands are
// intentionally thin: every cryptographic surface lives in
// `clipvault-core::peer_pairing`; the bridge forwards typed
// arguments, surfaces typed outcomes, and never touches the
// pairing state machine directly. The frontend can therefore
// drive the modal without importing the runtime crate.
//
// Wire contract: every payload is metadata-only. The commands
// never accept an IP, a port, a TLS key, a SAS candidate, a
// signature or any other peer-supplied bytes; the pairing runtime
// owns every byte that crosses the trust boundary.
// ---------------------------------------------------------------------------

/// Stable wire representation of a [`clipvault_core::PairingOutcome`].
/// The frontend branches on `kind` to render the modal's next
/// state without inspecting free-form strings or reconstructing the
/// typed variant on the TypeScript side.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerPairingOutcomeResponse {
    /// The reciprocal approval completed and the row is now
    /// `trusted`. The frontend offers `Desvincular` /
    /// `Bloquear` actions; the trusted row's `peer_id`,
    /// `display_name`, `short_fingerprint` and `paired_at` are
    /// safe to render.
    Trusted {
        peer_id: String,
        display_name: String,
        short_fingerprint: String,
        paired_at: String,
    },
    /// The session is waiting for the remote peer's approval.
    /// The frontend keeps the modal open and shows the SAS code
    /// until either the remote approval arrives or the timeout
    /// expires.
    AwaitingRemoteApproval { session_id: u64 },
    /// The session failed for one of the typed reasons the
    /// runtime surfaces. The frontend renders the matching
    /// copy without inspecting the message.
    Failed { reason: &'static str },
}

impl PeerPairingOutcomeResponse {
    fn from(outcome: clipvault_core::PairingOutcome) -> Self {
        match outcome {
            clipvault_core::PairingOutcome::Trusted(row) => {
                let short_fingerprint = if row.public_key_fingerprint.len() <= 8 {
                    row.public_key_fingerprint.clone()
                } else {
                    row.public_key_fingerprint[..8].to_string()
                };
                PeerPairingOutcomeResponse::Trusted {
                    peer_id: row.peer_id,
                    display_name: row.display_name,
                    short_fingerprint,
                    paired_at: row.paired_at,
                }
            }
            clipvault_core::PairingOutcome::AwaitingRemoteApproval(id) => {
                PeerPairingOutcomeResponse::AwaitingRemoteApproval {
                    session_id: id.as_u64(),
                }
            }
            clipvault_core::PairingOutcome::Failed(error) => {
                let reason = match error {
                    clipvault_core::PairingError::SessionExpired => "session_expired",
                    clipvault_core::PairingError::Cancelled => "cancelled",
                    clipvault_core::PairingError::IncompatibleProtocol => "incompatible_protocol",
                    clipvault_core::PairingError::UnknownOrKeyMismatch => "unknown_or_key_mismatch",
                    clipvault_core::PairingError::Blocked => "blocked",
                    clipvault_core::PairingError::Revoked => "revoked",
                    clipvault_core::PairingError::RateLimited => "rate_limited",
                    clipvault_core::PairingError::TransportUnavailable => "transport_unavailable",
                };
                PeerPairingOutcomeResponse::Failed { reason }
            }
        }
    }
}

/// Start an outbound pairing session against a known peer. The
/// runtime mints a fresh `session_id` and a candidate SAS code;
/// the modal renders the code and waits for the reciprocal
/// approval. The `peer_id` and `display_name` arguments come
/// from the `Equipos` snapshot the discovery runtime exposes; the
/// runtime refuses to start a session against an unknown peer so
/// the shell cannot accidentally prompt for a peer the
/// discovery surface has never observed. The fingerprint comes
/// from `known_peers.full_public_key_fingerprint` so the TLS
/// listener validates the canonical 64-hex SHA-256 (not the
/// 16-hex UI projection that the discovery-side TXT record
/// carries).
#[tauri::command]
pub fn clipvault_peer_pairing_start(
    state: State<'_, SharedState>,
    peer_id: String,
    display_name: String,
) -> Result<PeerPairingOutcomeResponse, CommandError> {
    let context = state.context();
    let runtime = context.peer_pairing();
    let fingerprint = peer_full_fingerprint_for(context, &peer_id)?;
    let outcome = runtime
        .start_outbound(&peer_id, &fingerprint, &display_name)
        .map_err(map_pairing_error)?;
    Ok(PeerPairingOutcomeResponse::from(outcome))
}

/// Record the local user's approval of the SAS code shown next
/// to the modal. The runtime marks the session locally approved;
/// the reciprocal approval is what triggers the
/// [`PeerPairingOutcomeResponse::Trusted`] outcome.
#[tauri::command]
pub fn clipvault_peer_pairing_approve_local(
    state: State<'_, SharedState>,
    session_id: u64,
) -> Result<PeerPairingOutcomeResponse, CommandError> {
    let context = state.context();
    let runtime = context.peer_pairing();
    let outcome = runtime.approve_local(clipvault_core::PairingSessionId(session_id));
    Ok(PeerPairingOutcomeResponse::from(outcome))
}

/// Cancel an in-flight pairing session. Idempotent.
#[tauri::command]
pub fn clipvault_peer_pairing_cancel(
    state: State<'_, SharedState>,
    session_id: u64,
) -> Result<PeerPairingOutcomeResponse, CommandError> {
    let context = state.context();
    let runtime = context.peer_pairing();
    let outcome = runtime.cancel(clipvault_core::PairingSessionId(session_id));
    Ok(PeerPairingOutcomeResponse::from(outcome))
}

/// Metadata-only snapshot of every in-flight pairing session.
/// The frontend renders the modal against the first row; the
/// runtime expires the entries automatically so the snapshot is
/// always in sync with the state machine's hard timeout.
#[tauri::command]
pub fn clipvault_peer_pairing_snapshot(
    state: State<'_, SharedState>,
) -> Result<Vec<clipvault_core::PairingSessionSnapshot>, CommandError> {
    let context = state.context();
    let runtime = context.peer_pairing();
    Ok(runtime.snapshot())
}

/// Disconnect (revoke) a previously-trusted peer. The runtime
/// marks the row `revoked`, clears any in-flight sessions and
/// returns the metadata-only DTO the snapshot renders. The
/// caller can re-pair the same peer later by accepting a new
/// reciprocal SAS exchange.
#[tauri::command]
pub fn clipvault_peer_pairing_revoke(
    state: State<'_, SharedState>,
    peer_id: String,
) -> Result<PeerTrustOperationResponse, CommandError> {
    let context = state.context();
    let runtime = context.peer_pairing();
    let outcome = runtime.revoke(&peer_id);
    Ok(PeerTrustOperationResponse::from(outcome))
}

/// Metadata-only health probe the runtime runs against a
/// trusted peer. The transport dials the remote listener over
/// mTLS, exchanges the bounded `health` envelope, and returns
/// the typed [`PeerPairingHealthResponse`] union. The union is
/// the only outcome the renderer ever sees — the productive
/// path collapses UnknownPeer / KeyMismatch / Revoked / Blocked
/// / IncompatibleProtocol / RateLimited / Cancelled /
/// SessionExpired / TransportUnavailable into the typed
/// `Failed` variant so the bridge never inspects free-form
/// strings or surfaces a `CommandError` the renderer has to
/// translate. The shell never opens history / fetch / import
/// routes — the transport refuses them as `not_available`.
#[tauri::command]
#[cfg(feature = "local-peer-pairing-tls")]
pub fn clipvault_peer_pairing_health(
    state: State<'_, SharedState>,
    peer_id: String,
) -> PeerPairingHealthResponse {
    let context = state.context();
    let runtime = context.peer_pairing();
    let cert_fingerprint = match runtime.cert_fingerprint_for(&peer_id) {
        Ok(value) => value,
        Err(error) => {
            return PeerPairingHealthResponse::from_pairing_error(error);
        }
    };
    match runtime.health_probe(&peer_id, &cert_fingerprint) {
        Ok(snapshot) => PeerPairingHealthResponse::from(snapshot),
        Err(error) => PeerPairingHealthResponse::from_pairing_error(error),
    }
}

#[cfg(not(feature = "local-peer-pairing-tls"))]
#[tauri::command]
pub fn clipvault_peer_pairing_health(
    _state: State<'_, SharedState>,
    _peer_id: String,
) -> PeerPairingHealthResponse {
    PeerPairingHealthResponse::Failed {
        reason: PeerPairingHealthFailureReason::TransportUnavailable,
    }
}

/// Stable wire representation of a
/// [`clipvault_core::PeerPairingHealthSnapshot`]. The frontend
/// branches on `kind` to render the matching copy without
/// inspecting free-form strings or reconstructing the typed
/// variant on the TypeScript side. The bridge never returns a
/// `CommandError` for the health probe: every typed failure
/// collapses into the `Failed` variant so the renderer stays
/// a thin adapter over the discriminated union.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerPairingHealthResponse {
    /// The transport validated the pinned cert fingerprint,
    /// completed the bounded health envelope exchange and
    /// surfaced presence + protocol major.
    Ok {
        peer_id: String,
        protocol_major: i64,
    },
    /// The transport refused the probe for one of the typed
    /// reasons the runtime already branches on. The reason is
    /// a stable identifier the renderer maps to copy without
    /// inspecting the underlying message.
    Failed {
        reason: PeerPairingHealthFailureReason,
    },
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerPairingHealthResponse {
    fn from(snapshot: clipvault_core::peer_pairing::PeerPairingHealthSnapshot) -> Self {
        PeerPairingHealthResponse::Ok {
            peer_id: snapshot.peer_id,
            protocol_major: snapshot.protocol_major,
        }
    }

    /// Bridge the typed [`clipvault_core::PairingError`] the
    /// productive pairing runtime surfaces into the typed
    /// [`PeerPairingHealthFailureReason`] the renderer maps to
    /// copy. Every variant the runtime exposes has a stable
    /// mapping here so the bridge never surfaces a `CommandError`
    /// for the health probe.
    fn from_pairing_error(error: clipvault_core::PairingError) -> Self {
        use clipvault_core::PairingError as E;
        let reason = match error {
            E::SessionExpired => PeerPairingHealthFailureReason::SessionExpired,
            E::Cancelled => PeerPairingHealthFailureReason::Cancelled,
            E::IncompatibleProtocol => PeerPairingHealthFailureReason::IncompatibleProtocol,
            E::UnknownOrKeyMismatch => PeerPairingHealthFailureReason::UnknownOrKeyMismatch,
            E::Blocked => PeerPairingHealthFailureReason::Blocked,
            E::Revoked => PeerPairingHealthFailureReason::Revoked,
            E::RateLimited => PeerPairingHealthFailureReason::RateLimited,
            E::TransportUnavailable => PeerPairingHealthFailureReason::TransportUnavailable,
        };
        PeerPairingHealthResponse::Failed { reason }
    }
}

#[cfg(not(feature = "local-peer-pairing-tls"))]
impl PeerPairingHealthResponse {
    #[allow(dead_code)]
    fn from_pairing_error(error: clipvault_core::PairingError) -> Self {
        let _ = error;
        PeerPairingHealthResponse::Failed {
            reason: PeerPairingHealthFailureReason::TransportUnavailable,
        }
    }
}

/// Stable, sorted identifiers the renderer maps to copy. The
/// wire shape mirrors the TypeScript
/// `PeerPairingHealthFailureReason` enum so a misnamed variant
/// surfaces as a compile-time type error in the renderer.
#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PeerPairingHealthFailureReason {
    UnknownOrKeyMismatch,
    Blocked,
    Revoked,
    IncompatibleProtocol,
    TransportUnavailable,
    Cancelled,
    SessionExpired,
    RateLimited,
}

/// Block a peer. The runtime marks the row `blocked`, clears any
/// in-flight sessions and refuses every future pairing /
/// health probe against the row. Unblocking requires the
/// explicit [`clipvault_peer_pairing_unblock`] command.
#[tauri::command]
pub fn clipvault_peer_pairing_block(
    state: State<'_, SharedState>,
    peer_id: String,
) -> Result<PeerTrustOperationResponse, CommandError> {
    let context = state.context();
    let runtime = context.peer_pairing();
    let outcome = runtime.block(&peer_id);
    Ok(PeerTrustOperationResponse::from(outcome))
}

/// Unblock a previously-blocked peer. The runtime clears the
/// cert fingerprint and paired_at columns so a re-detection
/// cannot claim the previous trust state. The row returns to
/// `unverified`; the user has to re-pair to restore trust.
#[tauri::command]
pub fn clipvault_peer_pairing_unblock(
    state: State<'_, SharedState>,
    peer_id: String,
) -> Result<PeerTrustOperationResponse, CommandError> {
    let context = state.context();
    let runtime = context.peer_pairing();
    let outcome = runtime.unblock(&peer_id);
    Ok(PeerTrustOperationResponse::from(outcome))
}

/// Wire representation of a [`clipvault_core::TrustOperationOutcome`].
/// The frontend branches on `kind` to render the matching copy
/// without inspecting the inner row.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerTrustOperationResponse {
    Stored { trust_state: &'static str },
    Conflict { trust_state: &'static str },
    Unknown,
}

impl PeerTrustOperationResponse {
    fn from(outcome: clipvault_core::TrustOperationOutcome) -> Self {
        match outcome {
            clipvault_core::TrustOperationOutcome::Stored(row) => {
                PeerTrustOperationResponse::Stored {
                    trust_state: trust_state_str(row.trust_state),
                }
            }
            clipvault_core::TrustOperationOutcome::Conflict(row) => {
                PeerTrustOperationResponse::Conflict {
                    trust_state: trust_state_str(row.trust_state),
                }
            }
            clipvault_core::TrustOperationOutcome::Unknown => PeerTrustOperationResponse::Unknown,
        }
    }
}

fn trust_state_str(state: clipvault_db::TrustState) -> &'static str {
    state.as_str()
}

/// Resolve the canonical full public-key fingerprint the pairing
/// runtime hands to the productive TLS listener. The
/// `known_peers.full_public_key_fingerprint` column is only
/// populated by a `capability = pairing` mDNS observation, so a
/// row that was first seen via `discovery_only` collapses into
/// [`CommandError`] of kind `pairing_fingerprint_missing`. The
/// `peer_pairing_start` command surfaces the typed reason so the
/// UI can prompt the user to wait for a pairing advertisement
/// instead of silently feeding the short 16-hex fingerprint into
/// the mTLS layer.
fn peer_full_fingerprint_for(
    context: &clipvault_core::AppContext,
    peer_id: &str,
) -> Result<String, CommandError> {
    let mut db = context.database().lock();
    let conn = db.connection_mut();
    let repo = clipvault_db::KnownPeerRepository::new(conn);
    let row = repo
        .get(peer_id)
        .map_err(|error| {
            CommandError::new(
                "known_peers_error",
                format!("known_peers lookup failed: {error}"),
            )
        })?
        .ok_or_else(|| {
            CommandError::new(
                "peer_not_found",
                format!("peer {peer_id} not in known_peers"),
            )
        })?;
    if row.full_public_key_fingerprint.is_empty() {
        return Err(CommandError::new(
            "pairing_fingerprint_missing",
            format!(
                "peer {peer_id} has no full pairing fingerprint yet; \
                 wait for a pairing advertisement before starting a session"
            ),
        ));
    }
    Ok(row.full_public_key_fingerprint)
}

fn map_pairing_error(error: clipvault_core::PairingError) -> CommandError {
    CommandError::new("pairing_error", format!("pairing failed: {error}"))
}

// ---------------------------------------------------------------------------
// `peer-text-history-browser` bridge.
//
// The command is a metadata-only thin adapter over
// [`clipvault_core::peer_text_history`]. The shell calls it
// whenever the user opens the `Equipos vinculados` row of a
// paired peer; the runtime consults the in-memory trust /
// active cache, projects the bounded transferable text page
// and returns a discriminated union the renderer branches on
// (`Ok` / `InvalidCursor` / `PeerUnavailable` / `PersistenceUnavailable`).
// The command never opens a network call by itself: the
// trust / active check happens locally against the cache the
// shell populated on every snapshot / health probe, and the
// projection only reads from SQLite.
// ---------------------------------------------------------------------------

/// Wire representation of
/// [`clipvault_core::peer_text_history::PeerHistoryOutcome`]. The
/// frontend branches on `kind` to render the matching copy
/// without inspecting the inner list. The bridge never returns
/// a `CommandError` for the page request: every typed failure
/// collapses into a discriminated variant so the renderer
/// stays a thin adapter over the union.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerHistoryBrowseResponse {
    /// The host returned a bounded transferable text page.
    /// `rows` carries the metadata-only DTOs the renderer
    /// renders; `next_cursor` is the opaque cursor the renderer
    /// must submit to fetch the next page (empty when the page
    /// is the last one); `snapshot_id` is the stable fingerprint
    /// the renderer can compare across page requests to detect
    /// a local capture that landed between the two.
    Ok {
        rows: Vec<PeerHistoryRow>,
        next_cursor: String,
        snapshot_id: String,
    },
    /// The cursor the renderer submitted was not minted by this
    /// host (or the per-peer secret rotated under it). The
    /// runtime never retries; the renderer surfaces a typed
    /// reason and asks the user to restart the browse.
    InvalidCursor,
    /// The peer is not currently eligible to serve a page
    /// (no known row, not trusted, or not active). The runtime
    /// never opened a network call; the renderer surfaces the
    /// stable reason copy.
    PeerUnavailable { reason: &'static str },
    /// The productive mTLS transport rejected the page request
    /// (`unavailable` / `unknown_peer` / `key_mismatch` /
    /// `revoked` / `blocked` / `incompatible_protocol` /
    /// `malformed`). The renderer surfaces the typed reason
    /// copy without retrying blindly.
    TransportUnavailable { reason: &'static str },
}

/// Metadata-only row the renderer renders. The struct mirrors
/// [`clipvault_core::peer_text_history::RemoteTextPreview`];
/// the bridge keeps the wire shape stable by serialising the
/// `content_type` as the canonical snake_case string the local
/// SQLite layer persists.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PeerHistoryRow {
    pub remote_entry_id: String,
    pub title: Option<String>,
    pub content_type: String,
    pub created_at: String,
    pub preview: String,
}

impl PeerHistoryBrowseResponse {
    fn from_outcome(outcome: clipvault_core::peer_text_history::PeerHistoryOutcome) -> Self {
        match outcome {
            clipvault_core::peer_text_history::PeerHistoryOutcome::Ok { page, snapshot_id } => {
                PeerHistoryBrowseResponse::Ok {
                    rows: page
                        .rows
                        .into_iter()
                        .map(|row| PeerHistoryRow {
                            remote_entry_id: row.remote_entry_id,
                            title: row.title,
                            content_type: row.content_type,
                            created_at: row.created_at,
                            preview: row.preview,
                        })
                        .collect(),
                    next_cursor: page
                        .next_cursor
                        .map(|cursor| cursor.as_str().to_string())
                        .unwrap_or_default(),
                    snapshot_id,
                }
            }
            clipvault_core::peer_text_history::PeerHistoryOutcome::InvalidCursor => {
                PeerHistoryBrowseResponse::InvalidCursor
            }
            clipvault_core::peer_text_history::PeerHistoryOutcome::PeerUnavailable { reason } => {
                PeerHistoryBrowseResponse::PeerUnavailable { reason }
            }
            clipvault_core::peer_text_history::PeerHistoryOutcome::TransportUnavailable {
                reason,
            } => PeerHistoryBrowseResponse::TransportUnavailable { reason },
        }
    }
}

/// Browse the transferable text history of `peer_id`. The
/// runtime consults the in-memory trust / active cache and
/// refuses to dial when the peer is not trusted, not
/// present or unknown. Once the gate opens, the service
/// reaches the remote listener through the productive
/// [`PeerTransport::list_recent_text`] mTLS path the
/// pairing change installed — the local SQLite layer is
/// NEVER consulted as a source of remote previews. The
/// command never mutates SQLite in response to a browsing
/// call and never emits a `history-updated` event.
///
/// The cert fingerprint the runtime ships to the transport
/// comes from the cached `PairingRuntime` snapshot (the
/// value the productive pairing handshake persisted at
/// promotion time). The bridge NEVER accepts a fingerprint
/// from the renderer: the UI cannot mint or rotate the pin.
#[tauri::command]
pub fn clipvault_peer_history_browse(
    state: State<'_, SharedState>,
    peer_id: String,
    cursor: Option<String>,
    limit: Option<u32>,
) -> PeerHistoryBrowseResponse {
    let context = state.context();
    let service = context.peer_text_history();
    let cursor = cursor
        .filter(|value| !value.is_empty())
        .map(clipvault_core::peer_text_history::RemoteHistoryCursor::from_string);
    let limit = limit
        .unwrap_or(clipvault_core::peer_text_history::DEFAULT_PAGE_ROWS as u32)
        .min(clipvault_core::peer_text_history::MAX_PAGE_ROWS as u32)
        .max(1);
    let cert_fingerprint = match context.peer_pairing().cert_fingerprint_for(&peer_id) {
        Ok(fingerprint) => fingerprint,
        Err(_) => {
            return PeerHistoryBrowseResponse::PeerUnavailable {
                reason: "not_trusted",
            };
        }
    };
    let outcome = service.browse(&peer_id, &cert_fingerprint, cursor.as_ref(), limit);
    PeerHistoryBrowseResponse::from_outcome(outcome)
}

/// Best-effort sync hook the shell calls after every peer
/// snapshot / health probe so the in-memory trust / active
/// cache the [`clipvault_peer_history_browse`] command
/// consults cannot outrun the runtime transition that should
/// invalidate it. The hook is metadata-only: it never mutates
/// SQLite, never opens a network call, and never emits a
/// `history-updated` event.
#[tauri::command]
pub fn clipvault_peer_history_record_state(
    state: State<'_, SharedState>,
    peer_id: String,
    trusted: bool,
    active: bool,
) {
    let context = state.context();
    let service = context.peer_text_history();
    service.record_peer_state(
        &peer_id,
        clipvault_core::peer_text_history::PeerActiveState { trusted, active },
    );
}

/// Forget the cache entry for `peer_id`. The shell calls this
/// after `Desvincular`, `Bloquear` and `Desbloquear` so a
/// subsequent browse collapses to
/// [`PeerHistoryBrowseResponse::PeerUnavailable`] without a
/// network round-trip.
#[tauri::command]
pub fn clipvault_peer_history_forget(state: State<'_, SharedState>, peer_id: String) {
    let context = state.context();
    let service = context.peer_text_history();
    service.forget_peer(&peer_id);
}

// ---------------------------------------------------------------------------
// `peer-text-import` bridge.
//
// The command is a metadata-only thin adapter over
// [`clipvault_core::peer_text_import::PeerImportService`]. The shell
// calls it whenever the user activates `Importar` for a row of a
// trusted, active peer; the runtime consults the in-memory trust /
// active cache, dials the productive mTLS transport, validates the
// body and commits the import transaction. Every typed failure
// collapses into a discriminated variant so the renderer branches
// on `kind` without inspecting free-form strings or content bytes.
// The bridge never returns a typed `CommandError` for an import
// request: every typed failure collapses into a variant.
// ---------------------------------------------------------------------------

/// Wire representation of
/// [`clipvault_core::peer_text_import::PeerImportOutcome`]. The
/// frontend branches on `kind` to render the matching copy
/// without inspecting the inner list. The bridge never returns
/// a `CommandError` for an import request: every typed failure
/// collapses into a discriminated variant so the renderer
/// stays a thin adapter over the union.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerImportResponse {
    /// The import transaction committed. `entry_id` is the
    /// local row the dedupe path produced (existing or freshly
    /// created); `collection_id` is the peer-bound collection
    /// the entry was added to; `deduplicated` distinguishes a
    /// fresh insert from a snapshot that reused an existing
    /// local entry.
    Imported {
        entry_id: i64,
        collection_id: i64,
        deduplicated: bool,
    },
    /// The peer is not currently eligible to serve an import
    /// (no known row, not trusted, or not active). The runtime
    /// never opened a network call; the renderer surfaces the
    /// stable reason copy.
    PeerUnavailable { reason: &'static str },
    /// The fetch transport rejected the request. The renderer
    /// surfaces the typed reason without retrying blindly.
    TransportUnavailable { reason: &'static str },
    /// The body the host returned exceeded the 1 MiB cap. The
    /// runtime collapsed the rejection into a typed outcome
    /// without persisting anything.
    BodyTooLarge,
    /// The body the host returned was not valid UTF-8. The
    /// runtime collapsed the rejection into a typed outcome
    /// without persisting anything.
    InvalidUtf8,
    /// The remote entry the user asked to import no longer
    /// exists on the host or is no longer transferrable. The
    /// runtime surfaces the typed outcome without mutating
    /// SQLite.
    NotTransferable,
    /// The imported body was empty after trimming. The runtime
    /// refuses to store empty entries; this outcome collapses
    /// the typed reason the renderer surfaces.
    EmptyContent,
    /// The remote title the host returned failed local title
    /// validation (too long after trimming). The runtime
    /// continues to import the entry without persisting the
    /// invalid title.
    TitleInvalid,
    /// The local SQLite layer refused the commit. The runtime
    /// rolled the whole transaction back so the local database
    /// stays consistent.
    PersistenceError { reason: &'static str },
}

impl PeerImportResponse {
    fn from_outcome(outcome: clipvault_core::peer_text_import::PeerImportOutcome) -> Self {
        use clipvault_core::peer_text_import::PeerImportOutcome as Core;
        match outcome {
            Core::Imported {
                entry_id,
                collection_id,
                deduplicated,
            } => PeerImportResponse::Imported {
                entry_id,
                collection_id,
                deduplicated,
            },
            Core::PeerUnavailable { reason } => PeerImportResponse::PeerUnavailable { reason },
            Core::TransportUnavailable { reason } => {
                PeerImportResponse::TransportUnavailable { reason }
            }
            Core::BodyTooLarge => PeerImportResponse::BodyTooLarge,
            Core::InvalidUtf8 => PeerImportResponse::InvalidUtf8,
            Core::NotTransferable => PeerImportResponse::NotTransferable,
            Core::EmptyContent => PeerImportResponse::EmptyContent,
            Core::TitleInvalid => PeerImportResponse::TitleInvalid,
            Core::PersistenceError { reason } => PeerImportResponse::PersistenceError { reason },
        }
    }
}

/// Import the canonical UTF-8 text of a remote entry. The
/// runtime consults the in-memory trust / active cache and
/// refuses to dial when the peer is not trusted, not present
/// or unknown. Once the gate opens, the service reaches the
/// remote listener through the productive mTLS dial driver the
/// pairing change installed; the local SQLite commit happens
/// inside a single transaction so a failure rolls the whole
/// import back. The command never writes to the clipboard,
/// never invokes the paste path and never emits a
/// `history-updated` event carrying the imported body.
///
/// On a successful `Imported` outcome the command emits the
/// existing `clipvault://organization-updated` event so the
/// sidebar refreshes the projection (the peer-bound collection
/// becomes visible and the origin marker is rendered) without a
/// manual re-fetch. The event payload is metadata-only (`()`)
/// so the body, the imported hash and the binding's `peer_id`
/// never cross the bridge.
#[tauri::command]
pub fn clipvault_peer_import_fetch(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    peer_id: String,
    remote_entry_id: String,
    display_name: String,
) -> PeerImportResponse {
    let context = state.context();
    let service = context.peer_text_import();
    let cert_fingerprint = match context.peer_pairing().cert_fingerprint_for(&peer_id) {
        Ok(fingerprint) => fingerprint,
        Err(_) => {
            return PeerImportResponse::PeerUnavailable {
                reason: "not_trusted",
            };
        }
    };
    let outcome = service.import(&peer_id, &cert_fingerprint, &remote_entry_id, &display_name);
    if matches!(
        outcome,
        clipvault_core::peer_text_import::PeerImportOutcome::Imported { .. }
    ) {
        emit_organization_updated(&handle);
    }
    PeerImportResponse::from_outcome(outcome)
}

/// Best-effort sync hook the shell calls after every peer
/// snapshot / health probe so the in-memory trust / active cache
/// the [`clipvault_peer_import_fetch`] command consults cannot
/// outrun the runtime transition that should invalidate it.
/// The hook is metadata-only: it never mutates SQLite, never
/// opens a network call, and never emits a `history-updated`
/// event.
#[tauri::command]
pub fn clipvault_peer_import_record_state(
    state: State<'_, SharedState>,
    peer_id: String,
    trusted: bool,
    active: bool,
) {
    let context = state.context();
    let service = context.peer_text_import();
    service.record_peer_state(
        &peer_id,
        clipvault_core::peer_text_import::PeerImportTrustState { trusted, active },
    );
}

/// Forget the cache entry for `peer_id`. The shell calls this
/// after `Desvincular`, `Bloquear` and `Desbloquear` so a
/// subsequent import collapses to
/// [`PeerImportResponse::PeerUnavailable`] without a network
/// round-trip.
#[tauri::command]
pub fn clipvault_peer_import_forget(state: State<'_, SharedState>, peer_id: String) {
    let context = state.context();
    let service = context.peer_text_import();
    service.forget_peer(&peer_id);
}

// ---------------------------------------------------------------------------
// `peer-image-import` bridge.
//
// The shell calls [`clipvault_peer_image_browse`] when the user opens
// a paired, active peer and the rail needs to render the transferable
// image rows the host projects. The shell calls
// [`clipvault_peer_image_fetch`] when the user activates `Importar`
// for an image row. Both commands are metadata-only thin adapters over
// the [`clipvault_core::peer_image_history::PeerImageHistoryService`]
// and [`clipvault_core::peer_image_import::PeerImageImportService`]
// the bootstrap installed against the shared SQLite handle and the
// productive mTLS pairing transport. The discriminated unions the
// bridge returns keep the wire contract stable: the renderer
// branches on `kind` (`ok` / `invalid_cursor` /
// `peer_unavailable` / `transport_unavailable` for the browse call;
// `imported` / `peer_unavailable` / `transport_unavailable` /
// `body_too_large` / `invalid_image` / `not_transferable` /
// `persistence_error` for the fetch call) without inspecting
// free-form strings or content bytes.
// ---------------------------------------------------------------------------

/// Wire representation of
/// [`clipvault_core::peer_image_history::PeerImageHistoryOutcome`].
/// The frontend branches on `kind` to render the matching copy
/// without inspecting the inner list. The bridge never returns a
/// `CommandError` for a page request: every typed failure collapses
/// into a discriminated variant so the renderer stays a thin
/// adapter over the union.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerImageBrowseResponse {
    /// The page was rendered.
    Ok {
        rows: Vec<PeerImageBrowseRow>,
        /// Opaque cursor the renderer submits verbatim to fetch the
        /// next page. Empty string when the host has no more rows.
        next_cursor: String,
        /// Stable fingerprint the renderer compares across page
        /// requests to detect a local capture that landed between
        /// the two.
        snapshot_id: String,
    },
    /// The cursor was not minted by this host or signed under a
    /// rotated secret.
    InvalidCursor,
    /// The peer is not Active / not trusted / not present.
    PeerUnavailable { reason: &'static str },
    /// The productive mTLS transport rejected the page request.
    TransportUnavailable { reason: &'static str },
}

/// Metadata-only row the renderer renders for an image row of a
/// paired, active peer. The struct mirrors the
/// [`clipvault_core::peer_image_history::RemoteImagePreview`]
/// projection the host emits and never carries image bytes,
/// thumbnails or asset references.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PeerImageBrowseRow {
    pub remote_entry_id: String,
    pub title: Option<String>,
    pub content_type: String,
    pub created_at: String,
    pub byte_size: u64,
    pub width: u32,
    pub height: u32,
}

impl PeerImageBrowseResponse {
    fn from_outcome(outcome: clipvault_core::peer_image_history::PeerImageHistoryOutcome) -> Self {
        use clipvault_core::peer_image_history::PeerImageHistoryOutcome as Core;
        match outcome {
            Core::Ok { page, snapshot_id } => PeerImageBrowseResponse::Ok {
                rows: page
                    .rows
                    .into_iter()
                    .map(|row| PeerImageBrowseRow {
                        remote_entry_id: row.remote_entry_id,
                        title: row.title,
                        content_type: row.content_type,
                        created_at: row.created_at,
                        byte_size: row.byte_size,
                        width: row.width,
                        height: row.height,
                    })
                    .collect(),
                next_cursor: page
                    .next_cursor
                    .map(|c| c.as_str().to_string())
                    .unwrap_or_default(),
                snapshot_id,
            },
            Core::InvalidCursor => PeerImageBrowseResponse::InvalidCursor,
            Core::PeerUnavailable { reason } => PeerImageBrowseResponse::PeerUnavailable { reason },
            Core::TransportUnavailable { reason } => {
                PeerImageBrowseResponse::TransportUnavailable { reason }
            }
        }
    }
}

#[tauri::command]
pub fn clipvault_peer_image_browse(
    state: State<'_, SharedState>,
    peer_id: String,
    cursor: Option<String>,
    limit: Option<u32>,
) -> PeerImageBrowseResponse {
    let context = state.context();
    let service = context.peer_image_history();
    let cursor = cursor
        .filter(|value| !value.is_empty())
        .map(clipvault_core::peer_image_history::RemoteImageHistoryCursor::from_string);
    let limit = limit
        .unwrap_or(clipvault_core::peer_image_history::DEFAULT_IMAGE_PAGE_ROWS as u32)
        .min(clipvault_core::peer_image_history::MAX_IMAGE_PAGE_ROWS as u32)
        .max(1);
    let cert_fingerprint = match context.peer_pairing().cert_fingerprint_for(&peer_id) {
        Ok(fingerprint) => fingerprint,
        Err(_) => {
            return PeerImageBrowseResponse::PeerUnavailable {
                reason: "not_trusted",
            };
        }
    };
    let outcome = service.browse(&peer_id, &cert_fingerprint, cursor.as_ref(), limit);
    PeerImageBrowseResponse::from_outcome(outcome)
}

#[tauri::command]
pub fn clipvault_peer_image_record_state(
    state: State<'_, SharedState>,
    peer_id: String,
    trusted: bool,
    active: bool,
) {
    let context = state.context();
    let service = context.peer_image_history();
    service.record_peer_state(
        &peer_id,
        clipvault_core::peer_image_history::PeerImageActiveState { trusted, active },
    );
}

#[tauri::command]
pub fn clipvault_peer_image_forget(state: State<'_, SharedState>, peer_id: String) {
    let context = state.context();
    let service = context.peer_image_history();
    service.forget_peer(&peer_id);
}

/// Wire representation of
/// [`clipvault_core::peer_image_import::PeerImageImportOutcome`].
/// Every variant is metadata-only; the bridge never returns a
/// `CommandError` for an image import request: every typed failure
/// collapses into a discriminated variant so the renderer stays a
/// thin adapter over the union.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerImageImportResponse {
    /// The import transaction committed.
    Imported {
        entry_id: i64,
        collection_id: i64,
        deduplicated: bool,
    },
    /// The peer is not currently eligible to serve an import.
    PeerUnavailable { reason: &'static str },
    /// The fetch transport rejected the request.
    TransportUnavailable { reason: &'static str },
    /// The body the host returned exceeded the cap.
    BodyTooLarge,
    /// The body the host returned failed PNG validation.
    InvalidImage,
    /// The remote entry the user asked to import no longer exists.
    NotTransferable,
    /// The remote title the host returned failed local validation.
    TitleInvalid,
    /// The local SQLite layer refused the commit.
    PersistenceError { reason: &'static str },
}

impl PeerImageImportResponse {
    fn from_outcome(outcome: clipvault_core::peer_image_import::PeerImageImportOutcome) -> Self {
        use clipvault_core::peer_image_import::PeerImageImportOutcome as Core;
        match outcome {
            Core::Imported {
                entry_id,
                collection_id,
                deduplicated,
            } => PeerImageImportResponse::Imported {
                entry_id,
                collection_id,
                deduplicated,
            },
            Core::PeerUnavailable { reason } => PeerImageImportResponse::PeerUnavailable { reason },
            Core::TransportUnavailable { reason } => {
                PeerImageImportResponse::TransportUnavailable { reason }
            }
            Core::BodyTooLarge => PeerImageImportResponse::BodyTooLarge,
            Core::InvalidImage => PeerImageImportResponse::InvalidImage,
            Core::NotTransferable => PeerImageImportResponse::NotTransferable,
            Core::TitleInvalid => PeerImageImportResponse::TitleInvalid,
            Core::PersistenceError { reason } => {
                PeerImageImportResponse::PersistenceError { reason }
            }
            // The image-import service does not surface the
            // `AssetError` / `EmptyContent` typed variants the
            // `peer-text-import` change keeps; they are
            // collapsed into the typed `persistence_error`
            // reason the renderer surfaces.
            Core::AssetError => PeerImageImportResponse::PersistenceError {
                reason: "asset_error",
            },
        }
    }
}

#[tauri::command]
pub fn clipvault_peer_image_fetch(
    state: State<'_, SharedState>,
    handle: AppHandle<tauri::Wry>,
    peer_id: String,
    remote_entry_id: String,
    display_name: String,
) -> PeerImageImportResponse {
    let context = state.context();
    let service = context.peer_image_import();
    let cert_fingerprint = match context.peer_pairing().cert_fingerprint_for(&peer_id) {
        Ok(fingerprint) => fingerprint,
        Err(_) => {
            return PeerImageImportResponse::PeerUnavailable {
                reason: "not_trusted",
            };
        }
    };
    let outcome = service.import(&peer_id, &cert_fingerprint, &remote_entry_id, &display_name);
    // Emit both events AFTER a successful commit so Historial
    // and the peer-bound collection refresh the metadata-only
    // bridge the spec pins. Both events are fired exactly once
    // per successful commit; a failure path keeps the local
    // state untouched (the importer never produced a row).
    if matches!(
        outcome,
        clipvault_core::peer_image_import::PeerImageImportOutcome::Imported { .. }
    ) {
        emit_history_updated(&handle);
        emit_organization_updated(&handle);
    }
    PeerImageImportResponse::from_outcome(outcome)
}

#[tauri::command]
pub fn clipvault_peer_image_import_record_state(
    state: State<'_, SharedState>,
    peer_id: String,
    trusted: bool,
    active: bool,
) {
    let context = state.context();
    let service = context.peer_image_import();
    service.record_peer_state(
        &peer_id,
        clipvault_core::peer_image_import::PeerImageImportTrustState { trusted, active },
    );
}

#[tauri::command]
pub fn clipvault_peer_image_import_forget(state: State<'_, SharedState>, peer_id: String) {
    let context = state.context();
    let service = context.peer_image_import();
    service.forget_peer(&peer_id);
}

/// Wire representation of
/// [`clipvault_core::peer_image_thumbnail::PeerImageThumbnailOutcome`].
/// Every variant is metadata-only; the bridge never returns a
/// `CommandError` for a thumbnail request: every typed failure
/// collapses into a discriminated variant so the renderer stays a
/// thin adapter over the union. The PNG body never crosses the
/// bridge on a failure path so a stale / drifted / unauthorised
/// caller cannot leak the original image bytes through an error.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerImageThumbnailResponse {
    /// The host returned a valid bounded PNG thumbnail. The
    /// `bytes_b64` field is the canonical PNG payload encoded
    /// with the standard base64 alphabet; the renderer decodes
    /// the field locally, re-validates the size against the cap
    /// and turns the bytes into an in-memory Object URL.
    Ok {
        bytes_b64: String,
        width: u32,
        height: u32,
    },
    /// The peer is not currently eligible to serve a thumbnail.
    PeerUnavailable { reason: &'static str },
    /// The peer did not advertise the `image_preview_thumbnail`
    /// capability.
    CapabilityMissing,
    /// The fetch transport rejected the request.
    TransportUnavailable { reason: &'static str },
    /// The host reached the per-peer decode/resize concurrency
    /// limit. The renderer keeps the static placeholder; a later
    /// refresh can retry the call.
    Busy,
    /// The body the host returned exceeded the documented byte
    /// cap.
    BodyTooLarge,
    /// The body the host returned failed PNG validation.
    InvalidPng,
    /// The remote entry the user asked to thumbnail no longer
    /// exists on the host or is no longer transferable.
    NotTransferable,
}

impl PeerImageThumbnailResponse {
    fn from_outcome(
        outcome: clipvault_core::peer_image_thumbnail::PeerImageThumbnailOutcome,
    ) -> Self {
        use clipvault_core::peer_image_thumbnail::PeerImageThumbnailOutcome as Core;
        match outcome {
            Core::Ok {
                bytes_b64,
                width,
                height,
            } => PeerImageThumbnailResponse::Ok {
                bytes_b64,
                width,
                height,
            },
            Core::PeerUnavailable { reason } => {
                PeerImageThumbnailResponse::PeerUnavailable { reason }
            }
            Core::CapabilityMissing => PeerImageThumbnailResponse::CapabilityMissing,
            Core::TransportUnavailable { reason } => {
                PeerImageThumbnailResponse::TransportUnavailable { reason }
            }
            Core::Busy => PeerImageThumbnailResponse::Busy,
            Core::BodyTooLarge => PeerImageThumbnailResponse::BodyTooLarge,
            Core::InvalidPng => PeerImageThumbnailResponse::InvalidPng,
            Core::NotTransferable => PeerImageThumbnailResponse::NotTransferable,
        }
    }
}

/// Thin Tauri command the
/// `peer-image-preview-thumbnails` change exposes through the
/// bridge. The command is the metadata-only adapter the
/// frontend drives when a remote image card intersects the
/// visible remote-history viewport. The command delegates to
/// [`clipvault_core::peer_image_thumbnail::PeerImageThumbnailService`]
/// so the runtime owns the trust / active gate, the capability
/// gate and the bytes validation; the bridge stays a thin
/// adapter that only projects the typed outcome.
#[tauri::command]
pub fn clipvault_peer_image_thumbnail_fetch(
    state: State<'_, SharedState>,
    peer_id: String,
    remote_entry_id: String,
) -> PeerImageThumbnailResponse {
    let context = state.context();
    let service = context.peer_image_thumbnail();
    // The runtime requires the mTLS pin to dial the remote
    // listener; a missing pin collapses to the typed
    // `not_trusted` reason the bridge surfaces so the renderer
    // keeps the static placeholder without surfacing a global
    // rail error.
    let cert_fingerprint = match context.peer_pairing().cert_fingerprint_for(&peer_id) {
        Ok(fingerprint) => fingerprint,
        Err(_) => {
            return PeerImageThumbnailResponse::PeerUnavailable {
                reason: "not_trusted",
            };
        }
    };
    let outcome = service.fetch_thumbnail(&peer_id, &cert_fingerprint, &remote_entry_id);
    PeerImageThumbnailResponse::from_outcome(outcome)
}

/// Mirror of [`clipvault_peer_image_import_record_state`] for
/// the thumbnail service. The frontend calls this whenever the
/// active peer's trust / active state changes so the service can
/// short-circuit the network round-trip when the peer is no
/// longer eligible.
#[tauri::command]
pub fn clipvault_peer_image_thumbnail_record_state(
    state: State<'_, SharedState>,
    peer_id: String,
    trusted: bool,
    active: bool,
) {
    let context = state.context();
    let service = context.peer_image_thumbnail();
    service.record_peer_state(
        &peer_id,
        clipvault_core::peer_image_thumbnail::PeerImageThumbnailTrustState { trusted, active },
    );
}

/// Mirror of [`clipvault_peer_image_import_forget`] for the
/// thumbnail service. The runtime calls this whenever the row
/// leaves the trusted / active state (revoke, block) so a stale
/// entry cannot resurrect the link.
#[tauri::command]
pub fn clipvault_peer_image_thumbnail_forget(state: State<'_, SharedState>, peer_id: String) {
    let context = state.context();
    let service = context.peer_image_thumbnail();
    service.forget_peer(&peer_id);
}

/// Typed bridge result for on-demand source-app presentation. Icon
/// bytes are returned only for a single visible remote entry after the
/// separate authenticated request; they are never included in browse
/// or thumbnail DTOs and are not persisted by this command.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerSourceAppPresentationResponse {
    Ok {
        source_app_name: Option<String>,
        source_app_icon_bytes: Option<Vec<u8>>,
    },
    PeerUnavailable,
    NotTrusted,
    CapabilityMissing,
    TransportUnavailable,
    InvalidIcon,
}

impl PeerSourceAppPresentationResponse {
    fn from_outcome(outcome: clipvault_core::PeerSourceAppPresentation) -> Self {
        use clipvault_core::PeerSourceAppPresentation as Core;
        match outcome {
            Core::Ok {
                source_app_name,
                source_app_icon_bytes,
            } => Self::Ok {
                source_app_name,
                source_app_icon_bytes,
            },
            Core::PeerUnavailable => Self::PeerUnavailable,
            Core::NotTrusted => Self::NotTrusted,
            Core::CapabilityMissing => Self::CapabilityMissing,
            Core::TransportUnavailable => Self::TransportUnavailable,
            Core::InvalidIcon => Self::InvalidIcon,
        }
    }
}

/// Fetch source-app name/icon for one visible row of the selected
/// remote rail. Core and transport revalidate trust, mTLS identity,
/// capability, eligibility, and payload bounds; the bridge does not
/// accept an icon reference or filesystem path from the frontend.
#[tauri::command]
pub fn clipvault_peer_source_app_presentation_fetch(
    state: State<'_, SharedState>,
    peer_id: String,
    remote_entry_id: String,
) -> PeerSourceAppPresentationResponse {
    let context = state.context();
    let service = context.peer_source_app_presentation();
    let cert_fingerprint = match context.peer_pairing().cert_fingerprint_for(&peer_id) {
        Ok(fingerprint) => fingerprint,
        Err(_) => return PeerSourceAppPresentationResponse::PeerUnavailable,
    };
    PeerSourceAppPresentationResponse::from_outcome(service.fetch(
        &peer_id,
        &cert_fingerprint,
        &remote_entry_id,
    ))
}

#[tauri::command]
pub fn clipvault_peer_source_app_presentation_record_state(
    state: State<'_, SharedState>,
    peer_id: String,
    trusted: bool,
    active: bool,
) {
    state
        .context()
        .peer_source_app_presentation()
        .record_peer_state(
        &peer_id,
        clipvault_core::peer_source_app_presentation_service::PeerSourceAppPresentationTrustState {
            trusted,
            active,
        },
    );
}

#[tauri::command]
pub fn clipvault_peer_source_app_presentation_forget(
    state: State<'_, SharedState>,
    peer_id: String,
) {
    state
        .context()
        .peer_source_app_presentation()
        .forget_peer(&peer_id);
}

/// Return peer-specific import attribution for a bounded set of local
/// rows, only when `collection_id` is bound to the provenance peer. The
/// repository returns no matches for general history or unrelated
/// collections; this DTO never contains clipboard bodies or peer IDs.
#[tauri::command]
pub fn clipvault_peer_import_source_app_presentations(
    state: State<'_, SharedState>,
    collection_id: i64,
    entry_ids: Vec<i64>,
) -> Result<Vec<clipvault_core::PeerImportedSourceAppPresentation>, CommandError> {
    state
        .context()
        .peer_text_import()
        .source_app_presentations_for_collection(collection_id, &entry_ids)
        .map_err(|_| {
            CommandError::new(
                "peer_import_projection_unavailable",
                "Peer import attribution is unavailable",
            )
        })
}
