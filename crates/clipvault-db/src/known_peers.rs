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
//! `last_discovered_at` and `updated_at`, and the public fields the
//! core re-validates (fingerprint, display name, protocol, capability).
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
/// the columns the `local-peer-discovery` migration adds; no field
/// carries endpoints, content or secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownPeer {
    pub peer_id: String,
    pub public_key_fingerprint: String,
    pub display_name: String,
    pub protocol_major: i64,
    pub capability: String,
    pub first_seen_at: String,
    pub last_discovered_at: String,
    pub updated_at: String,
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
    /// or capability reusing the same `peer_id`). The repository
    /// left the previously persisted row untouched and returned the
    /// existing record so the core can log the rejection without
    /// inspecting the conflicting bytes.
    Conflict(KnownPeer),
}

/// Fields the runtime validates and persists per observation. The
/// struct is `Clone` so the core can stage a draft before calling
/// [`KnownPeerRepository::upsert_observation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerObservation {
    pub peer_id: String,
    pub public_key_fingerprint: String,
    pub display_name: String,
    pub protocol_major: i64,
    pub capability: String,
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
    ///   name / protocol major / capability refreshes
    ///   `last_discovered_at` and `updated_at`; `first_seen_at` is
    ///   preserved.
    /// - A re-observed `peer_id` whose fingerprint, display name,
    ///   protocol major or capability diverges from the previously
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
                "SELECT peer_id, public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at \
                 FROM known_peers WHERE peer_id = ?1",
            )?;
            stmt.query_row(params![observation.peer_id], |row| {
                Ok(KnownPeer {
                    peer_id: row.get(0)?,
                    public_key_fingerprint: row.get(1)?,
                    display_name: row.get(2)?,
                    protocol_major: row.get(3)?,
                    capability: row.get(4)?,
                    first_seen_at: row.get(5)?,
                    last_discovered_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .optional()?
        };
        let outcome = match existing {
            None => {
                tx.execute(
                    "INSERT INTO known_peers \
                        (peer_id, public_key_fingerprint, display_name, protocol_major, \
                         capability, first_seen_at, last_discovered_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?6)",
                    params![
                        observation.peer_id,
                        observation.public_key_fingerprint,
                        observation.display_name,
                        observation.protocol_major,
                        observation.capability,
                        now,
                    ],
                )?;
                let row = tx
                    .query_row(
                        "SELECT peer_id, public_key_fingerprint, display_name, protocol_major, \
                                capability, first_seen_at, last_discovered_at, updated_at \
                         FROM known_peers WHERE peer_id = ?1",
                        params![observation.peer_id],
                        |row| {
                            Ok(KnownPeer {
                                peer_id: row.get(0)?,
                                public_key_fingerprint: row.get(1)?,
                                display_name: row.get(2)?,
                                protocol_major: row.get(3)?,
                                capability: row.get(4)?,
                                first_seen_at: row.get(5)?,
                                last_discovered_at: row.get(6)?,
                                updated_at: row.get(7)?,
                            })
                        },
                    )
                    .map_err(KnownPeersError::Sqlite)?;
                UpsertObservationOutcome::Stored(row)
            }
            Some(previous) => {
                if previous.public_key_fingerprint != observation.public_key_fingerprint
                    || previous.display_name != observation.display_name
                    || previous.protocol_major != observation.protocol_major
                    || previous.capability != observation.capability
                {
                    // Conflict: refuse to overwrite the trusted row.
                    // The core sees the conflict and surfaces it as
                    // a metadata-only rejection without logging the
                    // rejected bytes.
                    UpsertObservationOutcome::Conflict(previous)
                } else {
                    tx.execute(
                        "UPDATE known_peers \
                         SET last_discovered_at = ?1, updated_at = ?1 \
                         WHERE peer_id = ?2",
                        params![now, observation.peer_id],
                    )?;
                    let refreshed = tx
                        .query_row(
                            "SELECT peer_id, public_key_fingerprint, display_name, protocol_major, \
                                    capability, first_seen_at, last_discovered_at, updated_at \
                             FROM known_peers WHERE peer_id = ?1",
                            params![observation.peer_id],
                            |row| {
                                Ok(KnownPeer {
                                    peer_id: row.get(0)?,
                                    public_key_fingerprint: row.get(1)?,
                                    display_name: row.get(2)?,
                                    protocol_major: row.get(3)?,
                                    capability: row.get(4)?,
                                    first_seen_at: row.get(5)?,
                                    last_discovered_at: row.get(6)?,
                                    updated_at: row.get(7)?,
                                })
                            },
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
                "SELECT peer_id, public_key_fingerprint, display_name, protocol_major, \
                        capability, first_seen_at, last_discovered_at, updated_at \
                 FROM known_peers WHERE peer_id = ?1",
                params![peer_id],
                |row| {
                    Ok(KnownPeer {
                        peer_id: row.get(0)?,
                        public_key_fingerprint: row.get(1)?,
                        display_name: row.get(2)?,
                        protocol_major: row.get(3)?,
                        capability: row.get(4)?,
                        first_seen_at: row.get(5)?,
                        last_discovered_at: row.get(6)?,
                        updated_at: row.get(7)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// List every persisted peer, ordered by `last_discovered_at`
    /// descending so the most-recent observation is rendered first
    /// in the Equipos view.
    pub fn list(&self) -> Result<Vec<KnownPeer>, KnownPeersError> {
        let mut stmt = self.conn.prepare(
            "SELECT peer_id, public_key_fingerprint, display_name, protocol_major, \
                    capability, first_seen_at, last_discovered_at, updated_at \
             FROM known_peers ORDER BY last_discovered_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(KnownPeer {
                    peer_id: row.get(0)?,
                    public_key_fingerprint: row.get(1)?,
                    display_name: row.get(2)?,
                    protocol_major: row.get(3)?,
                    capability: row.get(4)?,
                    first_seen_at: row.get(5)?,
                    last_discovered_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })?
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
            display_name: display_name.to_string(),
            protocol_major,
            capability: capability.to_string(),
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
        assert_eq!(row.first_seen_at, row.last_discovered_at);
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
    fn upsert_rejects_a_conflicting_capability_without_overwriting() {
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
                1,
                "pairing",
                when,
            ))
            .expect("conflict insert");
        assert!(matches!(outcome, UpsertObservationOutcome::Conflict(_)));
        let reloaded = repo.get("peer-aaaa").expect("reload").expect("present");
        assert_eq!(reloaded.capability, "discovery_only");
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
}
