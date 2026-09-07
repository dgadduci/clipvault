//! Thin Tauri commands. Each command is a one-liner over
//! `clipvault-core` and forwards the typed outcome to the frontend.

use std::sync::Arc;

use clipvault_core::{
    ActiveAppDiagnostics, Capabilities, ClearOutcome, ClipboardAssetStore,
    CodeLanguageServiceError, CopyOutcome, DeleteOutcome, IgnoredAppEntry, IgnoredAppError,
    LocalSettingsReader, PasteMode, PasteOutcome, PickAndAddOutcome, PlatformGuidance,
    PlatformSettingsTarget, RetentionOutcome, RetentionPolicy, RetentionPreview,
    RichTextAssetStore, SetFavoriteResult, SetTitleOutcome, Settings, SettingsNavigator,
    SettingsOpenOutcome, SettingsServiceError, SettingsUpdate, TitleValidationError,
    ValidationCode, ValidationError, WatchTickOutcome,
};
use clipvault_platform::{
    read_icon_bytes, read_source_app_icon_bytes, ActiveAppError, IconReadError,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime, State};
use tracing::warn;

use crate::state::SharedState;

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
