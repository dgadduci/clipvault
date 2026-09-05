//! Privacy gate that mediates between the platform source-app probe and
//! the capture pipeline.
//!
//! The gate takes a snapshot of the source identifier and asks the
//! [`CoreBlacklistMatcher`] whether the identifier is blacklisted. The
//! outcome is a typed [`CaptureDecision`] the capture pipeline consumes
//! without inspecting SQLite or the platform adapter.
//!
//! The gate is a pure consumer of inputs: it does not perform I/O, it
//! does not log clipboard content and it does not depend on Tauri or
//! the GUI. The matcher can be updated atomically (`update_ignored`)
//! so a settings change propagates without rebuilding the gate.

use std::sync::Arc;

use clipvault_platform::ActiveApplicationProbe;
use parking_lot::RwLock;
use tracing::trace;

/// Decision returned by [`PrivacyGate::evaluate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureDecision {
    /// The event should be persisted.
    Allow,
    /// The event should not be persisted. `reason` is a stable label
    /// the capture pipeline can log without leaking identifier data.
    Discard { reason: &'static str },
}

impl CaptureDecision {
    pub fn kind(&self) -> &'static str {
        match self {
            CaptureDecision::Allow => "allow",
            CaptureDecision::Discard { .. } => "discard",
        }
    }
}

/// Matcher that wraps a platform probe and consults the blacklist of
/// ignored applications. The blacklist snapshot lives behind an
/// [`RwLock`] so the gate can be updated by the settings service
/// without rebuilding every consumer.
#[derive(Clone)]
pub struct CoreBlacklistMatcher {
    probe: Arc<dyn ActiveApplicationProbe>,
    ignored: Arc<RwLock<Vec<String>>>,
}

impl CoreBlacklistMatcher {
    pub fn new(probe: Arc<dyn ActiveApplicationProbe>) -> Self {
        Self {
            probe,
            ignored: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Build a matcher from a snapshot of blacklisted identifiers.
    /// Identifiers MUST be normalised (trim + lowercase) by the caller.
    pub fn with_ignored(probe: Arc<dyn ActiveApplicationProbe>, ignored: Vec<String>) -> Self {
        Self {
            probe,
            ignored: Arc::new(RwLock::new(ignored)),
        }
    }

    /// Replace the blacklist snapshot atomically. Existing readers
    /// continue with the previous snapshot until they release the lock;
    /// new readers see the replaced list.
    pub fn replace_ignored(&self, ignored: Vec<String>) {
        *self.ignored.write() = ignored;
    }

    /// Borrow the inner probe. Used by tests and by callers that need
    /// to query the active application directly.
    pub fn probe(&self) -> &Arc<dyn ActiveApplicationProbe> {
        &self.probe
    }

    /// Borrow the current blacklist snapshot.
    pub fn ignored(&self) -> Vec<String> {
        self.ignored.read().clone()
    }

    /// True when `identifier` matches one of the blacklisted entries
    /// after normalization. The function is exposed so tests and
    /// alternative adapters (for example the watcher) can reuse the
    /// matching logic without going through the platform probe.
    pub fn matches(&self, identifier: Option<&str>) -> bool {
        let Some(id) = identifier else {
            return false;
        };
        let normalized = normalize(id);
        if normalized.is_empty() {
            return false;
        }
        let snapshot = self.ignored.read();
        snapshot.iter().any(|entry| entry == &normalized)
    }
}

/// Normalise an application identifier so the matcher can compare two
/// snapshots without false negatives. The default behaviour is:
/// - trim surrounding whitespace,
/// - lowercase ASCII letters,
/// - drop identifiers that end up empty.
pub fn normalize(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.to_ascii_lowercase()
}

/// Gate the capture pipeline consults before persisting an event. The
/// gate is constructed once at boot and then shared through
/// [`AppContext`](crate::bootstrap::AppContext); clone is cheap.
#[derive(Clone)]
pub struct PrivacyGate {
    matcher: CoreBlacklistMatcher,
}

impl PrivacyGate {
    pub fn new(matcher: CoreBlacklistMatcher) -> Self {
        Self { matcher }
    }

    /// Build a gate from the probe and a snapshot of blacklisted
    /// identifiers. The bootstrap uses this constructor.
    pub fn from_probe(probe: Arc<dyn ActiveApplicationProbe>, ignored: Vec<String>) -> Self {
        Self::new(CoreBlacklistMatcher::with_ignored(probe, ignored))
    }

    /// Replace the blacklist snapshot. The matcher is shared so this
    /// mutates state visible to every consumer of the gate.
    pub fn update_ignored(&self, ignored: Vec<String>) {
        self.matcher.replace_ignored(ignored);
    }

    /// Evaluate a candidate capture event. The caller MAY pass a
    /// pre-resolved `source_identifier`; when `None` the gate will ask
    /// the platform probe.
    pub fn evaluate(&self, source_identifier: Option<&str>) -> CaptureDecision {
        let identifier = source_identifier
            .map(|s| s.to_string())
            .or_else(|| self.resolve_active_identifier());
        if self.matcher.matches(identifier.as_deref()) {
            return CaptureDecision::Discard {
                reason: "blacklisted",
            };
        }
        CaptureDecision::Allow
    }

    fn resolve_active_identifier(&self) -> Option<String> {
        match self.matcher.probe.active_application() {
            Ok(Some(app)) if !app.identifier.is_empty() => Some(app.identifier),
            Ok(_) => None,
            Err(error) => {
                trace!(
                    error = %error,
                    "active application probe failed; treating source as unknown"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipvault_platform::ActiveApplication;

    #[derive(Debug)]
    struct StaticProbe(Option<&'static str>);

    impl ActiveApplicationProbe for StaticProbe {
        fn active_application(
            &self,
        ) -> Result<Option<ActiveApplication>, clipvault_platform::ActiveAppError> {
            Ok(self.0.map(|id| ActiveApplication::new("TestApp", id)))
        }
        fn name(&self) -> &'static str {
            "static"
        }
    }

    #[test]
    fn normalize_trims_and_lowercases() {
        assert_eq!(normalize("  Com.Apple.Terminal  "), "com.apple.terminal");
    }

    #[test]
    fn normalize_returns_empty_for_blank_input() {
        assert_eq!(normalize("   "), "");
    }

    #[test]
    fn matcher_returns_true_when_identifier_is_blacklisted() {
        let probe = Arc::new(StaticProbe(Some("com.apple.Terminal")));
        let matcher =
            CoreBlacklistMatcher::with_ignored(probe, vec!["com.apple.terminal".to_string()]);
        assert!(matcher.matches(Some("com.apple.Terminal")));
    }

    #[test]
    fn matcher_is_case_insensitive_via_normalize() {
        let probe = Arc::new(StaticProbe(Some("Firefox")));
        let matcher = CoreBlacklistMatcher::with_ignored(probe, vec!["firefox".into()]);
        assert!(matcher.matches(Some("Firefox")));
    }

    #[test]
    fn matcher_returns_false_for_unknown_identifier() {
        let probe = Arc::new(StaticProbe(Some("Safari")));
        let matcher = CoreBlacklistMatcher::with_ignored(probe, vec!["firefox".into()]);
        assert!(!matcher.matches(Some("Safari")));
    }

    #[test]
    fn matcher_returns_false_when_probe_returns_none() {
        let probe = Arc::new(StaticProbe(None));
        let matcher = CoreBlacklistMatcher::with_ignored(probe, vec!["firefox".into()]);
        assert!(!matcher.matches(None));
    }

    #[test]
    fn replace_ignored_updates_snapshot() {
        let probe = Arc::new(StaticProbe(Some("firefox")));
        let matcher = CoreBlacklistMatcher::with_ignored(probe, vec!["keep".into()]);
        assert!(!matcher.matches(Some("firefox")));
        matcher.replace_ignored(vec!["firefox".into()]);
        assert!(matcher.matches(Some("firefox")));
    }

    #[test]
    fn gate_discards_blacklisted_source() {
        let probe = Arc::new(StaticProbe(Some("com.apple.Terminal")));
        let gate = PrivacyGate::from_probe(probe, vec!["com.apple.terminal".to_string()]);
        let decision = gate.evaluate(None);
        assert_eq!(
            decision,
            CaptureDecision::Discard {
                reason: "blacklisted"
            }
        );
    }

    #[test]
    fn gate_allows_unknown_source() {
        let probe = Arc::new(StaticProbe(Some("Safari")));
        let gate = PrivacyGate::from_probe(probe, vec!["firefox".into()]);
        let decision = gate.evaluate(None);
        assert_eq!(decision, CaptureDecision::Allow);
    }

    #[test]
    fn gate_accepts_explicit_identifier_argument() {
        let probe = Arc::new(StaticProbe(None));
        let gate = PrivacyGate::from_probe(probe, vec!["drop".into()]);
        assert_eq!(gate.evaluate(Some("keep")), CaptureDecision::Allow);
        assert_eq!(
            gate.evaluate(Some("drop")),
            CaptureDecision::Discard {
                reason: "blacklisted"
            }
        );
    }

    #[test]
    fn gate_propagates_update() {
        let probe = Arc::new(StaticProbe(Some("firefox")));
        let gate = PrivacyGate::from_probe(probe, vec![]);
        assert_eq!(gate.evaluate(None), CaptureDecision::Allow);
        gate.update_ignored(vec!["firefox".to_string()]);
        assert_eq!(
            gate.evaluate(None),
            CaptureDecision::Discard {
                reason: "blacklisted"
            }
        );
    }

    #[test]
    fn gate_uses_cached_probe_when_source_is_none() {
        // Bug 3 regression: when the watcher runs on a background
        // thread it cannot call `NSWorkspace` directly on macOS, so
        // it must rely on the cached probe the main thread keeps
        // fresh. The gate has to honour the cached identifier even
        // when `evaluate(None)` is invoked, otherwise the blacklist
        // never matches during background capture.
        use clipvault_platform::{ActiveApplication, CachedActiveApplication};

        struct OneShotProbe(Option<&'static str>);
        impl ActiveApplicationProbe for OneShotProbe {
            fn active_application(
                &self,
            ) -> Result<Option<ActiveApplication>, clipvault_platform::ActiveAppError> {
                Ok(self.0.map(|id| ActiveApplication::new("App", id)))
            }
            fn name(&self) -> &'static str {
                "one_shot"
            }
        }

        let cached =
            CachedActiveApplication::new(Arc::new(OneShotProbe(Some("com.apple.Terminal"))));
        // The first call drains the inner probe and warms the cache.
        cached.refresh().expect("refresh");
        // Subsequent background-thread calls must serve the cached
        // value without touching the inner probe.
        let gate = PrivacyGate::from_probe(
            Arc::new(cached.clone()) as Arc<dyn ActiveApplicationProbe>,
            vec!["com.apple.terminal".to_string()],
        );
        assert_eq!(
            gate.evaluate(None),
            CaptureDecision::Discard {
                reason: "blacklisted"
            }
        );
    }

    #[test]
    fn gate_rejects_explicit_blacklisted_source_via_cached_probe() {
        // Bug 3 regression: capture pipeline accepts an explicit
        // source identifier (typically resolved by the cached
        // probe). The matcher must consult the blacklist before any
        // persistence happens, even when the identifier arrives via
        // the cache.
        use clipvault_platform::ActiveApplication;

        struct FixedProbe(&'static str);
        impl ActiveApplicationProbe for FixedProbe {
            fn active_application(
                &self,
            ) -> Result<Option<ActiveApplication>, clipvault_platform::ActiveAppError> {
                Ok(Some(ActiveApplication::new("App", self.0)))
            }
            fn name(&self) -> &'static str {
                "fixed"
            }
        }

        let probe: Arc<dyn ActiveApplicationProbe> = Arc::new(FixedProbe("1password"));
        let gate = PrivacyGate::from_probe(probe, vec!["1password".to_string()]);
        assert_eq!(
            gate.evaluate(None),
            CaptureDecision::Discard {
                reason: "blacklisted"
            }
        );
        assert_eq!(
            gate.evaluate(Some("1password")),
            CaptureDecision::Discard {
                reason: "blacklisted"
            }
        );
    }
}
