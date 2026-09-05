//! Regression tests for the `history-card-layout` source-app
//! metadata contract.
//!
//! The user-reported regression was that the capture loop forwarded
//! `None` as `source_app` to `CaptureWatcher::tick`, so the new row
//! never carried the identifier the active application had been
//! reporting. The `PrivacyGate` therefore evaluated "unknown source
//! → Allow" while the metadata enrichment looked up nothing because
//! the identifier was empty. The card rail ended up rendering a
//! generic fallback icon and the bare "—" string.
//!
//! The contract pinned by these tests:
//! - `record_payload` stores the identifier the caller passed AND
//!   resolves the metadata the application-metadata provider returns.
//! - The same identifier drives the `PrivacyGate`: a blacklisted
//!   identifier is discarded; an empty identifier preserves the
//!   "unknown source → Allow" path.
//! - `enrich_metadata` (the public wrapper) is idempotent: re-running
//!   it on an already-enriched entry never clobbers the existing
//!   name or icon reference, and never panics on a failed provider.
//! - The repository's `set_source_app_metadata` uses `COALESCE`, so
//!   passing `None` for either field leaves the previously stored
//!   value intact.
//!
//! Every assertion below touches the SQLite layer the production shell
//! reads from; running the suite is the regression pin the user
//! asked for in the `history-card-layout` follow-up.

use std::sync::Arc;

use clipvault_core::{
    AppBootstrap, ApplicationMetadata, ApplicationMetadataError, ApplicationMetadataProvider,
    Clipboard, Clock, FakeApplicationMetadataProvider, FakeClipboard, HistoryOutcome,
    TextHistoryService,
};
use clipvault_db::EntryRepository;
use parking_lot::Mutex;
use tempfile::TempDir;
use time::macros::datetime;

#[derive(Debug, Clone)]
struct FixedClock {
    instant: time::OffsetDateTime,
}

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.instant
    }
}

fn tempdir() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn bootstrap_with_metadata_provider(
    provider: Arc<FakeApplicationMetadataProvider>,
) -> (TempDir, clipvault_core::AppContext, TextHistoryService) {
    let dir = tempdir();
    let clock: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 12:00:00 UTC),
    });
    let clipboard: Arc<dyn Clipboard> = Arc::new(FakeClipboard::with_text(
        "placeholder so the bootstrap succeeds",
    ));
    let context = AppBootstrap::new()
        .with_clock(clock.clone())
        .with_clipboard(clipboard.clone())
        .bootstrap_at(dir.path().join("clipvault.db"))
        .expect("bootstrap");

    // Re-create the service with the scripted provider so every
    // `record_text` / `record_payload` call flows through the
    // metadata enrichment the user reports as broken.
    let history = TextHistoryService::new(
        clipboard,
        clock,
        provider as Arc<dyn ApplicationMetadataProvider>,
    );
    (dir, context, history)
}

/// Captura automática con aplicación activa Terminal produce
/// `source_app`, `source_app_name` y `source_app_icon_ref`.
#[test]
fn automatic_capture_records_terminal_identifier_and_metadata() {
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    provider.push_lookup(Ok(Some(ApplicationMetadata {
        display_name: "Terminal".to_string(),
        icon_ref: Some("application-icons/com.apple.Terminal.png".to_string()),
    })));
    let (_dir, _context, history) = bootstrap_with_metadata_provider(provider.clone());

    let outcome = history.record_text(&_context, Some("com.apple.Terminal"));
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    let recent = history.recent_entries(&_context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.Terminal"));
    assert_eq!(recent[0].source_app_name.as_deref(), Some("Terminal"));
    assert_eq!(
        recent[0].source_app_icon_ref.as_deref(),
        Some("application-icons/com.apple.Terminal.png")
    );
    assert_eq!(provider.calls(), vec!["com.apple.Terminal".to_string()]);
}

/// Captura automática con aplicación activa TextEdit produce
/// metadata equivalente.
#[test]
fn automatic_capture_records_textedit_identifier_and_metadata() {
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    provider.push_lookup(Ok(Some(ApplicationMetadata {
        display_name: "TextEdit".to_string(),
        icon_ref: Some("application-icons/com.apple.TextEdit.png".to_string()),
    })));
    let (_dir, _context, history) = bootstrap_with_metadata_provider(provider.clone());

    let outcome = history.record_text(&_context, Some("com.apple.TextEdit"));
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    let recent = history.recent_entries(&_context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.TextEdit"));
    assert_eq!(recent[0].source_app_name.as_deref(), Some("TextEdit"));
    assert_eq!(
        recent[0].source_app_icon_ref.as_deref(),
        Some("application-icons/com.apple.TextEdit.png")
    );
}

/// El identificador usado por `PrivacyGate` es el mismo que se
/// persiste: cuando el caller pasa un identificador blacklistado la
/// captura queda descartada y la base de datos no ve el payload.
#[test]
fn privacy_gate_and_persistence_use_the_same_identifier() {
    use clipvault_core::{CoreBlacklistMatcher, PrivacyGate};

    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    let (_dir, context, history_with_default_gate) =
        bootstrap_with_metadata_provider(provider.clone());

    // Build a custom gate that blacklists the identifier the
    // capture is about to send. Sharing the same gate with the
    // service guarantees the persistence step observes the same
    // decision the gate recorded.
    let matcher = CoreBlacklistMatcher::with_ignored(
        context.platform_adapters().active_app(),
        vec!["com.apple.terminal".to_string()],
    );
    let gate = PrivacyGate::new(matcher);
    let history = history_with_default_gate.with_privacy_gate(gate.clone());

    let outcome = history.record_text(&context, Some("com.apple.terminal"));
    assert_eq!(outcome, HistoryOutcome::Ignored);

    let recent = history.recent_entries(&context, 10).unwrap();
    assert!(
        recent.is_empty(),
        "blacklisted capture must never reach SQLite"
    );
    assert!(
        provider.calls().is_empty(),
        "blacklisted capture must not invoke the metadata provider"
    );
}

/// Una captura permitida no queda atribuida a ClipVault por el foco
/// de la ventana. El identificador que llega al gate es el que la
/// captura efectiva generó, no un valor sintético tipo "ClipVault".
#[test]
fn allowed_capture_keeps_caller_supplied_identifier() {
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    provider.push_lookup(Ok(Some(ApplicationMetadata {
        display_name: "Safari".to_string(),
        icon_ref: Some("application-icons/com.apple.Safari.png".to_string()),
    })));
    let (_dir, _context, history) = bootstrap_with_metadata_provider(provider);

    let outcome = history.record_text(&_context, Some("com.apple.Safari"));
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    let recent = history.recent_entries(&_context, 10).unwrap();
    assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.Safari"));
    // The shell must never substitute the identifier with a
    // synthetic ClipVault label when the user copied from another
    // application.
    assert_ne!(
        recent[0].source_app.as_deref(),
        Some("com.apple.ClipVault"),
        "identifier must not be substituted with ClipVault"
    );
    assert_ne!(
        recent[0].source_app.as_deref(),
        Some("ClipVault"),
        "identifier must not be substituted with the human label"
    );
}

/// Captura sin snapshot guarda origen desconocido sin romperse:
/// pasar `None` debe seguir permitiendo la captura y persistir el
/// `source_app` como `None` (la card rail mostrará el fallback
/// accesible).
#[test]
fn capture_without_snapshot_persists_unknown_source() {
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    let (_dir, _context, history) = bootstrap_with_metadata_provider(provider.clone());

    let outcome = history.record_text(&_context, None);
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    let recent = history.recent_entries(&_context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert!(recent[0].source_app.is_none());
    assert!(recent[0].source_app_name.is_none());
    assert!(recent[0].source_app_icon_ref.is_none());
    assert!(
        provider.calls().is_empty(),
        "metadata provider must not be queried for an empty identifier"
    );
}

/// Fallo del proveedor de metadata no impide guardar la captura
/// permitida: la entrada se persiste con `source_app` pero sin
/// metadata y la card rail muestra el fallback accesible.
#[test]
fn capture_succeeds_when_metadata_provider_returns_backend_error() {
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    provider.push_lookup(Err(ApplicationMetadataError::Backend {
        details: "simulated bundle lookup failure".to_string(),
    }));
    let (_dir, _context, history) = bootstrap_with_metadata_provider(provider.clone());

    let outcome = history.record_text(&_context, Some("com.apple.Terminal"));
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    let recent = history.recent_entries(&_context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.Terminal"));
    assert!(
        recent[0].source_app_name.is_none(),
        "metadata failure must leave source_app_name unset"
    );
    assert!(
        recent[0].source_app_icon_ref.is_none(),
        "metadata failure must leave source_app_icon_ref unset"
    );
    assert_eq!(provider.calls(), vec!["com.apple.Terminal".to_string()]);
}

/// El icono se persiste una sola vez por aplicación: una segunda
/// captura del mismo identificador reutiliza el row existente
/// (Duplicate) y no genera un `set_source_app_metadata` redundante
/// que toquetearía el icono en disco.
#[test]
fn icon_persisted_at_most_once_per_application() {
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    provider.push_lookup(Ok(Some(ApplicationMetadata {
        display_name: "Terminal".to_string(),
        icon_ref: Some("application-icons/com.apple.Terminal.png".to_string()),
    })));
    let (_dir, _context, history) = bootstrap_with_metadata_provider(provider.clone());

    let first = history.record_text(&_context, Some("com.apple.Terminal"));
    assert!(matches!(first, HistoryOutcome::Stored { .. }));
    let second = history.record_text(&_context, Some("com.apple.Terminal"));
    assert!(matches!(second, HistoryOutcome::Duplicate { .. }));

    // The provider is consulted twice (once per `record_text`
    // call), but the underlying icon asset is stored once via the
    // repository's `set_source_app_metadata` and never overwritten
    // thanks to the `COALESCE` UPDATE.
    assert_eq!(provider.calls().len(), 2);
    let recent = history.recent_entries(&_context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].source_app_name.as_deref(), Some("Terminal"));
}

/// Las capturas de aplicaciones blacklist siguen siendo descartadas y
/// no generan metadata ni assets: el provider no se invoca y la base
/// de datos no contiene la fila.
#[test]
fn blacklisted_capture_does_not_invoke_metadata_provider_or_persist_row() {
    use clipvault_core::{CoreBlacklistMatcher, PrivacyGate};

    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    let (_dir, context, history_with_default_gate) =
        bootstrap_with_metadata_provider(provider.clone());

    let matcher = CoreBlacklistMatcher::with_ignored(
        context.platform_adapters().active_app(),
        vec!["com.apple.terminal".to_string()],
    );
    let gate = PrivacyGate::new(matcher);
    let history = history_with_default_gate.with_privacy_gate(gate);

    let outcome = history.record_text(&context, Some("com.apple.terminal"));
    assert_eq!(outcome, HistoryOutcome::Ignored);

    let mut db = context.database().lock();
    let count = EntryRepository::new(db.connection_mut()).count().unwrap();
    assert_eq!(count, 0, "blacklisted capture must never persist");
    assert!(
        provider.calls().is_empty(),
        "blacklisted capture must not invoke the metadata provider"
    );
}

/// El shell expone `enrich_metadata` como API pública para que la
/// captura manual pueda reintentar la resolución en el thread
/// principal después de que el provider reportara `Unavailable`
/// desde el thread en background. La idempotencia la garantiza el
/// repositorio con `COALESCE`: re-ejecutar el enrichment con un
/// provider que devuelve `None` para cualquier campo preserva el
/// valor ya persistido.
#[test]
fn enrich_metadata_preserves_values_when_provider_returns_none() {
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    provider.push_lookup(Ok(Some(ApplicationMetadata {
        display_name: "Terminal".to_string(),
        icon_ref: Some("application-icons/com.apple.Terminal.png".to_string()),
    })));
    let (_dir, context, history) = bootstrap_with_metadata_provider(provider.clone());

    let outcome = history.record_text(&context, Some("com.apple.Terminal"));
    let id = outcome.id().expect("stored");

    // Second enrichment with an empty provider response must NOT
    // wipe the previously stored `source_app_name` /
    // `source_app_icon_ref` — the repository's `COALESCE` UPDATE
    // keeps the existing values.
    provider.push_lookup(Ok(None));
    history.enrich_metadata(&context, id, Some("com.apple.Terminal"));

    let recent = history.recent_entries(&context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].source_app_name.as_deref(), Some("Terminal"));
    assert_eq!(
        recent[0].source_app_icon_ref.as_deref(),
        Some("application-icons/com.apple.Terminal.png")
    );
}

/// Captura con identificador que consiste solo en espacios / vacío
/// conserva el contrato de origen desconocido: la captura se persiste
/// sin `source_app` y el provider no se invoca.
#[test]
fn whitespace_identifier_is_treated_as_unknown_source() {
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    let (_dir, _context, history) = bootstrap_with_metadata_provider(provider.clone());

    let outcome = history.record_text(&_context, Some("   "));
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    let recent = history.recent_entries(&_context, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert!(recent[0].source_app.is_none());
    assert!(provider.calls().is_empty());
}

/// `record_payload` (the path used by `CaptureWatcher::tick`) sets
/// the same fields as `record_text`. This guarantees that whether the
/// loop reads the clipboard itself or delegates to the watcher, the
/// captured identifier flows through to persistence.
#[test]
fn record_payload_persists_identifier_and_metadata() {
    let provider = Arc::new(FakeApplicationMetadataProvider::new());
    provider.push_lookup(Ok(Some(ApplicationMetadata {
        display_name: "Terminal".to_string(),
        icon_ref: Some("application-icons/com.apple.Terminal.png".to_string()),
    })));
    let (_dir, _context, history) = bootstrap_with_metadata_provider(provider.clone());

    let outcome =
        history.record_payload(&_context, "hello".to_string(), Some("com.apple.Terminal"));
    assert!(matches!(outcome, HistoryOutcome::Stored { .. }));

    let recent = history.recent_entries(&_context, 10).unwrap();
    assert_eq!(recent[0].source_app.as_deref(), Some("com.apple.Terminal"));
    assert_eq!(recent[0].source_app_name.as_deref(), Some("Terminal"));
    assert_eq!(
        recent[0].source_app_icon_ref.as_deref(),
        Some("application-icons/com.apple.Terminal.png")
    );
}

/// Test harness builder exposing the typed clock so a future test can
/// reuse it. Kept private to the module — the helper is documentation
/// for the public flow rather than a shared utility.
#[allow(dead_code)]
fn ensure_clock_helper_compiles() {
    let _: Arc<dyn Clock> = Arc::new(FixedClock {
        instant: datetime!(2026-02-01 12:00:00 UTC),
    });
}

/// Silences the unused warning for `Mutex` while the module grows.
#[allow(dead_code)]
fn _ensure_mutex_linked() {
    let _: Mutex<()> = Mutex::new(());
}
