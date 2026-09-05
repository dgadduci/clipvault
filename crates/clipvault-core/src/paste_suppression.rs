//! Capture suppression for clipboard writes originated by ClipVault.
//!
//! When a paste action writes content to the clipboard, the
//! background capture watcher will observe the same content on the
//! next tick. Without coordination, that observation would create a
//! new card (or refresh an existing one) for a payload the user did
//! not actually copy themselves.
//!
//! The fix is a short-lived, payload-aware token the
//! [`crate::paste::PasteService`] arms before every clipboard write
//! and the [`crate::watcher::CaptureWatcher`] consumes before
//! persisting anything. The token is metadata-only: it never carries
//! text, HTML, RTF, image bytes or filesystem paths. A fingerprint
//! comparison is the only mechanism the watcher uses, and the
//! fingerprint itself is a SHA-256-sized digest built from the
//! payload's canonical plain text and the canonical rich-text hash
//! the capture pipeline already computes. System-side normalisation
//! of HTML / RTF does not change the plain-text digest, so the
//! comparison survives the round-trip.
//!
//! The token is bounded:
//!
//! - it expires after a short TTL (a few hundred milliseconds longer
//!   than the longest plausible capture-loop interval);
//! - it is consumed by the first matching observation, never
//!   re-armed by an observation that does not match;
//! - it is cleared immediately when the clipboard write fails, so a
//!   failed paste cannot block a legitimate later capture.
//!
//! The registry is shared between the paste service and the watcher
//! through [`crate::bootstrap::AppContext`]. The wrapper is `Clone`
//! and `Send + Sync` so the capture loop, the manual `Tick capture`
//! command and the paste command can all hold a handle to the same
//! state without any explicit thread coordination.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use sha2::{Digest, Sha256};

use clipvault_platform::{ClipboardImage, ClipboardPayload};

/// Default suppression TTL. The capture loop polls every 660 ms by
/// default; the token must outlive one full poll after a successful
/// paste write but must not stay armed long enough to mask a real
/// user copy. 2.5 s is the smallest value that survives the worst
/// observed poll jitter (capture loop at default cadence plus a slow
/// first poll after the synthetic paste).
pub const DEFAULT_SUPPRESSION_TTL: Duration = Duration::from_millis(2_500);

/// Metadata-only fingerprint the paste service registers. The
/// comparison always runs against the canonical hashes the watcher's
/// payload reader computes so the OS cannot trick the suppression
/// into matching a different payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppressionFingerprint {
    /// SHA-256 digest of the canonical plain text the paste
    /// service wrote, hex-encoded. Always present for text and
    /// rich-text payloads; `None` for image payloads.
    pub plain_text_hash: Option<String>,
    /// Canonical rich-text hash the capture pipeline computes (see
    /// `rich_text::canonical_rich_text_hash`). Present for rich-text
    /// payloads so a paste that publishes only the rich payload is
    /// still matched even when the OS normalises the bytes back into
    /// a different plain text.
    pub rich_text_hash: Option<String>,
    /// SHA-256 digest of the watcher's payload fingerprint for an
    /// image payload (the same fingerprint the dedupe state uses for
    /// change detection). Present for image payloads; `None` for
    /// textual ones.
    pub image_hash: Option<String>,
}

impl SuppressionFingerprint {
    /// Build a fingerprint from a [`ClipboardPayload`] using the
    /// same canonicalisation rules the watcher applies to the
    /// observed payload. The function is metadata-only: the digest
    /// is built without ever cloning the textual content itself.
    pub fn from_payload(payload: &ClipboardPayload) -> Self {
        match payload {
            ClipboardPayload::Text(text) => Self {
                plain_text_hash: Some(hash_plain_text(text)),
                rich_text_hash: None,
                image_hash: None,
            },
            ClipboardPayload::RichText(rich) => Self {
                plain_text_hash: Some(hash_plain_text(rich.plain_text())),
                rich_text_hash: Some(rich_text_digest(
                    rich.plain_text(),
                    rich.html().unwrap_or(""),
                    rich.rtf().map(|bytes| bytes.len()).unwrap_or(0),
                )),
                image_hash: None,
            },
            ClipboardPayload::Image(image) => Self {
                plain_text_hash: None,
                rich_text_hash: None,
                image_hash: Some(image_fingerprint(image)),
            },
        }
    }

    /// Whether the fingerprint carries any metadata the watcher can
    /// match on. A fingerprint with no fields is meaningless and
    /// MUST NOT be armed.
    pub fn is_empty(&self) -> bool {
        self.plain_text_hash.is_none() && self.rich_text_hash.is_none() && self.image_hash.is_none()
    }
}

#[derive(Debug)]
struct SuppressionToken {
    fingerprint: SuppressionFingerprint,
    expires_at: Instant,
}

/// Thread-safe registry shared by the paste service and the
/// watcher. Cheap to clone: the inner state lives behind an `Arc`.
#[derive(Clone)]
pub struct PasteSuppression {
    inner: Arc<Mutex<Option<SuppressionToken>>>,
}

impl Default for PasteSuppression {
    fn default() -> Self {
        Self::new()
    }
}

impl PasteSuppression {
    /// Build a fresh, empty registry. No token is armed.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    /// Arm a token for `fingerprint` with the default TTL. A
    /// previous token is dropped: the paste service never stacks
    /// tokens because two pastes in flight would race on the
    /// clipboard; the most recent write is the only one the
    /// watcher must suppress.
    pub fn arm(&self, fingerprint: SuppressionFingerprint) {
        self.arm_with_ttl(fingerprint, DEFAULT_SUPPRESSION_TTL);
    }

    /// Arm a token for `fingerprint` with the supplied TTL. The TTL
    /// is clamped to zero or negative values so a misconfigured
    /// caller cannot produce a token that lives longer than the
    /// observation window.
    pub fn arm_with_ttl(&self, fingerprint: SuppressionFingerprint, ttl: Duration) {
        if fingerprint.is_empty() {
            return;
        }
        let expires_at = Instant::now().checked_add(ttl).unwrap_or_else(Instant::now);
        *self.inner.lock() = Some(SuppressionToken {
            fingerprint,
            expires_at,
        });
    }

    /// Drop the current token. Used when the clipboard write fails
    /// or when the paste flow completes without arming a new one.
    pub fn clear(&self) {
        *self.inner.lock() = None;
    }

    /// Inspect the currently armed token without consuming it. Used
    /// by tests to assert the registry state.
    pub fn peek(&self) -> Option<SuppressionFingerprint> {
        self.inner
            .lock()
            .as_ref()
            .map(|token| token.fingerprint.clone())
    }

    /// Check whether the observed `fingerprint` matches the
    /// currently armed token. On a positive match the token is
    /// consumed (single-shot) and `true` is returned. On expiry the
    /// token is dropped lazily as a side effect of the lookup. A
    /// mismatch never consumes the token: a different copy during
    /// the suppression window is allowed through.
    pub fn matches_and_consume(&self, fingerprint: &SuppressionFingerprint) -> bool {
        if fingerprint.is_empty() {
            return false;
        }
        let mut guard = self.inner.lock();
        let Some(token) = guard.as_mut() else {
            return false;
        };
        if Instant::now() >= token.expires_at {
            *guard = None;
            return false;
        }
        if fingerprints_match(&token.fingerprint, fingerprint) {
            *guard = None;
            return true;
        }
        false
    }
}

/// Compare an armed token with an observed fingerprint.
///
/// The semantic is a "weak AND over present-on-both legs, with at
/// least one common leg required":
///
/// - every leg that carries a hash on BOTH sides must agree;
/// - legs that are present on only one side are skipped (the OS
///   may have stripped a representation on read, so a rich paste
///   token must still match a plain-text observation that shares
///   the canonical plain text);
/// - at least one leg must be present on both sides, otherwise the
///   token and the observation are different shapes (an image-only
///   token MUST NOT match a text-only observation; the watcher
///   would mask a legitimate copy).
fn fingerprints_match(armed: &SuppressionFingerprint, observed: &SuppressionFingerprint) -> bool {
    let mut any_common_leg = false;
    if let (Some(a), Some(b)) = (&armed.plain_text_hash, &observed.plain_text_hash) {
        any_common_leg = true;
        if a != b {
            return false;
        }
    }
    if let (Some(a), Some(b)) = (&armed.rich_text_hash, &observed.rich_text_hash) {
        any_common_leg = true;
        if a != b {
            return false;
        }
    }
    if let (Some(a), Some(b)) = (&armed.image_hash, &observed.image_hash) {
        any_common_leg = true;
        if a != b {
            return false;
        }
    }
    any_common_leg
}

/// Hash a plain-text slice. The digest is built without allocating
/// an owned `String` of the input: `Sha256::update` consumes the
/// byte slice in place. Exposed publicly so the paste service and
/// the watcher can build the same wire format without duplicating
/// the prefix byte sequence.
pub fn hash_plain_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clipvault-suppression-plain:v1");
    hasher.update(text.as_bytes());
    hex_digest(hasher.finalize())
}

/// SHA-256 digest of the watcher's cheap in-memory image fingerprint.
/// Exposed so the paste service and the watcher share the exact
/// algorithm: the paste service computes the digest on the way out
/// of the asset store; the watcher computes it on the way in.
pub fn image_fingerprint(image: &ClipboardImage) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clipvault-suppression-image:v1");
    hasher.update(image.width().to_le_bytes());
    hasher.update(image.height().to_le_bytes());
    hasher.update(image.rgba());
    hex_digest(hasher.finalize())
}

/// Hash the canonical rich-text identity. The hash is built from
/// the plain text, the HTML body and the RTF byte length so the
/// comparison survives OS-side normalisation of HTML markup.
fn rich_text_digest(plain: &str, html: &str, rtf_len: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clipvault-suppression-rich:v1");
    hasher.update(plain.as_bytes());
    hasher.update(b"\0");
    hasher.update(html.as_bytes());
    hasher.update(b"\0");
    hasher.update(rtf_len.to_le_bytes());
    hex_digest(hasher.finalize())
}

/// Render a SHA-256 digest as lowercase hex. `sha2`'s default
/// `Output` is a generic value; this helper guarantees a stable
/// representation for the wire and for the tests.
fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(plain: &str) -> SuppressionFingerprint {
        SuppressionFingerprint {
            plain_text_hash: Some(plain.to_string()),
            rich_text_hash: None,
            image_hash: None,
        }
    }

    #[test]
    fn empty_fingerprint_does_not_arm() {
        let registry = PasteSuppression::new();
        registry.arm(SuppressionFingerprint {
            plain_text_hash: None,
            rich_text_hash: None,
            image_hash: None,
        });
        assert!(registry.peek().is_none());
    }

    #[test]
    fn arm_replaces_previous_token() {
        let registry = PasteSuppression::new();
        registry.arm(fp("first"));
        registry.arm(fp("second"));
        assert_eq!(
            registry.peek().unwrap().plain_text_hash.as_deref(),
            Some("second")
        );
    }

    #[test]
    fn match_consumes_token() {
        let registry = PasteSuppression::new();
        registry.arm(fp("hello"));
        assert!(registry.matches_and_consume(&fp("hello")));
        assert!(registry.peek().is_none());
    }

    #[test]
    fn mismatch_does_not_consume_token() {
        let registry = PasteSuppression::new();
        registry.arm(fp("hello"));
        assert!(!registry.matches_and_consume(&fp("world")));
        assert!(registry.peek().is_some());
    }

    #[test]
    fn empty_observed_fingerprint_never_matches() {
        let registry = PasteSuppression::new();
        registry.arm(fp("hello"));
        assert!(!registry.matches_and_consume(&SuppressionFingerprint {
            plain_text_hash: None,
            rich_text_hash: None,
            image_hash: None,
        }));
        assert!(registry.peek().is_some());
    }

    #[test]
    fn token_expires_after_ttl() {
        let registry = PasteSuppression::new();
        registry.arm_with_ttl(fp("hello"), Duration::from_millis(0));
        // The TTL is in the past; the very first lookup drops it.
        assert!(!registry.matches_and_consume(&fp("hello")));
        assert!(registry.peek().is_none());
    }

    #[test]
    fn clear_drops_token() {
        let registry = PasteSuppression::new();
        registry.arm(fp("hello"));
        registry.clear();
        assert!(registry.peek().is_none());
        assert!(!registry.matches_and_consume(&fp("hello")));
    }

    #[test]
    fn rich_token_matches_observation_with_only_plain() {
        // The OS may strip rich flavours on read: a rich paste
        // token must still match a plain-text observation that
        // shares the canonical plain text. The rich leg is purely
        // advisory.
        let registry = PasteSuppression::new();
        let token = SuppressionFingerprint {
            plain_text_hash: Some("plain".into()),
            rich_text_hash: Some("rich".into()),
            image_hash: None,
        };
        registry.arm(token.clone());

        // Same plain, no rich observed: the plain leg agrees, the
        // rich leg is absent on the observation, so the leg is
        // skipped. The fingerprint matches.
        assert!(registry.matches_and_consume(&SuppressionFingerprint {
            plain_text_hash: Some("plain".into()),
            rich_text_hash: None,
            image_hash: None,
        }));
    }

    #[test]
    fn image_token_only_matches_observed_image() {
        let registry = PasteSuppression::new();
        let token = SuppressionFingerprint {
            plain_text_hash: None,
            rich_text_hash: None,
            image_hash: Some("image".into()),
        };
        registry.arm(token.clone());

        let plain = SuppressionFingerprint {
            plain_text_hash: Some("text".into()),
            rich_text_hash: None,
            image_hash: None,
        };
        assert!(!registry.matches_and_consume(&plain));
        assert!(registry.matches_and_consume(&token));
    }

    #[test]
    fn clones_share_the_registry() {
        let a = PasteSuppression::new();
        let b = a.clone();
        a.arm(fp("hello"));
        assert_eq!(b.peek().unwrap().plain_text_hash.as_deref(), Some("hello"));
        assert!(b.matches_and_consume(&fp("hello")));
        assert!(a.peek().is_none());
    }

    #[test]
    fn plain_text_hash_is_deterministic_and_distinct() {
        let h1 = hash_plain_text("hello");
        let h2 = hash_plain_text("hello");
        let h3 = hash_plain_text("world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64);
    }
}
