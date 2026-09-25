//! Persistent view of the metadata every ClipVault installation
//! observed on the local network.
//!
//! The repository stores ONLY the validated non-content fields the
//! `local-peer-discovery` change persists: a stable `peer_id`, the
//! public key fingerprint, the validated display name, the protocol
//! major version and the `capability` advertised (always
//! `discovery_only` for this change). The discovery surface is
//! metadata-only by construction — IP addresses, ports, raw public
//! key bytes, private keys, clipboard payloads, previews, hashes and
//! source identifiers never reach this table. The `core` layer is
//! responsible for trimming/normalising the display name before
//! calling `upsert_observation` so the comparison stays in one
//! place; the repository never re-normalises.
//!
//! Re-observing a known peer is idempotent: the merge updates
//! `last_discovered_at` and `updated_at`, and refreshes the public
//! fields the core re-validates (fingerprint, display name, protocol
//! and additive `caps_extra` capabilities).
//! `discovery_only` and `pairing` are a live listener-state transition
//! of that same identity, not a competing identity, so their transition
//! updates the capability while preserving a previously learned full
//! pairing fingerprint. The `caps_extra` value always reflects the latest
//! compatible advertisement so capability additions and withdrawals take
//! effect for rows persisted by older builds.
//! A conflicting announcement (a different `public_key_fingerprint`
//! reusing an existing `peer_id`) is intentionally rejected by the
//! repository: the core sees the rejection as an "ignore" and the
//! previously persisted metadata stays untouched.

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Error)]
pub enum KnownPeersError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// One observed peer as it lives in the database. The struct mirrors
/// the columns the `local-peer-discovery` and
/// `local-peer-mutual-pairing` migrations add; no field carries
/// endpoints, content, secrets or the raw TLS key bytes — the
/// transport layer persists only the SHA-256 fingerprint of the
/// DER-encoded certificate so a future cert rotation produces a
/// different pinned value and the runtime can reject the change
/// instead of accepting a possibly-compromised peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownPeer {
    pub peer_id: String,
    pub public_key_fingerprint: String,
    /// Full SHA-256 of the peer's Ed25519 public key (64 hex
    /// chars). The pairing change populates the column when the
    /// peer's TXT record declares `capability = pairing`. Empty
    /// for peers that were only ever observed via
    /// `discovery_only`; the pairing runtime refuses to start a
    /// session against a row whose full fingerprint is missing so
    /// the bridge never feeds the short 16-hex fingerprint into
    /// the TLS layer.
    pub full_public_key_fingerprint: String,
    pub display_name: String,
    pub protocol_major: i64,
    pub capability: String,
    pub first_seen_at: String,
    pub last_discovered_at: String,
    pub updated_at: String,
    /// Trust state the pairing runtime persists after the reciprocal
    /// approval. Defaults to [`TrustState::Unverified`] for every
    /// pre-pairing row; the runtime owns every transition
    /// (`Unverified` → `Trusted` → `Revoked` → `Unverified`,
    /// `*` → `Blocked` → `Unverified`).
    pub trust_state: TrustState,
    /// SHA-256 of the DER-encoded TLS cert the peer used during the
    /// last successful mTLS handshake. Empty until pairing
    /// completes. The runtime pins this value and rejects an
    /// incoming connection that does not match.
    pub tls_cert_fingerprint: String,
    /// Instant the reciprocal pairing was first persisted; empty
    /// while `trust_state = unverified`. The runtime uses the value
    /// only for diagnostics — it never participates in any
    /// gating decision.
    pub paired_at: String,
    /// Protocol major version at the moment pairing completed.
    /// Lets a future protocol bump surface an `IncompatibleProtocol`
    /// outcome before the cert pinning is consulted again.
    pub paired_protocol_major: i64,
    /// Per-peer 32-byte HMAC secret the host uses to sign and
    /// verify `RemoteHistoryCursor` envelopes for this peer.
    /// Stored as 64 lowercase-hex chars (the canonical SHA-256-sized
    /// key the HMAC-SHA256 scheme mandates) so the column never
    /// carries raw bytes, never appears in renderer payloads and
    /// never crosses the wire. Empty until the
    /// `peer-text-history-browser` runtime mints a fresh secret on
    /// the next `trust_state = trusted` transition; cleared by
    /// every revoke / block / unblock so a stale cursor cannot
    /// resurrect the link after the trust state changes.
    pub cursor_secret: String,
    /// Additive capability tokens the host advertises through the
    /// dedicated `caps_extra` TXT field. The column carries a
    /// comma-separated list (empty for legacy rows) so the
    /// canonical [`Self::capability`] stays at `pairing` /
    /// `discovery_only` exactly while the runtime resolves the
    /// additive surface through the helper
    /// [`crate::peer_discovery::decode_capabilities`].
    pub caps_extra: String,
}

/// Trust state the pairing runtime persists alongside every
/// `known_peers` row. The enum is the single source of truth the
/// runtime reads on every snapshot / health probe; the discovery
/// surface only writes `Unverified` (the implicit default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustState {
    /// Default state the discovery runtime leaves every row in.
    /// Pairing or revoke/block transitions move the row to one of
    /// the remaining variants.
    Unverified,
    /// Both endpoints approved the reciprocal pairing within the
    /// bounded session; the runtime pins the cert and unlocks the
    /// metadata-only health surface.
    Trusted,
    /// The user explicitly disconnected from this peer. The
    /// persisted row stays in place (so a re-detection can offer
    /// the pairing flow again) but health / presence is reported
    /// as `NotAvailable`.
    Revoked,
    /// The user blocked the peer. Pairing and health probes are
    /// rejected before any cryptographic work runs.
    Blocked,
}

impl TrustState {
    /// Stable snake-case string the runtime / bridge / UI uses as a
    /// discriminator.
    pub fn as_str(self) -> &'static str {
        match self {
            TrustState::Unverified => "unverified",
            TrustState::Trusted => "trusted",
            TrustState::Revoked => "revoked",
            TrustState::Blocked => "blocked",
        }
    }

    fn parse(raw: &str) -> Self {
        match raw {
            "trusted" => TrustState::Trusted,
            "revoked" => TrustState::Revoked,
            "blocked" => TrustState::Blocked,
            _ => TrustState::Unverified,
        }
    }
}

impl Default for TrustState {
    fn default() -> Self {
        TrustState::Unverified
    }
}

/// Outcome of an [`KnownPeerRepository::upsert_observation`] call.
/// The core reads the variant to decide whether to merge the new
/// observation or to ignore a conflicting announcement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpsertObservationOutcome {
    /// The observation was accepted and the row reflects the merged
    /// state (insert for a new peer; update for a known peer).
    Stored(KnownPeer),
    /// The observation conflicted with a previously persisted
    /// identity (different fingerprint, display name, protocol major
    /// or an incompatible capability transition reusing the same `peer_id`). The repository
    /// left the previously persisted row untouched and returned the
    /// existing record so the core can log the rejection without
    /// inspecting the conflicting bytes.
    Conflict(KnownPeer),
}

/// Outcome of the pairing-only `mark_trusted` /
/// [`KnownPeerRepository::apply_trust_transition`] call the
/// pairing runtime issues when both sides approved the
/// short-code. The repository stays the single place where the
/// trust_state column is mutated so the runtime can branch on the
/// reason without inspecting free-form strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustTransitionOutcome {
    /// The transition succeeded; the row reflects the new state
    /// (cert fingerprint, paired_at, paired_protocol_major set when
    /// transitioning to [`TrustState::Trusted`]).
    Stored(KnownPeer),
    /// The requested transition cannot be applied because the row
    /// is in a state that does not allow it (e.g. trying to
    /// `mark_trusted` a blocked peer). The previously persisted row
    /// is returned untouched so the caller can decide how to
    /// surface the rejection.
    Conflict(KnownPeer),
    /// The peer id is not present in `known_peers`. Pairing must be
    /// preceded by a discovery observation; the repository refuses
    /// to insert a row from a pairing attempt alone.
    Unknown,
}

/// Fields the runtime validates and persists per observation. The
/// struct is `Clone` so the core can stage a draft before calling
/// [`KnownPeerRepository::upsert_observation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerObservation {
    pub peer_id: String,
    pub public_key_fingerprint: String,
    /// Full SHA-256 of the peer's Ed25519 public key (64 hex
    /// chars). The pairing change populates this field when the
    /// incoming TXT record declares `capability = pairing`; the
    /// discovery-only path leaves it `None` so the column stays
    /// empty until the peer advertises pairing. The value is the
    /// canonical fingerprint the productive pairing layer hands
    /// to the TLS listener; the short
    /// [`Self::public_key_fingerprint`] remains the projection
    /// the discovery-side UI uses.
    pub full_public_key_fingerprint: Option<String>,
    pub display_name: String,
    pub protocol_major: i64,
    /// Canonical `capability` token the host advertises. Stays at
    /// `pairing` / `discovery_only` exactly so a legacy client
    /// that only accepts the literal keeps recognising the record;
    /// the additive `caps_extra` surface travels through the
    /// dedicated [`Self::caps_extra`] field.
    pub capability: String,
    /// Additive capability tokens the host publishes through the
    /// dedicated `caps_extra` TXT field. The value is a
    /// comma-separated list (empty for legacy rows) so the
    /// repository persists the additive surface verbatim and the
    /// bootstrap resolver can split it with the same helper the
    /// TXT validator uses.
    pub caps_extra: String,
    /// Instant the runtime observed the peer. The repository
    /// stamps it into `last_discovered_at` (and `first_seen_at`
    /// when the row is new); tests pass a deterministic
    /// `OffsetDateTime` here.
    pub observed_at: OffsetDateTime,
}

pub struct KnownPeerRepository<'a> {
    conn: &'a mut Connection,
}

impl<'a> KnownPeerRepository<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// Insert a new observation or merge it into the existing row.
    ///
    /// Conflict handling:
    /// - A new `peer_id` always inserts.
    /// - A re-observed `peer_id` with the same fingerprint / display
    ///   name / protocol major refreshes
    ///   `last_discovered_at` and `updated_at`; `first_seen_at` is
    ///   preserved.
    /// - `discovery_only` ↔ `pairing` is a compatible dynamic
    ///   transition. A pairing observation upgrades the canonical
    ///   full fingerprint; a discovery-only withdrawal changes the
    ///   current capability without erasing that fingerprint.
    /// - A re-observed `peer_id` whose fingerprint, display name,
    ///   protocol major or incompatible capability transition diverges from the previously
    ///   persisted row is reported as [`UpsertObservationOutcome::Conflict`]
    ///   and the persisted row is left intact.
    pub fn upsert_observation(
        &mut self,
        observation: &PeerObservation,
    ) -> Result<UpsertObservationOutcome, KnownPeersError> {
        let now = format_timestamp(observation.observed_at);
        let tx = self.conn.transaction()?;
        let existing = {
            let mut stmt = tx.prepare(
                "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at, \
                        trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                 FROM known_peers WHERE peer_id = ?1",
            )?;
            stmt.query_row(params![observation.peer_id], |row| Ok(read_row(row)?))
                .optional()?
        };
        let outcome = match existing {
            None => {
                tx.execute(
                    "INSERT INTO known_peers \
                        (peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                         capability, first_seen_at, last_discovered_at, updated_at, caps_extra) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?7, ?8)",
                    params![
                        observation.peer_id,
                        observation.public_key_fingerprint,
                        observation
                            .full_public_key_fingerprint
                            .clone()
                            .unwrap_or_default(),
                        observation.display_name,
                        observation.protocol_major,
                        observation.capability,
                        now,
                        observation.caps_extra,
                    ],
                )?;
                let row = tx
                    .query_row(
                        "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                                capability, first_seen_at, last_discovered_at, updated_at, \
                                trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                         FROM known_peers WHERE peer_id = ?1",
                        params![observation.peer_id],
                        |row| read_row(row),
                    )
                    .map_err(KnownPeersError::Sqlite)?;
                UpsertObservationOutcome::Stored(row)
            }
            Some(previous) => {
                let compatible_capability_transition = previous.capability
                    == observation.capability
                    || matches!(
                        (
                            previous.capability.as_str(),
                            observation.capability.as_str()
                        ),
                        ("discovery_only", "pairing") | ("pairing", "discovery_only")
                    );
                if previous.public_key_fingerprint != observation.public_key_fingerprint
                    || previous.display_name != observation.display_name
                    || previous.protocol_major != observation.protocol_major
                    || !compatible_capability_transition
                {
                    // Conflict: refuse to overwrite the trusted row.
                    // The core sees the conflict and surfaces it as
                    // a metadata-only rejection without logging the
                    // rejected bytes.
                    UpsertObservationOutcome::Conflict(previous)
                } else {
                    // Merge: refresh the full fingerprint if the new
                    // observation carries one and the previous row
                    // did not. A peer first observed via
                    // `discovery_only` is upgraded to the canonical
                    // 64-hex fingerprint the moment it switches to
                    // `pairing` capability — the runtime never
                    // reverses that upgrade because the pairing
                    // listener rejects a session whose full
                    // fingerprint column is empty.
                    let merged_full = observation
                        .full_public_key_fingerprint
                        .clone()
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| previous.full_public_key_fingerprint.clone());
                    tx.execute(
                        "UPDATE known_peers \
                         SET full_public_key_fingerprint = ?1, capability = ?2, caps_extra = ?3, \
                             last_discovered_at = ?4, updated_at = ?4 \
                         WHERE peer_id = ?5",
                        params![
                            merged_full,
                            observation.capability,
                            observation.caps_extra,
                            now,
                            observation.peer_id
                        ],
                    )?;
                    let refreshed = tx
                        .query_row(
                            "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                                    capability, first_seen_at, last_discovered_at, updated_at, \
                                    trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                             FROM known_peers WHERE peer_id = ?1",
                            params![observation.peer_id],
                            |row| read_row(row),
                        )
                        .map_err(KnownPeersError::Sqlite)?;
                    UpsertObservationOutcome::Stored(refreshed)
                }
            }
        };
        tx.commit()?;
        Ok(outcome)
    }

    /// Lookup a single peer by id.
    pub fn get(&self, peer_id: &str) -> Result<Option<KnownPeer>, KnownPeersError> {
        let row = self
            .conn
            .query_row(
                "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at, \
                        trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                 FROM known_peers WHERE peer_id = ?1",
                params![peer_id],
                |row| read_row(row),
            )
            .optional()?;
        Ok(row)
    }

    /// List every persisted peer, ordered by `last_discovered_at`
    /// descending so the most-recent observation is rendered first
    /// in the Equipos view.
    pub fn list(&self) -> Result<Vec<KnownPeer>, KnownPeersError> {
        let mut stmt = self.conn.prepare(
            "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                    capability, first_seen_at, last_discovered_at, updated_at, \
                    trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
             FROM known_peers ORDER BY last_discovered_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| read_row(row))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Total number of persisted peers. Useful for the snapshot
    /// the `Equipos` view renders ("N pares detectados") and for
    /// tests that exercise the merge.
    pub fn count(&self) -> Result<i64, KnownPeersError> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM known_peers", [], |row| row.get(0))?;
        Ok(n)
    }

    /// Delete every persisted peer. Reserved for tests and for the
    /// future "Forget all peers" affordance the pairing change may
    /// expose; the discovery surface itself never calls it.
    pub fn delete_all(&mut self) -> Result<usize, KnownPeersError> {
        let removed = self.conn.execute("DELETE FROM known_peers", [])?;
        Ok(removed)
    }

    /// Persist the reciprocal pairing: the runtime supplies the
    /// cert fingerprint, paired_at timestamp and protocol major it
    /// negotiated so the repository owns the SQL surface in one
    /// place. A blocked row refuses the transition with
    /// [`TrustTransitionOutcome::Conflict`] so a blocked peer can
    /// not bypass the block by re-pairing; the runtime surfaces
    /// the rejection as a typed `Blocked` outcome.
    pub fn mark_trusted(
        &mut self,
        peer_id: &str,
        tls_cert_fingerprint: &str,
        paired_at: OffsetDateTime,
        paired_protocol_major: i64,
    ) -> Result<TrustTransitionOutcome, KnownPeersError> {
        let paired_at = format_timestamp(paired_at);
        let tx = self.conn.transaction()?;
        let existing = {
            let mut stmt = tx.prepare(
                "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at, \
                        trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                 FROM known_peers WHERE peer_id = ?1",
            )?;
            stmt.query_row(params![peer_id], |row| read_row(row))
                .optional()?
        };
        let Some(previous) = existing else {
            tx.commit()?;
            return Ok(TrustTransitionOutcome::Unknown);
        };
        if previous.trust_state == TrustState::Blocked {
            tx.commit()?;
            return Ok(TrustTransitionOutcome::Conflict(previous));
        }
        tx.execute(
            "UPDATE known_peers \
             SET trust_state = ?1, tls_cert_fingerprint = ?2, paired_at = ?3, \
                 paired_protocol_major = ?4, updated_at = ?3 \
             WHERE peer_id = ?5",
            params![
                TrustState::Trusted.as_str(),
                tls_cert_fingerprint,
                paired_at,
                paired_protocol_major,
                peer_id,
            ],
        )?;
        let refreshed = tx
            .query_row(
                "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at, \
                        trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                 FROM known_peers WHERE peer_id = ?1",
                params![peer_id],
                |row| read_row(row),
            )
            .map_err(KnownPeersError::Sqlite)?;
        tx.commit()?;
        Ok(TrustTransitionOutcome::Stored(refreshed))
    }

    /// Move a row to [`TrustState::Revoked`]. The fingerprint and
    /// paired_at columns stay intact so a re-detection can offer
    /// the pairing flow again; the runtime treats the row as
    /// `NotAvailable` until both sides approve a fresh short-code.
    /// A blocked row refuses the transition with
    /// [`TrustTransitionOutcome::Conflict`] — revoke / block are
    /// mutually exclusive terminal states.
    pub fn mark_revoked(
        &mut self,
        peer_id: &str,
    ) -> Result<TrustTransitionOutcome, KnownPeersError> {
        let tx = self.conn.transaction()?;
        let existing = {
            let mut stmt = tx.prepare(
                "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at, \
                        trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                 FROM known_peers WHERE peer_id = ?1",
            )?;
            stmt.query_row(params![peer_id], |row| read_row(row))
                .optional()?
        };
        let Some(previous) = existing else {
            tx.commit()?;
            return Ok(TrustTransitionOutcome::Unknown);
        };
        if previous.trust_state == TrustState::Blocked {
            tx.commit()?;
            return Ok(TrustTransitionOutcome::Conflict(previous));
        }
        let now = format_timestamp(OffsetDateTime::now_utc());
        tx.execute(
            "UPDATE known_peers \
             SET trust_state = ?1, updated_at = ?2 \
             WHERE peer_id = ?3",
            params![TrustState::Revoked.as_str(), now, peer_id],
        )?;
        let refreshed = tx
            .query_row(
                "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at, \
                        trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                 FROM known_peers WHERE peer_id = ?1",
                params![peer_id],
                |row| read_row(row),
            )
            .map_err(KnownPeersError::Sqlite)?;
        tx.commit()?;
        Ok(TrustTransitionOutcome::Stored(refreshed))
    }

    /// Move a row to [`TrustState::Blocked`]. Pairing and health
    /// probes against the blocked row are rejected by the runtime
    /// before any cryptographic work runs. A blocked row refuses to
    /// transition again with
    /// [`TrustTransitionOutcome::Conflict`] so the explicit
    /// `unblock` operation is the only way to leave the blocked
    /// state.
    pub fn mark_blocked(
        &mut self,
        peer_id: &str,
    ) -> Result<TrustTransitionOutcome, KnownPeersError> {
        let tx = self.conn.transaction()?;
        let existing = {
            let mut stmt = tx.prepare(
                "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at, \
                        trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                 FROM known_peers WHERE peer_id = ?1",
            )?;
            stmt.query_row(params![peer_id], |row| read_row(row))
                .optional()?
        };
        let Some(previous) = existing else {
            tx.commit()?;
            return Ok(TrustTransitionOutcome::Unknown);
        };
        if previous.trust_state == TrustState::Blocked {
            tx.commit()?;
            return Ok(TrustTransitionOutcome::Conflict(previous));
        }
        let now = format_timestamp(OffsetDateTime::now_utc());
        tx.execute(
            "UPDATE known_peers \
             SET trust_state = ?1, updated_at = ?2 \
             WHERE peer_id = ?3",
            params![TrustState::Blocked.as_str(), now, peer_id],
        )?;
        let refreshed = tx
            .query_row(
                "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at, \
                        trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                 FROM known_peers WHERE peer_id = ?1",
                params![peer_id],
                |row| read_row(row),
            )
            .map_err(KnownPeersError::Sqlite)?;
        tx.commit()?;
        Ok(TrustTransitionOutcome::Stored(refreshed))
    }

    /// Leave the blocked / revoked state and return to
    /// [`TrustState::Unverified`]. The runtime offers the explicit
    /// operation (the `Desbloquear` UI action) so a misclick can be
    /// reverted. The fingerprint and paired_at columns are cleared
    /// so a re-detection cannot claim the previous trust state.
    pub fn unblock(&mut self, peer_id: &str) -> Result<TrustTransitionOutcome, KnownPeersError> {
        let tx = self.conn.transaction()?;
        let existing = {
            let mut stmt = tx.prepare(
                "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at, \
                        trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                 FROM known_peers WHERE peer_id = ?1",
            )?;
            stmt.query_row(params![peer_id], |row| read_row(row))
                .optional()?
        };
        let Some(previous) = existing else {
            tx.commit()?;
            return Ok(TrustTransitionOutcome::Unknown);
        };
        // Only `Blocked` rows can be unblocked; revoked rows stay
        // in place until the user explicitly pairs again.
        if previous.trust_state != TrustState::Blocked {
            tx.commit()?;
            return Ok(TrustTransitionOutcome::Conflict(previous));
        }
        let now = format_timestamp(OffsetDateTime::now_utc());
        tx.execute(
            "UPDATE known_peers \
             SET trust_state = ?1, tls_cert_fingerprint = '', paired_at = '', \
                 paired_protocol_major = 0, updated_at = ?2 \
             WHERE peer_id = ?3",
            params![TrustState::Unverified.as_str(), now, peer_id],
        )?;
        let refreshed = tx
            .query_row(
                "SELECT peer_id, public_key_fingerprint, full_public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at, \
                        trust_state, tls_cert_fingerprint, paired_at, paired_protocol_major, cursor_secret, caps_extra \
                 FROM known_peers WHERE peer_id = ?1",
                params![peer_id],
                |row| read_row(row),
            )
            .map_err(KnownPeersError::Sqlite)?;
        tx.commit()?;
        Ok(TrustTransitionOutcome::Stored(refreshed))
    }

    /// Persist a fresh HMAC cursor secret the host uses to sign and
    /// verify `RemoteHistoryCursor` envelopes for `peer_id`. The
    /// runtime mints the value through
    /// [`crate::peer_pairing::PeerCursorSecret::generate`] (or the
    /// core's `PeerCursorSecret` helper) exactly once per trust
    /// promotion so a rotated secret invalidates every cursor the
    /// previous secret minted. `secret_hex` MUST be the
    /// 64-lowercase-hex representation of the 32-byte key; the
    /// repository refuses any other length so a corrupted value
    /// cannot leak through the HMAC pipeline.
    ///
    /// The repository never inspects the secret content: it just
    /// writes the column and updates `updated_at`. No raw key bytes
    /// ever cross the API boundary; the helper hides the
    /// conversion behind the hex projection so the runtime cannot
    /// accidentally log the raw bytes.
    pub fn set_cursor_secret(
        &mut self,
        peer_id: &str,
        secret_hex: &str,
    ) -> Result<(), KnownPeersError> {
        if secret_hex.len() != 64 || !secret_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(KnownPeersError::Sqlite(rusqlite::Error::InvalidQuery));
        }
        let now = format_timestamp(OffsetDateTime::now_utc());
        let updated = self.conn.execute(
            "UPDATE known_peers \
             SET cursor_secret = ?1, updated_at = ?2 \
             WHERE peer_id = ?3",
            params![secret_hex, now, peer_id],
        )?;
        if updated == 0 {
            return Err(KnownPeersError::Sqlite(
                rusqlite::Error::QueryReturnedNoRows,
            ));
        }
        Ok(())
    }

    /// Drop the persisted HMAC cursor secret for `peer_id`. The
    /// runtime calls this on revoke / block / unblock so a stale
    /// cursor signed under the previous secret cannot resurrect
    /// the link after the trust state changes. The repository
    /// writes an empty string (the documented sentinel for
    /// "no secret") rather than `NULL` so the column keeps the
    /// `NOT NULL` contract the migration pins.
    pub fn clear_cursor_secret(&mut self, peer_id: &str) -> Result<(), KnownPeersError> {
        let now = format_timestamp(OffsetDateTime::now_utc());
        let updated = self.conn.execute(
            "UPDATE known_peers \
             SET cursor_secret = '', updated_at = ?1 \
             WHERE peer_id = ?2",
            params![now, peer_id],
        )?;
        if updated == 0 {
            return Err(KnownPeersError::Sqlite(
                rusqlite::Error::QueryReturnedNoRows,
            ));
        }
        Ok(())
    }
}

/// Read every column the pairing migration adds into a typed
/// [`KnownPeer`]. Centralising the mapping here keeps the seven SQL
/// statements that hydrate a row in lock-step so a future column
/// change touches exactly one site.
fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnownPeer> {
    Ok(KnownPeer {
        peer_id: row.get(0)?,
        public_key_fingerprint: row.get(1)?,
        full_public_key_fingerprint: row.get(2)?,
        display_name: row.get(3)?,
        protocol_major: row.get(4)?,
        capability: row.get(5)?,
        first_seen_at: row.get(6)?,
        last_discovered_at: row.get(7)?,
        updated_at: row.get(8)?,
        trust_state: TrustState::parse(&row.get::<_, String>(9)?),
        tls_cert_fingerprint: row.get(10)?,
        paired_at: row.get(11)?,
        paired_protocol_major: row.get(12)?,
        cursor_secret: row.get(13)?,
        caps_extra: row.get(14)?,
    })
}

fn format_timestamp(ts: OffsetDateTime) -> String {
    ts.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn open_temp_db() -> (tempfile::TempDir, crate::Database) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = crate::Database::open(dir.path().join("clipvault.db")).expect("open");
        let mut db = db;
        db.run_migrations(&crate::builtin_migrations())
            .expect("migrate");
        (dir, db)
    }

    fn observation(
        peer_id: &str,
        fingerprint: &str,
        display_name: &str,
        protocol_major: i64,
        capability: &str,
        observed_at: OffsetDateTime,
    ) -> PeerObservation {
        PeerObservation {
            peer_id: peer_id.to_string(),
            public_key_fingerprint: fingerprint.to_string(),
            full_public_key_fingerprint: None,
            display_name: display_name.to_string(),
            protocol_major,
            capability: capability.to_string(),
            caps_extra: String::new(),
            observed_at,
        }
    }

    #[test]
    fn upsert_inserts_a_new_peer() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let outcome = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "0123456789abcdef",
                "Studio",
                1,
                "discovery_only",
                when,
            ))
            .expect("insert");
        let UpsertObservationOutcome::Stored(row) = outcome else {
            panic!("expected Stored, got {outcome:?}");
        };
        assert_eq!(row.peer_id, "peer-aaaa");
        assert_eq!(row.display_name, "Studio");
        assert_eq!(row.protocol_major, 1);
        assert_eq!(row.capability, "discovery_only");
        // The discovery-only path leaves the full fingerprint
        // column empty; the pairing runtime refuses to mint a
        // session against a row that does not advertise the
        // canonical SHA-256 fingerprint.
        assert_eq!(row.full_public_key_fingerprint, "");
        assert_eq!(row.first_seen_at, row.last_discovered_at);
    }

    #[test]
    fn upsert_pairing_capability_persists_full_fingerprint() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        // A pairing announcement carries the canonical full
        // SHA-256 fingerprint in the optional column. The
        // discovery-only path leaves the column empty so a peer
        // that only ever showed up over discovery_only cannot
        // start a pairing session through the bridge.
        let full_fp = "f".repeat(64);
        let outcome = repo
            .upsert_observation(&PeerObservation {
                peer_id: "peer-aaaa".to_string(),
                public_key_fingerprint: "0123456789abcdef".to_string(),
                full_public_key_fingerprint: Some(full_fp.clone()),
                display_name: "Studio".to_string(),
                protocol_major: 1,
                capability: "pairing".to_string(),
                caps_extra: String::new(),
                observed_at: when,
            })
            .expect("insert pairing");
        let UpsertObservationOutcome::Stored(row) = outcome else {
            panic!("expected Stored on insert, got {outcome:?}");
        };
        assert_eq!(row.full_public_key_fingerprint, full_fp);

        // A subsequent pairing observation with the same full
        // fingerprint refreshes the merge timestamps but keeps the
        // column untouched.
        let outcome = repo
            .upsert_observation(&PeerObservation {
                peer_id: "peer-aaaa".to_string(),
                public_key_fingerprint: "0123456789abcdef".to_string(),
                full_public_key_fingerprint: Some(full_fp.clone()),
                display_name: "Studio".to_string(),
                protocol_major: 1,
                capability: "pairing".to_string(),
                caps_extra: String::new(),
                observed_at: datetime!(2026-01-02 04:04:05 UTC),
            })
            .expect("merge pairing");
        let UpsertObservationOutcome::Stored(row) = outcome else {
            panic!("expected Stored on merge, got {outcome:?}");
        };
        assert_eq!(row.full_public_key_fingerprint, full_fp);
        // An empty fingerprint payload does NOT overwrite a
        // previously populated full fingerprint; the merge is a
        // no-op for that column. This protects against a peer that
        // briefly loses the pairing capability (e.g. the listener
        // is still binding) but has not yet rolled its identity.
        let outcome = repo
            .upsert_observation(&PeerObservation {
                peer_id: "peer-aaaa".to_string(),
                public_key_fingerprint: "0123456789abcdef".to_string(),
                full_public_key_fingerprint: None,
                display_name: "Studio".to_string(),
                protocol_major: 1,
                capability: "pairing".to_string(),
                caps_extra: String::new(),
                observed_at: datetime!(2026-01-02 05:04:05 UTC),
            })
            .expect("merge pairing with empty fingerprint");
        let UpsertObservationOutcome::Stored(row) = outcome else {
            panic!("expected Stored on merge, got {outcome:?}");
        };
        assert_eq!(row.full_public_key_fingerprint, full_fp);
    }

    #[test]
    fn upsert_merges_an_observation_of_a_known_peer() {
        let (_dir, mut db) = open_temp_db();
        let first = datetime!(2026-01-02 03:04:05 UTC);
        let second = datetime!(2026-01-02 04:04:05 UTC);
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "0123456789abcdef",
                "Studio",
                1,
                "discovery_only",
                first,
            ))
            .expect("first insert");
        let outcome = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "0123456789abcdef",
                "Studio",
                1,
                "discovery_only",
                second,
            ))
            .expect("merge");
        let UpsertObservationOutcome::Stored(row) = outcome else {
            panic!("expected Stored on merge, got {outcome:?}");
        };
        assert_eq!(row.first_seen_at, format_timestamp(first));
        assert_eq!(row.last_discovered_at, format_timestamp(second));
        assert_eq!(row.updated_at, format_timestamp(second));
        assert_eq!(repo.count().expect("count"), 1);
    }

    #[test]
    fn upsert_refreshes_additive_capabilities_for_an_existing_peer() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        repo.upsert_observation(&observation(
            "peer-aaaa",
            "0123456789abcdef",
            "Studio",
            1,
            "pairing",
            when,
        ))
        .expect("insert legacy peer");

        let mut image_capable = observation(
            "peer-aaaa",
            "0123456789abcdef",
            "Studio",
            1,
            "pairing",
            when + time::Duration::minutes(1),
        );
        image_capable.caps_extra = "image_import".to_string();
        let UpsertObservationOutcome::Stored(row) = repo
            .upsert_observation(&image_capable)
            .expect("refresh image capability")
        else {
            panic!("expected compatible observation to be stored");
        };
        assert_eq!(row.capability, "pairing");
        assert_eq!(row.caps_extra, "image_import");

        let withdrawn = observation(
            "peer-aaaa",
            "0123456789abcdef",
            "Studio",
            1,
            "pairing",
            when + time::Duration::minutes(2),
        );
        let UpsertObservationOutcome::Stored(row) = repo
            .upsert_observation(&withdrawn)
            .expect("refresh withdrawn capability")
        else {
            panic!("expected compatible observation to be stored");
        };
        assert!(row.caps_extra.is_empty());
    }

    #[test]
    fn upsert_rejects_a_conflicting_fingerprint_without_overwriting() {
        // A conflicting announcement (different fingerprint reusing the
        // same peer_id) MUST NOT replace the persisted row. The
        // repository returns the Conflict variant carrying the
        // previously stored metadata so the core can ignore the
        // announcement without losing trusted data.
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "0123456789abcdef",
                "Studio",
                1,
                "discovery_only",
                when,
            ))
            .expect("first insert");
        let outcome = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "ffffffffffffffff",
                "Studio",
                1,
                "discovery_only",
                when,
            ))
            .expect("conflict insert");
        let UpsertObservationOutcome::Conflict(row) = outcome else {
            panic!("expected Conflict, got {outcome:?}");
        };
        assert_eq!(row.public_key_fingerprint, "0123456789abcdef");
        let reloaded = repo.get("peer-aaaa").expect("reload").expect("present");
        assert_eq!(reloaded.public_key_fingerprint, "0123456789abcdef");
    }

    #[test]
    fn upsert_rejects_a_conflicting_display_name_without_overwriting() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "0123456789abcdef",
                "Studio",
                1,
                "discovery_only",
                when,
            ))
            .expect("first insert");
        let outcome = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "0123456789abcdef",
                "Impostor",
                1,
                "discovery_only",
                when,
            ))
            .expect("conflict insert");
        assert!(matches!(outcome, UpsertObservationOutcome::Conflict(_)));
        let reloaded = repo.get("peer-aaaa").expect("reload").expect("present");
        assert_eq!(reloaded.display_name, "Studio");
    }

    #[test]
    fn upsert_rejects_a_conflicting_protocol_major_without_overwriting() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "0123456789abcdef",
                "Studio",
                1,
                "discovery_only",
                when,
            ))
            .expect("first insert");
        let outcome = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "0123456789abcdef",
                "Studio",
                99,
                "discovery_only",
                when,
            ))
            .expect("conflict insert");
        assert!(matches!(outcome, UpsertObservationOutcome::Conflict(_)));
        let reloaded = repo.get("peer-aaaa").expect("reload").expect("present");
        assert_eq!(reloaded.protocol_major, 1);
    }

    #[test]
    fn upsert_promotes_discovery_only_to_pairing_and_retains_fingerprint_on_withdrawal() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "0123456789abcdef",
                "Studio",
                1,
                "discovery_only",
                when,
            ))
            .expect("first insert");
        let full_fingerprint = "f".repeat(64);
        let outcome = repo
            .upsert_observation(&PeerObservation {
                peer_id: "peer-aaaa".to_string(),
                public_key_fingerprint: "0123456789abcdef".to_string(),
                full_public_key_fingerprint: Some(full_fingerprint.clone()),
                display_name: "Studio".to_string(),
                protocol_major: 1,
                capability: "pairing".to_string(),
                caps_extra: String::new(),
                observed_at: when + time::Duration::hours(1),
            })
            .expect("pairing upgrade");
        let UpsertObservationOutcome::Stored(upgraded) = outcome else {
            panic!("pairing capability must upgrade a discovery-only row");
        };
        assert_eq!(upgraded.capability, "pairing");
        assert_eq!(upgraded.full_public_key_fingerprint, full_fingerprint);

        let outcome = repo
            .upsert_observation(&observation(
                "peer-aaaa",
                "0123456789abcdef",
                "Studio",
                1,
                "discovery_only",
                when + time::Duration::hours(2),
            ))
            .expect("discovery-only withdrawal");
        let UpsertObservationOutcome::Stored(withdrawn) = outcome else {
            panic!("withdrawal must update the dynamic capability");
        };
        assert_eq!(withdrawn.capability, "discovery_only");
        assert_eq!(withdrawn.full_public_key_fingerprint, full_fingerprint);
    }

    #[test]
    fn list_orders_rows_by_last_discovered_at_desc() {
        let (_dir, mut db) = open_temp_db();
        let t1 = datetime!(2026-01-02 03:04:05 UTC);
        let t2 = datetime!(2026-01-02 05:04:05 UTC);
        let t3 = datetime!(2026-01-02 04:04:05 UTC);
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        for (id, when) in [("peer-aaaa", t1), ("peer-bbbb", t2), ("peer-cccc", t3)] {
            repo.upsert_observation(&observation(
                id,
                "0123456789abcdef",
                "Studio",
                1,
                "discovery_only",
                when,
            ))
            .expect("insert");
        }
        let rows = repo.list().expect("list");
        let ids: Vec<&str> = rows.iter().map(|r| r.peer_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["peer-bbbb", "peer-cccc", "peer-aaaa"],
            "rows must be ordered by last_discovered_at desc"
        );
    }

    #[test]
    fn delete_all_removes_every_row() {
        let (_dir, mut db) = open_temp_db();
        let when = datetime!(2026-01-02 03:04:05 UTC);
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        for id in ["peer-aaaa", "peer-bbbb"] {
            repo.upsert_observation(&observation(
                id,
                "0123456789abcdef",
                "Studio",
                1,
                "discovery_only",
                when,
            ))
            .expect("insert");
        }
        assert_eq!(repo.count().expect("count"), 2);
        let removed = repo.delete_all().expect("delete_all");
        assert_eq!(removed, 2);
        assert_eq!(repo.count().expect("count"), 0);
    }

    fn seed_peer(repo: &mut KnownPeerRepository, peer_id: &str, when: OffsetDateTime) -> KnownPeer {
        match repo
            .upsert_observation(&observation(
                peer_id,
                "0123456789abcdef",
                "Studio",
                1,
                "pairing",
                when,
            ))
            .expect("insert")
        {
            UpsertObservationOutcome::Stored(row) => row,
            other => panic!("expected Stored, got {other:?}"),
        }
    }

    #[test]
    fn fresh_row_defaults_to_unverified_with_empty_pin() {
        let (_dir, mut db) = open_temp_db();
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let row = seed_peer(&mut repo, "peer-aaaa", datetime!(2026-01-02 03:04:05 UTC));
        assert_eq!(row.trust_state, TrustState::Unverified);
        assert_eq!(row.tls_cert_fingerprint, "");
        assert_eq!(row.paired_at, "");
        assert_eq!(row.paired_protocol_major, 0);
    }

    #[test]
    fn mark_trusted_pins_fingerprint_and_protocol_major() {
        let (_dir, mut db) = open_temp_db();
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = seed_peer(&mut repo, "peer-aaaa", datetime!(2026-01-02 03:04:05 UTC));
        let when = datetime!(2026-01-02 04:04:05 UTC);
        let outcome = repo
            .mark_trusted("peer-aaaa", "abcdef0123456789", when, 1)
            .expect("mark_trusted");
        let row = match outcome {
            TrustTransitionOutcome::Stored(row) => row,
            other => panic!("expected Stored, got {other:?}"),
        };
        assert_eq!(row.trust_state, TrustState::Trusted);
        assert_eq!(row.tls_cert_fingerprint, "abcdef0123456789");
        assert_eq!(row.paired_protocol_major, 1);
        assert!(!row.paired_at.is_empty());
    }

    #[test]
    fn mark_trusted_unknown_peer_returns_unknown() {
        let (_dir, mut db) = open_temp_db();
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let outcome = repo
            .mark_trusted(
                "peer-aaaa",
                "fingerprint",
                datetime!(2026-01-02 03:04:05 UTC),
                1,
            )
            .expect("mark_trusted");
        assert!(matches!(outcome, TrustTransitionOutcome::Unknown));
    }

    #[test]
    fn mark_trusted_blocked_peer_conflicts() {
        let (_dir, mut db) = open_temp_db();
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = seed_peer(&mut repo, "peer-aaaa", datetime!(2026-01-02 03:04:05 UTC));
        let _ = repo.mark_blocked("peer-aaaa").expect("block");
        let outcome = repo
            .mark_trusted(
                "peer-aaaa",
                "fingerprint",
                datetime!(2026-01-02 04:04:05 UTC),
                1,
            )
            .expect("mark_trusted on blocked");
        assert!(matches!(outcome, TrustTransitionOutcome::Conflict(_)));
        let row = repo.get("peer-aaaa").expect("reload").expect("present");
        assert_eq!(row.trust_state, TrustState::Blocked);
        assert_eq!(row.tls_cert_fingerprint, "");
    }

    #[test]
    fn mark_revoked_keeps_persisted_row_intact() {
        let (_dir, mut db) = open_temp_db();
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = seed_peer(&mut repo, "peer-aaaa", datetime!(2026-01-02 03:04:05 UTC));
        let outcome = repo.mark_revoked("peer-aaaa").expect("revoke");
        let row = match outcome {
            TrustTransitionOutcome::Stored(row) => row,
            other => panic!("expected Stored, got {other:?}"),
        };
        assert_eq!(row.trust_state, TrustState::Revoked);
        assert_eq!(repo.count().expect("count"), 1);
    }

    #[test]
    fn mark_blocked_blocks_subsequent_revoke() {
        let (_dir, mut db) = open_temp_db();
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = seed_peer(&mut repo, "peer-aaaa", datetime!(2026-01-02 03:04:05 UTC));
        let _ = repo.mark_blocked("peer-aaaa").expect("block");
        let outcome = repo.mark_revoked("peer-aaaa").expect("revoke on blocked");
        assert!(matches!(outcome, TrustTransitionOutcome::Conflict(_)));
    }

    #[test]
    fn unblock_clears_pin_and_returns_to_unverified() {
        let (_dir, mut db) = open_temp_db();
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = seed_peer(&mut repo, "peer-aaaa", datetime!(2026-01-02 03:04:05 UTC));
        let _ = repo.mark_trusted("peer-aaaa", "abcdef", datetime!(2026-01-02 03:04:05 UTC), 1);
        let _ = repo.mark_blocked("peer-aaaa").expect("block");
        let outcome = repo.unblock("peer-aaaa").expect("unblock");
        let row = match outcome {
            TrustTransitionOutcome::Stored(row) => row,
            other => panic!("expected Stored, got {other:?}"),
        };
        assert_eq!(row.trust_state, TrustState::Unverified);
        assert_eq!(row.tls_cert_fingerprint, "");
        assert_eq!(row.paired_at, "");
        assert_eq!(row.paired_protocol_major, 0);
    }

    #[test]
    fn unblock_refuses_revoked_rows() {
        let (_dir, mut db) = open_temp_db();
        let mut repo = KnownPeerRepository::new(db.connection_mut());
        let _ = seed_peer(&mut repo, "peer-aaaa", datetime!(2026-01-02 03:04:05 UTC));
        let _ = repo.mark_revoked("peer-aaaa").expect("revoke");
        let outcome = repo.unblock("peer-aaaa").expect("unblock revoked");
        assert!(matches!(outcome, TrustTransitionOutcome::Conflict(_)));
    }

    #[test]
    fn trust_state_as_str_round_trips_through_parse() {
        for (raw, expected) in [
            ("unverified", TrustState::Unverified),
            ("trusted", TrustState::Trusted),
            ("revoked", TrustState::Revoked),
            ("blocked", TrustState::Blocked),
            ("unknown", TrustState::Unverified),
        ] {
            assert_eq!(TrustState::parse(raw), expected);
            assert_eq!(
                expected.as_str(),
                if raw == "unknown" { "unverified" } else { raw }
            );
        }
    }
}
