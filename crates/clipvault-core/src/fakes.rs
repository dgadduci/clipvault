//! Fake platform adapters used by core tests and by the bootstrap when
//! no real implementation is wired in.
//!
//! The fakes are deliberately programmable: each helper exposes a
//! setter or scripted queue so a test can drive any failure path
//! without standing up a real session. They implement the
//! `clipvault_platform` traits so the rest of the crate stays
//! platform-agnostic.

use std::sync::Arc;

use parking_lot::Mutex;

use clipvault_platform::{
    ActiveAppError, ActiveApplication, ActiveApplicationProbe, ApplicationMetadata,
    ApplicationMetadataError, ApplicationMetadataProvider, ClipboardBackend, ClipboardBackendError,
    ClipboardImage, HotkeyBinding, HotkeyError, HotkeyManager, HotkeyOutcome, PasteController,
    PasteError, PlatformSettingsTarget, RichTextPayload, SettingsNavigator, SettingsOpenOutcome,
    TrayAction, TrayController, TrayEntry, TrayError, TrayHandle, TrayOutcome,
};

/// Programmable clipboard backend for tests.
///
/// Text behaviour is unchanged from before image support: `push_read`
/// queues a scripted `read_text` answer and `written_payloads` returns
/// everything `write_text` received.
///
/// The image side mirrors it exactly — `push_image_read` queues a
/// scripted `read_image` answer and `written_images` returns everything
/// `write_image` received — and is **off by default**: a fake built with
/// [`FakeClipboardBackend::new`] reports no image support at all, so
/// every pre-existing test keeps observing the text-only behaviour. Call
/// [`FakeClipboardBackend::with_image_support`] to opt in, and
/// [`FakeClipboardBackend::set_image_support`] to model a session that
/// allows only one direction.
///
/// The rich-text side mirrors the same pattern: `push_rich_read`
/// queues a scripted `read_rich` answer and `written_rich_text` returns
/// every payload `write_rich` received. The support flags are also
/// off by default so the rich-text behaviour is exercised only by tests
/// that opt in.
#[derive(Debug, Default)]
pub struct FakeClipboardBackend {
    reads: Mutex<Vec<Result<Option<String>, ClipboardBackendError>>>,
    writes: Mutex<Vec<String>>,
    write_error: Mutex<Option<ClipboardBackendError>>,
    image_reads: Mutex<Vec<Result<Option<ClipboardImage>, ClipboardBackendError>>>,
    image_writes: Mutex<Vec<ClipboardImage>>,
    image_write_error: Mutex<Option<ClipboardBackendError>>,
    supports_image_read: Mutex<bool>,
    supports_image_write: Mutex<bool>,
    rich_reads: Mutex<Vec<Result<Option<RichTextPayload>, ClipboardBackendError>>>,
    rich_writes: Mutex<Vec<RichTextPayload>>,
    rich_write_error: Mutex<Option<ClipboardBackendError>>,
    supports_rich_read: Mutex<bool>,
    supports_rich_write: Mutex<bool>,
}

impl FakeClipboardBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder shorthand for a backend that supports both image and
    /// rich-text directions, mirroring a macOS / Linux X11 session.
    pub fn with_image_support() -> Self {
        let backend = Self::default();
        backend.set_image_support(true, true);
        backend
    }

    /// Builder shorthand for a backend that supports both rich-text
    /// directions.
    pub fn with_rich_support() -> Self {
        let backend = Self::default();
        backend.set_rich_support(true, true);
        backend
    }

    /// Queue a single scripted response for the next `read_text` call.
    /// Each call consumes one entry; remaining calls return
    /// `Ok(None)`.
    pub fn push_read(&self, response: Result<Option<String>, ClipboardBackendError>) {
        self.reads.lock().push(response);
    }

    /// Queue a single scripted response for the next `read_image` call.
    /// Each call consumes one entry; remaining calls return `Ok(None)`.
    pub fn push_image_read(&self, response: Result<Option<ClipboardImage>, ClipboardBackendError>) {
        self.image_reads.lock().push(response);
    }

    /// Queue a single scripted response for the next `read_rich` call.
    /// Each call consumes one entry; remaining calls return `Ok(None)`.
    pub fn push_rich_read(&self, response: Result<Option<RichTextPayload>, ClipboardBackendError>) {
        self.rich_reads.lock().push(response);
    }

    /// Model a session that supports only some image directions.
    pub fn set_image_support(&self, read: bool, write: bool) {
        *self.supports_image_read.lock() = read;
        *self.supports_image_write.lock() = write;
    }

    /// Model a session that supports only some rich-text directions.
    pub fn set_rich_support(&self, read: bool, write: bool) {
        *self.supports_rich_read.lock() = read;
        *self.supports_rich_write.lock() = write;
    }

    /// Make the next (and every following) `write_image` fail with
    /// `error`. Used to prove a paste failure never mutates history.
    pub fn fail_image_write(&self, error: ClipboardBackendError) {
        *self.image_write_error.lock() = Some(error);
    }

    /// Make the next (and every following) `write_rich` fail with
    /// `error`.
    pub fn fail_rich_write(&self, error: ClipboardBackendError) {
        *self.rich_write_error.lock() = Some(error);
    }

    /// Make every `write_text` fail with `error`.
    pub fn set_text_write_error(&self, error: ClipboardBackendError) {
        *self.write_error.lock() = Some(error);
    }

    /// Snapshot of every payload received by `write_text`. Useful for
    /// assertions.
    pub fn written_payloads(&self) -> Vec<String> {
        self.writes.lock().clone()
    }

    /// Snapshot of every bitmap received by `write_image`.
    pub fn written_images(&self) -> Vec<ClipboardImage> {
        self.image_writes.lock().clone()
    }

    /// Snapshot of every rich-text payload received by `write_rich`.
    pub fn written_rich_text(&self) -> Vec<RichTextPayload> {
        self.rich_writes.lock().clone()
    }
}

impl ClipboardBackend for FakeClipboardBackend {
    fn read_text(&self) -> Result<Option<String>, ClipboardBackendError> {
        self.reads.lock().pop().unwrap_or(Ok(None))
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardBackendError> {
        if let Some(error) = self.write_error.lock().take() {
            return Err(error);
        }
        self.writes.lock().push(text.to_string());
        Ok(())
    }

    fn read_rich(&self) -> Result<Option<RichTextPayload>, ClipboardBackendError> {
        if !*self.supports_rich_read.lock() {
            return Err(ClipboardBackendError::Unavailable {
                capability: clipvault_platform::Capability::ClipboardReadRichText,
            });
        }
        self.rich_reads.lock().pop().unwrap_or(Ok(None))
    }

    fn write_rich(&self, payload: &RichTextPayload) -> Result<(), ClipboardBackendError> {
        if !*self.supports_rich_write.lock() {
            return Err(ClipboardBackendError::Unavailable {
                capability: clipvault_platform::Capability::ClipboardWriteRichText,
            });
        }
        if let Some(error) = self.rich_write_error.lock().take() {
            return Err(error);
        }
        self.rich_writes.lock().push(payload.clone());
        Ok(())
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardBackendError> {
        if !*self.supports_image_read.lock() {
            return Err(ClipboardBackendError::Unavailable {
                capability: clipvault_platform::Capability::ClipboardReadImage,
            });
        }
        self.image_reads.lock().pop().unwrap_or(Ok(None))
    }

    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardBackendError> {
        if !*self.supports_image_write.lock() {
            return Err(ClipboardBackendError::Unavailable {
                capability: clipvault_platform::Capability::ClipboardWriteImage,
            });
        }
        if let Some(error) = self.image_write_error.lock().take() {
            return Err(error);
        }
        self.image_writes.lock().push(image.clone());
        Ok(())
    }

    fn supports_rich_read(&self) -> bool {
        *self.supports_rich_read.lock()
    }

    fn supports_rich_write(&self) -> bool {
        *self.supports_rich_write.lock()
    }

    fn supports_image_read(&self) -> bool {
        *self.supports_image_read.lock()
    }

    fn supports_image_write(&self) -> bool {
        *self.supports_image_write.lock()
    }

    fn name(&self) -> &'static str {
        "fake"
    }
}

/// Programmable hotkey manager. By default every `register` returns
/// [`HotkeyOutcome::Registered`]; tests can override via
/// [`FakeHotkeyManager::set_outcome`].
#[derive(Debug)]
pub struct FakeHotkeyManager {
    outcome: Mutex<HotkeyOutcome>,
    registered: Mutex<Vec<HotkeyBinding>>,
    unregistered: Mutex<bool>,
}

impl Default for FakeHotkeyManager {
    fn default() -> Self {
        Self {
            outcome: Mutex::new(HotkeyOutcome::Registered),
            registered: Mutex::new(Vec::new()),
            unregistered: Mutex::new(false),
        }
    }
}

impl FakeHotkeyManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the outcome returned by the next `register` call.
    pub fn set_outcome(&self, outcome: HotkeyOutcome) {
        *self.outcome.lock() = outcome;
    }

    /// Bindings that have been registered, newest first.
    pub fn registered_bindings(&self) -> Vec<HotkeyBinding> {
        self.registered.lock().clone()
    }

    /// `true` if `unregister_all` has been called.
    pub fn was_unregistered(&self) -> bool {
        *self.unregistered.lock()
    }
}

impl HotkeyManager for FakeHotkeyManager {
    fn register(
        &self,
        binding: &HotkeyBinding,
        _on_activate: Box<dyn Fn() + Send + Sync + 'static>,
    ) -> Result<HotkeyOutcome, HotkeyError> {
        self.registered.lock().push(binding.clone());
        Ok(self.outcome.lock().clone())
    }

    fn unregister_all(&self) -> Result<(), HotkeyError> {
        *self.unregistered.lock() = true;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "fake"
    }
}

/// Programmable active-application probe. Default returns
/// `Ok(Some("TestApp"))`.
#[derive(Debug)]
pub struct FakeActiveApplication {
    next: Mutex<Result<Option<ActiveApplication>, ActiveAppError>>,
}

impl Default for FakeActiveApplication {
    fn default() -> Self {
        Self {
            next: Mutex::new(Ok(Some(ActiveApplication::new(
                "TestApp",
                "com.example.TestApp",
            )))),
        }
    }
}

impl FakeActiveApplication {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_next(&self, value: Result<Option<ActiveApplication>, ActiveAppError>) {
        *self.next.lock() = value;
    }
}

impl ActiveApplicationProbe for FakeActiveApplication {
    fn active_application(&self) -> Result<Option<ActiveApplication>, ActiveAppError> {
        self.next.lock().clone()
    }

    fn name(&self) -> &'static str {
        "fake"
    }
}

/// Programmable paste controller.
#[derive(Debug)]
pub struct FakePasteController {
    next: Mutex<Result<(), PasteError>>,
    invocations: Mutex<u32>,
}

impl Default for FakePasteController {
    fn default() -> Self {
        Self {
            next: Mutex::new(Ok(())),
            invocations: Mutex::new(0),
        }
    }
}

impl FakePasteController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_next(&self, value: Result<(), PasteError>) {
        *self.next.lock() = value;
    }

    pub fn invocations(&self) -> u32 {
        *self.invocations.lock()
    }
}

impl PasteController for FakePasteController {
    fn paste(&self) -> Result<(), PasteError> {
        *self.invocations.lock() += 1;
        self.next.lock().clone()
    }

    fn name(&self) -> &'static str {
        "fake"
    }
}

/// Programmable tray controller that records every menu update and
/// invocation.
#[derive(Debug, Default, Clone)]
pub struct FakeTrayController {
    state: Arc<FakeTrayState>,
}

#[derive(Debug, Default)]
struct FakeTrayState {
    menus: Mutex<Vec<Vec<TrayEntry>>>,
    invocations: Mutex<Vec<TrayAction>>,
    shutdown_called: Mutex<bool>,
}

impl FakeTrayController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn recorded_menus(&self) -> Vec<Vec<TrayEntry>> {
        self.state.menus.lock().clone()
    }

    pub fn recorded_invocations(&self) -> Vec<TrayAction> {
        self.state.invocations.lock().clone()
    }

    pub fn was_shut_down(&self) -> bool {
        *self.state.shutdown_called.lock()
    }
}

impl TrayController for FakeTrayController {
    fn install(&self) -> Result<Box<dyn TrayHandle>, TrayError> {
        Ok(Box::new(FakeTrayHandle {
            state: Arc::clone(&self.state),
        }))
    }

    fn name(&self) -> &'static str {
        "fake"
    }
}

/// Opaque tray handle exposed by [`FakeTrayController`].
pub struct FakeTrayHandle {
    state: Arc<FakeTrayState>,
}

impl TrayHandle for FakeTrayHandle {
    fn set_menu(&self, entries: &[TrayEntry]) -> Result<(), TrayError> {
        self.state.menus.lock().push(entries.to_vec());
        Ok(())
    }

    fn invoke(&self, action: TrayAction) -> Result<TrayOutcome, TrayError> {
        self.state.invocations.lock().push(action);
        if action.is_supported_in_mvp() {
            Ok(TrayOutcome::Delivered)
        } else {
            Ok(TrayOutcome::Unavailable {
                action,
                reason: "fake action not implemented yet".into(),
            })
        }
    }

    fn shutdown(&self) -> Result<(), TrayError> {
        *self.state.shutdown_called.lock() = true;
        Ok(())
    }
}

/// Programmable settings navigator. Tests push outcomes via
/// [`FakeSettingsNavigator::push_outcome`] and the fake returns them
/// when `open` is invoked for the matching target. Every call is also
/// recorded for inspection via [`Self::calls`]. The default outcome is
/// [`SettingsOpenOutcome::Opened`] so the happy path needs no setup.
#[derive(Debug, Default)]
pub struct FakeSettingsNavigator {
    queued: Mutex<Vec<(PlatformSettingsTarget, SettingsOpenOutcome)>>,
    calls: Mutex<Vec<(PlatformSettingsTarget, SettingsOpenOutcome)>>,
}

impl FakeSettingsNavigator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue the next outcome for `target`. Tests can call this method
    /// multiple times to script success / fallback / failure sequences.
    pub fn push_outcome(&self, target: PlatformSettingsTarget, outcome: SettingsOpenOutcome) {
        self.queued.lock().push((target, outcome));
    }

    /// Every `(target, outcome)` pair that `open` has returned,
    /// including the default fallback.
    pub fn calls(&self) -> Vec<(PlatformSettingsTarget, SettingsOpenOutcome)> {
        self.calls.lock().clone()
    }
}

impl SettingsNavigator for FakeSettingsNavigator {
    fn open(&self, target: PlatformSettingsTarget) -> SettingsOpenOutcome {
        let outcome = {
            let mut guard = self.queued.lock();
            if let Some(index) = guard.iter().position(|(t, _)| *t == target) {
                guard.remove(index).1
            } else {
                SettingsOpenOutcome::Opened
            }
        };
        self.calls.lock().push((target, outcome.clone()));
        outcome
    }

    fn name(&self) -> &'static str {
        "fake"
    }
}

/// Programmable application-metadata provider. Tests push responses
/// via [`FakeApplicationMetadataProvider::push_lookup`] and the fake
/// consumes them in order. The default behaviour returns `Ok(None)`
/// so the happy path needs no setup.
#[derive(Debug, Default)]
pub struct FakeApplicationMetadataProvider {
    queued: Mutex<Vec<Result<Option<ApplicationMetadata>, ApplicationMetadataError>>>,
    calls: Mutex<Vec<String>>,
}

impl FakeApplicationMetadataProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue the next lookup outcome. The provider pops one entry per
    /// call; remaining calls return `Ok(None)`.
    pub fn push_lookup(
        &self,
        response: Result<Option<ApplicationMetadata>, ApplicationMetadataError>,
    ) {
        self.queued.lock().push(response);
    }

    /// Identifiers passed to `lookup`, in invocation order.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().clone()
    }
}

impl ApplicationMetadataProvider for FakeApplicationMetadataProvider {
    fn lookup(
        &self,
        identifier: &str,
    ) -> Result<Option<ApplicationMetadata>, ApplicationMetadataError> {
        self.calls.lock().push(identifier.to_string());
        self.queued.lock().pop().unwrap_or(Ok(None))
    }

    fn name(&self) -> &'static str {
        "fake"
    }
}
