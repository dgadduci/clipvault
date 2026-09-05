//! Business-logic core for ClipVault.
//!
//! `clipvault-core` is the only place that should know how to wire the
//! database, the platform adapters and the search engine together. It
//! is deliberately GUI-agnostic: no Tauri, no Svelte, no webview. The
//! desktop shell in `app/tauri` consumes an [`AppContext`] built by
//! [`bootstrap::AppBootstrap`].

pub mod active_app_diagnostics;
pub mod bootstrap;
pub mod clipboard;
pub mod clipboard_assets;
pub mod clock;
pub mod content_type;
pub mod diagnostics;
pub mod fakes;
pub mod history;
pub mod ignored_apps;
pub mod ignored_apps_service;
pub mod management;
pub mod organization;
pub mod paste;
pub mod paste_suppression;
pub mod platform;
pub mod platform_adapters;
pub mod privacy;
pub mod redact;
pub mod rich_text;
pub mod search;
pub mod settings;
pub mod settings_service;
pub mod watcher;

pub use active_app_diagnostics::{
    ActiveAppDiagnostics, ActiveAppDiagnosticsState, ActiveAppFailureKind, ActiveAppRefreshOutcome,
};
pub use bootstrap::{AppBootstrap, AppContext, BootstrapError, BootstrapOptions};
pub use clipboard::{Clipboard, ClipboardError, FakeClipboard};
pub use clipboard_assets::{
    asset_ref_for_hash, decode_png, normalize_image, sha256_hex, AssetError, ClipboardAssetStore,
    NormalizedImage, StoreOutcome, CLIPBOARD_ASSETS_DIR, CLIPBOARD_ASSET_EXTENSION,
    MAX_CLIPBOARD_ASSET_BYTES,
};
pub use clipvault_db::{Collection, CollectionKind, Tag};
pub use clipvault_platform::{
    checked_rgba_len, default_linux_binding, default_macos_binding, detect_capabilities,
    macos_accessibility_guidance, probe_image_clipboard, probe_rich_text_clipboard,
    ActiveAppBackendKind, ActiveAppError, ActiveApplication, ActiveApplicationProbe,
    ApplicationMetadata, ApplicationMetadataError, ApplicationMetadataProvider, Capabilities,
    Capability, ClipboardBackend, ClipboardBackendError, ClipboardBackendKind, ClipboardImage,
    ClipboardPayload, DefaultPlatform, DisplayServer, HotkeyBackendKind, HotkeyBinding,
    HotkeyError, HotkeyKey, HotkeyManager, HotkeyModifiers, HotkeyOutcome, ImageClipboardSupport,
    ImageValidationError, NoopActiveApplicationProbe, NoopApplicationMetadataProvider,
    NoopClipboardBackend, NoopHotkeyManager, NoopPasteController, NoopSettingsNavigator,
    NoopTrayController, NoopTrayHandle, OsFamily, PasteBackendKind, PasteController, PasteError,
    PlatformError, PlatformGuidance, PlatformInfo, PlatformIssueKind, PlatformSettingsTarget,
    RichTextClipboardSupport, RichTextPayload, SettingsNavigator, SettingsOpenOutcome, TrayAction,
    TrayBackendKind, TrayController, TrayEntry, TrayError, TrayHandle, TrayOutcome,
    APPLICATION_ICONS_DIR, MAX_CLIPBOARD_IMAGE_DIM, MAX_CLIPBOARD_IMAGE_RGBA_BYTES,
};
pub use clipvault_search::SearchQuery;
pub use clock::{Clock, SystemClock};
pub use content_type::detect_content_type;
pub use diagnostics::{
    DatabasePath, Diagnostics, DiagnosticsService, HistoryCount, MigrationsApplied,
};
pub use fakes::{
    FakeActiveApplication, FakeApplicationMetadataProvider, FakeClipboardBackend,
    FakeHotkeyManager, FakePasteController, FakeSettingsNavigator, FakeTrayController,
    FakeTrayHandle,
};
pub use history::{
    hash_content, HistoryOutcome, HistoryServiceError, SetTitleOutcome, TextHistoryService,
    TitleValidationError, MAX_TITLE_LENGTH,
};
pub use ignored_apps::{normalize_identifier, IgnoredAppEntry, IgnoredAppError, PickAndAddOutcome};
pub use ignored_apps_service::{IgnoredAppsService, IgnoredAppsServiceError};
pub use management::{
    ClearOutcome, DeleteOutcome, HistoryManagementService, LocalSettingsReader,
    ManagementServiceError, RetentionOutcome, RetentionPolicy, RetentionPreview, SetFavoriteResult,
    SettingsReader, DEFAULT_RETENTION, RETENTION_SETTING_KEY,
};
pub use organization::{
    OrganizationService, OrganizationServiceError, OrganizationSidebarSnapshot,
};
pub use paste::{
    PasteMode, PasteOutcome, PasteService, PasteServiceError, CLIPBOARD_WRITE_IMAGE_CAPABILITY,
    CLIPBOARD_WRITE_RICH_TEXT_CAPABILITY, SYNTHETIC_PASTE_CAPABILITY,
};
pub use paste_suppression::{PasteSuppression, SuppressionFingerprint, DEFAULT_SUPPRESSION_TTL};
pub use platform_adapters::PlatformAdapters;
pub use privacy::{CaptureDecision, CoreBlacklistMatcher, PrivacyGate};
pub use redact::{redact, RedactingMakeWriter, RedactingWriter};
pub use rich_text::{
    canonical_rich_text_hash, canonical_rich_text_repr, sanitize_html, RichTextAssetError,
    RichTextAssetOutcome, RichTextAssetStore, SanitizeError, SanitizedPreview,
    MAX_PREVIEW_INPUT_BYTES, MAX_RICH_TEXT_HTML_BYTES, MAX_RICH_TEXT_PREVIEW_BYTES,
    MAX_RICH_TEXT_RTF_BYTES, RICH_TEXT_ASSETS_DIR, RICH_TEXT_HTML_EXTENSION,
    RICH_TEXT_PREVIEW_EXTENSION, RICH_TEXT_RTF_EXTENSION,
};
pub use search::{
    SearchEntryHit, SearchFilter, SearchService, SearchServiceError, SearchServiceOutcome,
    SEARCH_DEFAULT_LIMIT, SEARCH_MAX_LIMIT,
};
pub use settings::{
    HotkeySpec, Settings, SettingsUpdate, ValidationCode, ValidationError,
    HOTKEY_SETTING_STORAGE_KEY, MAX_IDENTIFIER_LENGTH,
};
pub use settings_service::{SettingsService, SettingsServiceError};
pub use watcher::{CaptureWatcher, WatchTickOutcome};
