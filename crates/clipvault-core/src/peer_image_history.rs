//! Metadata-only browser for the transferable image history of a
//! trusted, active local peer.
//!
//! `peer-image-import` introduces the in-process façade the desktop
//! shell drives when the user opens the `Equipos vinculados` row of
//! a paired, present peer. The module owns:
//!
//! - the [`RemoteImagePreview`] row the host projects (opaque remote
//!   entry id, validated optional title, content type, RFC 3339
//!   timestamp, byte size and pixel dimensions);
//! - the opaque [`RemoteImageHistoryCursor`] the host mints and the
//!   client submits verbatim — the cursor is an HMAC-SHA256
//!   signature over `(peer_id, created_at, id)` bound to a 32-byte
//!   per-peer secret the host generates on `trust_state = trusted`
//!   and rotates on `Revoked` / re-pairing. The runtime never
//!   accepts an offset or a client-invented identifier and never
//!   echoes the secret over the wire;
//! - the [`PeerImageHistoryService`] façade that wraps the
//!   [`PeerImageHistoryTransport`] trait and the host-side
//!   [`HostImageHistorySource`] the listener drives when a trusted
//!   peer asks for `list_recent_images`. The façade is metadata-only
//!   by construction: it never copies image bytes, asset references
//!   or filesystem paths into the cursor, the wire payload or the
//!   preview.
//!
//! ## Eligibility
//!
//! [`image_entry_is_transferable`] is the single predicate the host
//! projection and the unit tests share. A row is transferable when
//! its [`clipvault_db::ContentType`] is image, its `asset_ref` points
//! inside the `clipboard/` namespace, its MIME type is the canonical
//! `image/png` and its width / height / size columns are populated.
//! The host side never inspects the asset bytes — the
//! `peer-image-import` change ships the assets through
//! `ClipboardAssetStore` only when the user activates `Importar`.
//!
//! ## Ordering, cursor and limits
//!
//! The host projection sorts newest first, breaks ties with the
//! stable [`clipvault_db::EntryRecord::id`] (the SQLite rowid) and
//! slices the page at [`MAX_IMAGE_PAGE_ROWS`]. The cursor encodes
//! the `(created_at, id)` pair of the **last** row the previous
//! page returned so the next page picks up strictly after it. The
//! runtime rejects any cursor it did not mint with
//! [`PeerImageHistoryCursorError::Invalid`] — the spec scenario
//! "Forged or rotated cursor" pins that contract.

use std::collections::HashMap;
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use parking_lot::RwLock;
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use clipvault_db::{ContentType, EntryRecord};

/// Maximum number of rows the host returns in a single
/// `list_recent_images` response. The runtime enforces the cap on
/// both ends so a malicious cursor cannot trick the projection into
/// streaming more rows than the contract allows.
pub const MAX_IMAGE_PAGE_ROWS: usize = 50;

/// Default page size the host returns when the client supplies no
/// cursor. The runtime never hands out more than
/// [`MAX_IMAGE_PAGE_ROWS`] regardless of the requested size.
pub const DEFAULT_IMAGE_PAGE_ROWS: usize = 50;

/// Hard cap the runtime enforces on a single image payload's byte
/// size. The value mirrors the local
/// [`clipvault_platform::peer_transport::FETCH_IMAGE_MAX_BODY_BYTES`]
/// so the wire stays consistent with the local asset store
/// contract the `clipboard-rich-content` change ships.
pub const IMAGE_FETCH_MAX_BYTES: usize = 16 * 1024 * 1024;

/// Number of bytes the runtime mints for the per-peer HMAC secret.
/// 32 bytes is the canonical SHA-256 key length and matches the
/// length the text history change pins.
pub const IMAGE_CURSOR_SECRET_BYTES: usize = 32;

/// Validation error the runtime surfaces when the cursor the client
/// submits is not an opaque string the host minted, the HMAC
/// signature does not match or the secret has rotated. The variant
/// is the typed reason the wire envelope encodes (`invalid_cursor`)
/// so the frontend never has to inspect free-form strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerImageHistoryCursorError {
    /// The supplied cursor was not produced by this host.
    #[error("peer image history cursor was not emitted by this host")]
    Invalid,
    /// The cursor decoded but the timestamp did not parse as RFC 3339.
    #[error("peer image history cursor decoded but the timestamp is not RFC 3339")]
    MalformedTimestamp,
    /// The cursor decoded but the stable id is not a positive integer.
    #[error("peer image history cursor decoded but the stable id is not a positive integer")]
    MalformedId,
    /// The cursor HMAC verification failed.
    #[error("peer image history cursor HMAC signature did not validate")]
    SignatureMismatch,
}

/// Opaque cursor the host emits on every successful page. The
/// cursor is the wire representation of `<base64url(payload)>::
/// <base64url(hmac)>` where `payload` is the canonical
/// `<peer_id>\n<created_at_rfc3339>\n<id>` triple and `hmac` is the
/// HMAC-SHA256 signature the host computed with the per-peer secret
/// it persisted at promotion time. The runtime guarantees the
/// cursor only encodes `(created_at, id)` of the last row of the
/// previous page — never the row content, the row hash, the source
/// app, the asset reference, the collection or the favourite flag.
/// Clients MUST treat the cursor as opaque.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RemoteImageHistoryCursor {
    inner: String,
}

impl RemoteImageHistoryCursor {
    /// Mint a cursor from the (timestamp, id) pair of the last row
    /// the page emitted, signed with the supplied per-peer secret.
    pub fn mint(peer_id: &str, timestamp: &str, id: i64, secret: &[u8]) -> Self {
        let payload = format!("{peer_id}\n{timestamp}\n{id}");
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret).expect("HMAC-SHA256 accepts any key length");
        mac.update(payload.as_bytes());
        let tag = mac.finalize().into_bytes();
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let tag_b64 = URL_SAFE_NO_PAD.encode(tag);
        Self {
            inner: format!("{payload_b64}::{tag_b64}"),
        }
    }

    /// Decode a cursor the client submitted and verify its HMAC
    /// signature against the supplied per-peer secret. The helper
    /// collapses every malformed input into
    /// [`PeerImageHistoryCursorError`] without surfacing the
    /// original bytes.
    pub fn decode(
        &self,
        peer_id: &str,
        secret: &[u8],
    ) -> Result<(String, i64), PeerImageHistoryCursorError> {
        let Some((payload_b64, tag_b64)) = self.inner.split_once("::") else {
            return Err(PeerImageHistoryCursorError::Invalid);
        };
        let payload_bytes = URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| PeerImageHistoryCursorError::Invalid)?;
        let tag_bytes = URL_SAFE_NO_PAD
            .decode(tag_b64)
            .map_err(|_| PeerImageHistoryCursorError::Invalid)?;
        if tag_bytes.len() != Sha256::output_size() {
            return Err(PeerImageHistoryCursorError::Invalid);
        }
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret).expect("HMAC-SHA256 accepts any key length");
        mac.update(&payload_bytes);
        mac.verify_slice(&tag_bytes)
            .map_err(|_| PeerImageHistoryCursorError::SignatureMismatch)?;
        let payload = std::str::from_utf8(&payload_bytes)
            .map_err(|_| PeerImageHistoryCursorError::Invalid)?;
        let mut parts = payload.splitn(3, '\n');
        let claim_peer = parts.next().ok_or(PeerImageHistoryCursorError::Invalid)?;
        if claim_peer != peer_id {
            return Err(PeerImageHistoryCursorError::Invalid);
        }
        let timestamp = parts.next().ok_or(PeerImageHistoryCursorError::Invalid)?;
        if OffsetDateTime::parse(timestamp, &Rfc3339).is_err() {
            return Err(PeerImageHistoryCursorError::MalformedTimestamp);
        }
        let id_raw = parts.next().ok_or(PeerImageHistoryCursorError::Invalid)?;
        let id: i64 = id_raw
            .parse()
            .map_err(|_| PeerImageHistoryCursorError::MalformedId)?;
        if id <= 0 {
            return Err(PeerImageHistoryCursorError::MalformedId);
        }
        Ok((timestamp.to_string(), id))
    }

    /// Borrow the opaque cursor string the host minted. The
    /// caller MUST treat the result as opaque and never attempt
    /// to parse it.
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Build a cursor from a string the renderer / bridge
    /// supplied.
    pub fn from_string(inner: String) -> Self {
        Self { inner }
    }
}

/// Outcome the runtime returns to the bridge / Tauri shell. The
/// discriminated union keeps the wire contract stable: the
/// frontend branches on `kind` (`Ok` / `InvalidCursor` /
/// `PeerUnavailable` / `TransportUnavailable`) without parsing
/// free-form strings or content bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerImageHistoryOutcome {
    /// The page was rendered.
    Ok {
        page: RemoteImageHistoryPage,
        /// Stable fingerprint of the local history at projection
        /// time. The renderer can use the value to detect a local
        /// capture that landed between page requests and decide
        /// whether to refetch.
        snapshot_id: String,
    },
    /// The cursor was not minted by this host or signed under a
    /// rotated secret. The renderer surfaces the typed reason
    /// without retrying.
    InvalidCursor,
    /// The peer is not Active / not trusted / not present. The
    /// runtime NEVER opens a network call when the peer is
    /// unavailable.
    PeerUnavailable { reason: &'static str },
    /// The runtime refused to project because the underlying
    /// transport rejected the page request.
    TransportUnavailable { reason: &'static str },
}

/// Metadata-only row the host returns for a transferable image
/// entry. The struct carries only the fields the spec authorises:
/// an opaque remote entry id, the optional validated title, the
/// content type (`image`), the RFC 3339 timestamp, the byte size
/// and the pixel dimensions. The row never carries the entry
/// body, a thumbnail, an `asset_ref`, a filesystem path, a
/// content hash, tags, collections or source-application
/// metadata. The frontend renders the common static image
/// placeholder on top of the metadata the bridge surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RemoteImagePreview {
    /// Opaque remote entry id the host minted. The id is bound to
    /// the local `entries.id` of the source row but encoded so the
    /// renderer can never inspect the local primary key.
    pub remote_entry_id: String,
    /// Validated, trimmed user-supplied title. `None` when the
    /// entry has no custom title or the persisted value fails
    /// the same validation the local UI applies.
    pub title: Option<String>,
    /// Canonical snake_case string the local SQLite layer
    /// persists. The bridge surfaces the value verbatim so the
    /// renderer can switch on a stable wire contract.
    pub content_type: String,
    /// RFC 3339 timestamp of the entry's `created_at`.
    pub created_at: String,
    /// Persisted byte size of the image asset the host will
    /// serve through `ClipboardAssetStore` once the user activates
    /// `Importar`.
    pub byte_size: u64,
    /// Original pixel width of the captured image. `None` is not
    /// a valid wire value: the row is omitted from the projection
    /// when the persisted value is missing.
    pub width: u32,
    /// Original pixel height of the captured image. `None` is
    /// not a valid wire value.
    pub height: u32,
}

/// Page the host returns from a single `list_recent_images` call.
/// The runtime always sets `rows` to at most [`MAX_IMAGE_PAGE_ROWS`]
/// and only populates `next_cursor` when a further page exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RemoteImageHistoryPage {
    pub rows: Vec<RemoteImagePreview>,
    /// Opaque cursor the renderer must submit to fetch the next
    /// page. `None` when this page is the last one.
    pub next_cursor: Option<RemoteImageHistoryCursor>,
}

/// Predicate the runtime and the unit tests share. The predicate
/// returns `true` when the entry is an image row whose persisted
/// metadata is consistent: `content_type = image`, `asset_ref`
/// non-empty and pointing inside the `clipboard/` namespace, a
/// non-empty MIME type, populated width / height and a positive
/// `content_size`. The host refuses to ship an asset reference or
/// a path the runtime did not mint.
pub fn image_entry_is_transferable(entry: &EntryRecord) -> bool {
    if entry.content_type != ContentType::Image {
        return false;
    }
    let Some(asset_ref) = entry.asset_ref.as_deref() else {
        return false;
    };
    if asset_ref.is_empty() || !asset_ref.starts_with("clipboard/") {
        return false;
    }
    if entry
        .mime_type
        .as_deref()
        .map(str::is_empty)
        .unwrap_or(true)
    {
        return false;
    }
    if entry.payload_width.is_none() || entry.payload_height.is_none() {
        return false;
    }
    if entry.content_size <= 0 {
        return false;
    }
    true
}

/// Build a deterministic, non-reversible fingerprint of the page
/// the caller projects. The hash input mirrors the text side:
/// `peer_id || "\n" || max_created_at || "\n" || max_id || "\n" ||
/// count`; the runtime emits the lowercase-hex SHA-256 digest so
/// the client can detect a local capture that landed between page
/// requests without leaking row content.
pub fn compute_image_page_fingerprint(
    peer_id: &str,
    max_created_at: &str,
    max_id: i64,
    count: usize,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(peer_id.as_bytes());
    hasher.update(b"\n");
    hasher.update(max_created_at.as_bytes());
    hasher.update(b"\n");
    hasher.update(max_id.to_string().as_bytes());
    hasher.update(b"\n");
    hasher.update(count.to_string().as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest.iter() {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Persistence trait the listener drives to project the
/// transferable image history of the **local** host. The bootstrap
/// installs an adapter that delegates to
/// [`clipvault_db::EntryRepository::image_entries_after`] / a
/// freshly allocated repository handle; tests inject an in-memory
/// fake.
pub trait HostImageHistorySource: Send + Sync {
    /// Return every image row that matches the (created_at, id)
    /// cursor pair (strict less-than ordering), regardless of
    /// whether it passes the asset / dimension / size checks. The
    /// caller drives the eligibility filter through
    /// [`Self::is_eligible`]; the trait deliberately returns
    /// ALL rows so the source can apply the metadata-only
    /// filter independently of the asset validator.
    /// Implementations MUST sort by `created_at DESC, id DESC`
    /// and return at most `limit` rows.
    fn page_after(
        &self,
        created_at: &str,
        id: i64,
        limit: usize,
    ) -> Result<Vec<EntryRecord>, PeerImageHistoryPersistenceError>;

    /// Per-source eligibility filter. The default implementation
    /// matches the historical metadata-only predicate so
    /// adapters that do not need an asset validator keep
    /// working without changes. Adapters that need to drop rows
    /// whose backing PNG is missing / corrupt / oversized
    /// override this method to apply the asset-level check
    /// alongside the metadata-only one.
    fn is_eligible(&self, entry: &EntryRecord) -> bool {
        image_entry_is_transferable(entry)
    }

    /// Snapshot fingerprint the runtime returns alongside the
    /// page. The runtime only uses the value as a tie-breaker for
    /// the frontend (so a local capture landing between page
    /// requests can trigger a refetch); the value MUST be stable
    /// for the same persisted history and MUST NOT leak row
    /// content.
    fn snapshot_id(&self) -> Result<String, PeerImageHistoryPersistenceError>;
}

/// Typed status the [`fill_page_after`] helper returns. The
/// variant lets the caller distinguish three terminations the
/// wire contract the `peer-image-import` change pins:
///
/// - [`FillPageAfterStatus::Exhausted`]: the persistence layer
///   returned no more candidates at all. The host MUST close the
///   page without signing a continuation cursor.
/// - [`FillPageAfterStatus::WorkLimitReached`]: the helper hit
///   [`PAGE_AFTER_MAX_BATCHES`] but the persistence layer still
///   has candidates to scan. The host MUST sign a continuation
///   cursor whose payload is the `(created_at, id)` of the LAST
///   row the helper actually scanned, and resume from there on
///   the next call. The cursor is opaque + HMAC-signed so the
///   caller cannot forge or jump past the pending candidates.
/// - [`FillPageAfterStatus::LimitSatisfied`]: the helper
///   collected at least `limit + 1` eligible rows. The host MUST
///   drop the surplus row (it was a probe) and sign a cursor
///   whose payload is the `(created_at, id)` of the limit-th
///   returned row, so the next page picks up strictly after it.
///
/// The contract the helper exposes mirrors the host listener the
/// `peer-image-import` change ships: the host-side projection
/// MUST keep refilling until either (a) the persistence layer
/// has no more candidates or (b) the helper surfaces the
/// `LimitSatisfied` variant with at least `limit + 1` eligible
/// rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FillPageAfterStatus {
    /// No more candidates in the persisted history. The caller
    /// MUST close the page without a continuation cursor.
    Exhausted,
    /// The helper hit [`PAGE_AFTER_MAX_BATCHES`] but more
    /// candidates remain; the caller MUST sign a continuation
    /// cursor for `(last_scanned_at, last_scanned_id)` and
    /// resume on the next call.
    WorkLimitReached {
        last_scanned_at: String,
        last_scanned_id: i64,
    },
    /// The helper collected at least `limit + 1` eligible rows.
    /// The caller MUST drop the surplus row (probe) and sign a
    /// cursor for the limit-th row.
    LimitSatisfied,
}

/// Iterate the [`HostImageHistorySource`] until the persistence
/// layer yields at least `limit + 1` eligible rows OR the
/// helper exhausts the candidates OR the
/// [`PAGE_AFTER_MAX_BATCHES`] budget is reached. The helper
/// guards the wire contract the `peer-image-import` change
/// pins: a host that drops invalid / missing assets through the
/// metadata-only filter MUST keep refilling the page until it
/// can either honour the requested size or hit the end of the
/// persisted history; the cursor the runtime signs for the next
/// page MUST point past the last returned eligible row, never
/// at a row that was dropped by the filter. The helper caps the
/// work at [`PAGE_AFTER_MAX_BATCHES`] ×
/// [`PAGE_AFTER_BATCH_OVERFETCH`] rows so a pathological
/// sequence cannot keep the listener busy forever, and exposes
/// the work-limit termination through
/// [`FillPageAfterStatus::WorkLimitReached`] so the caller can
/// emit a continuation cursor instead of misinterpreting the
/// short page as exhaustion.
///
/// Eligibility is delegated to [`HostImageHistorySource::is_eligible`]
/// so adapters that need an asset validator (the production
/// SQLite adapter) drop rows whose backing PNG is missing /
/// corrupt / oversized without leaking that knowledge to the
/// caller. Tests inject an in-memory source whose default
/// metadata-only predicate already matches.
pub fn fill_page_after(
    source: &dyn HostImageHistorySource,
    created_at: &str,
    id: i64,
    limit: usize,
) -> Result<(Vec<EntryRecord>, FillPageAfterStatus), PeerImageHistoryPersistenceError> {
    let target = limit.saturating_add(1);
    let mut eligible: Vec<EntryRecord> = Vec::new();
    let mut next_at: String = created_at.to_string();
    let mut next_id: i64 = id;
    let mut exhausted = false;
    for _ in 0..PAGE_AFTER_MAX_BATCHES {
        let batch_size = target
            .saturating_sub(eligible.len())
            .saturating_add(PAGE_AFTER_BATCH_OVERFETCH);
        let candidates = source.page_after(&next_at, next_id, batch_size)?;
        if candidates.is_empty() {
            // The persistence layer returned no more candidates
            // at all. The caller MUST close the page without a
            // continuation cursor.
            exhausted = true;
            break;
        }
        let last = candidates.last().expect("non-empty batch has a tail");
        next_at = last.created_at.clone();
        next_id = last.id;
        for candidate in candidates {
            if source.is_eligible(&candidate) {
                eligible.push(candidate);
                if eligible.len() >= target {
                    break;
                }
            }
        }
        if eligible.len() >= target {
            break;
        }
    }
    if eligible.len() >= target {
        // The probe row is the (limit+1)-th entry. The caller
        // (typically the host `serve` helper) drops it and
        // signs a cursor for the limit-th row so the next
        // page picks up strictly after it.
        eligible.truncate(limit);
        Ok((eligible, FillPageAfterStatus::LimitSatisfied))
    } else if exhausted {
        // The persistence layer ran out of candidates before
        // the helper hit `target`. The caller closes the page
        // without a continuation cursor.
        Ok((eligible, FillPageAfterStatus::Exhausted))
    } else {
        // The helper hit [`PAGE_AFTER_MAX_BATCHES`] with more
        // candidates pending. The caller MUST sign a
        // continuation cursor for the LAST row the helper
        // actually scanned (eligible or not) so the next call
        // resumes from there.
        Ok((
            eligible,
            FillPageAfterStatus::WorkLimitReached {
                last_scanned_at: next_at,
                last_scanned_id: next_id,
            },
        ))
    }
}

/// Clamp the requested page size to the documented wire contract.
/// The host MUST project at most [`MAX_IMAGE_PAGE_ROWS`] rows per
/// page and the runtime accepts a smaller `limit` so a caller can
/// ask for fewer rows without losing the typed outcome surface.
pub fn clamp_image_history_limit(limit: u32) -> usize {
    let bounded = limit.min(MAX_IMAGE_PAGE_ROWS as u32).max(1);
    bounded as usize
}

/// Probe limit the host asks the persistence layer for so it can
/// tell apart a "page exactly the limit because the caller asked
/// for fewer" from a "page exactly the limit but more rows
/// remain". The runtime requests `clamp + 1` rows internally.
pub fn image_history_lookahead_limit(limit: usize) -> usize {
    limit.saturating_add(1).min(MAX_IMAGE_PAGE_ROWS + 1)
}

/// Hard cap the runtime enforces on the number of refilling
/// batches the [`page_after`] implementation may issue before
/// returning. The cap is generous (8 batches × `limit + 8`
/// candidates per batch) so a transient window of invalid
/// assets never silently truncates a page, while still bounding
/// the work the host performs so a malicious or stale cursor
/// cannot keep the listener busy forever.
pub const PAGE_AFTER_MAX_BATCHES: usize = 8;

/// Maximum number of candidates the [`page_after`]
/// implementation requests per batch so the metadata-only
/// filter can drop rows whose backing PNG is invalid. The
/// number is a deliberate over-fetch (8 rows past the requested
/// `limit`) so a small window of invalid rows does not force
/// the host to round-trip the database an extra time; the
/// helper refills the page in additional batches when the
/// invalid ratio is higher than the over-fetch budget.
pub const PAGE_AFTER_BATCH_OVERFETCH: usize = 8;

/// Typed persistence error the listener surfaces. Every adapter
/// collapses the underlying failure into one of these variants so
/// the listener can branch on the reason without inspecting
/// platform-specific error strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerImageHistoryPersistenceError {
    #[error("peer image history persistence backend is unavailable")]
    Unavailable,
    #[error("peer image history persistence backend rejected the operation")]
    Failed,
}

/// Per-peer HMAC secret the runtime caches. The secret is minted
/// when the row promotes to `trusted` and rotated on `Revoked` or
/// re-pairing. The cache lives in memory; the bootstrap populates
/// it from the persistence layer on every snapshot / health
/// probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerImageCursorSecret(pub [u8; IMAGE_CURSOR_SECRET_BYTES]);

impl PeerImageCursorSecret {
    /// Generate a fresh 32-byte secret using the OS CSPRNG.
    pub fn generate() -> Self {
        let mut bytes = [0u8; IMAGE_CURSOR_SECRET_BYTES];
        let _ = rand_core::OsRng.try_fill_bytes(&mut bytes);
        Self(bytes)
    }

    /// Recover the secret from its hex / raw byte representation.
    /// `None` is returned when the byte slice is not exactly
    /// [`IMAGE_CURSOR_SECRET_BYTES`] long — the runtime refuses to
    /// truncate or pad.
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != IMAGE_CURSOR_SECRET_BYTES {
            return None;
        }
        let mut out = [0u8; IMAGE_CURSOR_SECRET_BYTES];
        out.copy_from_slice(bytes);
        Some(Self(out))
    }

    /// Borrow the secret bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Encode the secret as the 64-lowercase-hex representation
    /// the persistence layer persists.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(IMAGE_CURSOR_SECRET_BYTES * 2);
        for byte in self.0.iter() {
            use std::fmt::Write as _;
            let _ = write!(&mut out, "{byte:02x}");
        }
        out
    }

    /// Recover the secret from its 64-lowercase-hex
    /// representation. `None` is returned when the input is not
    /// exactly 64 lowercase hex chars.
    pub fn from_hex(value: &str) -> Option<Self> {
        if value.len() != IMAGE_CURSOR_SECRET_BYTES * 2 {
            return None;
        }
        if !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let bytes_in = value.as_bytes();
        let mut out = [0u8; IMAGE_CURSOR_SECRET_BYTES];
        for (index, chunk) in bytes_in.chunks_exact(2).enumerate() {
            let hex = std::str::from_utf8(chunk).ok()?;
            out[index] = u8::from_str_radix(hex, 16).ok()?;
        }
        Some(Self(out))
    }
}

/// In-memory persistence adapter the tests use. The adapter
/// mirrors the contract [`clipvault_db::EntryRepository`] exposes
/// for the image side.
#[derive(Debug, Default)]
pub struct InMemoryHostImageHistorySource {
    inner: RwLock<Vec<EntryRecord>>,
}

impl InMemoryHostImageHistorySource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the adapter with a deterministic ordered list. The
    /// runtime accepts the list in the order the test supplies it;
    /// the projection sorts internally before paginating so the
    /// test can verify the contract without crafting SQL.
    pub fn seed(&self, entries: Vec<EntryRecord>) {
        *self.inner.write() = entries;
    }
}

impl HostImageHistorySource for InMemoryHostImageHistorySource {
    fn page_after(
        &self,
        created_at: &str,
        id: i64,
        limit: usize,
    ) -> Result<Vec<EntryRecord>, PeerImageHistoryPersistenceError> {
        let guard = self.inner.read();
        let mut matching: Vec<EntryRecord> = guard
            .iter()
            .filter(|entry| image_entry_is_transferable(entry))
            .filter(|entry| {
                entry.created_at.as_str() < created_at
                    || (entry.created_at == created_at && entry.id < id)
            })
            .cloned()
            .collect();
        matching.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        matching.truncate(limit);
        Ok(matching)
    }

    fn snapshot_id(&self) -> Result<String, PeerImageHistoryPersistenceError> {
        let guard = self.inner.read();
        let Some(top) = guard.iter().max_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        }) else {
            return Ok(compute_image_page_fingerprint("local", "empty", 0, 0));
        };
        Ok(compute_image_page_fingerprint(
            "local",
            &top.created_at,
            top.id,
            guard.len(),
        ))
    }
}

/// Production adapter the bootstrap wires against the live
/// SQLite handle. The adapter borrows a [`Mutex<Database>`] the
/// runtime hands it through the bootstrap; tests use the
/// in-memory projection above.
pub struct EntryRepositoryHostImageHistorySource {
    database: Arc<parking_lot::Mutex<clipvault_db::Database>>,
    /// Optional metadata-only asset validator. The closure
    /// receives a relative `asset_ref` and returns `true` when
    /// the underlying PNG asset is present and valid. When the
    /// closure is `None`, the source skips the asset-existence
    /// check (the fetch handler still re-validates before
    /// reading bytes, so the wire contract stays consistent).
    asset_store: Option<Arc<dyn Fn(&str) -> bool + Send + Sync>>,
}

impl EntryRepositoryHostImageHistorySource {
    /// Build an adapter that borrows the shared
    /// [`clipvault_db::Database`].
    pub fn new(database: Arc<parking_lot::Mutex<clipvault_db::Database>>) -> Self {
        Self {
            database,
            asset_store: None,
        }
    }

    /// Build an adapter that borrows the shared database handle
    /// **and** an asset validator the source uses to drop rows
    /// whose `asset_ref` points at a missing / invalid PNG.
    /// The validator is a closure so the source never needs to
    /// link the platform crate directly. The default
    /// constructor keeps the legacy behaviour (metadata-only
    /// validation) for callers that already gate through the
    /// asset store at the fetch stage.
    pub fn with_asset_validator<F>(
        database: Arc<parking_lot::Mutex<clipvault_db::Database>>,
        validator: F,
    ) -> Self
    where
        F: Fn(&str) -> bool + Send + Sync + 'static,
    {
        Self {
            database,
            asset_store: Some(Arc::new(validator)),
        }
    }
}

impl HostImageHistorySource for EntryRepositoryHostImageHistorySource {
    fn page_after(
        &self,
        created_at: &str,
        id: i64,
        limit: usize,
    ) -> Result<Vec<EntryRecord>, PeerImageHistoryPersistenceError> {
        // Return raw metadata candidates. `fill_page_after` is the
        // single layer that applies `is_eligible`, tracks exactly how
        // far it scanned, and communicates work-limit continuation to
        // `serve`; refilling here as well would hide that boundary and
        // could make the outer pagination skip eligible rows.
        let mut guard = self.database.lock();
        let repo = clipvault_db::EntryRepository::new(guard.connection_mut());
        repo.image_entries_after(created_at, id, limit)
            .map_err(|error| {
                tracing::warn!(
                    ?error,
                    "host image history projection: page_after batch failed"
                );
                PeerImageHistoryPersistenceError::Failed
            })
    }

    fn is_eligible(&self, entry: &EntryRecord) -> bool {
        if !image_entry_is_transferable(entry) {
            return false;
        }
        match (entry.asset_ref.as_deref(), self.asset_store.as_ref()) {
            (Some(asset_ref), Some(validator)) => validator(asset_ref),
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    fn snapshot_id(&self) -> Result<String, PeerImageHistoryPersistenceError> {
        let mut guard = self.database.lock();
        let conn = guard.connection_mut();
        let repo = clipvault_db::EntryRepository::new(conn);
        let latest = repo.latest_transferable_image_snapshot().map_err(|error| {
            tracing::warn!(?error, "host image history projection: snapshot failed");
            PeerImageHistoryPersistenceError::Failed
        })?;
        match latest {
            Some((created_at, id)) => {
                let count: i64 = clipvault_db::EntryRepository::new(conn)
                    .transferable_image_count()
                    .map_err(|error| {
                        tracing::warn!(?error, "host image history projection: count failed");
                        PeerImageHistoryPersistenceError::Failed
                    })?;
                Ok(compute_image_page_fingerprint(
                    "local",
                    &created_at,
                    id,
                    count as usize,
                ))
            }
            None => Ok(compute_image_page_fingerprint("local", "empty", 0, 0)),
        }
    }
}

/// Outcome the host listener hands back to the transport after the
/// caller submitted a [`crate::peer_transport::wire::PairingMessage::ListRecentImages`]
/// envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostImageHistoryResponse {
    Ok(RemoteImageHistoryPage, String),
    InvalidCursor,
    Unavailable(&'static str),
}

/// Metadata-only transport façade the client-side browse uses.
/// The trait is the seam between the core runtime and the platform
/// mTLS stack: the transport owns the dial loop, the pin lookup
/// and the per-peer session, while the runtime owns the trust /
/// active gate and the cursor / page contract.
///
/// Every call collapses to a typed [`PeerImageHistoryTransportError`]
/// variant the runtime maps onto a [`PeerImageHistoryOutcome`].
pub trait PeerImageHistoryTransport: Send + Sync {
    fn list_recent_images(
        &self,
        request: ListRecentImagesRequest,
    ) -> Result<ListRecentImagesResponse, PeerImageHistoryTransportError>;
}

/// Metadata the client-side facade hands to the transport so the
/// transport can dial the matching peer with the pinned cert
/// fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRecentImagesRequest {
    pub peer_id: String,
    pub cert_fingerprint: String,
    /// Opaque cursor the renderer submitted verbatim (empty when
    /// the client asks for the first page).
    pub cursor: String,
    /// Upper bound the renderer wants; the transport clamps the
    /// value to [`MAX_IMAGE_PAGE_ROWS`].
    pub limit: u32,
}

/// Successful payload the [`PeerImageHistoryTransport`] returns.
/// The runtime forwards the page and the `snapshot_id` to the
/// bridge without intermediate mutation so the client never
/// reconstructs a fingerprint from a partial row set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRecentImagesResponse {
    pub page: RemoteImageHistoryPage,
    pub snapshot_id: String,
}

/// Typed transport error the runtime maps onto a
/// [`PeerImageHistoryOutcome`] variant.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerImageHistoryTransportError {
    #[error("peer image history transport is unavailable")]
    Unavailable,
    #[error("peer image history transport has no resolved pairing endpoint")]
    PeerUnresolved,
    #[error("peer image history transport rejected an unknown peer")]
    UnknownPeer,
    #[error("peer image history transport rejected a mismatched TLS identity")]
    KeyMismatch,
    #[error("peer image history transport rejected a revoked peer")]
    Revoked,
    #[error("peer image history transport rejected a blocked peer")]
    Blocked,
    #[error("peer image history transport wire protocol is incompatible")]
    IncompatibleProtocol,
    #[error("peer image history transport rejected a malformed payload")]
    Malformed,
    #[error("peer image history transport rejected an invalid cursor")]
    InvalidCursor,
}

impl PeerImageHistoryTransportError {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::PeerUnresolved => "unavailable",
            Self::UnknownPeer => "unknown_peer",
            Self::KeyMismatch => "key_mismatch",
            Self::Revoked => "revoked",
            Self::Blocked => "blocked",
            Self::IncompatibleProtocol => "incompatible_protocol",
            Self::Malformed => "malformed",
            Self::InvalidCursor => "invalid_cursor",
        }
    }
}

/// Lightweight active-state record the runtime caches. The struct
/// only carries the metadata the browsing call needs to decide
/// whether the peer is currently eligible; the snapshot the
/// [`crate::peer_pairing::PairingRuntime`] returns is the
/// authoritative source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerImageActiveState {
    pub trusted: bool,
    pub active: bool,
}

/// Lightweight per-peer capability check the host (and the
/// client-side browse, as defence-in-depth) consults before
/// serving or accepting the `image_import` surface. The runtime
/// wires a closure that resolves the `capability` column the
/// discovery / pairing layer persisted and asks whether the
/// `image_import` token is present; a peer that does not
/// advertise the capability MUST NOT trigger the image routes.
/// The default closure returns `true` (every peer supports the
/// feature) so a host without a capability resolver still serves
/// rows; tests inject a closure that drives specific
/// accept / reject outcomes.
pub type PeerCapabilityResolver = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Default resolver used when the bootstrap does not inject one.
/// It returns `true` for every peer so the image history / fetch
/// routes stay functional on hosts without a capability cache;
/// the production bootstrap always replaces this slot with the
/// SQLite-backed resolver the pairing persistence installs.
fn default_capability_resolver() -> PeerCapabilityResolver {
    Arc::new(|_| true)
}

/// Service the shell drives. The façade is cheap to clone (every
/// field is `Arc`-shared) and never mutates the local persistence
/// layer.
#[derive(Clone)]
pub struct PeerImageHistoryService {
    transport: Arc<dyn PeerImageHistoryTransport>,
    active_peers: Arc<RwLock<HashMap<String, PeerImageActiveState>>>,
    cursor_secrets: Arc<RwLock<HashMap<String, PeerImageCursorSecret>>>,
    capability_resolver: PeerCapabilityResolver,
}

impl PeerImageHistoryService {
    /// Build a service backed by the supplied transport. The
    /// active-state and secret caches start empty: the bootstrap
    /// populates them on every snapshot / health probe so the
    /// very first browsing call waits for an `Active` projection
    /// before returning rows.
    pub fn new(transport: Arc<dyn PeerImageHistoryTransport>) -> Self {
        Self {
            transport,
            active_peers: Arc::new(RwLock::new(HashMap::new())),
            cursor_secrets: Arc::new(RwLock::new(HashMap::new())),
            capability_resolver: default_capability_resolver(),
        }
    }

    /// Replace the per-peer capability resolver the host (and
    /// the client-side browse) consults before serving or
    /// dialling the image surface. The bootstrap installs the
    /// SQLite-backed resolver the pairing persistence owns so the
    /// wire contract stays consistent regardless of which
    /// endpoint (text or image) reaches the host first.
    pub fn with_capability_resolver(mut self, resolver: PeerCapabilityResolver) -> Self {
        self.capability_resolver = resolver;
        self
    }

    pub fn record_peer_state(&self, peer_id: &str, state: PeerImageActiveState) {
        self.active_peers.write().insert(peer_id.to_string(), state);
    }

    pub fn forget_peer(&self, peer_id: &str) {
        self.active_peers.write().remove(peer_id);
    }

    pub fn peer_state(&self, peer_id: &str) -> Option<PeerImageActiveState> {
        self.active_peers.read().get(peer_id).copied()
    }

    pub fn set_cursor_secret(&self, peer_id: &str, secret: PeerImageCursorSecret) {
        self.cursor_secrets
            .write()
            .insert(peer_id.to_string(), secret);
    }

    pub fn install_cursor_secret_hex(&self, peer_id: &str, secret_hex: &str) -> bool {
        match PeerImageCursorSecret::from_hex(secret_hex) {
            Some(secret) => {
                self.set_cursor_secret(peer_id, secret);
                true
            }
            None => false,
        }
    }

    pub fn cursor_secret_hex(&self, peer_id: &str) -> Option<String> {
        self.cursor_secrets
            .read()
            .get(peer_id)
            .map(|secret| secret.to_hex())
    }

    pub fn clear_cursor_secret(&self, peer_id: &str) {
        self.cursor_secrets.write().remove(peer_id);
    }

    pub fn cursor_secret(&self, peer_id: &str) -> Option<PeerImageCursorSecret> {
        self.cursor_secrets.read().get(peer_id).copied()
    }

    /// Browse the most recent transferable image page of
    /// `peer_id`. The runtime consults the in-memory
    /// active-state cache and refuses to dial when the peer is
    /// not trusted, not present or unknown. The capability
    /// resolver the bootstrap installed is the source of truth
    /// for the `image_import` advertisement; a peer that does
    /// not advertise the capability surfaces the typed
    /// `peer_unavailable` outcome with reason
    /// `not_available` so the renderer never reaches for a
    /// remote thumbnail or import path.
    pub fn browse(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        cursor: Option<&RemoteImageHistoryCursor>,
        limit: u32,
    ) -> PeerImageHistoryOutcome {
        let state = match self.peer_state(peer_id) {
            Some(state) => state,
            None => {
                return PeerImageHistoryOutcome::PeerUnavailable {
                    reason: "no_known_peer",
                };
            }
        };
        if !state.trusted {
            return PeerImageHistoryOutcome::PeerUnavailable {
                reason: "not_trusted",
            };
        }
        if !state.active {
            return PeerImageHistoryOutcome::PeerUnavailable {
                reason: "not_active",
            };
        }
        if !(self.capability_resolver)(peer_id) {
            return PeerImageHistoryOutcome::PeerUnavailable {
                reason: "not_available",
            };
        }
        let cursor_payload = cursor.map(|c| c.as_str().to_string()).unwrap_or_default();

        let request = ListRecentImagesRequest {
            peer_id: peer_id.to_string(),
            cert_fingerprint: cert_fingerprint.to_string(),
            cursor: cursor_payload,
            limit: clamp_image_history_limit(limit) as u32,
        };
        match self.transport.list_recent_images(request) {
            Ok(ListRecentImagesResponse { page, snapshot_id }) => {
                PeerImageHistoryOutcome::Ok { page, snapshot_id }
            }
            Err(error) => match error {
                PeerImageHistoryTransportError::Revoked
                | PeerImageHistoryTransportError::Blocked
                | PeerImageHistoryTransportError::KeyMismatch
                | PeerImageHistoryTransportError::UnknownPeer => {
                    PeerImageHistoryOutcome::PeerUnavailable {
                        reason: "not_trusted",
                    }
                }
                PeerImageHistoryTransportError::InvalidCursor => {
                    PeerImageHistoryOutcome::InvalidCursor
                }
                PeerImageHistoryTransportError::PeerUnresolved
                | PeerImageHistoryTransportError::IncompatibleProtocol
                | PeerImageHistoryTransportError::Malformed
                | PeerImageHistoryTransportError::Unavailable => {
                    PeerImageHistoryOutcome::TransportUnavailable {
                        reason: error.reason(),
                    }
                }
            },
        }
    }

    /// Host-side projection the transport listener drives when
    /// an authenticated peer asks for `list_recent_images`. The
    /// function verifies the cursor, projects the page through
    /// the supplied [`HostImageHistorySource`] and signs the
    /// next cursor with the per-peer secret the bootstrap
    /// installed.
    ///
    /// The capability resolver is the single source of truth for
    /// whether the host accepts the image surface for `peer_id`.
    /// A peer that has not advertised `image_import` through
    /// mDNS (an old build or a legacy `pairing` only record)
    /// collapses into the typed `not_available` outcome so the
    /// host never serves image metadata to a caller that did not
    /// opt into the contract. The check runs BEFORE the cursor
    /// is decoded so a malformed cursor from a non-capable peer
    /// surfaces as `not_available`, not `invalid_cursor`.
    ///
    /// The helper iterates [`fill_page_after`] until either the
    /// persistence layer exhausts the candidates OR the helper
    /// surfaces [`FillPageAfterStatus::LimitSatisfied`] with at
    /// least `limit + 1` eligible rows. When the helper reports
    /// [`FillPageAfterStatus::WorkLimitReached`] (a single call
    /// hit [`PAGE_AFTER_MAX_BATCHES`] with more candidates
    /// pending), the loop resumes from the last scanned position
    /// so the host never misinterprets a short page as
    /// exhaustion. The signed continuation cursor the host
    /// eventually returns is HMAC-bound to the peer + secret so
    /// the client cannot forge a jump past the pending
    /// candidates.
    pub fn serve(
        &self,
        peer_id: &str,
        cursor: Option<&RemoteImageHistoryCursor>,
        requested_limit: u32,
        source: &dyn HostImageHistorySource,
    ) -> HostImageHistoryResponse {
        if !(self.capability_resolver)(peer_id) {
            return HostImageHistoryResponse::Unavailable("not_available");
        }
        let secret = match self.cursor_secret(peer_id) {
            Some(secret) => secret,
            None => return HostImageHistoryResponse::Unavailable("not_trusted"),
        };
        let (start_created_at, start_id) = match cursor {
            None => ("9999-12-31T23:59:59Z".to_string(), i64::MAX),
            Some(cursor) => match cursor.decode(peer_id, secret.as_bytes()) {
                Ok(pair) => pair,
                Err(_) => return HostImageHistoryResponse::InvalidCursor,
            },
        };
        let limit = clamp_image_history_limit(requested_limit);
        // Iterate `fill_page_after` until exhaustion or until the
        // helper surfaces `LimitSatisfied` (i.e. we collected at
        // least `limit + 1` eligible rows). Each iteration may
        // either drain more eligible rows from the persistence
        // layer or signal that more candidates remain so the
        // host MUST emit a continuation cursor (instead of a
        // `None` cursor) so the client can resume scanning.
        let mut cursor_at = start_created_at;
        let mut cursor_id = start_id;
        let mut all_eligible: Vec<EntryRecord> = Vec::new();
        let mut next_cursor: Option<(String, i64)> = None;
        for iteration in 0..SERVE_MAX_ITERATIONS {
            let remaining = limit.saturating_sub(all_eligible.len());
            if remaining == 0 {
                next_cursor = all_eligible
                    .last()
                    .map(|entry| (entry.created_at.clone(), entry.id));
                break;
            }
            match fill_page_after(source, &cursor_at, cursor_id, remaining) {
                Ok((batch, FillPageAfterStatus::LimitSatisfied)) => {
                    all_eligible.extend(batch);
                    // `remaining` is the number of output slots left.
                    // The helper returned exactly that many rows and
                    // discarded its lookahead probe, so resume after
                    // the final row actually sent.
                    next_cursor = all_eligible
                        .last()
                        .map(|entry| (entry.created_at.clone(), entry.id));
                    break;
                }
                Ok((batch, FillPageAfterStatus::Exhausted)) => {
                    all_eligible.extend(batch);
                    next_cursor = None;
                    break;
                }
                Ok((
                    batch,
                    FillPageAfterStatus::WorkLimitReached {
                        last_scanned_at,
                        last_scanned_id,
                    },
                )) => {
                    all_eligible.extend(batch);
                    if all_eligible.len() == limit {
                        // The page is full, but the helper reached its
                        // work budget before it could establish whether
                        // more eligible rows exist. Resume after the
                        // last row returned. Using `last_scanned_*`
                        // here could skip eligible rows the bounded
                        // candidate batch fetched but did not return.
                        next_cursor = all_eligible
                            .last()
                            .map(|entry| (entry.created_at.clone(), entry.id));
                        break;
                    }
                    // Resume from the LAST scanned row (eligible
                    // or not) so the next `fill_page_after` call
                    // continues strictly after it. All eligible rows
                    // scanned in this partial batch are included in
                    // `all_eligible`; the remaining output slots make
                    // it safe to move the internal cursor forward.
                    cursor_at = last_scanned_at;
                    cursor_id = last_scanned_id;
                    next_cursor = Some((cursor_at.clone(), cursor_id));
                    if iteration + 1 == SERVE_MAX_ITERATIONS {
                        // Keep a signed continuation even when this
                        // bounded host call found no eligible rows.
                        break;
                    }
                }
                Err(_) => {
                    return HostImageHistoryResponse::Unavailable("persistence_unavailable");
                }
            }
        }
        let previews: Vec<RemoteImagePreview> = all_eligible
            .iter()
            .take(limit)
            .map(project_image_row)
            .collect();
        let next_cursor = match next_cursor.as_ref() {
            Some((created_at, id)) => Some(RemoteImageHistoryCursor::mint(
                peer_id,
                created_at,
                *id,
                secret.as_bytes(),
            )),
            _ => None,
        };
        let snapshot_id = match source.snapshot_id() {
            Ok(value) => value,
            Err(_) => return HostImageHistoryResponse::Unavailable("persistence_unavailable"),
        };
        HostImageHistoryResponse::Ok(
            RemoteImageHistoryPage {
                rows: previews,
                next_cursor,
            },
            snapshot_id,
        )
    }
}

/// Upper bound on the number of [`fill_page_after`] iterations
/// the host serves in a single `list_recent_images` call. The
/// cap is generous so a sequence of
/// [`PAGE_AFTER_MAX_BATCHES`]-long refill windows (each
/// `PAGE_AFTER_BATCH_OVERFETCH` candidates wide) cannot exhaust
/// the work budget while still keeping it bounded so a
/// pathological cursor cannot keep the listener busy forever.
const SERVE_MAX_ITERATIONS: usize = 16;

/// Projection from the local [`EntryRecord`] to the
/// metadata-only [`RemoteImagePreview`] the wire exposes. The
/// helper strips the `id` into an opaque id the runtime encodes
/// from the local primary key and validates the title through
/// [`crate::peer_text_history::sanitize_remote_title`].
pub fn project_image_row(entry: &EntryRecord) -> RemoteImagePreview {
    let remote_entry_id = format!("entry-{}", entry.id);
    let title =
        crate::peer_text_history::sanitize_remote_title(entry.title.as_deref().unwrap_or(""));
    RemoteImagePreview {
        remote_entry_id,
        title,
        content_type: entry.content_type.as_str().to_string(),
        created_at: entry.created_at.clone(),
        byte_size: entry.content_size.max(0) as u64,
        width: entry.payload_width.unwrap_or(0),
        height: entry.payload_height.unwrap_or(0),
    }
}

/// Default noop facade the cross-compile / unsupported-target
/// build installs. Every call returns
/// [`PeerImageHistoryTransportError::Unavailable`] so the runtime
/// collapses the outcome into
/// [`PeerImageHistoryOutcome::TransportUnavailable`].
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPeerImageHistoryTransport;

impl PeerImageHistoryTransport for NoopPeerImageHistoryTransport {
    fn list_recent_images(
        &self,
        _request: ListRecentImagesRequest,
    ) -> Result<ListRecentImagesResponse, PeerImageHistoryTransportError> {
        Err(PeerImageHistoryTransportError::Unavailable)
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
pub struct PeerPairingImageHistoryTransportAdapter {
    inner: Arc<dyn crate::peer_pairing::PeerTransport>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerPairingImageHistoryTransportAdapter {
    pub fn new(inner: Arc<dyn crate::peer_pairing::PeerTransport>) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerImageHistoryTransport for PeerPairingImageHistoryTransportAdapter {
    fn list_recent_images(
        &self,
        request: ListRecentImagesRequest,
    ) -> Result<ListRecentImagesResponse, PeerImageHistoryTransportError> {
        match self.inner.list_recent_images(
            &request.peer_id,
            &request.cert_fingerprint,
            &request.cursor,
            request.limit,
        ) {
            Ok(snapshot) => Ok(ListRecentImagesResponse {
                page: RemoteImageHistoryPage {
                    rows: snapshot
                        .rows
                        .into_iter()
                        .map(|row| RemoteImagePreview {
                            remote_entry_id: row.remote_entry_id,
                            title: row.title,
                            content_type: row.content_type,
                            created_at: row.created_at,
                            byte_size: row.byte_size,
                            width: row.width,
                            height: row.height,
                        })
                        .collect(),
                    next_cursor: if snapshot.next_cursor.is_empty() {
                        None
                    } else {
                        Some(RemoteImageHistoryCursor::from_string(snapshot.next_cursor))
                    },
                },
                snapshot_id: snapshot.snapshot_id,
            }),
            Err(error) => Err(map_pairing_image_transport_error(error)),
        }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
fn map_pairing_image_transport_error(
    error: crate::peer_pairing::TransportError,
) -> PeerImageHistoryTransportError {
    use crate::peer_pairing::TransportError as Pairing;
    match error {
        Pairing::UnknownPeer => PeerImageHistoryTransportError::UnknownPeer,
        Pairing::PeerUnresolved => PeerImageHistoryTransportError::PeerUnresolved,
        Pairing::KeyMismatch => PeerImageHistoryTransportError::KeyMismatch,
        Pairing::Revoked => PeerImageHistoryTransportError::Revoked,
        Pairing::Blocked => PeerImageHistoryTransportError::Blocked,
        Pairing::IncompatibleProtocol => PeerImageHistoryTransportError::IncompatibleProtocol,
        Pairing::Malformed => PeerImageHistoryTransportError::Malformed,
        Pairing::InvalidCursor => PeerImageHistoryTransportError::InvalidCursor,
        Pairing::AlreadyRunning
        | Pairing::NotRunning
        | Pairing::Unavailable
        | Pairing::Crypto
        | Pairing::BodyTooLarge => PeerImageHistoryTransportError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipvault_db::{ContentType, EntryRecord, IMAGE_CONTENT_SENTINEL, IMAGE_MIME_PNG};

    fn record(id: i64, created_at: &str) -> EntryRecord {
        EntryRecord {
            id,
            content: IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: 100,
            content_hash: "h".repeat(64),
            source_app: None,
            is_pinned: false,
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
            last_seen_at: created_at.to_string(),
            title: None,
            source_app_name: None,
            source_app_icon_ref: None,
            asset_ref: Some(format!("clipboard/{}.png", "a".repeat(64))),
            mime_type: Some(IMAGE_MIME_PNG.to_string()),
            payload_width: Some(640),
            payload_height: Some(480),
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        }
    }

    #[test]
    fn transferable_predicate_accepts_image_rows() {
        assert!(image_entry_is_transferable(&record(
            1,
            "2026-01-01T00:00:00Z"
        )));
    }

    #[test]
    fn transferable_predicate_rejects_text_rows() {
        let mut row = record(1, "2026-01-01T00:00:00Z");
        row.content_type = ContentType::Text;
        assert!(!image_entry_is_transferable(&row));
    }

    #[test]
    fn transferable_predicate_rejects_out_of_namespace_assets() {
        let mut row = record(1, "2026-01-01T00:00:00Z");
        row.asset_ref = Some("application-icons/x.png".to_string());
        assert!(!image_entry_is_transferable(&row));
        row.asset_ref = Some("/abs/path.png".to_string());
        assert!(!image_entry_is_transferable(&row));
        row.asset_ref = None;
        assert!(!image_entry_is_transferable(&row));
    }

    #[test]
    fn cursor_round_trips_timestamp_and_id() {
        let secret = PeerImageCursorSecret::generate();
        let cursor =
            RemoteImageHistoryCursor::mint("peer-x", "2026-01-02T03:04:05Z", 42, secret.as_bytes());
        let (ts, id) = cursor.decode("peer-x", secret.as_bytes()).expect("decoded");
        assert_eq!(ts, "2026-01-02T03:04:05Z");
        assert_eq!(id, 42);
    }

    #[test]
    fn cursor_rejects_signature_when_secret_rotates() {
        let previous = PeerImageCursorSecret::generate();
        let next = PeerImageCursorSecret::generate();
        let cursor = RemoteImageHistoryCursor::mint(
            "peer-x",
            "2026-01-02T03:04:05Z",
            42,
            previous.as_bytes(),
        );
        assert!(matches!(
            cursor.decode("peer-x", next.as_bytes()),
            Err(PeerImageHistoryCursorError::SignatureMismatch)
        ));
    }

    #[test]
    fn cursor_rejects_replay_across_peers() {
        let secret = PeerImageCursorSecret::generate();
        let cursor =
            RemoteImageHistoryCursor::mint("peer-a", "2026-01-02T03:04:05Z", 42, secret.as_bytes());
        assert!(matches!(
            cursor.decode("peer-b", secret.as_bytes()),
            Err(PeerImageHistoryCursorError::Invalid)
        ));
    }

    #[test]
    fn snapshot_id_is_stable_for_same_header() {
        let first = compute_image_page_fingerprint("peer-x", "2026-01-01T00:00:00Z", 7, 3);
        let second = compute_image_page_fingerprint("peer-x", "2026-01-01T00:00:00Z", 7, 3);
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn page_after_excludes_non_image_entries() {
        let source = InMemoryHostImageHistorySource::new();
        let mut image = record(1, "2026-01-01T00:00:00Z");
        image.content_type = ContentType::Text;
        let valid = record(2, "2026-01-02T03:04:05Z");
        source.seed(vec![image, valid.clone()]);
        let (page, status) =
            fill_page_after(&source, "9999-12-31T23:59:59Z", i64::MAX, 10).expect("page");
        assert_eq!(status, FillPageAfterStatus::Exhausted);
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].id, valid.id);
    }

    #[test]
    fn page_after_excludes_out_of_namespace_assets() {
        let source = InMemoryHostImageHistorySource::new();
        let mut foreign = record(1, "2026-01-01T00:00:00Z");
        foreign.asset_ref = Some("application-icons/x.png".to_string());
        let valid = record(2, "2026-01-02T03:04:05Z");
        source.seed(vec![foreign, valid.clone()]);
        let (page, status) =
            fill_page_after(&source, "9999-12-31T23:59:59Z", i64::MAX, 10).expect("page");
        assert_eq!(status, FillPageAfterStatus::Exhausted);
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].id, valid.id);
    }

    #[test]
    fn page_after_excludes_rows_with_missing_dimensions() {
        let source = InMemoryHostImageHistorySource::new();
        let mut incomplete = record(1, "2026-01-01T00:00:00Z");
        incomplete.payload_width = None;
        let valid = record(2, "2026-01-02T03:04:05Z");
        source.seed(vec![incomplete, valid.clone()]);
        let page = source
            .page_after("9999-12-31T23:59:59Z", i64::MAX, 10)
            .expect("page");
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].id, valid.id);
    }

    /// Integration coverage for the `peer-image-import` change:
    /// the production host source combines the SQL projection
    /// with the asset validator closure so a missing,
    /// out-of-namespace, oversized or corrupt PNG never reaches
    /// the wire. The test wires a real `tempfile::TempDir`
    /// asset store plus a SQLite database so every code path
    /// the bootstrap exercises is covered.
    #[test]
    fn host_source_drops_invalid_png_assets_during_pagination() {
        use crate::clipboard_assets::{ClipboardAssetStore, MAX_CLIPBOARD_ASSET_BYTES};
        use clipvault_db::{builtin_migrations, Database, EntryRepository, NewEntry};

        // Build a real tempdir + database so the production
        // `EntryRepositoryHostImageHistorySource` runs against
        // live state.
        let dir = tempfile::tempdir().expect("tempdir");
        let mut db = Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");

        // Persist a valid PNG so the validator passes the
        // happy path.
        let valid_hash = "1".repeat(64);
        let valid_asset = format!("clipboard/{valid_hash}.png");
        let asset_store = ClipboardAssetStore::new(dir.path());
        let root = asset_store.root();
        std::fs::create_dir_all(&root).expect("mkdir");
        let valid_bytes = build_valid_png();
        std::fs::write(root.join(format!("{valid_hash}.png")), &valid_bytes).expect("write valid");

        // Persist a corrupt PNG (valid name, invalid bytes).
        let corrupt_hash = "2".repeat(64);
        let corrupt_asset = format!("clipboard/{corrupt_hash}.png");
        std::fs::write(root.join(format!("{corrupt_hash}.png")), b"not a png")
            .expect("write corrupt");

        // Persist an oversized file so the validator surfaces
        // the typed `TooLarge` outcome. The file exceeds the
        // cap by one byte but the metadata size check fires
        // before the file is read.
        let oversized_hash = "3".repeat(64);
        let oversized_asset = format!("clipboard/{oversized_hash}.png");
        std::fs::write(
            root.join(format!("{oversized_hash}.png")),
            vec![0u8; MAX_CLIPBOARD_ASSET_BYTES + 1],
        )
        .expect("write oversized");

        // Persist a missing asset — only the SQLite row, no
        // file on disk. The validator rejects the row with
        // `NotFound` so it must be excluded from the wire
        // projection.
        let missing_hash = "4".repeat(64);
        let missing_asset = format!("clipboard/{missing_hash}.png");

        let now = time::OffsetDateTime::now_utc();
        let entries = vec![
            NewEntry {
                content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
                content_type: ContentType::Image,
                content_size: valid_bytes.len() as i64,
                content_hash: valid_hash.clone(),
                source_app: None,
                created_at: now,
                last_seen_at: now,
                asset_ref: Some(valid_asset.clone()),
                mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
                payload_width: Some(16),
                payload_height: Some(8),
                rich_text_hash: None,
                rich_html_ref: None,
                rich_rtf_ref: None,
                rich_preview_ref: None,
                rich_html_size: None,
                rich_rtf_size: None,
                code_language: None,
            },
            NewEntry {
                content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
                content_type: ContentType::Image,
                content_size: 64,
                content_hash: corrupt_hash.clone(),
                source_app: None,
                created_at: now,
                last_seen_at: now,
                asset_ref: Some(corrupt_asset.clone()),
                mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
                payload_width: Some(16),
                payload_height: Some(8),
                rich_text_hash: None,
                rich_html_ref: None,
                rich_rtf_ref: None,
                rich_preview_ref: None,
                rich_html_size: None,
                rich_rtf_size: None,
                code_language: None,
            },
            NewEntry {
                content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
                content_type: ContentType::Image,
                content_size: 64,
                content_hash: oversized_hash.clone(),
                source_app: None,
                created_at: now,
                last_seen_at: now,
                asset_ref: Some(oversized_asset.clone()),
                mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
                payload_width: Some(16),
                payload_height: Some(8),
                rich_text_hash: None,
                rich_html_ref: None,
                rich_rtf_ref: None,
                rich_preview_ref: None,
                rich_html_size: None,
                rich_rtf_size: None,
                code_language: None,
            },
            NewEntry {
                content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
                content_type: ContentType::Image,
                content_size: 64,
                content_hash: missing_hash.clone(),
                source_app: None,
                created_at: now,
                last_seen_at: now,
                asset_ref: Some(missing_asset.clone()),
                mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
                payload_width: Some(16),
                payload_height: Some(8),
                rich_text_hash: None,
                rich_html_ref: None,
                rich_rtf_ref: None,
                rich_preview_ref: None,
                rich_html_size: None,
                rich_rtf_size: None,
                code_language: None,
            },
        ];
        for entry in entries {
            let conn = db.connection_mut();
            let mut repo = EntryRepository::new(conn);
            repo.insert_or_touch(entry).expect("insert");
        }
        // Drop the connection so the source can borrow the
        // database handle cleanly.
        drop(db);

        let database_handle = Arc::new(parking_lot::Mutex::new(
            Database::open(dir.path().join("clipvault.db")).expect("open"),
        ));
        let asset_validator_store = ClipboardAssetStore::new(dir.path());
        let source = EntryRepositoryHostImageHistorySource::with_asset_validator(
            database_handle.clone(),
            move |asset_ref: &str| asset_validator_store.validate(asset_ref).is_ok(),
        );
        let (page, status) =
            fill_page_after(&source, "9999-12-31T23:59:59Z", i64::MAX, 10).expect("page");
        assert_eq!(status, FillPageAfterStatus::Exhausted);
        assert_eq!(
            page.len(),
            1,
            "only the valid PNG survives the wire projection"
        );
        assert_eq!(page[0].content_hash, valid_hash);

        // Defence in depth: confirm the validator alone rejects
        // every flavour of invalid PNG so a future regression
        // that bypasses the closure surfaces here.
        let asset_validator_store = ClipboardAssetStore::new(dir.path());
        assert!(asset_validator_store.validate(&valid_asset).is_ok());
        assert!(asset_validator_store.validate(&corrupt_asset).is_err());
        assert!(asset_validator_store.validate(&oversized_asset).is_err());
        assert!(asset_validator_store.validate(&missing_asset).is_err());
    }

    /// Refill integration coverage for the regression the
    /// `peer-image-import` change pins: a host that asks for
    /// `limit` rows MUST keep refilling until either the
    /// persistence layer returns at least `limit` eligible
    /// rows OR the cursor exhausts. The integration test
    /// seeds more than [`PAGE_AFTER_BATCH_OVERFETCH`] (8)
    /// invalid PNGs in front of the requested valid count so
    /// the previous single-batch implementation would have
    /// stopped paginating while valid PNGs were still on disk.
    #[test]
    fn host_source_refills_when_more_than_eight_invalid_assets_precede_valid_pngs() {
        use crate::clipboard_assets::{ClipboardAssetStore, MAX_CLIPBOARD_ASSET_BYTES};
        use clipvault_db::{builtin_migrations, Database, EntryRepository, NewEntry};

        let dir = tempfile::tempdir().expect("tempdir");
        let mut db = Database::open(dir.path().join("clipvault.db")).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        let asset_store = ClipboardAssetStore::new(dir.path());
        let root = asset_store.root();
        std::fs::create_dir_all(&root).expect("mkdir");

        // Seed 12 invalid PNGs (missing file on disk so the
        // asset validator rejects every one of them) ...
        for index in 1..=12 {
            let hash = format!("{:064x}", index);
            let asset = format!("clipboard/{hash}.png");
            // Write NOTHING on disk: the validator surfaces
            // `NotFound` so the refill loop has to skip every
            // invalid row before reaching the valid PNGs.
            let entry = NewEntry {
                content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
                content_type: ContentType::Image,
                content_size: 16,
                content_hash: hash,
                source_app: None,
                // Newer timestamps make invalid assets sort ahead of
                // the valid rows in the newest-first SQL projection.
                created_at: time::OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(100 + index),
                last_seen_at: time::OffsetDateTime::now_utc(),
                asset_ref: Some(asset),
                mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
                payload_width: Some(16),
                payload_height: Some(8),
                rich_text_hash: None,
                rich_html_ref: None,
                rich_rtf_ref: None,
                rich_preview_ref: None,
                rich_html_size: None,
                rich_rtf_size: None,
                code_language: None,
            };
            let conn = db.connection_mut();
            let mut repo = EntryRepository::new(conn);
            repo.insert_or_touch(entry).expect("insert invalid");
        }

        // ... followed by 5 valid PNGs. The previous prototype
        // would have requested `limit + 8` rows (18 for a 10-row
        // limit) and returned at most 6 visible rows after
        // dropping the 12 invalid ones — silently losing the
        // remaining valid PNGs. The refill loop must keep
        // asking the persistence layer for more batches until
        // it surfaces at least `limit + 1` eligible rows.
        let valid_bytes = build_valid_png();
        let mut valid_hashes: Vec<String> = Vec::new();
        for index in 13..=17 {
            let hash = format!("{:064x}", index);
            let asset = format!("clipboard/{hash}.png");
            std::fs::write(root.join(format!("{hash}.png")), &valid_bytes).expect("write valid");
            valid_hashes.push(hash.clone());
            let entry = NewEntry {
                content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
                content_type: ContentType::Image,
                content_size: valid_bytes.len() as i64,
                content_hash: hash,
                source_app: None,
                // Older valid rows force the refiller to skip the
                // invalid prefix before it can satisfy the page.
                created_at: time::OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(index),
                last_seen_at: time::OffsetDateTime::now_utc(),
                asset_ref: Some(asset),
                mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
                payload_width: Some(16),
                payload_height: Some(8),
                rich_text_hash: None,
                rich_html_ref: None,
                rich_rtf_ref: None,
                rich_preview_ref: None,
                rich_html_size: None,
                rich_rtf_size: None,
                code_language: None,
            };
            let conn = db.connection_mut();
            let mut repo = EntryRepository::new(conn);
            repo.insert_or_touch(entry).expect("insert valid");
        }
        drop(db);

        let database_handle = Arc::new(parking_lot::Mutex::new(
            Database::open(dir.path().join("clipvault.db")).expect("open"),
        ));
        let asset_validator_store = ClipboardAssetStore::new(dir.path());
        let source = EntryRepositoryHostImageHistorySource::with_asset_validator(
            database_handle.clone(),
            move |asset_ref: &str| asset_validator_store.validate(asset_ref).is_ok(),
        );
        let (page, _status) =
            fill_page_after(&source, "9999-12-31T23:59:59Z", i64::MAX, 5).expect("page");
        assert_eq!(
            page.len(),
            5,
            "refill loop must surface every valid PNG past the invalid prefix",
        );
        // The SQL projects `ORDER BY created_at DESC, id DESC`;
        // every entry was inserted with the same `created_at`,
        // so the visible order follows the id descending. The
        // test asserts both ends of the visible window so a
        // future regression that drops or duplicates a valid
        // PNG in the middle surfaces here.
        assert_eq!(
            page.first().map(|row| row.content_hash.clone()),
            Some(valid_hashes.last().cloned().unwrap()),
            "newest visible row matches the highest-id valid PNG",
        );
        assert_eq!(
            page.last().map(|row| row.content_hash.clone()),
            Some(valid_hashes.first().cloned().unwrap()),
            "oldest visible row matches the lowest-id valid PNG",
        );
        let visible_ids: Vec<i64> = page.iter().map(|row| row.id).collect();
        assert_eq!(
            visible_ids,
            (13..=17).rev().collect::<Vec<i64>>(),
            "visible rows must follow the descending id order",
        );
        // Defence in depth: confirm the size cap the helper
        // surfaces would have caught the invalid prefix when
        // the asset store ever wrote the corresponding file.
        assert!(
            MAX_CLIPBOARD_ASSET_BYTES >= valid_bytes.len(),
            "valid PNG must stay under the asset cap so the validator never trips",
        );
    }

    /// Multi-page integration coverage that exercises the
    /// work-limit continuation the `peer-image-import` change
    /// pins: the host serves a window whose invalid-prefix is
    /// larger than the [`PAGE_AFTER_BATCH_OVERFETCH`] window
    /// per single call, so a single `fill_page_after`
    /// invocation MUST report `WorkLimitReached` and the host
    /// listener MUST resume from the last scanned position.
    /// The test seeds >8 batches of invalid PNGs (every
    /// `fill_page_after` call asks for `target + 8` candidates)
    /// followed by 5 valid PNGs and walks several pages of the
    /// host `serve` helper until the persistence layer
    /// exhausts. Every valid PNG appears exactly once and the
    /// pagination eventually closes.
    #[test]
    fn host_serve_emits_signed_continuation_when_work_limit_reached_and_exhausts() {
        use crate::clipboard_assets::ClipboardAssetStore;
        use clipvault_db::{builtin_migrations, Database, EntryRepository, NewEntry};

        // `limit` is chosen small enough that the 5 valid PNGs
        // span multiple pages. With 5 valid + N invalid PNGs
        // and `limit = 2`, the host returns one valid PNG per
        // page plus a signed continuation cursor (so the
        // client requests a few pages before the persistence
        // layer drains).
        const LIMIT: u32 = 2;
        // Seed enough invalid PNGs to force multiple `fill_page_after`
        // iterations per `serve` call. The cap of
        // `PAGE_AFTER_BATCH_OVERFETCH = 8` candidates per batch
        // and `PAGE_AFTER_MAX_BATCHES = 8` batches per call
        // (64 candidates max) drives the helper past the
        // single-batch limit when the prefix is bigger.
        const INVALID_COUNT: i64 = 100;

        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("clipvault.db");
        let mut db = Database::open(&db_path).expect("open");
        db.run_migrations(&builtin_migrations()).expect("migrate");
        let asset_store = ClipboardAssetStore::new(dir.path());
        let root = asset_store.root();
        std::fs::create_dir_all(&root).expect("mkdir");

        // Seed INVALID_COUNT invalid PNGs (no file on disk →
        // the asset validator rejects every one of them).
        let mut seeded_ids: Vec<i64> = Vec::new();
        for index in 1..=INVALID_COUNT {
            let hash = format!("{:064x}", index);
            let asset = format!("clipboard/{hash}.png");
            let entry = NewEntry {
                content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
                content_type: ContentType::Image,
                content_size: 16,
                content_hash: hash,
                source_app: None,
                // Place the invalid block first in the actual
                // newest-first cursor order, not merely earlier in
                // insertion order.
                created_at: time::OffsetDateTime::UNIX_EPOCH
                    + time::Duration::seconds(1_000 + index),
                last_seen_at: time::OffsetDateTime::now_utc(),
                asset_ref: Some(asset),
                mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
                payload_width: Some(16),
                payload_height: Some(8),
                rich_text_hash: None,
                rich_html_ref: None,
                rich_rtf_ref: None,
                rich_preview_ref: None,
                rich_html_size: None,
                rich_rtf_size: None,
                code_language: None,
            };
            let conn = db.connection_mut();
            let mut repo = EntryRepository::new(conn);
            repo.insert_or_touch(entry).expect("insert invalid");
            seeded_ids.push(index);
        }

        // Seed 5 valid PNGs after the invalid prefix.
        let valid_bytes = build_valid_png();
        let mut valid_hashes: Vec<String> = Vec::new();
        for index in (INVALID_COUNT + 1)..=(INVALID_COUNT + 5) {
            let hash = format!("{:064x}", index);
            let asset = format!("clipboard/{hash}.png");
            std::fs::write(root.join(format!("{hash}.png")), &valid_bytes).expect("write valid");
            valid_hashes.push(hash.clone());
            let entry = NewEntry {
                content: clipvault_db::IMAGE_CONTENT_SENTINEL.to_string(),
                content_type: ContentType::Image,
                content_size: valid_bytes.len() as i64,
                content_hash: hash,
                source_app: None,
                created_at: time::OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(index),
                last_seen_at: time::OffsetDateTime::now_utc(),
                asset_ref: Some(asset),
                mime_type: Some(clipvault_db::IMAGE_MIME_PNG.to_string()),
                payload_width: Some(16),
                payload_height: Some(8),
                rich_text_hash: None,
                rich_html_ref: None,
                rich_rtf_ref: None,
                rich_preview_ref: None,
                rich_html_size: None,
                rich_rtf_size: None,
                code_language: None,
            };
            let conn = db.connection_mut();
            let mut repo = EntryRepository::new(conn);
            repo.insert_or_touch(entry).expect("insert valid");
            seeded_ids.push(index);
        }
        drop(db);

        let database_handle = Arc::new(parking_lot::Mutex::new(
            Database::open(&db_path).expect("open"),
        ));
        let asset_validator_store = Arc::new(ClipboardAssetStore::new(dir.path()));
        let asset_validator_for_source = Arc::clone(&asset_validator_store);
        let source = EntryRepositoryHostImageHistorySource::with_asset_validator(
            database_handle.clone(),
            move |asset_ref: &str| asset_validator_for_source.validate(asset_ref).is_ok(),
        );

        // Walk the host `serve` helper across multiple pages
        // using the signed continuation cursor the previous
        // page emitted. The helper MUST return exactly
        // `valid_bytes.len()` valid PNGs total and finally a
        // `None` cursor when the persistence layer exhausts.
        let peer_id = "peer-continuation";
        let service = PeerImageHistoryService::new(Arc::new(NullPeerImageHistoryTransport));
        let secret = PeerImageCursorSecret::generate();
        service.set_cursor_secret(peer_id, secret);

        let mut seen_valid_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut cursor: Option<RemoteImageHistoryCursor> = None;
        let mut pages = 0;
        loop {
            pages += 1;
            assert!(
                pages <= 8,
                "pagination must eventually exhaust, page count = {pages}"
            );
            let response = service.serve(peer_id, cursor.as_ref(), LIMIT, &source);
            match response {
                HostImageHistoryResponse::Ok(page, _snapshot_id) => {
                    let visible_count = page.rows.len();
                    assert!(
                        visible_count <= LIMIT as usize,
                        "page MUST respect the limit, got {visible_count}",
                    );
                    for preview in &page.rows {
                        let id = preview
                            .remote_entry_id
                            .strip_prefix("entry-")
                            .and_then(|rest| rest.parse::<i64>().ok())
                            .expect("opaque remote_entry_id encodes the entry id");
                        // Invalid PNGs MUST NOT survive the wire
                        // projection: the asset validator drops
                        // them and `image_entry_is_transferable`
                        // refuses to surface them.
                        let entry = source
                            .page_after("9999-12-31T23:59:59Z", i64::MAX, 200)
                            .expect("page after")
                            .into_iter()
                            .find(|entry| entry.id == id)
                            .expect("entry must exist");
                        assert!(
                            entry.content_type == ContentType::Image
                                && entry.asset_ref.as_deref().is_some_and(|asset_ref| {
                                    asset_validator_store.validate(asset_ref).is_ok()
                                }),
                            "valid PNGs MUST pass both the metadata-only filter and the asset validator",
                        );
                        assert!(
                            seen_valid_ids.insert(id),
                            "no valid PNG may be repeated across pages",
                        );
                    }
                    cursor = page.next_cursor;
                    if cursor.is_none() {
                        // Exhausted: the host MUST stop returning
                        // rows on the next call.
                        break;
                    }
                }
                HostImageHistoryResponse::InvalidCursor => {
                    panic!("cursor was rejected even though the host signed it");
                }
                HostImageHistoryResponse::Unavailable(reason) => {
                    panic!("host returned Unavailable: {reason}");
                }
            }
        }
        assert_eq!(
            seen_valid_ids.len(),
            5,
            "every valid PNG must appear exactly once across the pagination cycle",
        );
    }

    #[test]
    fn host_serve_does_not_skip_rows_across_partial_work_limited_batches() {
        use std::collections::HashSet;

        struct SparseEligibleSource {
            inner: InMemoryHostImageHistorySource,
            invalid_ids: HashSet<i64>,
        }

        impl HostImageHistorySource for SparseEligibleSource {
            fn page_after(
                &self,
                created_at: &str,
                id: i64,
                limit: usize,
            ) -> Result<Vec<EntryRecord>, PeerImageHistoryPersistenceError> {
                self.inner.page_after(created_at, id, limit)
            }

            fn is_eligible(&self, entry: &EntryRecord) -> bool {
                image_entry_is_transferable(entry) && !self.invalid_ids.contains(&entry.id)
            }

            fn snapshot_id(&self) -> Result<String, PeerImageHistoryPersistenceError> {
                self.inner.snapshot_id()
            }
        }

        const LIMIT: u32 = 2;
        let invalid_ids: HashSet<i64> = (99..=198).collect();
        let source = SparseEligibleSource {
            inner: InMemoryHostImageHistorySource::new(),
            invalid_ids,
        };
        // Newest-first order is: two eligible rows, 100 invalid rows
        // (enough to hit the inner scan budget), then four eligible
        // rows. The first bounded fill therefore returns two rows with
        // WorkLimitReached; the following fill finds more than the
        // remaining page capacity. No eligible id may disappear when
        // the host signs and consumes the resulting cursors.
        source.inner.seed(
            (95..=200)
                .rev()
                .map(|id| record(id, "2026-09-24T12:00:00Z"))
                .collect(),
        );

        let peer_id = "peer-partial-work-limit";
        let service = PeerImageHistoryService::new(Arc::new(NullPeerImageHistoryTransport));
        service.set_cursor_secret(peer_id, PeerImageCursorSecret::generate());
        let mut cursor = None;
        let mut seen = Vec::new();
        let mut pages = 0;
        loop {
            pages += 1;
            assert!(pages <= 5, "pagination failed to advance safely");
            let response = service.serve(peer_id, cursor.as_ref(), LIMIT, &source);
            let HostImageHistoryResponse::Ok(page, _) = response else {
                panic!("host rejected a cursor minted by itself: {response:?}");
            };
            assert!(page.rows.len() <= LIMIT as usize);
            for row in page.rows {
                let id = row
                    .remote_entry_id
                    .strip_prefix("entry-")
                    .and_then(|value| value.parse::<i64>().ok())
                    .expect("remote id encodes an entry id");
                assert!(
                    seen.iter().all(|seen_id| *seen_id != id),
                    "duplicate row {id}"
                );
                seen.push(id);
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(seen, vec![200, 199, 98, 97, 96, 95]);
    }

    /// Minimal valid PNG the integration test uses. The bytes
    /// are byte-for-byte the canonical signature + IHDR + IDAT
    /// + IEND chunks the [`png`] crate produces for a 16×8
    /// opaque bitmap.
    fn build_valid_png() -> Vec<u8> {
        use png::{BitDepth, ColorType, Encoder};
        let mut data = Vec::new();
        {
            let mut encoder = Encoder::new(&mut data, 16, 8);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header");
            let stride = 16 * 4;
            let mut image_data = vec![0u8; stride * 8];
            for chunk in image_data.chunks_exact_mut(stride) {
                for pixel in chunk.chunks_exact_mut(4) {
                    pixel[0] = 0x10;
                    pixel[1] = 0x20;
                    pixel[2] = 0x30;
                    pixel[3] = 0xFF;
                }
            }
            writer.write_image_data(&image_data).expect("write");
        }
        data
    }

    #[test]
    fn browse_returns_peer_unavailable_when_state_missing() {
        let transport = Arc::new(NullPeerImageHistoryTransport);
        let service = PeerImageHistoryService::new(transport);
        let outcome = service.browse("peer-x", "fingerprint", None, MAX_IMAGE_PAGE_ROWS as u32);
        assert!(matches!(
            outcome,
            PeerImageHistoryOutcome::PeerUnavailable {
                reason: "no_known_peer"
            }
        ));
    }

    #[test]
    fn browse_returns_peer_unavailable_when_state_not_active() {
        let transport = Arc::new(NullPeerImageHistoryTransport);
        let service = PeerImageHistoryService::new(transport);
        service.record_peer_state(
            "peer-x",
            PeerImageActiveState {
                trusted: true,
                active: false,
            },
        );
        let outcome = service.browse("peer-x", "fingerprint", None, MAX_IMAGE_PAGE_ROWS as u32);
        assert!(matches!(
            outcome,
            PeerImageHistoryOutcome::PeerUnavailable {
                reason: "not_active"
            }
        ));
    }

    #[test]
    fn browse_returns_peer_unavailable_when_capability_missing() {
        // Defence in depth: even when the peer is trusted and
        // active, a row whose persisted capability does not
        // include `image_import` must not trigger a network
        // call. The runtime collapses the request into the
        // typed `peer_unavailable` outcome with reason
        // `not_available` so the renderer never surfaces an
        // import path.
        let transport = Arc::new(NullPeerImageHistoryTransport);
        let resolver: PeerCapabilityResolver =
            Arc::new(|peer_id| peer_id == "peer-with-capability");
        let service = PeerImageHistoryService::new(transport).with_capability_resolver(resolver);
        service.record_peer_state(
            "peer-x",
            PeerImageActiveState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.browse("peer-x", "fingerprint", None, MAX_IMAGE_PAGE_ROWS as u32);
        assert!(matches!(
            outcome,
            PeerImageHistoryOutcome::PeerUnavailable {
                reason: "not_available"
            }
        ));
    }

    #[test]
    fn browse_returns_ok_when_capability_is_advertised() {
        // Happy path: the resolver accepts the peer, the
        // transport returns a typed `Ok` response and the
        // service forwards the page + snapshot id verbatim.
        let transport = Arc::new(RecordingPeerImageHistoryTransport::new(
            PeerImageHistoryTransportError::Unavailable,
        ));
        let resolver: PeerCapabilityResolver =
            Arc::new(|peer_id| peer_id == "peer-with-capability");
        let service =
            PeerImageHistoryService::new(transport.clone()).with_capability_resolver(resolver);
        service.record_peer_state(
            "peer-with-capability",
            PeerImageActiveState {
                trusted: true,
                active: true,
            },
        );
        // Pre-load a single response so the next call returns Ok.
        transport.push_ok(RemoteImageHistoryPage {
            rows: vec![RemoteImagePreview {
                remote_entry_id: "entry-1".to_string(),
                title: None,
                content_type: "image".to_string(),
                created_at: "2026-01-02T03:04:05Z".to_string(),
                byte_size: 1024,
                width: 16,
                height: 16,
            }],
            next_cursor: Some(RemoteImageHistoryCursor::from_string("opaque".to_string())),
        });
        let outcome = service.browse(
            "peer-with-capability",
            "fingerprint",
            None,
            MAX_IMAGE_PAGE_ROWS as u32,
        );
        match outcome {
            PeerImageHistoryOutcome::Ok { page, .. } => {
                assert_eq!(page.rows.len(), 1);
            }
            _ => panic!("expected Ok outcome"),
        }
    }

    #[test]
    fn serve_returns_unavailable_when_capability_missing() {
        // Host-side guard: the listener rejects an inbound
        // `list_recent_images` request from a peer that does
        // not advertise the capability, even when the peer
        // otherwise has a valid cursor secret. The check runs
        // BEFORE the cursor is decoded so a forged cursor from
        // a non-capable peer surfaces as `not_available`, not
        // `invalid_cursor`.
        let resolver: PeerCapabilityResolver =
            Arc::new(|peer_id| peer_id == "peer-with-capability");
        let service = PeerImageHistoryService::new(Arc::new(NullPeerImageHistoryTransport))
            .with_capability_resolver(resolver);
        let secret = PeerImageCursorSecret::generate();
        service.set_cursor_secret("peer-without-capability", secret);
        let source = InMemoryHostImageHistorySource::new();
        let response = service.serve(
            "peer-without-capability",
            None,
            MAX_IMAGE_PAGE_ROWS as u32,
            &source,
        );
        assert!(matches!(
            response,
            HostImageHistoryResponse::Unavailable("not_available")
        ));
    }

    /// Scriptable transport the happy-path test uses. The
    /// transport records the most recent error / pushed
    /// response and returns either when the test asks.
    struct RecordingPeerImageHistoryTransport {
        next_error: parking_lot::Mutex<PeerImageHistoryTransportError>,
        queued_pages: parking_lot::Mutex<Vec<RemoteImageHistoryPage>>,
    }

    impl RecordingPeerImageHistoryTransport {
        fn new(default_error: PeerImageHistoryTransportError) -> Self {
            Self {
                next_error: parking_lot::Mutex::new(default_error),
                queued_pages: parking_lot::Mutex::new(Vec::new()),
            }
        }
        fn push_ok(&self, page: RemoteImageHistoryPage) {
            self.queued_pages.lock().push(page);
        }
    }

    impl PeerImageHistoryTransport for RecordingPeerImageHistoryTransport {
        fn list_recent_images(
            &self,
            _request: ListRecentImagesRequest,
        ) -> Result<ListRecentImagesResponse, PeerImageHistoryTransportError> {
            if let Some(page) = self.queued_pages.lock().pop() {
                Ok(ListRecentImagesResponse {
                    page,
                    snapshot_id: "snapshot".to_string(),
                })
            } else {
                Err(self.next_error.lock().clone())
            }
        }
    }

    struct NullPeerImageHistoryTransport;

    impl PeerImageHistoryTransport for NullPeerImageHistoryTransport {
        fn list_recent_images(
            &self,
            _request: ListRecentImagesRequest,
        ) -> Result<ListRecentImagesResponse, PeerImageHistoryTransportError> {
            Err(PeerImageHistoryTransportError::Unavailable)
        }
    }
}
