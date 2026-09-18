//! Local peer identity service.
//!
//! `clipvault-core` consumes the platform-defined types
//! ([`PeerIdentityStore`], [`LocalPeerIdentity`], …) and adds the
//! [`PeerIdentityService`] wrapper the bootstrap and the settings
//! layer consume. The wrapper is the only place that merges the
//! cryptographic identity with the validated visible device name
//! stored in `app_settings`; renaming a device never alters the
//! peer_id because the two values are persisted independently.
//!
//! The [`InMemoryPeerIdentityStore`] fake lives here so unit tests
//! can exercise the service without linking the keychain backend.
//! It mirrors the real store's idempotent behaviour (a successful
//! first call followed by any number of additional calls always
//! returns the same `LocalPeerIdentity`) and offers a
//! `fail_next_with_unavailable` switch for the
//! `secure_store_unavailable` scenarios the spec calls out.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use rand_core::RngCore;

pub use clipvault_platform::peer_identity::{
    LocalPeerIdentity, PeerFingerprint, PeerId, PeerIdentityError, PeerIdentityOutcome,
    PeerIdentityStore, PEER_IDENTITY_SERVICE, PEER_IDENTITY_USERNAME,
};

/// Metadata-only projection the shell surfaces to the frontend.
/// The struct carries only the public material: peer_id, the
/// abbreviated fingerprint and the validated display name. The
/// private key, the raw public key bytes and any other secret
/// material never cross the core/shell/frontend boundary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalPeerProfile {
    pub peer_id: String,
    pub fingerprint: String,
    pub display_name: Option<String>,
}

impl LocalPeerProfile {
    /// Convert a [`LocalPeerIdentity`] plus the persisted display
    /// name into the metadata-only DTO.
    pub fn from_parts(identity: &LocalPeerIdentity, display_name: Option<String>) -> Self {
        Self {
            peer_id: identity.peer_id.as_str().to_string(),
            fingerprint: identity.fingerprint.as_str().to_string(),
            display_name,
        }
    }
}

/// In-memory fake store used by the test suite and by platforms
/// that cannot reach a secure store (CI without a desktop session,
/// `Other` `OsFamily`, …). The fake is deterministic in the sense
/// that `load_or_create` always returns the same identity it
/// generated on the first call; the user can prime it with
/// [`InMemoryPeerIdentityStore::with_seed`] when a test needs a
/// stable value across processes.
#[derive(Debug, Default)]
pub struct InMemoryPeerIdentityStore {
    inner: Mutex<Option<LocalPeerIdentity>>,
    fail_flag: AtomicBool,
    /// When `true`, every call returns
    /// [`PeerIdentityError::SecureStoreUnavailable`]. Used as the
    /// production default when no platform keychain is wired so the
    /// shell surfaces a typed `Unavailable` outcome instead of
    /// silently minting an in-memory identity that would not
    /// survive a restart.
    always_unavailable: bool,
}

impl InMemoryPeerIdentityStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a store that always reports
    /// [`PeerIdentityError::SecureStoreUnavailable`]. Production
    /// shells use this as the default `PeerIdentityStore` whenever
    /// the keychain-backed implementation is not wired (the spec's
    /// "secure store unavailable" branch). Tests inject a regular
    /// `InMemoryPeerIdentityStore::new()` or `with_seed` instead.
    pub fn always_unavailable() -> Self {
        Self {
            inner: Mutex::new(None),
            fail_flag: AtomicBool::new(false),
            always_unavailable: true,
        }
    }

    /// Prime the store with a deterministic 32-byte seed so tests
    /// can pin the `peer_id` and fingerprint to known values
    /// without standing up a real keychain.
    pub fn with_seed(seed: [u8; 32]) -> Self {
        let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();
        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(verifying.as_bytes());
        let identity = LocalPeerIdentity {
            peer_id: PeerId::from_public_key(&public_key),
            fingerprint: PeerFingerprint::from_public_key(&public_key),
            public_key,
        };
        Self {
            inner: Mutex::new(Some(identity)),
            fail_flag: AtomicBool::new(false),
            always_unavailable: false,
        }
    }

    /// Make the next call to [`Self::load_or_create`] return
    /// [`PeerIdentityError::SecureStoreUnavailable`]. Used by the
    /// tests that exercise the unavailable path.
    pub fn fail_next_with_unavailable(&self) {
        self.fail_flag.store(true, Ordering::Release);
    }
}

impl PeerIdentityStore for InMemoryPeerIdentityStore {
    fn load_or_create(&self) -> Result<LocalPeerIdentity, PeerIdentityError> {
        if self.always_unavailable {
            return Err(PeerIdentityError::SecureStoreUnavailable);
        }
        if self.fail_flag.swap(false, Ordering::AcqRel) {
            return Err(PeerIdentityError::SecureStoreUnavailable);
        }
        let mut guard = self.inner.lock();
        if let Some(existing) = guard.as_ref() {
            return Ok(existing.clone());
        }
        let mut seed = [0u8; 32];
        fill_random_bytes(&mut seed)?;
        let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();
        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(verifying.as_bytes());
        let identity = LocalPeerIdentity {
            peer_id: PeerId::from_public_key(&public_key),
            fingerprint: PeerFingerprint::from_public_key(&public_key),
            public_key,
        };
        *guard = Some(identity.clone());
        Ok(identity)
    }
}

fn fill_random_bytes(buf: &mut [u8]) -> Result<(), PeerIdentityError> {
    rand_core::OsRng
        .try_fill_bytes(buf)
        .map_err(|_| PeerIdentityError::SecureStoreUnavailable)
}

/// Service the shell calls to obtain the local peer profile and to
/// update the validated display name. The service is cheap to
/// clone and holds the [`PeerIdentityStore`] behind an `Arc` so
/// tests can swap in a fake without rebuilding the bootstrap.
#[derive(Clone)]
pub struct PeerIdentityService {
    store: Arc<dyn PeerIdentityStore>,
}

impl PeerIdentityService {
    pub fn new(store: Arc<dyn PeerIdentityStore>) -> Self {
        Self { store }
    }

    /// Build a service that always reports
    /// [`PeerIdentityOutcome::Unavailable`]. The default
    /// `SettingsService` constructor uses this so callers that
    /// never wired a platform keychain get a typed failure instead
    /// of an in-memory fake identity that would not survive a
    /// restart.
    pub fn unavailable() -> Self {
        Self {
            store: Arc::new(InMemoryPeerIdentityStore::always_unavailable()),
        }
    }

    pub fn store(&self) -> Arc<dyn PeerIdentityStore> {
        Arc::clone(&self.store)
    }

    /// Load or create the identity, returning the metadata-only
    /// [`LocalPeerProfile`] merged with the supplied display
    /// name. The `display_name` argument is the validated value
    /// persisted in `app_settings`; the identity itself never
    /// carries the name so renaming a device cannot ever alter the
    /// peer_id.
    pub fn profile(&self, display_name: Option<String>) -> PeerIdentityOutcome<LocalPeerProfile> {
        match self.store.load_or_create() {
            Ok(identity) => {
                PeerIdentityOutcome::Ok(LocalPeerProfile::from_parts(&identity, display_name))
            }
            Err(_) => PeerIdentityOutcome::Unavailable,
        }
    }

    /// Returned when the secure store is reachable but the request
    /// itself is invalid (currently a defensive guard for future
    /// service extensions). Surfaced as `Unavailable` because the
    /// shell cannot recover by itself; the caller is expected to
    /// reject the bad input before reaching this branch.
    pub fn load_identity(&self) -> PeerIdentityOutcome<LocalPeerIdentity> {
        match self.store.load_or_create() {
            Ok(identity) => PeerIdentityOutcome::Ok(identity),
            Err(_) => PeerIdentityOutcome::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_returns_stable_identity_across_calls() {
        let store = InMemoryPeerIdentityStore::new();
        let first = store
            .load_or_create()
            .expect("first load_or_create must succeed");
        let second = store
            .load_or_create()
            .expect("second load_or_create must succeed");
        assert_eq!(first.peer_id, second.peer_id);
        assert_eq!(first.public_key, second.public_key);
    }

    #[test]
    fn in_memory_store_with_seed_is_deterministic() {
        let seed = [3u8; 32];
        let a = InMemoryPeerIdentityStore::with_seed(seed)
            .load_or_create()
            .unwrap();
        let b = InMemoryPeerIdentityStore::with_seed(seed)
            .load_or_create()
            .unwrap();
        assert_eq!(a.peer_id, b.peer_id);
        assert_eq!(a.public_key, b.public_key);

        let different_seed = [4u8; 32];
        let c = InMemoryPeerIdentityStore::with_seed(different_seed)
            .load_or_create()
            .unwrap();
        assert_ne!(a.peer_id, c.peer_id);
    }

    #[test]
    fn in_memory_store_reports_unavailable_when_primed() {
        let store = InMemoryPeerIdentityStore::new();
        store.fail_next_with_unavailable();
        let err = store.load_or_create().expect_err("must report unavailable");
        assert!(matches!(err, PeerIdentityError::SecureStoreUnavailable));
        // The flag is consumed by the failed call; the next call
        // must succeed without forcing the test to re-prime the
        // flag.
        let _ = store
            .load_or_create()
            .expect("subsequent call must succeed");
    }

    #[test]
    fn service_returns_unavailable_when_store_errors() {
        let store = Arc::new(InMemoryPeerIdentityStore::new());
        store.fail_next_with_unavailable();
        let service = PeerIdentityService::new(store);
        let outcome = service.profile(Some("Studio".to_string()));
        assert!(matches!(outcome, PeerIdentityOutcome::Unavailable));
    }

    #[test]
    fn profile_merges_identity_and_display_name() {
        let store = Arc::new(InMemoryPeerIdentityStore::new());
        let service = PeerIdentityService::new(store);
        let profile = match service.profile(Some("Studio".into())) {
            PeerIdentityOutcome::Ok(value) => value,
            other => panic!("expected Ok, got {other:?}"),
        };
        assert_eq!(profile.display_name.as_deref(), Some("Studio"));
        assert!(!profile.peer_id.is_empty());
        assert!(!profile.fingerprint.is_empty());
        // The metadata-only DTO MUST NOT carry the public key bytes
        // (hex-encoded or otherwise); a future regression that adds
        // the field would let the frontend surface enough material
        // to attempt impersonation once pairing lands.
        let forbidden = [
            "public_key_hex",
            "public_key",
            "private_key",
            "secret",
            "seed",
        ];
        let json = serde_json::to_string(&profile).expect("serialize");
        for field in forbidden {
            assert!(
                !json.contains(field),
                "profile payload must not contain {field}; got {json}",
            );
        }
    }

    #[test]
    fn profile_carries_no_display_name_when_setting_is_absent() {
        let store = Arc::new(InMemoryPeerIdentityStore::new());
        let service = PeerIdentityService::new(store);
        let profile = match service.profile(None) {
            PeerIdentityOutcome::Ok(value) => value,
            other => panic!("expected Ok, got {other:?}"),
        };
        assert!(profile.display_name.is_none());
        assert!(!profile.peer_id.is_empty());
    }

    #[test]
    fn always_unavailable_store_never_returns_identity() {
        // The store used as the production default MUST surface the
        // typed failure on every call. The bootstrap wires this
        // store whenever the platform keychain is not available so
        // the shell never advertises an identity it cannot reload.
        let store = InMemoryPeerIdentityStore::always_unavailable();
        let first = store.load_or_create();
        let second = store.load_or_create();
        assert!(matches!(
            first,
            Err(PeerIdentityError::SecureStoreUnavailable)
        ));
        assert!(matches!(
            second,
            Err(PeerIdentityError::SecureStoreUnavailable)
        ));
    }

    #[test]
    fn unavailable_service_constructor_never_returns_ok() {
        // `PeerIdentityService::unavailable` is the default the
        // `SettingsService` 2-arg constructor installs. Pin the
        // contract so a future refactor that swaps it for an
        // in-memory fake is caught before it ships.
        let service = PeerIdentityService::unavailable();
        let outcome = service.profile(Some("Studio".into()));
        assert!(matches!(outcome, PeerIdentityOutcome::Unavailable));
    }

    #[test]
    fn simulated_restart_with_persistent_store_returns_same_identity() {
        // The `keyring` backend persists the 32-byte seed across
        // process restarts. Simulate that here with a tiny
        // `PersistentSeedStore` that stores the seed in shared
        // memory and replay it across two `PeerIdentityStore`
        // instances — the resulting `peer_id` and `fingerprint`
        // MUST match byte-for-byte. This is the regression that
        // would catch a future refactor that rotated the identity
        // on every call instead of reading it from the store.
        let persistent: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
        let first_store = Arc::new(PersistentSeedStore::new(Arc::clone(&persistent)));
        let first_identity = first_store.load_or_create().expect("first seed");

        // Drop the first handle and re-open against the same
        // backing storage — the equivalent of restarting the
        // process.
        drop(first_store);
        let second_store = PersistentSeedStore::new(Arc::clone(&persistent));
        let second_identity = second_store.load_or_create().expect("second seed");

        assert_eq!(first_identity.peer_id, second_identity.peer_id);
        assert_eq!(first_identity.fingerprint, second_identity.fingerprint);
        assert_eq!(first_identity.public_key, second_identity.public_key);
    }

    /// Test-only [`PeerIdentityStore`] that persists the 32-byte
    /// seed in a caller-supplied shared buffer. Mirrors the
    /// real-world lifecycle of the keychain entry: the first call
    /// mints and writes the seed, every later call (including
    /// after a simulated restart that recreates the wrapper)
    /// reads it back unchanged.
    struct PersistentSeedStore {
        buffer: Arc<Mutex<Option<Vec<u8>>>>,
    }

    impl PersistentSeedStore {
        fn new(buffer: Arc<Mutex<Option<Vec<u8>>>>) -> Self {
            Self { buffer }
        }
    }

    impl PeerIdentityStore for PersistentSeedStore {
        fn load_or_create(&self) -> Result<LocalPeerIdentity, PeerIdentityError> {
            let mut guard = self.buffer.lock();
            let bytes = match guard.as_ref() {
                Some(existing) => existing.clone(),
                None => {
                    let mut seed = [0u8; 32];
                    fill_random_bytes(&mut seed)?;
                    *guard = Some(seed.to_vec());
                    seed.to_vec()
                }
            };
            if bytes.len() != 32 {
                return Err(PeerIdentityError::SecureStoreFailed);
            }
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes);
            let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
            let verifying = signing.verifying_key();
            let mut public_key = [0u8; 32];
            public_key.copy_from_slice(verifying.as_bytes());
            Ok(LocalPeerIdentity {
                peer_id: PeerId::from_public_key(&public_key),
                fingerprint: PeerFingerprint::from_public_key(&public_key),
                public_key,
            })
        }
    }

    #[test]
    fn renaming_does_not_rotate_peer_id_or_fingerprint() {
        // The display name lives in `app_settings` and is
        // orthogonal to the cryptographic identity. Renaming the
        // device MUST NOT alter `peer_id` or `fingerprint` — that
        // is the contract the spec scenario "User changes visible
        // name" pins.
        let store: Arc<dyn PeerIdentityStore> = Arc::new(InMemoryPeerIdentityStore::new());
        let service = PeerIdentityService::new(Arc::clone(&store));
        let first = match service.profile(Some("Studio".into())) {
            PeerIdentityOutcome::Ok(value) => value,
            other => panic!("expected Ok, got {other:?}"),
        };
        // Re-load the identity directly to confirm it survives any
        // state mutation the service performs internally.
        let identity_after_name_change = match service.profile(Some("Renamed".into())) {
            PeerIdentityOutcome::Ok(value) => value,
            other => panic!("expected Ok, got {other:?}"),
        };
        assert_eq!(first.peer_id, identity_after_name_change.peer_id);
        assert_eq!(first.fingerprint, identity_after_name_change.fingerprint);
        // The raw store still reports the same public key.
        let raw = store.load_or_create().expect("store must succeed");
        assert_eq!(raw.peer_id.as_str(), first.peer_id);
    }
}
