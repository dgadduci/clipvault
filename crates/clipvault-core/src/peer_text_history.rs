//! Metadata-only browser for the transferable text history of a
//! trusted, active local peer.
//!
//! `peer-text-history-browser` introduces the in-process façade the
//! desktop shell drives when the user opens the `Equipos vinculados`
//! row of a paired, present peer. The module owns:
//!
//! - the [`RemoteTextPreview`] row the host projects (opaque remote
//!   entry id, optional validated title, content type, RFC 3339
//!   timestamp, escaped bounded preview);
//! - the opaque [`RemoteHistoryCursor`] the host mints and the client
//!   submits verbatim — the cursor is an HMAC-SHA256 signature over
//!   `(peer_id, created_at, id)` bound to a 32-byte per-peer secret
//!   the host generates on `trust_state = trusted` and rotates on
//!   `Revoked` / re-pairing. The runtime never accepts an offset or a
//!   client-invented identifier and never echoes the secret over the
//!   wire;
//! - the [`PeerTextHistoryService`] façade that wraps the
//!   [`PeerTransport`] the bootstrap installs. The façade is
//!   metadata-only by construction: it never copies the entry body
//!   into the cursor, the wire payload or the preview, only the
//!   fields the design
//!   (`peer-text-history-browser/design.md` §"Paginación y preview")
//!   authorises. The local SQLite layer is consulted read-only by the
//!   [`HostHistorySource`] trait the listener drives when a trusted
//!   peer asks for `list_recent_text`; the client-side facade does
//!   NOT touch SQLite when serving a remote page — it dials the
//!   remote listener over mTLS and forwards the typed outcome the
//!   transport returns.
//! - the typed [`PeerHistoryOutcome`] the bridge / Tauri shell returns
//!   to the frontend. Every variant collapses to a stable identifier
//!   the UI branches on (`InvalidCursor`, `PeerUnavailable`,
//!   `PermissionDenied`, `Empty`, `Ok`) without parsing free-form
//!   strings or content bytes.
//!
//! ## Eligibility
//!
//! [`entry_is_transferable`] is the single predicate the host
//! projection and the unit tests share. A row is transferable when
//! its [`clipvault_db::ContentType`] is textual and carries no image
//! asset, MIME payload or dimensions. Rich-text metadata is never
//! transported, but a textual row that also has a rich representation
//! remains eligible through its normalized plain-text preview.
//! [`ContentType::Html`] is excluded explicitly: the wire is plain
//! text only and the local HTML preview escapes information the remote
//! card never delivers to the wire. Image and HTML rows are filtered
//! out before the projection runs so a peer never sees a disabled row.
//!
//! ## Ordering, cursor and limits
//!
//! The host projection sorts newest first, breaks ties with the
//! stable [`EntryRecord::id`] (the SQLite rowid) and slices the
//! page at [`MAX_PAGE_ROWS`]. The cursor encodes the
//! `(created_at, id)` pair of the **last** row the previous page
//! returned so the next page picks up strictly after it (strict
//! less-than, inclusive of stable id). The runtime rejects any
//! cursor it did not mint with [`PeerHistoryOutcome::InvalidCursor`]
//! — the spec scenario "Forged or rotated cursor" pins that
//! contract. The host re-validates the signature against the
//! current secret on every page request so a rotated secret always
//! rejects previous cursors.
//!
//! ## Preview shape
//!
//! The preview is bounded by [`PREVIEW_MAX_CHARS`] characters and
//! [`PREVIEW_MAX_LINES`] lines (whichever is shorter). The text is
//! escaped (HTML entity substitution), whitespace-collapsed and
//! trimmed before the projection. The service never interpolates
//! HTML and never includes the original content hash, body bytes,
//! tags, collections, source-app metadata or favourite flags in the
//! row.

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

/// Maximum number of rows the host returns in a single page. The
/// value mirrors the protocol limit the design
/// (`peer-text-history-browser/design.md` §"Paginación y preview")
/// pins for `list_recent_text`. The runtime enforces the cap so a
/// malicious cursor cannot trick the projection into streaming more
/// rows than the contract allows.
pub const MAX_PAGE_ROWS: usize = 50;

/// Maximum characters the host returns in a single preview line
/// before escaping. The actual cap the runtime enforces is the
/// minimum of this constant and [`PREVIEW_MAX_CHARS`], whichever
/// fits the document shape (`PREVIEW_MAX_CHARS` is the absolute
/// bound the wire enforces; `PREVIEW_MAX_LINES` is the line-bound
/// the projection respects so the renderer can drop to two lines
/// without truncating).
pub const PREVIEW_MAX_CHARS: usize = 300;

/// Maximum number of lines the host renders inside the bounded
/// preview. The runtime collapses additional lines into whitespace
/// so the preview always fits a two-line slot in the renderer.
pub const PREVIEW_MAX_LINES: usize = 2;

/// Default page size the host returns when the client supplies no
/// cursor. The runtime never hands out more than [`MAX_PAGE_ROWS`]
/// regardless of the requested size.
pub const DEFAULT_PAGE_ROWS: usize = 50;

/// Number of bytes the runtime mints for the per-peer HMAC secret.
/// 32 bytes is the canonical SHA-256 key length and matches the
/// length the design
/// (`peer-text-history-browser/design.md` §"Cursor firmado") pins.
pub const CURSOR_SECRET_BYTES: usize = 32;

/// Validation error the runtime surfaces when the cursor the client
/// submits is not an opaque string the host minted, the HMAC
/// signature does not match or the secret has rotated. The variant
/// is the typed reason the wire envelope encodes (`invalid_cursor`)
/// so the frontend never has to inspect free-form strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerHistoryCursorError {
    /// The supplied cursor was not produced by this host.
    #[error("peer history cursor was not emitted by this host")]
    Invalid,
    /// The cursor decoded but the timestamp did not parse as RFC 3339.
    #[error("peer history cursor decoded but the timestamp is not RFC 3339")]
    MalformedTimestamp,
    /// The cursor decoded but the stable id is not a positive integer.
    #[error("peer history cursor decoded but the stable id is not a positive integer")]
    MalformedId,
    /// The cursor HMAC verification failed.
    #[error("peer history cursor HMAC signature did not validate")]
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
/// app, the collection or the favourite flag. Clients MUST treat
/// the cursor as opaque: submitting a fabricated cursor or a
/// cursor signed under a rotated secret returns
/// [`PeerHistoryCursorError::Invalid`] or
/// [`PeerHistoryCursorError::SignatureMismatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RemoteHistoryCursor {
    inner: String,
}

impl RemoteHistoryCursor {
    /// Mint a cursor from the (timestamp, id) pair of the last row
    /// the page emitted, signed with the supplied per-peer secret.
    /// The host is the only entity that ever has the
    /// `(secret, payload)` pair. The renderer and the bridge treat
    /// the value as opaque and never parse it.
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
    /// [`PeerHistoryCursorError`] without surfacing the original
    /// bytes; the runtime never logs the decoded timestamp / id
    /// pair because they identify a single local entry the host
    /// should not leak through the wire.
    pub fn decode(
        &self,
        peer_id: &str,
        secret: &[u8],
    ) -> Result<(String, i64), PeerHistoryCursorError> {
        let Some((payload_b64, tag_b64)) = self.inner.split_once("::") else {
            return Err(PeerHistoryCursorError::Invalid);
        };
        let payload_bytes = URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| PeerHistoryCursorError::Invalid)?;
        let tag_bytes = URL_SAFE_NO_PAD
            .decode(tag_b64)
            .map_err(|_| PeerHistoryCursorError::Invalid)?;
        if tag_bytes.len() != Sha256::output_size() {
            return Err(PeerHistoryCursorError::Invalid);
        }
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret).expect("HMAC-SHA256 accepts any key length");
        mac.update(&payload_bytes);
        // `verify_slice` is constant-time so a forged signature
        // cannot be distinguished from a legitimate one through a
        // timing side channel. The conversion to `GenericArray`
        // from a `&[u8]` slice is what `hmac` already exposes.
        mac.verify_slice(&tag_bytes)
            .map_err(|_| PeerHistoryCursorError::SignatureMismatch)?;
        let payload =
            std::str::from_utf8(&payload_bytes).map_err(|_| PeerHistoryCursorError::Invalid)?;
        // The canonical payload is `<peer_id>\n<created_at_rfc3339>\n<id>`.
        // Reject any cursor whose peer_id does not match the
        // session-supplied value so a cursor minted for peer A
        // cannot be replayed against peer B.
        let mut parts = payload.splitn(3, '\n');
        let claim_peer = parts.next().ok_or(PeerHistoryCursorError::Invalid)?;
        if claim_peer != peer_id {
            return Err(PeerHistoryCursorError::Invalid);
        }
        let timestamp = parts.next().ok_or(PeerHistoryCursorError::Invalid)?;
        if OffsetDateTime::parse(timestamp, &Rfc3339).is_err() {
            return Err(PeerHistoryCursorError::MalformedTimestamp);
        }
        let id_raw = parts.next().ok_or(PeerHistoryCursorError::Invalid)?;
        let id: i64 = id_raw
            .parse()
            .map_err(|_| PeerHistoryCursorError::MalformedId)?;
        if id <= 0 {
            return Err(PeerHistoryCursorError::MalformedId);
        }
        Ok((timestamp.to_string(), id))
    }

    /// Borrow the opaque cursor string the host minted. The caller
    /// MUST treat the result as opaque and never attempt to parse
    /// it.
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Build a cursor from a string the renderer / bridge
    /// supplied. The helper keeps the field private so the
    /// runtime is the only place that ever mints a cursor;
    /// callers that want to inspect the inner value must use
    /// [`Self::as_str`].
    pub fn from_string(inner: String) -> Self {
        Self { inner }
    }
}

/// Outcome the runtime returns to the bridge / Tauri shell. The
/// discriminated union keeps the wire contract stable: the frontend
/// branches on `kind` without parsing free-form strings or content
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerHistoryOutcome {
    /// The page was rendered. `rows` is empty when the host has no
    /// transferable text entries (the renderer stays on the
    /// "Sin transferencias" placeholder).
    Ok {
        page: RemoteTextHistoryPage,
        /// Stable fingerprint of the local history at projection
        /// time. The renderer can use the value to detect a local
        /// capture that landed between page requests and decide
        /// whether to refetch (without leaking the row content).
        snapshot_id: String,
    },
    /// The cursor was not minted by this host or signed under a
    /// rotated secret. The renderer surfaces the typed reason
    /// without retrying.
    InvalidCursor,
    /// The peer is not Active / not trusted / not present. The
    /// renderer MUST NOT retry until presence flips; the runtime
    /// never opens a network call when the peer is unavailable so
    /// the failure cannot race a recent health probe.
    PeerUnavailable {
        /// Stable reason string the UI branches on
        /// (`not_active` / `not_trusted` / `no_known_peer`). The
        /// runtime NEVER returns a free-form platform detail here.
        reason: &'static str,
    },
    /// The runtime refused to project because the underlying
    /// transport rejected the page request (`unavailable`,
    /// `unknown_peer`, `key_mismatch`, `revoked`, `blocked`,
    /// `incompatible_protocol`, `malformed`). The renderer
    /// surfaces a typed failure copy without retrying blindly.
    TransportUnavailable { reason: &'static str },
}

/// Single metadata-only row the host returns for a transferable
/// textual entry. The struct carries only the fields the spec and
/// the design authorise: an opaque remote entry id, the optional
/// validated title, the content type, the RFC 3339 timestamp and an
/// escaped bounded preview. The row never carries the entry body,
/// the row hash, the source-app metadata, the favourite flag, tags,
/// collections or asset references.
///
/// `content_type` is serialised as the canonical snake_case string
/// the local SQLite layer persists so the renderer can switch on it
/// without parsing free-form text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RemoteTextPreview {
    /// Opaque remote entry id the host minted. The id is bound to
    /// the local `entries.id` of the source row but encoded so the
    /// renderer can never inspect the local primary key.
    pub remote_entry_id: String,
    /// Validated, trimmed user-supplied title. `None` when the entry
    /// has no custom title or the persisted value fails the same
    /// validation the local UI applies (so the renderer can fall
    /// back to the content-type label without surfacing garbage).
    pub title: Option<String>,
    /// Canonical snake_case string the local SQLite layer persists.
    /// The bridge surfaces the value verbatim so the renderer can
    /// switch on a stable wire contract.
    pub content_type: String,
    /// RFC 3339 timestamp of the entry's `created_at`.
    pub created_at: String,
    /// Bounded, escaped preview. Always trimmed and never longer
    /// than [`PREVIEW_MAX_CHARS`] Unicode scalar values.
    pub preview: String,
}

impl RemoteTextPreview {
    /// Decode the wire-form [`Self::content_type`] into the typed
    /// [`ContentType`] variant. Falls back to
    /// [`ContentType::Text`] for unknown strings — the runtime
    /// treats them as plain text so a future content-type addition
    /// does not break the renderer.
    pub fn content_type_typed(&self) -> ContentType {
        parse_content_type(&self.content_type)
    }
}

fn parse_content_type(value: &str) -> ContentType {
    match value {
        "text" => ContentType::Text,
        "url" => ContentType::Url,
        "email" => ContentType::Email,
        "json" => ContentType::Json,
        "jwt" => ContentType::Jwt,
        "uuid" => ContentType::Uuid,
        "ipv4" => ContentType::Ipv4,
        "ipv6" => ContentType::Ipv6,
        "hex_color" => ContentType::HexColor,
        "html" => ContentType::Html,
        "file_path" => ContentType::FilePath,
        "shell_command" => ContentType::ShellCommand,
        "sql" => ContentType::Sql,
        "code" => ContentType::Code,
        _ => ContentType::Text,
    }
}

/// Page the host returns from a single `list_recent_text` call. The
/// runtime always sets `rows` to at most [`MAX_PAGE_ROWS`] and only
/// populates `next_cursor` when a further page exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RemoteTextHistoryPage {
    pub rows: Vec<RemoteTextPreview>,
    /// Opaque cursor the renderer must submit to fetch the next
    /// page. `None` when this page is the last one.
    pub next_cursor: Option<RemoteHistoryCursor>,
}

/// Predicate the runtime and the unit tests share. The predicate
/// returns `true` when the entry is textual, carries no
/// non-transferable payload metadata (image asset, image MIME/size)
/// and is not [`ContentType::Html`]. Rich metadata is never exposed,
/// but does not discard the normalized plain text the preview uses.
/// The HTML exclusion
/// matches the design
/// (`peer-text-history-browser/design.md` §"Dependencia y contrato")
/// — the wire is plain text and the local HTML preview would lose
/// information the remote card never exposes.
///
pub fn entry_is_transferable(entry: &EntryRecord) -> bool {
    if !entry.content_type.is_textual() {
        return false;
    }
    // Explicit exclusion: even though `Html` returns `true` for
    // `is_textual()` (the local search needs to surface it), the
    // wire is plain text only. The design pin forbids shipping
    // HTML rows; the local card already filters them out of the
    // "transferable text" surface so the contract stays consistent.
    if matches!(entry.content_type, ContentType::Html) {
        return false;
    }
    if entry.is_renderable_image() {
        return false;
    }
    if entry.asset_ref.is_some() {
        return false;
    }
    if entry.mime_type.is_some() {
        return false;
    }
    if entry.payload_width.is_some() || entry.payload_height.is_some() {
        return false;
    }
    true
}

/// Build a metadata-only preview from an [`EntryRecord`]. The
/// helper escapes HTML entities, collapses whitespace, trims to
/// [`PREVIEW_MAX_CHARS`] Unicode scalar values and to at most
/// [`PREVIEW_MAX_LINES`] lines. The function is pure: no I/O, no
/// logging, no allocation beyond the returned `String`.
pub fn build_preview(content: &str) -> String {
    // Escape HTML-sensitive characters in the same order the local
    // UI applies; this keeps the renderer safe and avoids a
    // divergence between the local preview and the remote one.
    let escaped = escape_html_text(content);
    let trimmed = escaped.trim();
    let mut out = String::with_capacity(trimmed.len().min(PREVIEW_MAX_CHARS + 16));
    let mut chars = 0usize;
    let mut lines = 0usize;
    let mut in_whitespace_run = false;
    for ch in trimmed.chars() {
        if chars >= PREVIEW_MAX_CHARS {
            break;
        }
        if ch == '\n' || ch == '\r' {
            if !in_whitespace_run {
                if lines + 1 >= PREVIEW_MAX_LINES {
                    break;
                }
                out.push(' ');
                chars += 1;
                lines += 1;
                in_whitespace_run = true;
            }
            continue;
        }
        if ch.is_whitespace() {
            if in_whitespace_run {
                continue;
            }
            out.push(' ');
            chars += 1;
            in_whitespace_run = true;
            continue;
        }
        out.push(ch);
        chars += 1;
        in_whitespace_run = false;
    }
    out.trim_end().to_string()
}

fn escape_html_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Validate a user-supplied title using the same trim / length rules
/// the local UI enforces. The helper returns `None` for empty or
/// over-long inputs and trims the result so the wire payload never
/// contains leading / trailing whitespace.
pub fn sanitize_remote_title(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Match the documented title cap. The constant lives in
    // `clipvault_core::history::MAX_TITLE_LENGTH`; we re-use the
    // raw value here to keep the boundary tight without pulling
    // the history module into the wire payload.
    if trimmed.chars().count() > 80 {
        return None;
    }
    Some(trimmed.to_string())
}

/// Build a deterministic, non-reversible fingerprint of the page
/// the caller projects. The hash input is
/// `peer_id || "\n" || max_created_at || "\n" || max_id || "\n" ||
/// count`; the runtime emits the lowercase-hex SHA-256 digest so
/// the client can detect a local capture that landed between page
/// requests without leaking row content. The function is pure and
/// never inspects the row body.
pub fn compute_page_fingerprint(
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

/// Persistence trait the listener drives to project the transferable
/// text history of the **local** host. The bootstrap installs an
/// adapter that delegates to
/// [`clipvault_db::EntryRepository::text_entries_after`] / a freshly
/// allocated repository handle; tests inject an in-memory fake.
///
/// The listener accepts `ListRecentText` envelopes only after the
/// mTLS handshake validated the caller against the persisted pin for
/// the matching `peer_id`. The trait never accepts a `peer_id` from
/// outside and never returns row content beyond the metadata-only
/// projection.
pub trait HostHistorySource: Send + Sync {
    /// Return every transferable textual entry that matches the
    /// (created_at, id) cursor pair (strict less-than ordering). The
    /// implementation MUST sort by `created_at DESC, id DESC` and
    /// return at most `limit` rows. The runtime requests `limit + 1`
    /// rows so the projection can distinguish a "page exactly full
    /// with at least one more row to fetch" from a "page filled the
    /// limit because the user asked for fewer than the cap" — the
    /// listener drops the lookahead row before forwarding the page
    /// so the renderer never sees an extra entry.
    fn page_after(
        &self,
        created_at: &str,
        id: i64,
        limit: usize,
    ) -> Result<Vec<EntryRecord>, PeerHistoryPersistenceError>;

    /// Snapshot fingerprint the runtime returns alongside the page.
    /// The runtime only uses the value as a tie-breaker for the
    /// frontend (so a local capture landing between page requests
    /// can trigger a refetch); the value MUST be stable for the
    /// same persisted history and MUST NOT leak row content. The
    /// production adapter returns a SHA-256 over
    /// `(peer_id, max_created_at, max_id, count)`; tests may return
    /// a deterministic placeholder.
    fn snapshot_id(&self) -> Result<String, PeerHistoryPersistenceError>;
}

/// Clamp the requested page size to the documented wire contract.
/// The host MUST project at most [`MAX_PAGE_ROWS`] rows per page
/// and the runtime accepts a smaller `limit` so a caller can ask
/// for fewer rows without losing the typed outcome surface. The
/// helper always returns at least `1` so a malicious or buggy
/// renderer cannot request an empty page.
pub fn clamp_history_limit(limit: u32) -> usize {
    let bounded = limit.min(MAX_PAGE_ROWS as u32).max(1);
    bounded as usize
}

/// Probe limit the host asks the persistence layer for so it can
/// tell apart a "page exactly the limit because the caller asked
/// for fewer" from a "page exactly the limit but more rows
/// remain". The runtime requests `clamp + 1` rows internally;
/// when the projection returns exactly that count the host
/// knows more results exist and emits a signed `next_cursor`; the
/// last row of the lookahead is dropped before the host hands
/// the page back to the bridge so the renderer never sees an
/// off-by-one entry.
pub fn history_lookahead_limit(limit: usize) -> usize {
    limit.saturating_add(1).min(MAX_PAGE_ROWS + 1)
}

/// Typed persistence error the listener surfaces. Every adapter
/// collapses the underlying failure into one of these variants so
/// the listener can branch on the reason without inspecting
/// platform-specific error strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerHistoryPersistenceError {
    #[error("peer history persistence backend is unavailable")]
    Unavailable,
    #[error("peer history persistence backend rejected the operation")]
    Failed,
}

/// Per-peer HMAC secret the runtime caches. The secret is minted
/// when the row promotes to `trusted` and rotated on `Revoked` or
/// re-pairing. The cache lives in memory; the bootstrap populates
/// it from the persistence layer on every snapshot / health probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerCursorSecret(pub [u8; CURSOR_SECRET_BYTES]);

impl PeerCursorSecret {
    /// Generate a fresh 32-byte secret using the OS CSPRNG. The
    /// runtime NEVER derives a secret from a low-entropy source
    /// (peer_id, fingerprint, hash, …); a CSPRNG-backed mint keeps
    /// the wire robust against brute force attempts.
    pub fn generate() -> Self {
        let mut bytes = [0u8; CURSOR_SECRET_BYTES];
        let _ = rand_core::OsRng.try_fill_bytes(&mut bytes);
        Self(bytes)
    }

    /// Recover the secret from its hex / raw byte representation.
    /// `None` is returned when the byte slice is not exactly
    /// [`CURSOR_SECRET_BYTES`] long — the runtime refuses to
    /// truncate or pad so a corrupted / truncated persistence
    /// layer never feeds a deterministic key to the cursor.
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != CURSOR_SECRET_BYTES {
            return None;
        }
        let mut out = [0u8; CURSOR_SECRET_BYTES];
        out.copy_from_slice(bytes);
        Some(Self(out))
    }

    /// Borrow the secret bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Encode the secret as the 64-lowercase-hex representation the
    /// persistence layer persists in `known_peers.cursor_secret`.
    /// The runtime uses this projection so the database never
    /// stores raw key bytes and so a future migration can swap the
    /// encoding without breaking the wire protocol.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(CURSOR_SECRET_BYTES * 2);
        for byte in self.0.iter() {
            use std::fmt::Write as _;
            let _ = write!(&mut out, "{byte:02x}");
        }
        out
    }

    /// Recover the secret from its 64-lowercase-hex representation.
    /// The runtime uses this on bootstrap so the in-memory cache
    /// can be primed from the persisted row the previous
    /// successful pairing minted. `None` is returned when the input
    /// is not exactly 64 lowercase hex chars so a corrupted row
    /// cannot feed a deterministic key to the cursor pipeline.
    pub fn from_hex(value: &str) -> Option<Self> {
        if value.len() != CURSOR_SECRET_BYTES * 2 {
            return None;
        }
        if !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let bytes_in = value.as_bytes();
        let mut out = [0u8; CURSOR_SECRET_BYTES];
        for (index, chunk) in bytes_in.chunks_exact(2).enumerate() {
            let hex = std::str::from_utf8(chunk).ok()?;
            out[index] = u8::from_str_radix(hex, 16).ok()?;
        }
        Some(Self(out))
    }
}

/// In-memory persistence adapter the tests use. The adapter mirrors
/// the contract [`clipvault_db::EntryRepository`] exposes — the
/// runtime trusts the same outcomes regardless of the backend.
#[derive(Debug, Default)]
pub struct InMemoryHostHistorySource {
    inner: RwLock<Vec<EntryRecord>>,
}

impl InMemoryHostHistorySource {
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

impl HostHistorySource for InMemoryHostHistorySource {
    fn page_after(
        &self,
        created_at: &str,
        id: i64,
        limit: usize,
    ) -> Result<Vec<EntryRecord>, PeerHistoryPersistenceError> {
        let guard = self.inner.read();
        let mut matching: Vec<EntryRecord> = guard
            .iter()
            .filter(|entry| entry_is_transferable(entry))
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

    fn snapshot_id(&self) -> Result<String, PeerHistoryPersistenceError> {
        let guard = self.inner.read();
        let Some(top) = guard.iter().max_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        }) else {
            return Ok(compute_page_fingerprint("local", "empty", 0, 0));
        };
        Ok(compute_page_fingerprint(
            "local",
            &top.created_at,
            top.id,
            guard.len(),
        ))
    }
}

/// Production adapter the bootstrap wires against the live SQLite
/// handle. The adapter borrows a [`Mutex<Database>`] the runtime
/// hands it through the bootstrap; tests use the in-memory
/// projection above.
pub struct EntryRepositoryHostHistorySource {
    database: Arc<parking_lot::Mutex<clipvault_db::Database>>,
}

impl EntryRepositoryHostHistorySource {
    /// Build an adapter that borrows the shared [`clipvault_db::Database`].
    pub fn new(database: Arc<parking_lot::Mutex<clipvault_db::Database>>) -> Self {
        Self { database }
    }
}

impl HostHistorySource for EntryRepositoryHostHistorySource {
    fn page_after(
        &self,
        created_at: &str,
        id: i64,
        limit: usize,
    ) -> Result<Vec<EntryRecord>, PeerHistoryPersistenceError> {
        let mut guard = self.database.lock();
        let conn = guard.connection_mut();
        let repo = clipvault_db::EntryRepository::new(conn);
        let page = repo
            .text_entries_after(created_at, id, limit)
            .map_err(|error| {
                tracing::warn!(?error, "host history projection: page_after failed");
                PeerHistoryPersistenceError::Failed
            })?;
        Ok(page)
    }

    fn snapshot_id(&self) -> Result<String, PeerHistoryPersistenceError> {
        let mut guard = self.database.lock();
        let conn = guard.connection_mut();
        let repo = clipvault_db::EntryRepository::new(conn);
        let latest = repo.latest_transferable_text_snapshot().map_err(|error| {
            tracing::warn!(?error, "host history projection: snapshot failed");
            PeerHistoryPersistenceError::Failed
        })?;
        match latest {
            Some((created_at, id)) => {
                let count: i64 = clipvault_db::EntryRepository::new(conn)
                    .transferable_text_count()
                    .map_err(|error| {
                        tracing::warn!(?error, "host history projection: count failed");
                        PeerHistoryPersistenceError::Failed
                    })?;
                Ok(compute_page_fingerprint(
                    "local",
                    &created_at,
                    id,
                    count as usize,
                ))
            }
            None => Ok(compute_page_fingerprint("local", "empty", 0, 0)),
        }
    }
}

/// Outcome the host listener hands back to the transport after the
/// caller submitted a [`crate::peer_transport::wire::PairingMessage::ListRecentText`]
/// envelope. The transport never inspects the row payload beyond
/// the type check; the variants collapse to the typed reason the
/// wire envelope already encodes (`ListRecentTextAck` /
/// `ListRecentTextInvalid` / `ListRecentTextUnavailable`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostHistoryResponse {
    Ok(RemoteTextHistoryPage, String),
    InvalidCursor,
    Unavailable(&'static str),
}

/// Service the shell drives. The façade is cheap to clone (every
/// field is `Arc`-shared) and never mutates the local persistence
/// layer. The service is metadata-only by construction: a remote
/// browse call dials the mTLS-backed transport and forwards the
/// typed outcome the transport returns; a host-side projection is
/// driven by the transport listener through a separate
/// [`HostHistorySource`] trait.
#[derive(Clone)]
pub struct PeerTextHistoryService {
    /// Transport the client-side browse dials. The runtime never
    /// opens an mTLS connection outside of this trait; tests inject
    /// a scriptable fake so they can verify the wire contract
    /// without binding a real listener.
    transport: Arc<dyn PeerHistoryTransport>,
    /// Per-peer active-state cache. The cache is metadata-only
    /// (no content bytes) and is what the runtime consults before
    /// opening a cursor pagination. The cache lives in-memory; the
    /// bootstrap populates it from the
    /// [`clipvault_core::peer_pairing::PairingRuntime`] on every
    /// peer update so a browsing call cannot outrun the trust
    /// transition that unlocked it.
    active_peers: Arc<RwLock<HashMap<String, PeerActiveState>>>,
    /// Per-peer HMAC secret the runtime mints when the row
    /// promotes to `trusted` and rotates on `Revoked` /
    /// re-pairing. The cache is the only place the secret lives;
    /// the listener signs cursors with the value the bootstrap
    /// installed on trust promotion.
    cursor_secrets: Arc<RwLock<HashMap<String, PeerCursorSecret>>>,
}

/// Lightweight active-state record the runtime caches. The struct
/// only carries the metadata the browsing call needs to decide
/// whether the peer is currently eligible; the snapshot the
/// [`PairingRuntime`] returns is the authoritative source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerActiveState {
    pub trusted: bool,
    pub active: bool,
}

/// Metadata the client-side facade hands to the transport so the
/// transport can dial the matching peer with the pinned cert
/// fingerprint. The runtime reads the cert fingerprint from the
/// [`crate::clipvault::peer_pairing::PairingPersistence`] it caches
/// at install time — the bridge never accepts the fingerprint from
/// the renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRecentTextRequest {
    pub peer_id: String,
    pub cert_fingerprint: String,
    /// Opaque cursor the renderer submitted verbatim (empty when
    /// the client asks for the first page). The transport never
    /// inspects the payload.
    pub cursor: String,
    /// Upper bound the renderer wants; the transport clamps the
    /// value to [`MAX_PAGE_ROWS`].
    pub limit: u32,
}

/// Metadata-only transport facade the client-side browse uses. The
/// trait is the seam between the core runtime and the platform
/// mTLS stack: the transport owns the dial loop, the pin lookup and
/// the per-peer session, while the runtime owns the trust / active
/// gate and the cursor / page contract.
///
/// Every call collapses to a typed [`PeerHistoryTransportError`]
/// variant the runtime maps onto a [`PeerHistoryOutcome`]; the
/// trait never returns row content beyond the metadata-only
/// projection. The successful return bundles the bounded
/// [`RemoteTextHistoryPage`] the host projected alongside the
/// `snapshot_id` fingerprint the host minted on its own header;
/// the runtime MUST surface both values verbatim and MUST NOT
/// recompute the fingerprint from the visible rows.
pub trait PeerHistoryTransport: Send + Sync {
    fn list_recent_text(
        &self,
        request: ListRecentTextRequest,
    ) -> Result<ListRecentTextResponse, PeerHistoryTransportError>;
}

/// Successful payload the [`PeerHistoryTransport`] returns. The
/// runtime forwards the page and the `snapshot_id` to the bridge
/// without intermediate mutation so the client never reconstructs
/// a fingerprint from a partial row set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListRecentTextResponse {
    pub page: RemoteTextHistoryPage,
    pub snapshot_id: String,
}

/// Typed transport error the runtime maps onto a
/// [`PeerHistoryOutcome`] variant. The variants mirror
/// [`crate::peer_transport::TransportError`] but the trait
/// collapses the wire detail into the same stable reasons the
/// frontend branches on. The dedicated [`Self::InvalidCursor`]
/// variant keeps the cursor-rejection path lossless: a forged
/// cursor, a cursor replayed against another peer or a cursor
/// signed under a rotated HMAC secret surfaces as the typed
/// `invalid_cursor` outcome instead of being hidden behind a
/// generic network error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerHistoryTransportError {
    #[error("peer history transport is unavailable")]
    Unavailable,
    #[error("peer history transport has no resolved pairing endpoint")]
    PeerUnresolved,
    #[error("peer history transport rejected an unknown peer")]
    UnknownPeer,
    #[error("peer history transport rejected a mismatched TLS identity")]
    KeyMismatch,
    #[error("peer history transport rejected a revoked peer")]
    Revoked,
    #[error("peer history transport rejected a blocked peer")]
    Blocked,
    #[error("peer history transport wire protocol is incompatible")]
    IncompatibleProtocol,
    #[error("peer history transport rejected a malformed payload")]
    Malformed,
    /// The host refused the page request because the caller
    /// submitted a cursor it did not mint. The renderer MUST NOT
    /// treat this as a transient network failure.
    #[error("peer history transport rejected an invalid cursor")]
    InvalidCursor,
}

impl PeerHistoryTransportError {
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

impl PeerTextHistoryService {
    /// Build a service backed by the supplied transport. The
    /// active-state and secret caches start empty: the bootstrap
    /// populates them on every snapshot / health probe so the very
    /// first browsing call waits for an `Active` projection before
    /// returning rows.
    pub fn new(transport: Arc<dyn PeerHistoryTransport>) -> Self {
        Self {
            transport,
            active_peers: Arc::new(RwLock::new(HashMap::new())),
            cursor_secrets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update the active-state cache for a single peer. The shell
    /// calls this on every snapshot refresh so a peer that lost
    /// presence collapses to [`PeerHistoryOutcome::PeerUnavailable`]
    /// without a network round-trip.
    pub fn record_peer_state(&self, peer_id: &str, state: PeerActiveState) {
        self.active_peers.write().insert(peer_id.to_string(), state);
    }

    /// Drop the active-state cache entry for a peer the user
    /// unlinked / blocked so subsequent browsing calls collapse to
    /// [`PeerHistoryOutcome::PeerUnavailable`].
    pub fn forget_peer(&self, peer_id: &str) {
        self.active_peers.write().remove(peer_id);
    }

    /// Read the active-state the cache currently holds for `peer_id`.
    pub fn peer_state(&self, peer_id: &str) -> Option<PeerActiveState> {
        self.active_peers.read().get(peer_id).copied()
    }

    /// Mint or replace the per-peer HMAC secret the cursor signer
    /// uses. The bootstrap calls this exactly once per trust
    /// promotion so the host only signs cursors with a freshly
    /// minted secret; calling `set_cursor_secret` again invalidates
    /// every cursor signed under the previous secret.
    pub fn set_cursor_secret(&self, peer_id: &str, secret: PeerCursorSecret) {
        self.cursor_secrets
            .write()
            .insert(peer_id.to_string(), secret);
    }

    /// Install the per-peer HMAC secret the bootstrap loaded from
    /// the persisted `known_peers.cursor_secret` row. The runtime
    /// consults the cache when the productive `HistoryHostHandler`
    /// serves a `list_recent_text` request so a restart never has
    /// to mint a fresh secret (which would invalidate cursors the
    /// peer already holds). The helper refuses a malformed value
    /// so a corrupted row cannot downgrade the HMAC pipeline to a
    /// deterministic key.
    pub fn install_cursor_secret_hex(&self, peer_id: &str, secret_hex: &str) -> bool {
        match PeerCursorSecret::from_hex(secret_hex) {
            Some(secret) => {
                self.set_cursor_secret(peer_id, secret);
                true
            }
            None => false,
        }
    }

    /// Read the persisted per-peer HMAC secret as the 64-hex
    /// representation the database stores. The bootstrap uses this
    /// helper to round-trip the secret through the
    /// [`crate::peer_pairing::PairingPersistence`] trait without
    /// ever inspecting the raw key bytes.
    pub fn cursor_secret_hex(&self, peer_id: &str) -> Option<String> {
        self.cursor_secrets
            .read()
            .get(peer_id)
            .map(|secret| secret.to_hex())
    }

    /// Drop the per-peer HMAC secret. The bootstrap calls this on
    /// `Revoked`, `Blocked` and `Desvincular` so a subsequent
    /// `ListRecentText` from a stale cursor cannot validate.
    pub fn clear_cursor_secret(&self, peer_id: &str) {
        self.cursor_secrets.write().remove(peer_id);
    }

    /// Read the per-peer HMAC secret the cursor signer currently
    /// holds for `peer_id`.
    pub fn cursor_secret(&self, peer_id: &str) -> Option<PeerCursorSecret> {
        self.cursor_secrets.read().get(peer_id).copied()
    }

    /// Browse the most recent transferable text page of `peer_id`.
    ///
    /// The runtime consults the in-memory active-state cache and
    /// refuses to dial when the peer is not trusted, not present
    /// or unknown. A `cursor = None` request starts the newest
    /// page; a `Some(cursor)` request forwards the opaque string
    /// the renderer submitted verbatim — the client NEVER decodes
    /// the cursor because the HMAC secret is private to the host
    /// that minted it. The remote listener is the only entity that
    /// ever validates the cursor; a forged, rotated, replayed or
    /// truncated cursor therefore reaches the wire and the host
    /// surfaces the typed `invalid_cursor` reason.
    ///
    /// The function is **always** metadata-only: it never mutates
    /// SQLite, never emits a `history-updated` event and never
    /// returns row content beyond the bounded projection. The
    /// transport dials the remote listener; the runtime only
    /// forwards the typed outcome the transport returns.
    pub fn browse(
        &self,
        peer_id: &str,
        cert_fingerprint: &str,
        cursor: Option<&RemoteHistoryCursor>,
        limit: u32,
    ) -> PeerHistoryOutcome {
        let state = match self.peer_state(peer_id) {
            Some(state) => state,
            None => {
                return PeerHistoryOutcome::PeerUnavailable {
                    reason: "no_known_peer",
                };
            }
        };
        if !state.trusted {
            return PeerHistoryOutcome::PeerUnavailable {
                reason: "not_trusted",
            };
        }
        if !state.active {
            return PeerHistoryOutcome::PeerUnavailable {
                reason: "not_active",
            };
        }
        // Forward the cursor verbatim. The cursor is opaque on the
        // client side: the HMAC secret is bound to the host that
        // minted the cursor and only the host validates it. A
        // bogus, manipulated, rotated or replayed cursor therefore
        // travels over the wire and surfaces as a typed
        // `ListRecentTextInvalid` envelope the runtime maps onto
        // [`PeerHistoryOutcome::InvalidCursor`].
        let cursor_payload = cursor.map(|c| c.as_str().to_string()).unwrap_or_default();

        let request = ListRecentTextRequest {
            peer_id: peer_id.to_string(),
            cert_fingerprint: cert_fingerprint.to_string(),
            cursor: cursor_payload,
            limit: clamp_history_limit(limit) as u32,
        };
        match self.transport.list_recent_text(request) {
            Ok(ListRecentTextResponse { page, snapshot_id }) => {
                // The transport hands back the page exactly as
                // the host minted it: the metadata-only rows, the
                // opaque `next_cursor` and the `snapshot_id`
                // fingerprint the host computed from its own
                // header. The runtime NEVER rebuilds a remote
                // fingerprint locally — a partial reconstruction
                // would diverge from the host the moment a new
                // capture lands or a row is removed between page
                // requests.
                PeerHistoryOutcome::Ok { page, snapshot_id }
            }
            Err(error) => match error {
                // The transport rejected the caller for a reason
                // tied to trust / secret state on the remote host.
                // A `Revoked` / `Blocked` / `KeyMismatch` /
                // `UnknownPeer` collapse into the typed
                // `not_trusted` outcome so the renderer can show a
                // trust-specific copy instead of mislabelling the
                // peer as `not_active`. The `not_active` reason
                // remains reserved for peers the local cache
                // already knows are inactive (no presence flip
                // required); the runtime never sends a network
                // request for those.
                PeerHistoryTransportError::Revoked
                | PeerHistoryTransportError::Blocked
                | PeerHistoryTransportError::KeyMismatch
                | PeerHistoryTransportError::UnknownPeer => PeerHistoryOutcome::PeerUnavailable {
                    reason: "not_trusted",
                },
                PeerHistoryTransportError::InvalidCursor => PeerHistoryOutcome::InvalidCursor,
                PeerHistoryTransportError::PeerUnresolved
                | PeerHistoryTransportError::IncompatibleProtocol
                | PeerHistoryTransportError::Malformed
                | PeerHistoryTransportError::Unavailable => {
                    PeerHistoryOutcome::TransportUnavailable {
                        reason: error.reason(),
                    }
                }
            },
        }
    }

    /// Host-side projection the transport listener drives when an
    /// authenticated peer asks for `list_recent_text`. The function
    /// verifies the cursor, projects the page through the supplied
    /// [`HostHistorySource`] and signs the next cursor with the
    /// per-peer secret the bootstrap installed. The bootstrap
    /// guarantees the caller already authenticated against the
    /// pinned cert for `peer_id`; this method only enforces the
    /// typed outcome the wire envelope encodes.
    ///
    /// `requested_limit` is the page size the caller asked for; the
    /// helper clamps it to `[1, MAX_PAGE_ROWS]` so the host never
    /// returns more rows than the contract allows and never zero. To
    /// distinguish a "page full because there are more rows" from a
    /// "page full because the caller requested fewer than the cap"
    /// the helper asks the persistence layer for `clamp + 1` rows and
    /// emits a signed `next_cursor` ONLY when the lookahead returned
    /// an extra entry.
    pub fn serve(
        &self,
        peer_id: &str,
        cursor: Option<&RemoteHistoryCursor>,
        requested_limit: u32,
        source: &dyn HostHistorySource,
    ) -> HostHistoryResponse {
        let secret = match self.cursor_secret(peer_id) {
            Some(secret) => secret,
            None => return HostHistoryResponse::Unavailable("not_trusted"),
        };
        let (start_created_at, start_id) = match cursor {
            None => ("9999-12-31T23:59:59Z".to_string(), i64::MAX),
            Some(cursor) => match cursor.decode(peer_id, secret.as_bytes()) {
                Ok(pair) => pair,
                Err(_) => return HostHistoryResponse::InvalidCursor,
            },
        };
        let limit = clamp_history_limit(requested_limit);
        let probe = history_lookahead_limit(limit);
        let rows = match source.page_after(&start_created_at, start_id, probe) {
            Ok(rows) => rows,
            Err(_) => return HostHistoryResponse::Unavailable("persistence_unavailable"),
        };
        let mut previews: Vec<RemoteTextPreview> = Vec::with_capacity(limit);
        let mut has_more = false;
        for (index, row) in rows.into_iter().enumerate() {
            if index >= limit {
                has_more = true;
                break;
            }
            previews.push(project_row(&row));
        }
        let next_cursor = if has_more {
            previews.last().map(|preview| {
                RemoteHistoryCursor::mint(
                    peer_id,
                    &preview.created_at,
                    decode_remote_id(&preview.remote_entry_id),
                    secret.as_bytes(),
                )
            })
        } else {
            None
        };
        let snapshot_id = match source.snapshot_id() {
            Ok(value) => value,
            Err(_) => return HostHistoryResponse::Unavailable("persistence_unavailable"),
        };
        HostHistoryResponse::Ok(
            RemoteTextHistoryPage {
                rows: previews,
                next_cursor,
            },
            snapshot_id,
        )
    }
}

/// Projection from the local [`EntryRecord`] to the metadata-only
/// [`RemoteTextPreview`] the wire exposes. The helper strips the
/// `id` into an opaque id the runtime encodes from the local
/// primary key, escapes the bounded preview and validates the
/// title through [`sanitize_remote_title`].
pub fn project_row(entry: &EntryRecord) -> RemoteTextPreview {
    let remote_entry_id = format!("entry-{}", entry.id);
    let preview = build_preview(&entry.content);
    let title = sanitize_remote_title(entry.title.as_deref().unwrap_or(""));
    RemoteTextPreview {
        remote_entry_id,
        title,
        content_type: entry.content_type.as_str().to_string(),
        created_at: entry.created_at.clone(),
        preview,
    }
}

fn decode_remote_id(remote_entry_id: &str) -> i64 {
    remote_entry_id
        .strip_prefix("entry-")
        .and_then(|rest| rest.parse::<i64>().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipvault_db::{ContentType, EntryRecord, IMAGE_CONTENT_SENTINEL, IMAGE_MIME_PNG};

    #[cfg(feature = "local-peer-pairing-tls")]
    #[test]
    fn pairing_cursor_secret_cache_updates_text_and_image_services_together() {
        let text_service = PeerTextHistoryService::new(Arc::new(NoopPeerHistoryTransport));
        let image_service = crate::peer_image_history::PeerImageHistoryService::new(Arc::new(
            crate::peer_image_history::NoopPeerImageHistoryTransport,
        ));
        let cache = PeerHistoryCursorSecretCache::new(text_service.clone(), image_service.clone());
        let secret = PeerCursorSecret::generate();
        let secret_hex = secret.to_hex();

        crate::peer_pairing::PeerCursorSecretCache::install(&cache, "peer-shared", secret);
        assert_eq!(
            text_service.cursor_secret_hex("peer-shared"),
            Some(secret_hex.clone())
        );
        assert_eq!(
            image_service.cursor_secret_hex("peer-shared"),
            Some(secret_hex)
        );

        crate::peer_pairing::PeerCursorSecretCache::clear(&cache, "peer-shared");
        assert!(text_service.cursor_secret_hex("peer-shared").is_none());
        assert!(image_service.cursor_secret_hex("peer-shared").is_none());
    }

    fn record(id: i64, content_type: ContentType, content: &str, created_at: &str) -> EntryRecord {
        EntryRecord {
            id,
            content: content.to_string(),
            content_type,
            content_size: content.len() as i64,
            content_hash: "h".repeat(64),
            source_app: None,
            is_pinned: false,
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
            last_seen_at: created_at.to_string(),
            title: None,
            source_app_name: None,
            source_app_icon_ref: None,
            asset_ref: None,
            mime_type: None,
            payload_width: None,
            payload_height: None,
            rich_text_hash: None,
            rich_html_ref: None,
            rich_rtf_ref: None,
            rich_preview_ref: None,
            rich_html_size: None,
            rich_rtf_size: None,
            code_language: None,
        }
    }

    fn image_record(id: i64) -> EntryRecord {
        let mut record = record(
            id,
            ContentType::Image,
            IMAGE_CONTENT_SENTINEL,
            "2026-01-01T00:00:00Z",
        );
        record.asset_ref = Some(format!("clipboard/{}.png", "a".repeat(64)));
        record.mime_type = Some(IMAGE_MIME_PNG.to_string());
        record.payload_width = Some(1);
        record.payload_height = Some(1);
        record
    }

    #[test]
    fn transferable_predicate_rejects_image_rows() {
        assert!(!entry_is_transferable(&image_record(1)));
    }

    #[test]
    fn transferable_predicate_keeps_plain_preview_for_rich_text_rows() {
        let mut record = record(1, ContentType::Text, "hello", "2026-01-01T00:00:00Z");
        record.rich_text_hash = Some("a".repeat(64));
        record.rich_html_ref = Some("rich-text/a.html".to_string());
        assert!(entry_is_transferable(&record));
        assert_eq!(build_preview(&record.content), "hello");
    }

    #[test]
    fn transferable_predicate_rejects_html_rows() {
        // Even though `Html` returns `true` for `is_textual()` (the
        // local search needs to surface it), the wire is plain
        // text only — the predicate must explicitly exclude it.
        let html = record(1, ContentType::Html, "<p>hello</p>", "2026-01-01T00:00:00Z");
        assert!(!entry_is_transferable(&html));
    }

    #[test]
    fn transferable_predicate_accepts_textual_rows() {
        assert!(entry_is_transferable(&record(
            1,
            ContentType::Text,
            "hello",
            "2026-01-01T00:00:00Z"
        )));
    }

    #[test]
    fn preview_escapes_html_and_trims_to_two_lines() {
        let raw = "<script>alert(1)</script>\n\nsecond line\nthird";
        let preview = build_preview(raw);
        assert!(!preview.contains('<'));
        assert!(!preview.contains('>'));
        assert!(preview.contains("&lt;"));
        assert!(preview.contains("&gt;"));
        assert!(preview.lines().count() <= PREVIEW_MAX_LINES);
    }

    #[test]
    fn preview_truncates_to_max_chars() {
        let raw: String = std::iter::repeat('x').take(PREVIEW_MAX_CHARS * 2).collect();
        let preview = build_preview(&raw);
        assert!(preview.chars().count() <= PREVIEW_MAX_CHARS);
    }

    #[test]
    fn cursor_round_trips_timestamp_and_id_with_secret() {
        let secret = PeerCursorSecret::generate();
        let cursor =
            RemoteHistoryCursor::mint("peer-x", "2026-01-02T03:04:05Z", 42, secret.as_bytes());
        let (ts, id) = cursor.decode("peer-x", secret.as_bytes()).expect("decoded");
        assert_eq!(ts, "2026-01-02T03:04:05Z");
        assert_eq!(id, 42);
    }

    #[test]
    fn cursor_rejects_forged_payload() {
        let secret = PeerCursorSecret::generate();
        // Same shape as a host-minted cursor but signed under a
        // different key: the verifier MUST refuse the payload
        // before reaching the page phase.
        let cursor = RemoteHistoryCursor::mint(
            "peer-x",
            "2026-01-02T03:04:05Z",
            42,
            b"different-secret-bytes-padding-padding-padding",
        );
        assert!(matches!(
            cursor.decode("peer-x", secret.as_bytes()),
            Err(PeerHistoryCursorError::SignatureMismatch)
        ));
    }

    #[test]
    fn cursor_rejects_signature_when_secret_rotates() {
        let previous = PeerCursorSecret::generate();
        let next = PeerCursorSecret::generate();
        let cursor =
            RemoteHistoryCursor::mint("peer-x", "2026-01-02T03:04:05Z", 42, previous.as_bytes());
        assert!(matches!(
            cursor.decode("peer-x", next.as_bytes()),
            Err(PeerHistoryCursorError::SignatureMismatch)
        ));
    }

    #[test]
    fn cursor_rejects_replay_across_peers() {
        let secret = PeerCursorSecret::generate();
        let cursor =
            RemoteHistoryCursor::mint("peer-a", "2026-01-02T03:04:05Z", 42, secret.as_bytes());
        assert!(matches!(
            cursor.decode("peer-b", secret.as_bytes()),
            Err(PeerHistoryCursorError::Invalid)
        ));
    }

    #[test]
    fn cursor_rejects_garbage_input() {
        let secret = PeerCursorSecret::generate();
        let cursor = RemoteHistoryCursor::from_string("not-a-valid-cursor".to_string());
        assert!(matches!(
            cursor.decode("peer-x", secret.as_bytes()),
            Err(PeerHistoryCursorError::Invalid)
        ));
    }

    #[test]
    fn cursor_rejects_non_rfc3339_timestamp() {
        // Build a cursor whose timestamp is non-RFC 3339. The
        // signature is still valid (the secret signs the raw bytes
        // verbatim) but the timestamp parser refuses the payload.
        let secret = PeerCursorSecret::generate();
        let payload = "peer-x\nnot-a-date\n1";
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        let tag = mac.finalize().into_bytes();
        let cursor = RemoteHistoryCursor::from_string(format!(
            "{}::{}",
            URL_SAFE_NO_PAD.encode(payload.as_bytes()),
            URL_SAFE_NO_PAD.encode(tag),
        ));
        assert!(matches!(
            cursor.decode("peer-x", secret.as_bytes()),
            Err(PeerHistoryCursorError::MalformedTimestamp)
        ));
    }

    #[test]
    fn cursor_rejects_non_positive_id() {
        let secret = PeerCursorSecret::generate();
        let payload = "peer-x\n2026-01-02T03:04:05Z\n0";
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        let tag = mac.finalize().into_bytes();
        let cursor = RemoteHistoryCursor::from_string(format!(
            "{}::{}",
            URL_SAFE_NO_PAD.encode(payload.as_bytes()),
            URL_SAFE_NO_PAD.encode(tag),
        ));
        assert!(matches!(
            cursor.decode("peer-x", secret.as_bytes()),
            Err(PeerHistoryCursorError::MalformedId)
        ));
    }

    #[test]
    fn page_fingerprint_is_stable_for_same_header() {
        let first = compute_page_fingerprint("peer-x", "2026-01-01T00:00:00Z", 7, 3);
        let second = compute_page_fingerprint("peer-x", "2026-01-01T00:00:00Z", 7, 3);
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn page_fingerprint_changes_with_content() {
        let baseline = compute_page_fingerprint("peer-x", "2026-01-01T00:00:00Z", 7, 3);
        assert_ne!(
            baseline,
            compute_page_fingerprint("peer-x", "2026-01-02T00:00:00Z", 7, 3)
        );
        assert_ne!(
            baseline,
            compute_page_fingerprint("peer-x", "2026-01-01T00:00:00Z", 8, 3)
        );
        assert_ne!(
            baseline,
            compute_page_fingerprint("peer-x", "2026-01-01T00:00:00Z", 7, 4)
        );
        assert_ne!(
            baseline,
            compute_page_fingerprint("peer-y", "2026-01-01T00:00:00Z", 7, 3)
        );
    }

    #[test]
    fn snapshot_id_returns_persistence_unavailable_when_source_fails() {
        struct FailingSource;
        impl HostHistorySource for FailingSource {
            fn page_after(
                &self,
                _created_at: &str,
                _id: i64,
                _limit: usize,
            ) -> Result<Vec<EntryRecord>, PeerHistoryPersistenceError> {
                Err(PeerHistoryPersistenceError::Failed)
            }
            fn snapshot_id(&self) -> Result<String, PeerHistoryPersistenceError> {
                Err(PeerHistoryPersistenceError::Failed)
            }
        }
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        service.set_cursor_secret("peer-x", PeerCursorSecret::generate());
        let response = service.serve("peer-x", None, MAX_PAGE_ROWS as u32, &FailingSource);
        assert!(matches!(
            response,
            HostHistoryResponse::Unavailable("persistence_unavailable")
        ));
    }

    /// Noop transport the tests use when the assertion does not
    /// exercise the dial path. The transport returns
    /// [`PeerHistoryTransportError::Unavailable`] so the runtime
    /// collapses the outcome into
    /// [`PeerHistoryOutcome::TransportUnavailable`] without a
    /// network round-trip.
    struct NullPeerHistoryTransport;

    impl PeerHistoryTransport for NullPeerHistoryTransport {
        fn list_recent_text(
            &self,
            _request: ListRecentTextRequest,
        ) -> Result<ListRecentTextResponse, PeerHistoryTransportError> {
            Err(PeerHistoryTransportError::Unavailable)
        }
    }

    /// Scriptable transport the tests use to simulate a host
    /// response. The helper returns a single
    /// [`ListRecentTextResponse`] so the test surface stays small;
    /// the runtime forwards the snapshot_id verbatim and never
    /// builds one locally.
    struct ScriptedPeerHistoryTransport {
        response: parking_lot::Mutex<Result<ListRecentTextResponse, PeerHistoryTransportError>>,
        last_request: parking_lot::Mutex<Option<ListRecentTextRequest>>,
    }

    impl ScriptedPeerHistoryTransport {
        fn new(response: Result<ListRecentTextResponse, PeerHistoryTransportError>) -> Self {
            Self {
                response: parking_lot::Mutex::new(response),
                last_request: parking_lot::Mutex::new(None),
            }
        }
        fn last_request(&self) -> Option<ListRecentTextRequest> {
            self.last_request.lock().clone()
        }
    }

    impl PeerHistoryTransport for ScriptedPeerHistoryTransport {
        fn list_recent_text(
            &self,
            request: ListRecentTextRequest,
        ) -> Result<ListRecentTextResponse, PeerHistoryTransportError> {
            *self.last_request.lock() = Some(request);
            match self.response.lock().clone() {
                Ok(response) => Ok(response),
                Err(error) => Err(error),
            }
        }
    }

    fn page_with(
        rows: Vec<RemoteTextPreview>,
        next_cursor: Option<RemoteHistoryCursor>,
    ) -> ListRecentTextResponse {
        let snapshot_id = "f".repeat(64);
        ListRecentTextResponse {
            page: RemoteTextHistoryPage { rows, next_cursor },
            snapshot_id,
        }
    }

    #[test]
    fn browse_returns_peer_unavailable_when_state_missing() {
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        let outcome = service.browse("peer-x", "fingerprint", None, MAX_PAGE_ROWS as u32);
        assert!(matches!(
            outcome,
            PeerHistoryOutcome::PeerUnavailable {
                reason: "no_known_peer"
            }
        ));
    }

    #[test]
    fn browse_returns_peer_unavailable_when_state_not_trusted() {
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: false,
                active: false,
            },
        );
        let outcome = service.browse("peer-x", "fingerprint", None, MAX_PAGE_ROWS as u32);
        assert!(matches!(
            outcome,
            PeerHistoryOutcome::PeerUnavailable {
                reason: "not_trusted"
            }
        ));
    }

    /// Pin the typed distinction the spec scenario "Revoked /
    /// Blocked / not trusted / pin invalid / peer ausente"
    /// imposes: a peer whose local cache says `active = false`
    /// surfaces `not_active` (no network round-trip), while a
    /// peer whose transport rejection was triggered by a trust
    /// / secret condition (Revoked, Blocked, KeyMismatch,
    /// UnknownPeer) surfaces `not_trusted`. The two outcomes
    /// stay distinct so the renderer can render the matching
    /// copy without conflating them.
    #[test]
    fn browse_distinguishes_not_active_from_not_trusted_outcomes() {
        // Local cache says the peer is offline: no network call,
        // `not_active` is returned verbatim. The local cache
        // wins regardless of any transport signal.
        let offline_service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        offline_service.record_peer_state(
            "peer-offline",
            PeerActiveState {
                trusted: true,
                active: false,
            },
        );
        let outcome =
            offline_service.browse("peer-offline", "fingerprint", None, MAX_PAGE_ROWS as u32);
        assert!(
            matches!(
                outcome,
                PeerHistoryOutcome::PeerUnavailable {
                    reason: "not_active"
                }
            ),
            "active=false peer must surface not_active, got {outcome:?}"
        );

        // Local cache says the peer is online AND trusted, but
        // the transport rejected the page for a trust reason.
        // The runtime must surface `not_trusted`, NOT
        // `not_active` — the renderer needs the distinction to
        // render the correct copy without retrying blindly.
        for error in [
            PeerHistoryTransportError::Revoked,
            PeerHistoryTransportError::Blocked,
            PeerHistoryTransportError::KeyMismatch,
            PeerHistoryTransportError::UnknownPeer,
        ] {
            let transport = Arc::new(ScriptedPeerHistoryTransport::new(Err(error.clone())));
            let service = PeerTextHistoryService::new(transport);
            service.record_peer_state(
                "peer-x",
                PeerActiveState {
                    trusted: true,
                    active: true,
                },
            );
            let outcome = service.browse("peer-x", "fingerprint", None, MAX_PAGE_ROWS as u32);
            assert!(
                matches!(
                    outcome,
                    PeerHistoryOutcome::PeerUnavailable {
                        reason: "not_trusted"
                    }
                ),
                "trust rejection {error:?} must surface not_trusted (got {outcome:?})"
            );
        }
    }

    #[test]
    fn browse_returns_peer_unavailable_when_state_not_active() {
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: false,
            },
        );
        let outcome = service.browse("peer-x", "fingerprint", None, MAX_PAGE_ROWS as u32);
        assert!(matches!(
            outcome,
            PeerHistoryOutcome::PeerUnavailable {
                reason: "not_active"
            }
        ));
    }

    #[test]
    fn browse_forwards_opaque_cursor_without_decoding() {
        // The client side treats the cursor as fully opaque: the
        // HMAC secret lives on the host that minted the cursor
        // and only the host validates it. A bogus, truncated or
        // rotated cursor therefore travels over the wire and the
        // host surfaces the typed `invalid_cursor` reason.
        let transport = Arc::new(ScriptedPeerHistoryTransport::new(Ok(page_with(
            vec![],
            None,
        ))));
        let service = PeerTextHistoryService::new(transport.clone());
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );
        // No secret registered: the client never inspects the
        // secret at all.
        let cursor = RemoteHistoryCursor::from_string("forged".to_string());
        let outcome = service.browse("peer-x", "fingerprint", Some(&cursor), MAX_PAGE_ROWS as u32);
        assert!(matches!(outcome, PeerHistoryOutcome::Ok { .. }));
        let request = transport.last_request().expect("request captured");
        // The transport receives the opaque cursor verbatim.
        assert_eq!(request.cursor, "forged");
    }

    #[test]
    fn browse_dials_transport_and_returns_page() {
        let transport = Arc::new(ScriptedPeerHistoryTransport::new(Ok(page_with(
            vec![RemoteTextPreview {
                remote_entry_id: "entry-7".to_string(),
                title: Some("title".to_string()),
                content_type: "text".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                preview: "hello".to_string(),
            }],
            None,
        ))));
        let service = PeerTextHistoryService::new(transport.clone());
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );
        service.set_cursor_secret("peer-x", PeerCursorSecret::generate());
        let outcome = service.browse("peer-x", "fingerprint", None, MAX_PAGE_ROWS as u32);
        let PeerHistoryOutcome::Ok { page, snapshot_id } = outcome else {
            panic!("expected Ok variant");
        };
        assert_eq!(page.rows.len(), 1);
        assert!(page.next_cursor.is_none());
        assert_eq!(snapshot_id.len(), 64);
        let request = transport.last_request().expect("request captured");
        assert_eq!(request.peer_id, "peer-x");
        assert_eq!(request.cert_fingerprint, "fingerprint");
        assert_eq!(request.cursor, "");
    }

    #[test]
    fn browse_does_not_require_a_local_cursor_secret() {
        // The client never consults the cursor secret: a peer
        // whose secret was rotated, lost or never minted still
        // accepts a `browse` request because the validation lives
        // on the remote host. The transport receives the cursor
        // verbatim and the host returns the typed outcome.
        let transport = Arc::new(ScriptedPeerHistoryTransport::new(Ok(page_with(
            vec![],
            None,
        ))));
        let service = PeerTextHistoryService::new(transport.clone());
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );
        let cursor = RemoteHistoryCursor::from_string("any-cursor".to_string());
        let outcome = service.browse("peer-x", "fingerprint", Some(&cursor), MAX_PAGE_ROWS as u32);
        assert!(matches!(outcome, PeerHistoryOutcome::Ok { .. }));
        let request = transport.last_request().expect("request captured");
        assert_eq!(request.cursor, "any-cursor");
    }

    #[test]
    fn browse_translates_transport_errors_into_outcomes() {
        for (error, expected_reason) in [
            // Trust / secret rejections from the remote host collapse
            // into the typed `not_trusted` reason so the renderer can
            // distinguish "the host refuses the caller" from "the
            // local cache says the peer is offline".
            (
                PeerHistoryTransportError::UnknownPeer,
                PeerHistoryOutcome::PeerUnavailable {
                    reason: "not_trusted",
                },
            ),
            (
                PeerHistoryTransportError::Revoked,
                PeerHistoryOutcome::PeerUnavailable {
                    reason: "not_trusted",
                },
            ),
            (
                PeerHistoryTransportError::Blocked,
                PeerHistoryOutcome::PeerUnavailable {
                    reason: "not_trusted",
                },
            ),
            (
                PeerHistoryTransportError::KeyMismatch,
                PeerHistoryOutcome::PeerUnavailable {
                    reason: "not_trusted",
                },
            ),
            (
                PeerHistoryTransportError::InvalidCursor,
                PeerHistoryOutcome::InvalidCursor,
            ),
            (
                PeerHistoryTransportError::IncompatibleProtocol,
                PeerHistoryOutcome::TransportUnavailable {
                    reason: "incompatible_protocol",
                },
            ),
            (
                PeerHistoryTransportError::Malformed,
                PeerHistoryOutcome::TransportUnavailable {
                    reason: "malformed",
                },
            ),
            (
                PeerHistoryTransportError::Unavailable,
                PeerHistoryOutcome::TransportUnavailable {
                    reason: "unavailable",
                },
            ),
            (
                PeerHistoryTransportError::PeerUnresolved,
                PeerHistoryOutcome::TransportUnavailable {
                    reason: "unavailable",
                },
            ),
        ] {
            let transport = Arc::new(ScriptedPeerHistoryTransport::new(Err(error.clone())));
            let service = PeerTextHistoryService::new(transport);
            service.record_peer_state(
                "peer-x",
                PeerActiveState {
                    trusted: true,
                    active: true,
                },
            );
            service.set_cursor_secret("peer-x", PeerCursorSecret::generate());
            let outcome = service.browse("peer-x", "fingerprint", None, MAX_PAGE_ROWS as u32);
            assert_eq!(outcome, expected_reason, "transport error: {error:?}");
        }
    }

    #[test]
    fn serve_projects_first_page_with_signed_next_cursor() {
        // The host emits a `next_cursor` ONLY when the page is
        // exactly `MAX_PAGE_ROWS` rows long — otherwise the
        // previous page did not exhaust the host and the renderer
        // does not need to fetch again. Build a full page + 1 so
        // the host mints a next cursor that the cursor secret
        // verifies.
        let mut entries = Vec::new();
        for id in 1..=(MAX_PAGE_ROWS + 1) as i64 {
            entries.push(record(
                id,
                ContentType::Text,
                &format!("row-{id}"),
                "2026-01-01T00:00:00Z",
            ));
        }
        let source = InMemoryHostHistorySource::new();
        source.seed(entries);
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        let secret = PeerCursorSecret::generate();
        service.set_cursor_secret("peer-x", secret);
        let response = service.serve("peer-x", None, MAX_PAGE_ROWS as u32, &source);
        let HostHistoryResponse::Ok(page, snapshot_id) = response else {
            panic!("expected Ok response");
        };
        assert_eq!(page.rows.len(), MAX_PAGE_ROWS);
        // The host orders by created_at DESC, id DESC, so the page starts
        // with `id = MAX_PAGE_ROWS` (the newest) and ends with
        // `id = 2` (the 50th row, since we built 51 entries).
        let previews: Vec<&str> = page.rows.iter().map(|row| row.preview.as_str()).collect();
        assert_eq!(previews.first().copied(), Some("row-51"));
        assert_eq!(previews.last().copied(), Some("row-2"));
        let cursor = page.next_cursor.expect("next cursor emitted");
        let (ts, id) = cursor
            .decode("peer-x", secret.as_bytes())
            .expect("cursor decodes with the secret");
        assert_eq!(ts, "2026-01-01T00:00:00Z");
        assert_eq!(id, 2);
        assert_eq!(snapshot_id.len(), 64);
    }

    #[test]
    fn serve_rejects_invalid_cursor() {
        let source = InMemoryHostHistorySource::new();
        source.seed(vec![record(
            1,
            ContentType::Text,
            "new",
            "2026-01-01T00:00:00Z",
        )]);
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        service.set_cursor_secret("peer-x", PeerCursorSecret::generate());
        let cursor = RemoteHistoryCursor::from_string("forged".to_string());
        let response = service.serve("peer-x", Some(&cursor), MAX_PAGE_ROWS as u32, &source);
        assert!(matches!(response, HostHistoryResponse::InvalidCursor));
    }

    #[test]
    fn serve_rejects_when_secret_is_missing() {
        let source = InMemoryHostHistorySource::new();
        source.seed(vec![record(
            1,
            ContentType::Text,
            "new",
            "2026-01-01T00:00:00Z",
        )]);
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        let response = service.serve("peer-x", None, MAX_PAGE_ROWS as u32, &source);
        assert!(matches!(
            response,
            HostHistoryResponse::Unavailable("not_trusted")
        ));
    }

    #[test]
    fn serve_excludes_image_and_html_but_keeps_rich_plain_preview() {
        let entries = vec![
            record(1, ContentType::Text, "transferable", "2026-01-01T00:00:00Z"),
            image_record(2),
        ];
        let mut rich = record(3, ContentType::Text, "rich", "2026-01-01T00:00:00Z");
        rich.rich_text_hash = Some("a".repeat(64));
        rich.rich_html_ref = Some("rich-text/a.html".to_string());
        let html = record(4, ContentType::Html, "<p>html</p>", "2026-01-01T00:00:00Z");
        let source = InMemoryHostHistorySource::new();
        source.seed(vec![entries[0].clone(), entries[1].clone(), rich, html]);
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        service.set_cursor_secret("peer-x", PeerCursorSecret::generate());
        let response = service.serve("peer-x", None, MAX_PAGE_ROWS as u32, &source);
        let HostHistoryResponse::Ok(page, _) = response else {
            panic!("expected Ok response");
        };
        assert_eq!(page.rows.len(), 2);
        let previews = page
            .rows
            .iter()
            .map(|row| row.preview.as_str())
            .collect::<Vec<_>>();
        assert!(previews.contains(&"transferable"));
        assert!(previews.contains(&"rich"));
    }

    #[test]
    fn serve_does_not_expose_body_hashes_or_source_metadata() {
        let mut entry = record(7, ContentType::Text, "<b>html</b>", "2026-01-01T00:00:00Z");
        entry.title = Some("hello".to_string());
        entry.source_app = Some("com.example.Editor".to_string());
        entry.content_hash = "deadbeef".repeat(8);
        let source = InMemoryHostHistorySource::new();
        source.seed(vec![entry]);
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        service.set_cursor_secret("peer-x", PeerCursorSecret::generate());
        let response = service.serve("peer-x", None, MAX_PAGE_ROWS as u32, &source);
        let HostHistoryResponse::Ok(page, _) = response else {
            panic!("expected Ok response");
        };
        let row = &page.rows[0];
        assert!(row.preview.contains("&lt;b&gt;"));
        assert!(!row.preview.contains("com.example.Editor"));
        assert!(!row.preview.contains("hello"));
        assert!(!row.preview.contains("deadbeef"));
    }

    #[test]
    fn forget_peer_drops_state_and_re_routes_to_no_known_peer() {
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );
        service.forget_peer("peer-x");
        let outcome = service.browse("peer-x", "fingerprint", None, MAX_PAGE_ROWS as u32);
        assert!(matches!(
            outcome,
            PeerHistoryOutcome::PeerUnavailable {
                reason: "no_known_peer"
            }
        ));
    }

    /// Sanity pin for the page-size helper the host and the
    /// adapter share. The clamp collapses `0` to `1` (so the host
    /// never returns zero rows by accident) and trims any value
    /// above [`MAX_PAGE_ROWS`] to the documented cap; any
    /// intermediate value is forwarded unchanged so the renderer
    /// can ask for a tiny preview window without altering the
    /// typed outcome surface.
    #[test]
    fn clamp_history_limit_keeps_pages_within_the_wire_contract() {
        assert_eq!(clamp_history_limit(0), 1);
        assert_eq!(clamp_history_limit(1), 1);
        assert_eq!(clamp_history_limit(7), 7);
        assert_eq!(clamp_history_limit(MAX_PAGE_ROWS as u32), MAX_PAGE_ROWS);
        assert_eq!(clamp_history_limit(MAX_PAGE_ROWS as u32 + 1), MAX_PAGE_ROWS);
        assert_eq!(
            clamp_history_limit(u32::MAX),
            MAX_PAGE_ROWS,
            "out-of-range limits must clamp to MAX_PAGE_ROWS"
        );
    }

    /// `next_cursor` must be minted ONLY when more rows remain
    /// after the page, NOT merely because the page reached the
    /// requested size. The host asks the persistence layer for
    /// `limit + 1` rows internally; when the projection receives
    /// only `limit` rows back there is no follow-up page and the
    /// cursor collapses to `None`.
    #[test]
    fn serve_omits_next_cursor_when_no_more_rows_remain() {
        let source = InMemoryHostHistorySource::new();
        source.seed(
            (1..=5_i64)
                .map(|id| {
                    record(
                        id,
                        ContentType::Text,
                        &format!("row-{id}"),
                        "2026-01-01T00:00:00Z",
                    )
                })
                .collect(),
        );
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        service.set_cursor_secret("peer-x", PeerCursorSecret::generate());
        // Caller asks for the cap, the host has exactly the cap
        // entries: a previous implementation would mint a next
        // cursor here, forcing the renderer to fetch an empty
        // page.
        let response = service.serve("peer-x", None, 5, &source);
        let HostHistoryResponse::Ok(page, snapshot_id) = response else {
            panic!("expected Ok response");
        };
        assert_eq!(page.rows.len(), 5);
        assert!(
            page.next_cursor.is_none(),
            "limit=5 with 5 rows on the host must not emit a next cursor"
        );
        assert_eq!(snapshot_id.len(), 64);
    }

    /// When the host has exactly one more row than the requested
    /// limit, the projection must emit a next cursor that lets
    /// the renderer fetch the trailing row.
    #[test]
    fn serve_emits_next_cursor_when_more_rows_remain() {
        let mut entries = Vec::new();
        for id in 1..=6_i64 {
            entries.push(record(
                id,
                ContentType::Text,
                &format!("row-{id}"),
                "2026-01-01T00:00:00Z",
            ));
        }
        let source = InMemoryHostHistorySource::new();
        source.seed(entries);
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        service.set_cursor_secret("peer-x", PeerCursorSecret::generate());
        let response = service.serve("peer-x", None, 5, &source);
        let HostHistoryResponse::Ok(page, _) = response else {
            panic!("expected Ok response");
        };
        assert_eq!(page.rows.len(), 5);
        assert!(
            page.next_cursor.is_some(),
            "limit=5 with 6 rows on the host must emit a next cursor"
        );
    }

    /// Page-size cap protection: the host must never return more
    /// rows than the caller requested even if `limit + 1` rows
    /// were available from the persistence layer. The probe is
    /// bounded by [`history_lookahead_limit`] and the
    /// [`PeerTextHistoryHostHandlerAdapter`] falls back to a
    /// defensive `.take()` before serialising the wire envelope.
    #[test]
    fn serve_returns_no_more_rows_than_the_requested_limit() {
        let mut entries = Vec::new();
        for id in 1..=60_i64 {
            entries.push(record(
                id,
                ContentType::Text,
                &format!("row-{id}"),
                "2026-01-01T00:00:00Z",
            ));
        }
        let source = InMemoryHostHistorySource::new();
        source.seed(entries);
        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        service.set_cursor_secret("peer-x", PeerCursorSecret::generate());
        let response = service.serve("peer-x", None, 7, &source);
        let HostHistoryResponse::Ok(page, _) = response else {
            panic!("expected Ok response");
        };
        assert_eq!(page.rows.len(), 7);
    }

    /// The transport must forward the host-emitted `snapshot_id`
    /// verbatim. The runtime must NEVER reconstruct the
    /// fingerprint from the rows it received; a partial
    /// reconstruction would diverge from the host the moment a
    /// capture is added or removed between page requests.
    #[test]
    fn browse_forwards_host_snapshot_id_without_recomputing_it() {
        let host_fingerprint = "a".repeat(64);
        let transport = Arc::new(ScriptedPeerHistoryTransport::new(Ok(
            ListRecentTextResponse {
                page: RemoteTextHistoryPage {
                    rows: vec![RemoteTextPreview {
                        remote_entry_id: "entry-7".to_string(),
                        title: Some("title".to_string()),
                        content_type: "text".to_string(),
                        created_at: "2026-01-01T00:00:00Z".to_string(),
                        preview: "hello".to_string(),
                    }],
                    next_cursor: None,
                },
                snapshot_id: host_fingerprint.clone(),
            },
        )));
        let service = PeerTextHistoryService::new(transport);
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.browse("peer-x", "fingerprint", None, MAX_PAGE_ROWS as u32);
        let PeerHistoryOutcome::Ok {
            page: _,
            snapshot_id,
        } = outcome
        else {
            panic!("expected Ok variant");
        };
        assert_eq!(snapshot_id, host_fingerprint);
    }

    /// Productive `HistoryHostHandler` adapter unit test. The
    /// adapter the bootstrap installs on the pairing transport
    /// MUST return a typed `InvalidCursor` when the host
    /// cannot verify the HMAC, a typed `Unavailable` when no
    /// secret is registered for the peer (the row left the
    /// trusted state without the runtime clearing the cache),
    /// and a typed `Unavailable` when the projection layer
    /// cannot read SQLite. The test pins every outcome so a
    /// regression that flattens the contract surfaces here.
    #[cfg(feature = "local-peer-pairing-tls")]
    #[test]
    fn productive_host_handler_adapter_returns_typed_outcomes() {
        use crate::peer_pairing::PeerTextHistoryHostHandlerAdapter;
        use clipvault_platform::peer_transport::HistoryHostHandler as _;
        use clipvault_platform::peer_transport::HistoryHostResponse;

        let mut entries = Vec::new();
        for id in 1..=MAX_PAGE_ROWS as i64 + 1 {
            // The body is intentionally large so the preview
            // truncates the trailing secret marker.
            let secret_body = format!(
                "PUBLIC_PREFIX_{id}_{}__FULL_BODY_THAT_NEVER_APPEARS__",
                "secret_suffix_that_must_never_appear_in_preview_".repeat(20)
            );
            entries.push(record(
                id,
                ContentType::Text,
                &secret_body,
                "2026-01-01T00:00:00Z",
            ));
        }
        let source = InMemoryHostHistorySource::new();
        source.seed(entries);

        let service = PeerTextHistoryService::new(Arc::new(NullPeerHistoryTransport));
        let secret = PeerCursorSecret::generate();
        service.set_cursor_secret("peer-x", secret);
        let adapter = PeerTextHistoryHostHandlerAdapter::new(service.clone(), Arc::new(source));

        // First page mints a `next_cursor` and returns exactly
        // `MAX_PAGE_ROWS` rows. The wire payload carries no body,
        // hash or secret material.
        let first = adapter.list_recent_text("peer-x", "", MAX_PAGE_ROWS as u32);
        let HistoryHostResponse::Ok {
            rows, next_cursor, ..
        } = first
        else {
            panic!("expected Ok response");
        };
        assert_eq!(rows.len(), MAX_PAGE_ROWS);
        assert!(!next_cursor.is_empty());
        let serialized = format!("{rows:?}");
        // The full body must never appear in the wire payload.
        // The preview is bounded to PREVIEW_MAX_CHARS so the
        // unique trailing secret marker the projection slices
        // past the 300-char cursor must never surface in the
        // response.
        for forbidden in ["__FULL_BODY_THAT_NEVER_APPEARS__", "hash-01", "deadbeef"] {
            assert!(
                !serialized.contains(forbidden),
                "first page leaked {forbidden} into the wire payload",
            );
        }

        // Second page with the signed cursor returns the
        // remaining rows and never mints a follow-up cursor
        // (only one row remains after the first page).
        let first_page_cursor = next_cursor.clone();
        let second = adapter.list_recent_text("peer-x", &first_page_cursor, MAX_PAGE_ROWS as u32);
        let HistoryHostResponse::Ok {
            rows, next_cursor, ..
        } = second
        else {
            panic!("expected Ok response on second page");
        };
        assert_eq!(rows.len(), 1);
        assert!(next_cursor.is_empty());

        // Manipulated cursor collapses to a typed
        // `InvalidCursor`. The adapter never leaks the decoded
        // timestamp or id back to the caller.
        let tampered =
            manipulate_cursor(&RemoteHistoryCursor::from_string(first_page_cursor.clone()));
        let tampered_response = adapter.list_recent_text("peer-x", &tampered, MAX_PAGE_ROWS as u32);
        assert!(matches!(
            tampered_response,
            HistoryHostResponse::InvalidCursor,
        ));

        // Cursor minted under a different peer collapses to
        // `InvalidCursor` because the canonical payload
        // carries the wrong peer_id.
        let foreign_secret = PeerCursorSecret::generate();
        let foreign_cursor = RemoteHistoryCursor::mint(
            "peer-y",
            "2026-01-01T00:00:30Z",
            30,
            foreign_secret.as_bytes(),
        );
        assert!(matches!(
            adapter.list_recent_text("peer-x", foreign_cursor.as_str(), MAX_PAGE_ROWS as u32),
            HistoryHostResponse::InvalidCursor,
        ));

        // Rotated secret collapses to `InvalidCursor` exactly
        // like the productive path documents.
        service.set_cursor_secret("peer-x", PeerCursorSecret::generate());
        let first_after_rotation =
            adapter.list_recent_text("peer-x", &first_page_cursor, MAX_PAGE_ROWS as u32);
        assert!(matches!(
            first_after_rotation,
            HistoryHostResponse::InvalidCursor
        ));

        // No-secret state collapses to `Unavailable("not_trusted")`
        // so the wire contract stays stable even when the row
        // leaves the trusted state without the runtime clearing
        // the cache.
        service.clear_cursor_secret("peer-x");
        assert!(matches!(
            adapter.list_recent_text("peer-x", "", MAX_PAGE_ROWS as u32),
            HistoryHostResponse::Unavailable {
                reason: "not_trusted"
            },
        ));
    }

    /// Tamper helper the productive adapter test uses to
    /// fabricate a cursor whose HMAC no longer verifies. The
    /// helper keeps the cursor shape (`::` separator) so the
    /// listener reaches the typed HMAC check instead of failing
    /// on a free-form decode error.
    #[cfg(feature = "local-peer-pairing-tls")]
    fn manipulate_cursor(cursor: &RemoteHistoryCursor) -> String {
        let mut bytes = cursor.as_str().as_bytes().to_vec();
        for byte in bytes.iter_mut() {
            let ch = *byte as char;
            if ch.is_ascii_hexdigit() {
                *byte = if ch == '0' { b'1' } else { b'0' };
                break;
            }
        }
        String::from_utf8(bytes).expect("cursor must remain utf-8")
    }

    /// Suppress the unused helper warning when the local-peer-
    /// pairing-tls feature gate is enabled and the helper above
    /// is the only place the symbol appears. The helper is
    /// kept around for future re-use but currently the
    /// `manipulate_cursor` helper above is what the test calls.
    #[cfg(feature = "local-peer-pairing-tls")]
    #[allow(dead_code)]
    fn flip_first_hex_digit(cursor: &str) -> String {
        manipulate_cursor(&RemoteHistoryCursor::from_string(cursor.to_string()))
    }

    /// Productive end-to-end test: two real `TlsPeerTransport`
    /// listeners on `127.0.0.1`, the
    /// [`PeerTextHistoryHostHandlerAdapter`] the bootstrap
    /// installs on the host, a [`PeerTextHistoryService`] whose
    /// [`PeerPairingHistoryTransportAdapter`] dials the remote
    /// listener through mTLS, a real [`PeerCursorSecret`] the host
    /// mints on trust promotion, an [`InMemoryHostHistorySource`]
    /// the host projects from, and the typed outcomes the spec
    /// pins (`Ok`, `InvalidCursor`, `PeerUnavailable`,
    /// `TransportUnavailable`). The test is the productive evidence
    /// the OpenSpec 2.6 reopening asks for; it does NOT use a
    /// sentinel cursor, a synthetic `HistoryHostHandler` or any
    /// `peer_pairing` fake — every byte that crosses the wire is
    /// produced by the real mTLS listener / dial loop.
    ///
    /// Coverage:
    ///
    /// 1. First page over mTLS: the host returns the bounded
    ///    `MAX_PAGE_ROWS` rows newest-first and mints a real
    ///    HMAC-SHA256 `next_cursor` signed with the
    ///    `PeerCursorSecret` the host owns.
    /// 2. Second page through the HMAC-signed cursor the host
    ///    just minted: the cursor round-trips end-to-end.
    /// 3. Smaller page size (`limit = 7`): the host honours the
    ///    requested limit and emits a `next_cursor` only when more
    ///    rows remain.
    /// 4. Forged cursor (`definitely-not-a-host-cursor`):
    ///    the adapter collapses to the typed `InvalidCursor` and
    ///    the client surfaces it as
    ///    [`PeerHistoryOutcome::InvalidCursor`] (NOT
    ///    `TransportUnavailable` or `Malformed`).
    /// 5. Cursor signed for another peer (`peer-y` minted with
    ///    a different secret): the host's HMAC check rejects
    ///    the cross-peer replay and surfaces `InvalidCursor`.
    /// 6. Cursor minted under a rotated / replaced secret: the
    ///    host rotates the secret mid-test and the previously
    ///    valid cursor collapses to `InvalidCursor`.
    /// 7. No-secret state: clearing the host's secret collapses
    ///    to `Unavailable("not_trusted")` and the client surfaces
    ///    `PeerUnavailable`.
    /// 8. No-handler state: a fresh host listener without the
    ///    adapter collapses to the typed `TransportUnavailable`
    ///    outcome so the renderer never confuses the
    ///    `not_available` reason with a cursor failure.
    /// 9. Persistence failure: an `InMemoryHostHistorySource`
    ///    the test can flip into a failing state collapses to
    ///    `Unavailable("persistence_unavailable")` and the
    ///    client surfaces the typed outcome without retrying.
    /// 10. Wrong pin: a mismatched cert fingerprint is rejected
    ///     by the mTLS handshake before the protocol layer sees
    ///     any bytes.
    ///
    /// The test also asserts the wire payload the client surfaces
    /// never carries the HMAC secret, the entry body, the entry
    /// hash, the host's bound port, the loopback IP or the pinned
    /// cert fingerprint — every assertion a future regression that
    /// leaks metadata would break.
    #[cfg(feature = "local-peer-pairing-tls")]
    #[test]
    fn productive_core_history_round_trip_over_two_real_tls_transports() {
        use crate::peer_pairing::PeerTextHistoryHostHandlerAdapter;
        use crate::peer_text_history::PeerPairingHistoryTransportAdapter;
        use clipvault_platform::peer_transport::derive_cert_fingerprint;
        use clipvault_platform::peer_transport::tls::{
            install_with_material_and_resolver, install_with_material_resolver_and_history,
            RecordingAdvertisementSink,
        };
        use clipvault_platform::peer_transport::PeerTransportObservation;
        use clipvault_platform::peer_transport::{
            PairingAdvertisementSink, PeerTransport as _, RemotePeerResolver, TlsPeerTransport,
            TransportSink,
        };
        use clipvault_platform::LocalIdentityMaterial;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        use std::sync::Mutex as StdMutex;

        const ROWS: i64 = MAX_PAGE_ROWS as i64 + 3;

        // Build the host + client identity material from
        // deterministic seeds so the test does not depend on
        // the OS RNG. `LocalIdentityMaterial::from_seed`
        // mints a fresh keypair + cert every call; we hold the
        // two materials so we can arm pins against the cert
        // fingerprints they expose.
        let host_material = LocalIdentityMaterial::from_seed([0xA1u8; 32]).expect("host material");
        let client_material =
            LocalIdentityMaterial::from_seed([0xB1u8; 32]).expect("client material");
        let host_peer_id = host_material.identity().peer_id.to_string();
        let client_peer_id = client_material.identity().peer_id.to_string();
        let host_cert_fingerprint = derive_cert_fingerprint(host_material.cert_der());
        let client_cert_fingerprint = derive_cert_fingerprint(client_material.cert_der());

        // Host source: a `toggle`-able failure source the test
        // uses to assert the persistence-failure outcome. The
        // `failing` flag flips between the two pages so the
        // first page succeeds and the second collapses to
        // `Unavailable("persistence_unavailable")`.
        struct ToggleSource {
            entries: Vec<clipvault_db::EntryRecord>,
            failing: StdMutex<bool>,
        }
        impl HostHistorySource for ToggleSource {
            fn page_after(
                &self,
                created_at: &str,
                id: i64,
                limit: usize,
            ) -> Result<Vec<EntryRecord>, PeerHistoryPersistenceError> {
                if *self.failing.lock().expect("fail lock") {
                    return Err(PeerHistoryPersistenceError::Failed);
                }
                let mut matching: Vec<EntryRecord> = self
                    .entries
                    .iter()
                    .filter(|entry| entry_is_transferable(entry))
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
            fn snapshot_id(&self) -> Result<String, PeerHistoryPersistenceError> {
                if *self.failing.lock().expect("fail lock") {
                    return Err(PeerHistoryPersistenceError::Failed);
                }
                Ok(compute_page_fingerprint(
                    "local",
                    "2026-01-01T00:00:00Z",
                    ROWS,
                    ROWS as usize,
                ))
            }
        }

        let mut entries = Vec::new();
        for id in 1..=ROWS {
            // Each row embeds the `SECRET_BODY` marker AFTER
            // enough filler text that the bounded preview
            // (PREVIEW_MAX_CHARS = 300) truncates the marker
            // before it reaches the wire. The wire payload
            // assertion below confirms the marker never
            // surfaces in `format!("{payload_outcome:?}")` —
            // the productive `PeerTextHistoryService::serve`
            // project is bounded by the runtime, not by the
            // transport. Keeping the body short overall keeps
            // the envelope below the platform's
            // `MAX_INBOUND_PAYLOAD` cap (8 KiB) so the
            // productive transport can carry a full
            // `MAX_PAGE_ROWS`-sized page in a single
            // envelope.
            let filler = "a".repeat(PREVIEW_MAX_CHARS);
            let body = format!("row-{id} {filler} SECRET_BODY_THAT_MUST_NOT_LEAK");
            entries.push(record(id, ContentType::Text, &body, "2026-01-01T00:00:00Z"));
        }
        let source = Arc::new(ToggleSource {
            entries,
            failing: StdMutex::new(false),
        });

        // Productive host-side wiring:
        // - `PeerTextHistoryService` with a real
        //   `PeerCursorSecret` the host owns;
        // - `PeerTextHistoryHostHandlerAdapter` translates
        //   `HistoryHostHandler::list_recent_text` into
        //   `PeerTextHistoryService::serve` over the host source.
        // The host side never dials itself; the host service
        // uses the noop history transport as a safe fallback so
        // the `set_cursor_secret` / `cursor_secret` API works
        // even though only `serve` is exercised on the host side.
        // The secret is keyed by the **dialer's** peer_id
        // (`client_peer_id`): when the host mints a cursor for a
        // page that the client just requested, the cursor must
        // validate against the secret that the host stored for
        // that peer — never against the host's own identity.
        let host_service = PeerTextHistoryService::new(Arc::new(NoopPeerHistoryTransport));
        let host_secret = PeerCursorSecret::generate();
        let host_secret_hex = host_secret.to_hex();
        host_service.set_cursor_secret(&client_peer_id, host_secret);
        let host_adapter: Arc<dyn clipvault_platform::peer_transport::HistoryHostHandler> =
            Arc::new(PeerTextHistoryHostHandlerAdapter::new(
                host_service.clone(),
                source.clone() as Arc<dyn HostHistorySource>,
            ));

        // Install the host listener with the productive handler
        // already wired so the very first inbound
        // `ListRecentText` envelope lands on
        // `PeerTextHistoryService::serve` over the toggle source.
        struct NoOpSink;
        impl TransportSink for NoOpSink {
            fn on_pairing_observed(&self, _observation: PeerTransportObservation) {}
        }
        let host_transport = Arc::new(TlsPeerTransport::new());
        let host_advertisement: Arc<dyn PairingAdvertisementSink> =
            Arc::new(RecordingAdvertisementSink::new());
        let host_sink: Arc<dyn TransportSink> = Arc::new(NoOpSink);
        let host_port = install_with_material_resolver_and_history(
            host_transport.as_ref(),
            host_material.clone(),
            "host".to_string(),
            host_advertisement,
            host_sink,
            None,
            Some(Arc::clone(&host_adapter)),
            None,
            None,
            None,
        )
        .expect("install host");

        // Client-side wiring:
        // - `PeerTextHistoryService` with a real
        //   `PeerPairingHistoryTransportAdapter` driving the
        //   productive mTLS dial loop;
        // - the resolver points at the host's loopback port.
        let client_transport = Arc::new(TlsPeerTransport::new());
        let host_port_slot: Arc<parking_lot::Mutex<Option<u16>>> =
            Arc::new(parking_lot::Mutex::new(Some(host_port)));
        struct HostPortResolver {
            host: Arc<parking_lot::Mutex<Option<u16>>>,
            peer_id: String,
        }
        impl RemotePeerResolver for HostPortResolver {
            fn resolve(&self, peer_id: &str) -> Option<SocketAddr> {
                if peer_id != self.peer_id {
                    return None;
                }
                let guard = self.host.lock();
                let port = *guard;
                Some(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    port?,
                ))
            }
        }
        let resolver: Arc<dyn RemotePeerResolver> = Arc::new(HostPortResolver {
            host: Arc::clone(&host_port_slot),
            peer_id: host_peer_id.clone(),
        });
        let client_advertisement: Arc<dyn PairingAdvertisementSink> =
            Arc::new(RecordingAdvertisementSink::new());
        let client_sink: Arc<dyn TransportSink> = Arc::new(NoOpSink);
        install_with_material_and_resolver(
            client_transport.as_ref(),
            client_material.clone(),
            "client".to_string(),
            client_advertisement,
            client_sink,
            Some(resolver),
        )
        .expect("install client");

        // Arm the pins so the productive dial completes the
        // mTLS handshake against the host listener.
        client_transport
            .arm_pin(&host_peer_id, &host_cert_fingerprint)
            .expect("arm pin (client -> host)");
        host_transport
            .arm_pin(&client_peer_id, &client_cert_fingerprint)
            .expect("arm pin (host -> client)");

        // Yield so the host accept loop polls at least once
        // before the dialer fires.
        std::thread::sleep(std::time::Duration::from_millis(150));

        // Client-side facade: the productive dial driver the
        // shell uses, wired against the client `TlsPeerTransport`
        // we just installed. Marking the peer active so the
        // `browse` call reaches the dial loop.
        let client_history_transport: Arc<dyn PeerHistoryTransport> = Arc::new(
            PeerPairingHistoryTransportAdapter::new(client_transport.clone()),
        );
        let client_service = PeerTextHistoryService::new(client_history_transport);
        client_service.record_peer_state(
            &host_peer_id,
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );

        // ----------------------------------------------------------------
        // 1. First page over mTLS — host returns the bounded
        //    page and mints a real HMAC-SHA256 `next_cursor`.
        //    The seeded source carries `ROWS` entries (> limit)
        //    so the host mints a cursor the second page can
        //    verify. Requesting `MAX_PAGE_ROWS` makes the serialized ACK
        //    larger than the pairing frame, so this is also the productive
        //    regression for the history-specific response budget.
        // ----------------------------------------------------------------
        let first_outcome = client_service.browse(
            &host_peer_id,
            &host_cert_fingerprint,
            None,
            MAX_PAGE_ROWS as u32,
        );
        let PeerHistoryOutcome::Ok {
            page: first_page,
            snapshot_id: first_snapshot_id,
        } = first_outcome
        else {
            panic!("first page must return Ok variant, got {first_outcome:?}");
        };
        assert_eq!(first_page.rows.len(), MAX_PAGE_ROWS);
        assert_eq!(first_snapshot_id.len(), 64);
        let first_next_cursor = first_page
            .next_cursor
            .as_ref()
            .expect("first page mints a cursor when more rows remain");

        // ----------------------------------------------------------------
        // 2. Second page through the HMAC-signed cursor.
        // ----------------------------------------------------------------
        let second_outcome = client_service.browse(
            &host_peer_id,
            &host_cert_fingerprint,
            Some(first_next_cursor),
            MAX_PAGE_ROWS as u32,
        );
        let PeerHistoryOutcome::Ok {
            page: second_page, ..
        } = second_outcome
        else {
            panic!("second page must return Ok variant");
        };
        assert_eq!(second_page.rows.len(), 3);
        assert!(second_page.next_cursor.is_none());

        // ----------------------------------------------------------------
        // 3. Smaller page size — `limit = 3` over `ROWS` rows
        //    so a follow-up cursor mints again.
        // ----------------------------------------------------------------
        let small_outcome = client_service.browse(&host_peer_id, &host_cert_fingerprint, None, 3);
        let PeerHistoryOutcome::Ok {
            page: small_page, ..
        } = small_outcome
        else {
            panic!("small page must return Ok variant");
        };
        assert_eq!(small_page.rows.len(), 3);
        let small_cursor = small_page
            .next_cursor
            .as_ref()
            .expect("small page must emit a cursor when more rows remain");

        // ----------------------------------------------------------------
        // 4. Forged cursor — the host must surface typed
        //    `InvalidCursor` and the client must surface the
        //    typed `PeerHistoryOutcome::InvalidCursor` (NOT
        //    `TransportUnavailable` or `Malformed`).
        // ----------------------------------------------------------------
        let forged = RemoteHistoryCursor::from_string("definitely-not-a-host-cursor".to_string());
        let outcome = client_service.browse(
            &host_peer_id,
            &host_cert_fingerprint,
            Some(&forged),
            MAX_PAGE_ROWS as u32,
        );
        assert!(
            matches!(outcome, PeerHistoryOutcome::InvalidCursor),
            "forged cursor must surface typed InvalidCursor, got {outcome:?}"
        );

        // ----------------------------------------------------------------
        // 5. Cursor signed for another peer — `peer-y` minted
        //    with a different secret. The host's HMAC check
        //    MUST reject the cross-peer replay.
        // ----------------------------------------------------------------
        let foreign_secret = PeerCursorSecret::generate();
        let foreign_cursor = RemoteHistoryCursor::mint(
            "peer-y",
            "2026-01-02T03:04:05Z",
            30,
            foreign_secret.as_bytes(),
        );
        let outcome = client_service.browse(
            &host_peer_id,
            &host_cert_fingerprint,
            Some(&foreign_cursor),
            MAX_PAGE_ROWS as u32,
        );
        assert!(
            matches!(outcome, PeerHistoryOutcome::InvalidCursor),
            "cross-peer cursor must surface typed InvalidCursor, got {outcome:?}"
        );

        // ----------------------------------------------------------------
        // 6. Cursor minted under a rotated / replaced secret.
        //    The host rotates its secret mid-test and the
        //    previously valid cursor collapses to `InvalidCursor`.
        // ----------------------------------------------------------------
        // Re-mint the secret and confirm `first_next_cursor`
        // (minted under the previous secret) now fails.
        host_service.set_cursor_secret(&client_peer_id, PeerCursorSecret::generate());
        let outcome = client_service.browse(
            &host_peer_id,
            &host_cert_fingerprint,
            Some(first_next_cursor),
            MAX_PAGE_ROWS as u32,
        );
        assert!(
            matches!(outcome, PeerHistoryOutcome::InvalidCursor),
            "rotated-secret cursor must surface typed InvalidCursor, got {outcome:?}"
        );

        // ----------------------------------------------------------------
        // 7. No-secret state — clearing the host's secret
        //    collapses to `Unavailable("not_trusted")` at the
        //    wire, which the productive transport surfaces as
        //    `TransportError::Revoked` and the client finally
        //    reports as `PeerUnavailable("not_trusted")`. The
        //    typed outcome is stable; the runtime never collapses
        //    a cursor-secret failure into a generic network
        //    error and the renderer can distinguish a missing
        //    secret from a peer that simply lost presence.
        // ----------------------------------------------------------------
        host_service.clear_cursor_secret(&client_peer_id);
        let outcome = client_service.browse(
            &host_peer_id,
            &host_cert_fingerprint,
            None,
            MAX_PAGE_ROWS as u32,
        );
        assert!(
            matches!(
                outcome,
                PeerHistoryOutcome::PeerUnavailable {
                    reason: "not_trusted"
                }
            ),
            "no-secret state must surface PeerUnavailable(not_trusted), got {outcome:?}"
        );

        // Restore the original secret for the persistence test.
        host_service.install_cursor_secret_hex(&client_peer_id, &host_secret_hex);

        // ----------------------------------------------------------------
        // 8. No-handler state — install a fresh host listener
        //    without the adapter so the wire contract collapses
        //    to `ListRecentTextUnavailable("not_available")` and
        //    the client surfaces `TransportUnavailable`.
        // ----------------------------------------------------------------
        let bare_host = Arc::new(TlsPeerTransport::new());
        let bare_advertisement: Arc<dyn PairingAdvertisementSink> =
            Arc::new(RecordingAdvertisementSink::new());
        let bare_sink: Arc<dyn TransportSink> = Arc::new(NoOpSink);
        let bare_port = install_with_material_resolver_and_history(
            bare_host.as_ref(),
            host_material.clone(),
            "bare-host".to_string(),
            bare_advertisement,
            bare_sink,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("install bare host");
        let bare_fingerprint = derive_cert_fingerprint(host_material.cert_der());
        let bare_port_slot: Arc<parking_lot::Mutex<Option<u16>>> =
            Arc::new(parking_lot::Mutex::new(Some(bare_port)));
        let bare_resolver: Arc<dyn RemotePeerResolver> = Arc::new(HostPortResolver {
            host: Arc::clone(&bare_port_slot),
            peer_id: host_peer_id.clone(),
        });
        let bare_client = Arc::new(TlsPeerTransport::new());
        let bare_advertisement_c: Arc<dyn PairingAdvertisementSink> =
            Arc::new(RecordingAdvertisementSink::new());
        let bare_sink_c: Arc<dyn TransportSink> = Arc::new(NoOpSink);
        install_with_material_and_resolver(
            bare_client.as_ref(),
            client_material.clone(),
            "bare-client".to_string(),
            bare_advertisement_c,
            bare_sink_c,
            Some(bare_resolver),
        )
        .expect("install bare client");
        bare_client
            .arm_pin(&host_peer_id, &bare_fingerprint)
            .expect("arm pin bare");
        std::thread::sleep(std::time::Duration::from_millis(150));
        let bare_history_transport: Arc<dyn PeerHistoryTransport> =
            Arc::new(PeerPairingHistoryTransportAdapter::new(bare_client.clone()));
        let bare_service = PeerTextHistoryService::new(bare_history_transport);
        bare_service.record_peer_state(
            &host_peer_id,
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );
        let outcome =
            bare_service.browse(&host_peer_id, &bare_fingerprint, None, MAX_PAGE_ROWS as u32);
        assert!(
            matches!(
                outcome,
                PeerHistoryOutcome::TransportUnavailable {
                    reason: "unavailable"
                }
            ),
            "no-handler state must surface typed TransportUnavailable, got {outcome:?}"
        );
        bare_client.stop().expect("stop bare client");
        bare_host.stop().expect("stop bare host");

        // ----------------------------------------------------------------
        // 9. Persistence failure — flip the toggle source into
        //    its failing state and confirm the client surfaces
        //    the typed `TransportUnavailable` outcome without
        //    retrying.
        // ----------------------------------------------------------------
        *source.failing.lock().expect("fail lock") = true;
        let outcome = client_service.browse(
            &host_peer_id,
            &host_cert_fingerprint,
            None,
            MAX_PAGE_ROWS as u32,
        );
        assert!(
            matches!(
                outcome,
                PeerHistoryOutcome::TransportUnavailable {
                    reason: "unavailable"
                }
            ),
            "persistence failure must surface typed TransportUnavailable, got {outcome:?}"
        );
        *source.failing.lock().expect("fail lock") = false;

        // ----------------------------------------------------------------
        // 10. Wrong pin — the productive dial driver MUST
        //     reject a mismatched cert fingerprint and the
        //     client must surface the typed outcome without
        //     leaking any row payload. A wrong pin collapses to
        //     `TransportError::KeyMismatch` which the runtime
        //     maps onto `PeerUnavailable { reason: "not_trusted" }`
        //     — the renderer can therefore distinguish a TLS
        //     trust failure from a peer that simply lost
        //     presence.
        // ----------------------------------------------------------------
        let wrong_pin = derive_cert_fingerprint(client_material.cert_der());
        let outcome = client_service.browse(&host_peer_id, &wrong_pin, None, MAX_PAGE_ROWS as u32);
        assert!(
            matches!(
                outcome,
                PeerHistoryOutcome::PeerUnavailable {
                    reason: "not_trusted"
                }
            ),
            "wrong pin must surface typed PeerUnavailable(not_trusted), got {outcome:?}"
        );

        // ----------------------------------------------------------------
        // 11. Payload sanity — the wire response the client
        //     surfaces must never carry the HMAC secret, the
        //     entry body, the entry hash, the host's bound
        //     port, the loopback IP or the pinned cert
        //     fingerprint. A regression that leaks metadata
        //     would surface here.
        // ----------------------------------------------------------------
        // Fetch a fresh page after restoring the toggle source
        // so the payload assertion has a populated `Ok`
        // outcome to inspect.
        let payload_outcome = client_service.browse(&host_peer_id, &host_cert_fingerprint, None, 5);
        let payload_serialized = format!("{payload_outcome:?}");
        for forbidden in [
            host_cert_fingerprint.as_str(),
            client_cert_fingerprint.as_str(),
            "127.0.0.1",
            "/tmp",
            "localhost",
            host_secret_hex.as_str(),
            "SECRET_BODY_THAT_MUST_NOT_LEAK",
            "deadbeef",
        ] {
            assert!(
                !payload_serialized.contains(forbidden),
                "wire payload leaked {forbidden}"
            );
        }
        // The previews are bounded to PREVIEW_MAX_CHARS so the
        // body must be truncated; spot-check the visible
        // preview escaped the body and never reached the
        // secret marker.
        if let PeerHistoryOutcome::Ok { page, .. } = &payload_outcome {
            for row in &page.rows {
                // The marker MUST survive in the persisted body
                // but never reach the wire (the body is bounded
                // by PREVIEW_MAX_CHARS so the marker — which is
                // appended after a long body — is sliced off).
                assert!(
                    row.preview.contains("row-"),
                    "preview must show the visible row marker: {}",
                    row.preview
                );
                assert!(
                    !row.preview.contains("SECRET_BODY_THAT_MUST_NOT_LEAK"),
                    "preview must not leak the secret body marker: {}",
                    row.preview
                );
                assert!(!row.preview.contains('<') && !row.preview.contains('>'));
            }
        }

        // Cleanup: stop every transport so the next test
        // starts from a clean slate.
        host_transport.stop().expect("stop host");
        client_transport.stop().expect("stop client");

        // Silence the unused-helper warning when the feature
        // gate is enabled.
        let _ = small_cursor;
    }
}

/// Default noop facade the cross-compile / unsupported-target
/// build installs. Every call returns
/// [`PeerHistoryTransportError::Unavailable`] so the runtime
/// collapses the outcome into
/// [`PeerHistoryOutcome::TransportUnavailable`] without ever
/// reaching the pairing transport. The struct is a safe fallback —
/// it is the canonical "this host doesn't ship the mTLS history
/// driver" return path the bootstrap documents for cross-compiles
/// and Windows builds.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPeerHistoryTransport;

impl PeerHistoryTransport for NoopPeerHistoryTransport {
    fn list_recent_text(
        &self,
        _request: ListRecentTextRequest,
    ) -> Result<ListRecentTextResponse, PeerHistoryTransportError> {
        Err(PeerHistoryTransportError::Unavailable)
    }
}

/// Productive adapter that delegates to the
/// [`crate::peer_pairing::PeerTransport`] the bootstrap already
/// installed for the pairing change. The adapter owns no
/// transport state of its own: it is a thin translation layer
/// that converts the [`ListRecentTextRequest`] the runtime hands it
/// into the typed [`PeerHistorySnapshot`] the productive pairing
/// transport hands back. The transport wrapper exists so the core
/// owns the typed outcome surface and the platform crate owns the
/// mTLS dial loop — neither side reaches across the boundary.
#[cfg(feature = "local-peer-pairing-tls")]
pub struct PeerPairingHistoryTransportAdapter {
    inner: Arc<dyn crate::peer_pairing::PeerTransport>,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerPairingHistoryTransportAdapter {
    /// Build an adapter that delegates `list_recent_text` to the
    /// supplied pairing transport. The adapter is cheap to clone
    /// (`Arc`-shared); the bootstrap keeps a single instance per
    /// app context.
    pub fn new(inner: Arc<dyn crate::peer_pairing::PeerTransport>) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerHistoryTransport for PeerPairingHistoryTransportAdapter {
    fn list_recent_text(
        &self,
        request: ListRecentTextRequest,
    ) -> Result<ListRecentTextResponse, PeerHistoryTransportError> {
        match self.inner.list_recent_text(
            &request.peer_id,
            &request.cert_fingerprint,
            &request.cursor,
            request.limit,
        ) {
            Ok(snapshot) => Ok(ListRecentTextResponse {
                page: RemoteTextHistoryPage {
                    rows: snapshot
                        .rows
                        .into_iter()
                        .map(|row| crate::peer_text_history::RemoteTextPreview {
                            remote_entry_id: row.remote_entry_id,
                            title: row.title,
                            content_type: row.content_type,
                            created_at: row.created_at,
                            preview: row.preview,
                        })
                        .collect(),
                    next_cursor: if snapshot.next_cursor.is_empty() {
                        None
                    } else {
                        Some(crate::peer_text_history::RemoteHistoryCursor::from_string(
                            snapshot.next_cursor,
                        ))
                    },
                },
                // The host emitted the `snapshot_id` SHA-256 over
                // its own header; the adapter forwards the value
                // verbatim and never recomputes a fingerprint
                // from the visible rows. A partial reconstruction
                // would diverge from the host the moment a new
                // capture landed or a row was removed between
                // page requests.
                snapshot_id: snapshot.snapshot_id,
            }),
            Err(error) => Err(map_pairing_transport_error(error)),
        }
    }
}

/// Translate [`crate::peer_pairing::TransportError`] into the
/// typed [`PeerHistoryTransportError`] the runtime branches on. The
/// mapping keeps every outcome the runtime already understands
/// (`UnknownPeer`, `KeyMismatch`, `Revoked`, `Blocked`,
/// `Unavailable`, `IncompatibleProtocol`, `Malformed`,
/// `InvalidCursor`) so the `browse` flow does not need to inspect
/// free-form strings. The dedicated `InvalidCursor` mapping is the
/// only path through which a cursor-rejection signal from the
/// remote host reaches the renderer; collapsing it into `Malformed`
/// or `Unavailable` would hide a forged / rotated / replayed cursor
/// behind a network-shaped error.
#[cfg(feature = "local-peer-pairing-tls")]
fn map_pairing_transport_error(
    error: crate::peer_pairing::TransportError,
) -> PeerHistoryTransportError {
    use crate::peer_pairing::TransportError as Pairing;
    match error {
        Pairing::UnknownPeer => PeerHistoryTransportError::UnknownPeer,
        Pairing::PeerUnresolved => PeerHistoryTransportError::PeerUnresolved,
        Pairing::KeyMismatch => PeerHistoryTransportError::KeyMismatch,
        Pairing::Revoked => PeerHistoryTransportError::Revoked,
        Pairing::Blocked => PeerHistoryTransportError::Blocked,
        Pairing::IncompatibleProtocol => PeerHistoryTransportError::IncompatibleProtocol,
        Pairing::Malformed => PeerHistoryTransportError::Malformed,
        Pairing::InvalidCursor => PeerHistoryTransportError::InvalidCursor,
        Pairing::AlreadyRunning
        | Pairing::NotRunning
        | Pairing::Unavailable
        | Pairing::Crypto
        | Pairing::BodyTooLarge => PeerHistoryTransportError::Unavailable,
    }
}

/// Production adapter the bootstrap installs into
/// [`crate::peer_pairing::PairingRuntime::install_cursor_secret_cache`].
/// It mirrors every persisted cursor-secret transition into both
/// history services, whose caches are projections of the same
/// trusted `known_peers` row. The adapter never logs or exposes
/// the raw key material.
#[cfg(feature = "local-peer-pairing-tls")]
pub struct PeerHistoryCursorSecretCache {
    text_service: PeerTextHistoryService,
    image_service: crate::peer_image_history::PeerImageHistoryService,
}

#[cfg(feature = "local-peer-pairing-tls")]
impl PeerHistoryCursorSecretCache {
    /// Build the shared cache adapter the bootstrap wires against
    /// the productive pairing runtime. Both services are cheap
    /// `Arc`-backed clones and retain independent cursor types.
    pub fn new(
        text_service: PeerTextHistoryService,
        image_service: crate::peer_image_history::PeerImageHistoryService,
    ) -> Self {
        Self {
            text_service,
            image_service,
        }
    }
}

#[cfg(feature = "local-peer-pairing-tls")]
impl crate::peer_pairing::PeerCursorSecretCache for PeerHistoryCursorSecretCache {
    fn install(&self, peer_id: &str, secret: PeerCursorSecret) {
        let image_secret =
            crate::peer_image_history::PeerImageCursorSecret::from_hex(&secret.to_hex())
                .expect("text and image cursor secrets share the persisted 32-byte encoding");
        self.text_service.set_cursor_secret(peer_id, secret);
        self.image_service.set_cursor_secret(peer_id, image_secret);
    }

    fn clear(&self, peer_id: &str) {
        self.text_service.clear_cursor_secret(peer_id);
        self.image_service.clear_cursor_secret(peer_id);
    }
}
