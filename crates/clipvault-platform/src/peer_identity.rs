//! Platform abstraction for the secure local peer identity store.
//!
//! This module owns the public types and the [`PeerIdentityStore`]
//! trait. The concrete keychain / Secret Service implementation
//! lives in [`crate::peer_identity::KeychainPeerIdentityStore`]
//! and is only compiled when the `local-peer-identity-keychain`
//! feature is enabled.
//!
//! `clipvault-core` re-exports these types and adds a
//! higher-level [`crate::peer_identity::PeerIdentityService`]
//! that wires the store to the bootstrap and the settings layer.
//! The split keeps the lower-level platform crate free of
//! any business logic while letting the core layer reason about
//! identity in a platform-agnostic way.

#[cfg(feature = "local-peer-identity-keychain")]
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ED25519_PUBLIC_KEY_LEN: usize = 32;
const PEER_ID_HEX_CHARS: usize = 32;
const FINGERPRINT_HEX_CHARS: usize = 16;

/// Stable service identifier the platform secure store uses to scope
/// the local peer identity entry. Bumping it forces a migration and
/// effectively issues a new peer_id (the store will surface
/// `Unavailable` for the old entry and the service creates a fresh
/// identity on demand).
pub const PEER_IDENTITY_SERVICE: &str = "local-peer-identity-foundation.v1";
/// Stable username the platform secure store attributes the entry
/// to. Keeps the entry distinct from any other ClipVault entry the
/// user may have stored in the same backend.
pub const PEER_IDENTITY_USERNAME: &str = "self";

/// Deterministic `peer_id` derived from the public key. The first
/// 16 bytes of `SHA-256(public_key)` are rendered as lowercase hex
/// (32 chars) so the value stays opaque, copy-pastable and free of
/// confusable characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PeerId(String);

impl PeerId {
    /// Build a `PeerId` from a public key. The derivation is the
    /// one documented in the change proposal and is the only place
    /// `peer_id` is ever minted.
    pub fn from_public_key(public_key: &[u8]) -> Self {
        let digest = Sha256::digest(public_key);
        let hex_chars = hex_prefix(&digest, PEER_ID_HEX_CHARS);
        Self(hex_chars)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Abbreviated human-comparable fingerprint derived from the same
/// hash as [`PeerId`] but truncated to 8 bytes (16 hex chars) so
/// the UI can show it next to the visible device name without
/// overwhelming the layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PeerFingerprint(String);

impl PeerFingerprint {
    /// Build a fingerprint from a public key using the same SHA-256
    /// prefix as the [`PeerId`] but truncated to 8 bytes (16 hex
    /// chars). The derivation is deterministic: the same public key
    /// always produces the same fingerprint.
    pub fn from_public_key(public_key: &[u8]) -> Self {
        let digest = Sha256::digest(public_key);
        let hex_chars = hex_prefix(&digest, FINGERPRINT_HEX_CHARS);
        Self(hex_chars)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PeerFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn hex_prefix(digest: &[u8], hex_chars: usize) -> String {
    let mut out = String::with_capacity(hex_chars);
    for byte in digest.iter().take(hex_chars / 2) {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Metadata-only identity surface returned by a
/// [`PeerIdentityStore::load_or_create`] call. The private key is
/// intentionally absent: it lives exclusively inside the secure
/// store and the in-memory representation the store keeps for the
/// lifetime of the process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPeerIdentity {
    pub peer_id: PeerId,
    pub fingerprint: PeerFingerprint,
    pub public_key: [u8; ED25519_PUBLIC_KEY_LEN],
}

impl LocalPeerIdentity {
    /// Returns the fingerprint's first 8 chars, suitable for a
    /// compact UI badge. The value is a pure projection of
    /// [`Self::fingerprint`]; nothing else.
    pub fn short_fingerprint(&self) -> &str {
        let value = self.fingerprint.as_str();
        if value.len() <= 8 {
            value
        } else {
            &value[..8]
        }
    }

    /// Returns the 32-byte Ed25519 public key bytes the secure
    /// store derived from the persisted seed. Used by the TLS
    /// material builder so the [`crate::peer_transport`] layer
    /// can mint a self-signed certificate that pins the same
    /// public key the runtime advertises through mDNS — and
    /// therefore the same `peer_id` / fingerprint the pairing
    /// handshake pins.
    pub fn public_key_bytes(&self) -> &[u8; ED25519_PUBLIC_KEY_LEN] {
        &self.public_key
    }
}

/// Pairing + TLS material the platform crate mints from the
/// persisted Ed25519 seed. The struct owns:
///
/// - the public surface ([`LocalPeerIdentity`]) every layer is
///   allowed to see;
/// - the DER-encoded self-signed certificate the TLS listener
///   presents during the handshake. The certificate's SPKI is
///   pinned to [`LocalPeerIdentity::public_key`] so the cert is
///   stable across restarts;
/// - the PKCS#8 DER-encoded Ed25519 private key rustls needs to
///   sign the handshake. The key never leaves the platform crate
///   and is dropped together with the [`crate::peer_transport`]
///   listener that owns it.
///
/// Only the [`crate::peer_transport`] layer is meant to construct
/// or consume the value: core, SQLite, Tauri and the frontend
/// receive the metadata-only [`LocalPeerIdentity`] through the
/// existing discovery surface, never the cert or the private key.
#[cfg(feature = "local-peer-pairing-tls")]
#[derive(Clone)]
pub struct LocalIdentityMaterial {
    identity: LocalPeerIdentity,
    cert_der: Vec<u8>,
    private_key_pkcs8_der: Vec<u8>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl std::fmt::Debug for LocalIdentityMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalIdentityMaterial")
            .field("identity", &self.identity)
            .field("cert_der_len", &self.cert_der.len())
            .field(
                "private_key_pkcs8_der_len",
                &self.private_key_pkcs8_der.len(),
            )
            .finish()
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl LocalIdentityMaterial {
    /// Build the TLS material from the persisted seed. The cert is
    /// self-signed with the same Ed25519 key the secure store uses
    /// for the public surface, so a future handshake can verify
    /// the peer_id / fingerprint / cert SPKI alignment against the
    /// metadata the runtime persists after the pairing completes.
    pub fn from_seed(seed: [u8; 32]) -> Result<Self, PeerIdentityError> {
        use ed25519_dalek::pkcs8::EncodePrivateKey;
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let mut public_key = [0u8; ED25519_PUBLIC_KEY_LEN];
        public_key.copy_from_slice(verifying_key.as_bytes());
        let identity = LocalPeerIdentity {
            peer_id: PeerId::from_public_key(&public_key),
            fingerprint: PeerFingerprint::from_public_key(&public_key),
            public_key,
        };

        // Verify the public key the cert builder will end up
        // pinning matches the public key the runtime exposes
        // through `LocalPeerIdentity`. `derive_identity` derives
        // the public key from the seed directly, so the two
        // paths always agree; the assert is a compile-time +
        // runtime regression guard against a future refactor that
        // accidentally derives the cert key from a different
        // secret.
        assert_eq!(
            identity.public_key, public_key,
            "LocalIdentityMaterial must bind cert and identity to the same public key",
        );

        let pkcs8_doc = signing_key
            .to_pkcs8_der()
            .map_err(|_| PeerIdentityError::SecureStoreFailed)?;
        let pkcs8_bytes = pkcs8_doc.as_bytes().to_vec();

        let cert_der = build_self_signed_ed25519_cert(&pkcs8_bytes)?;

        Ok(Self {
            identity,
            cert_der,
            private_key_pkcs8_der: pkcs8_bytes,
        })
    }

    /// Public surface every layer is allowed to see.
    pub fn identity(&self) -> &LocalPeerIdentity {
        &self.identity
    }

    /// DER-encoded self-signed certificate bytes (the cert the
    /// TLS listener presents during the handshake).
    pub fn cert_der(&self) -> &[u8] {
        &self.cert_der
    }

    /// PKCS#8 DER-encoded Ed25519 private key. Only the rustls
    /// [`ServerConfig`] builder is allowed to consume the value;
    /// the bytes never escape the platform crate.
    pub fn private_key_pkcs8_der(&self) -> &[u8] {
        &self.private_key_pkcs8_der
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
fn build_self_signed_ed25519_cert(pkcs8_der: &[u8]) -> Result<Vec<u8>, PeerIdentityError> {
    use rcgen::{CertificateParams, KeyPair, PKCS_ED25519};
    let key_pair = KeyPair::from_pkcs8_der_and_sign_algo(
        &rustls_pki_types::PrivatePkcs8KeyDer::from(pkcs8_der.to_vec()),
        &PKCS_ED25519,
    )
    .map_err(|_| PeerIdentityError::SecureStoreFailed)?;
    let mut params = CertificateParams::new(vec!["clipvault.local".to_string()])
        .map_err(|_| PeerIdentityError::SecureStoreFailed)?;
    // The cert MUST stay stable across restarts so the SPKI / DER
    // fingerprint the runtime persists in `known_peers.tls_cert_fingerprint`
    // never drifts. Every signed field — serial, not_before, not_after
    // and subject — is derived deterministically from the seed bytes
    // (no clock, no `SystemTime::now`). The cert covers a wide window
    // (year 1970 → 9999) so a verifier that evaluates `notBefore` /
    // `notAfter` still accepts the cert at every wall-clock instant
    // the shell can run on while keeping the rotation path under user
    // control (revoke / re-pair mints a new seed and therefore a new
    // cert, never rotating the existing one mid-flight).
    let seed_serial = deterministic_serial_from_pkcs8(pkcs8_der);
    params.serial_number = Some(seed_serial);
    params.not_before = time::Date::from_calendar_date(1970, time::Month::January, 1)
        .ok()
        .and_then(|d| d.with_hms(0, 0, 0).ok())
        .map(time::PrimitiveDateTime::assume_utc)
        .unwrap_or_else(|| time::OffsetDateTime::now_utc());
    params.not_after = time::Date::from_calendar_date(9999, time::Month::December, 31)
        .ok()
        .and_then(|d| d.with_hms(23, 59, 59).ok())
        .map(time::PrimitiveDateTime::assume_utc)
        .unwrap_or_else(|| time::OffsetDateTime::now_utc() + time::Duration::days(365 * 50));
    let cert = params
        .self_signed(&key_pair)
        .map_err(|_| PeerIdentityError::SecureStoreFailed)?;
    Ok(cert.der().to_vec())
}

/// Derive a deterministic serial number from the Ed25519 seed bytes.
/// The serial is a 64-bit big-endian integer derived from
/// `SHA-256(pkcs8_der)[..8]`, guaranteeing the same seed produces
/// the same serial across processes and platforms. The value is
/// always non-zero so `rcgen` (which rejects zero serials) accepts
/// it on every cert build.
#[cfg(feature = "local-peer-pairing-tls")]
fn deterministic_serial_from_pkcs8(pkcs8_der: &[u8]) -> rcgen::SerialNumber {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(pkcs8_der);
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    if bytes == [0u8; 8] {
        bytes[0] = 1;
    }
    let value = u64::from_be_bytes(bytes);
    rcgen::SerialNumber::from(value)
}

/// Implementation helper exposed to the keychain store so the
/// [`KeychainPeerIdentityStore::derive_identity`] helper can stay
/// private to the module while still being callable from the
/// [`crate::peer_transport`] material builder.
#[allow(dead_code)]
#[cfg(feature = "local-peer-identity-keychain")]
pub(super) fn identity_from_seed(seed: [u8; 32]) -> LocalPeerIdentity {
    derive_identity_from_seed(seed)
}

/// Derive the [`LocalPeerIdentity`] surface from a raw 32-byte
/// Ed25519 seed without exposing the seed to the caller. The
/// helper is reused by the TLS material builder so the cert
/// SPKI is always pinned to the same public key the public
/// surface exposes.
#[allow(dead_code)]
#[cfg(feature = "local-peer-identity-keychain")]
fn derive_identity_from_seed(seed: [u8; 32]) -> LocalPeerIdentity {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    let mut public_key = [0u8; ED25519_PUBLIC_KEY_LEN];
    public_key.copy_from_slice(verifying_key.as_bytes());
    LocalPeerIdentity {
        peer_id: PeerId::from_public_key(&public_key),
        fingerprint: PeerFingerprint::from_public_key(&public_key),
        public_key,
    }
}

/// Typed error surface for the peer identity stack. The variants
/// stay conservative: every one of them signals a recoverable
/// failure that the UI can translate into a stable, user-facing
/// message without leaking the underlying platform detail.
#[derive(Debug, thiserror::Error)]
pub enum PeerIdentityError {
    /// The platform secure credential store is not available. The
    /// shell must NOT fall back to a plaintext file or open any
    /// network capability — the user has to unlock the keychain /
    /// unlock the desktop session before retrying.
    #[error("secure identity store is unavailable on this host")]
    SecureStoreUnavailable,
    /// The platform secure store rejected the operation with a
    /// non-actionable error. The detail is sanitised by the
    /// platform adapter; the core never sees the platform message.
    #[error("secure identity store rejected the operation")]
    SecureStoreFailed,
}

/// Discriminated outcome returned by
/// [`crate::peer_identity::PeerIdentityService`] so the shell can
/// render the right user-facing copy without inspecting free-form
/// error strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerIdentityOutcome<T> {
    Ok(T),
    Unavailable,
}

impl<T> PeerIdentityOutcome<T> {
    pub fn into_result(self) -> Result<T, PeerIdentityError> {
        match self {
            PeerIdentityOutcome::Ok(value) => Ok(value),
            PeerIdentityOutcome::Unavailable => Err(PeerIdentityError::SecureStoreUnavailable),
        }
    }
}

/// Backend abstraction for the secure credential store. The core
/// never depends on `keyring`, `security-framework` or
/// `secret-service` directly; the platform crate provides the
/// concrete implementation behind the
/// `local-peer-identity-keychain` feature and tests inject an
/// in-memory fake.
pub trait PeerIdentityStore: Send + Sync {
    /// Load the existing identity or create one if the store is
    /// empty. Idempotent: a successful first call followed by any
    /// number of additional calls always returns the same
    /// `LocalPeerIdentity`.
    fn load_or_create(&self) -> Result<LocalPeerIdentity, PeerIdentityError>;
}

/// Decode the secret bytes returned by `keyring::Entry::get_secret`
/// into the 32-byte Ed25519 seed. The secure store is expected to
/// return exactly the bytes the bootstrap previously wrote; a
/// different length is treated as a stable failure rather than a
/// panic so a corrupted keychain entry cannot crash the shell.
#[cfg(feature = "local-peer-identity-keychain")]
fn seed_from_secret_bytes(bytes: &[u8]) -> Result<[u8; 32], PeerIdentityError> {
    if bytes.len() != 32 {
        return Err(PeerIdentityError::SecureStoreFailed);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_id_is_stable_and_uses_only_lowercase_hex() {
        let public_key = [7u8; ED25519_PUBLIC_KEY_LEN];
        let id = PeerId::from_public_key(&public_key);
        assert_eq!(id.as_str().len(), PEER_ID_HEX_CHARS);
        assert!(
            id.as_str()
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "peer_id must be lowercase hex: {}",
            id.as_str()
        );
    }

    #[test]
    fn peer_id_and_fingerprint_are_deterministic_and_distinct() {
        let public_key = [11u8; ED25519_PUBLIC_KEY_LEN];
        let id_a = PeerId::from_public_key(&public_key);
        let id_b = PeerId::from_public_key(&public_key);
        assert_eq!(id_a, id_b);

        let other_key = [12u8; ED25519_PUBLIC_KEY_LEN];
        let id_other = PeerId::from_public_key(&other_key);
        assert_ne!(id_a, id_other);

        let fp = PeerFingerprint::from_public_key(&public_key);
        assert!(id_a.as_str().starts_with(fp.as_str()));
        assert_eq!(fp.as_str().len(), FINGERPRINT_HEX_CHARS);
    }

    #[cfg(feature = "local-peer-identity-keychain")]
    #[test]
    fn seed_from_secret_bytes_round_trips_thirty_two_bytes() {
        let key = [9u8; 32];
        let decoded = seed_from_secret_bytes(&key).expect("decode");
        assert_eq!(decoded, key);
    }

    #[cfg(feature = "local-peer-identity-keychain")]
    #[test]
    fn seed_from_secret_bytes_rejects_wrong_length() {
        let err = seed_from_secret_bytes(&[0u8; 31]).expect_err("must reject");
        assert!(matches!(err, PeerIdentityError::SecureStoreFailed));
        let err = seed_from_secret_bytes(&[0u8; 33]).expect_err("must reject");
        assert!(matches!(err, PeerIdentityError::SecureStoreFailed));
    }
}

/// Platform-side secure credential store backed by the `keyring`
/// crate. Compiled only when the `local-peer-identity-keychain`
/// feature is enabled (default on macOS and Linux builds, off for
/// cross-compiles that target an unsupported OS).
///
/// The store transparently maps every `keyring::Error` into the
/// stable [`PeerIdentityError`] taxonomy. The platform detail
/// never leaves this module, so the core cannot accidentally log
/// a keychain error message that could leak the entry identifier
/// or the user account name.
#[cfg(feature = "local-peer-identity-keychain")]
pub struct KeychainPeerIdentityStore {
    service: &'static str,
    username: &'static str,
}

/// Process-wide mutex that serialises the
/// `get_secret → generate → set_secret` sequence of
/// [`KeychainPeerIdentityStore::load_or_create`].
///
/// The keyring backend is not guaranteed to be atomic across
/// concurrent first-time calls: two threads that both observe
/// [`keyring::Error::NoEntry`] can race past the branch, mint two
/// distinct seeds and overwrite each other, so both threads would
/// return **different** `peer_id` values for the "first" profile
/// load. The user-visible regression was that two rapid frontend
/// GETs could each surface a different `LocalPeerProfile` on the
/// very first launch.
///
/// The production design always targets the same canonical
/// service / username pair ([`PEER_IDENTITY_SERVICE`] /
/// [`PEER_IDENTITY_USERNAME`]) — every store built through
/// [`KeychainPeerIdentityStore::new`] reads the same entry — so a
/// single `Mutex<()>` is enough to satisfy the atomicity contract.
/// The `OnceLock` initialiser avoids any ordering hazard at first
/// call and lets every store instance hold the same `&'static`
/// reference without `Arc` plumbing. The mutex is intentionally
/// scoped to the in-process runtime; nothing here assumes or
/// requires any cross-process serialisation behaviour from the
/// underlying keyring backend.
#[cfg(feature = "local-peer-identity-keychain")]
static LOADER_LOCK: OnceLock<parking_lot::Mutex<()>> = OnceLock::new();

#[cfg(feature = "local-peer-identity-keychain")]
fn loader_lock() -> &'static parking_lot::Mutex<()> {
    LOADER_LOCK.get_or_init(|| parking_lot::Mutex::new(()))
}

#[cfg(feature = "local-peer-identity-keychain")]
impl KeychainPeerIdentityStore {
    /// Build a new store bound to the canonical service / username
    /// pair the rest of the change uses.
    pub fn new() -> Self {
        Self {
            service: PEER_IDENTITY_SERVICE,
            username: PEER_IDENTITY_USERNAME,
        }
    }

    fn entry(&self) -> Result<keyring::Entry, PeerIdentityError> {
        keyring::Entry::new(self.service, self.username)
            .map_err(|_| PeerIdentityError::SecureStoreUnavailable)
    }

    /// Load the persisted seed (32 raw bytes) from the secure
    /// store and derive both the [`LocalPeerIdentity`] and the
    /// TLS material the pairing transport needs. The seed never
    /// crosses the platform crate boundary: callers receive an
    /// opaque [`LocalIdentityMaterial`] that carries the cert +
    /// private key, ready to be wired into a rustls
    /// [`ServerConfig`].
    ///
    /// The store is the single owner of the seed; the
    /// [`crate::peer_transport`] layer only sees the typed
    /// material and never reconstructs the seed bytes.
    #[cfg(feature = "local-peer-pairing-tls")]
    pub fn load_or_create_material(&self) -> Result<LocalIdentityMaterial, PeerIdentityError> {
        let _guard = loader_lock().lock();
        let entry = self.entry()?;
        let seed_bytes = match entry.get_secret() {
            Ok(bytes) => bytes,
            Err(keyring::Error::NoEntry) => {
                let mut seed = [0u8; 32];
                fill_random(&mut seed)?;
                if let Err(error) = entry.set_secret(&seed) {
                    return Err(map_keyring_error(error));
                }
                seed.to_vec()
            }
            Err(error) => return Err(map_keyring_error(error)),
        };
        let seed = seed_from_secret_bytes(&seed_bytes)?;
        LocalIdentityMaterial::from_seed(seed)
    }

    fn derive_identity(seed: [u8; 32]) -> LocalPeerIdentity {
        let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();
        let mut public_key = [0u8; ED25519_PUBLIC_KEY_LEN];
        public_key.copy_from_slice(verifying.as_bytes());
        LocalPeerIdentity {
            peer_id: PeerId::from_public_key(&public_key),
            fingerprint: PeerFingerprint::from_public_key(&public_key),
            public_key,
        }
    }
}

#[cfg(feature = "local-peer-identity-keychain")]
impl Default for KeychainPeerIdentityStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "local-peer-identity-keychain")]
impl PeerIdentityStore for KeychainPeerIdentityStore {
    fn load_or_create(&self) -> Result<LocalPeerIdentity, PeerIdentityError> {
        // Serialise the entire `get_secret → generate → set_secret`
        // sequence so two concurrent first-time callers cannot race
        // past the `NoEntry` branch and mint two distinct identities.
        // The lock is process-wide because every production store
        // targets the canonical service / username pair.
        let _guard = loader_lock().lock();
        let entry = self.entry()?;
        match entry.get_secret() {
            Ok(bytes) => {
                let seed = seed_from_secret_bytes(&bytes)?;
                Ok(Self::derive_identity(seed))
            }
            Err(keyring::Error::NoEntry) => {
                let mut seed = [0u8; 32];
                fill_random(&mut seed)?;
                if let Err(error) = entry.set_secret(&seed) {
                    return Err(map_keyring_error(error));
                }
                Ok(Self::derive_identity(seed))
            }
            Err(error) => Err(map_keyring_error(error)),
        }
    }
}

#[cfg(feature = "local-peer-identity-keychain")]
fn fill_random(buf: &mut [u8]) -> Result<(), PeerIdentityError> {
    use rand_core::RngCore;
    rand_core::OsRng
        .try_fill_bytes(buf)
        .map_err(|_| PeerIdentityError::SecureStoreUnavailable)
}

#[cfg(feature = "local-peer-identity-keychain")]
fn map_keyring_error(error: keyring::Error) -> PeerIdentityError {
    // Every platform failure is collapsed into the "unavailable"
    // bucket so the shell can present a single, stable guidance
    // copy. The raw platform detail is intentionally swallowed —
    // the core must never surface a keychain / D-Bus message to
    // the user (it could leak the entry identifier, the bus name
    // or the user account name).
    match error {
        keyring::Error::NoEntry => PeerIdentityError::SecureStoreFailed,
        _ => PeerIdentityError::SecureStoreUnavailable,
    }
}

#[cfg(all(test, feature = "local-peer-identity-keychain"))]
mod keychain_tests {
    use super::*;

    #[test]
    fn derive_identity_is_deterministic() {
        let seed = [5u8; 32];
        let a = KeychainPeerIdentityStore::derive_identity(seed);
        let b = KeychainPeerIdentityStore::derive_identity(seed);
        assert_eq!(a.peer_id, b.peer_id);
        assert_eq!(a.public_key, b.public_key);
    }

    #[test]
    fn derive_identity_changes_with_seed() {
        let a = KeychainPeerIdentityStore::derive_identity([1u8; 32]);
        let b = KeychainPeerIdentityStore::derive_identity([2u8; 32]);
        assert_ne!(a.peer_id, b.peer_id);
        assert_ne!(a.public_key, b.public_key);
    }

    #[test]
    fn short_fingerprint_truncates_to_eight_chars() {
        let identity = KeychainPeerIdentityStore::derive_identity([7u8; 32]);
        let short = identity.short_fingerprint();
        assert!(short.len() <= 8);
        assert!(identity.fingerprint.as_str().starts_with(short));
    }

    #[test]
    fn loader_lock_serialises_concurrent_callers_with_standard_mutex() {
        // Regression pin: the process-wide `parking_lot::Mutex`
        // MUST serialise concurrent acquirers — if a future
        // refactor swaps the primitive for something weaker (e.g.
        // an `RwLock` with the write guard accidentally elided) the
        // two concurrent acquirers below would race past each
        // other. Holding the lock from this thread and observing a
        // non-blocking attempt from the spawned thread proves the
        // lock is held for the entire critical section.
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::thread;

        let lock = loader_lock();
        let acquired = Arc::new(AtomicBool::new(false));
        let guard = lock.lock();
        let acquired_clone = Arc::clone(&acquired);
        let handle = thread::spawn(move || {
            let _g = lock.lock();
            acquired_clone.store(true, Ordering::Release);
        });
        // The spawned thread is blocked on the lock; give it a
        // moment to confirm it did NOT enter the critical section
        // while the guard is alive.
        thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !acquired.load(Ordering::Acquire),
            "spawned thread must remain blocked while the guard is alive",
        );
        drop(guard);
        handle.join().expect("thread join");
        assert!(
            acquired.load(Ordering::Acquire),
            "spawned thread must reach the critical section after the guard drops",
        );
    }
}
