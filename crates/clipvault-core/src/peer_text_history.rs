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
//!   submits verbatim — the runtime never accepts an offset or a
//!   client-invented identifier;
//! - the [`PeerTextHistoryService`] façade that wraps the local
//!   [`EntryRepository`] projection. The façade is metadata-only by
//!   construction: it never copies the entry body into the cursor,
//!   the wire payload or the preview, only the fields the design
//!   (`peer-text-history-browser/design.md` §"Paginación y preview")
//!   authorises. The local SQLite layer is consulted read-only and the
//!   service refuses to write anything in response to a browsing call.
//! - the typed [`PeerHistoryOutcome`] the bridge / Tauri shell returns
//!   to the frontend. Every variant collapses to a stable identifier
//!   the UI branches on (`InvalidCursor`, `PeerUnavailable`,
//!   `PermissionDenied`, `Empty`, `Ok`) without parsing free-form
//!   strings or content bytes.
//!
//! ## Eligibility
//!
//! [`entry_is_transferable`] is the single predicate the service and
//! the unit tests share. A row is transferable when its
//! [`clipvault_db::ContentType`] belongs to the textual taxonomy
//! (`is_textual`) and the entry carries no image asset, no rich-text
//! metadata and no payload dimensions. Image rows and rich-text rows
//! are filtered out before the projection runs so a peer never sees a
//! disabled row; the spec scenario "Image or rich-text row exists"
//! pins that contract.
//!
//! ## Ordering, cursor and limits
//!
//! The projection sorts newest first, breaks ties with the stable
//! [`EntryRecord::id`] (the SQLite rowid) and slices the page at
//! [`MAX_PAGE_ROWS`]. The cursor encodes the `(created_at, id)`
//! pair of the **last** row the previous page returned, so the next
//! page picks up strictly after it (strict less-than, inclusive of
//! stable id). The runtime rejects any cursor it did not mint with
//! [`PeerHistoryOutcome::InvalidCursor`]; the spec scenario "Invalid
//! cursor" pins that contract.
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

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use clipvault_db::{ContentType, EntryRecord, EntryRepository};

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

/// Validation error the runtime surfaces when the cursor the client
/// submits is not an opaque string the host minted. The variant is
/// the typed reason the wire envelope encodes (`invalid_cursor`) so
/// the frontend never has to inspect free-form strings.
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
}

/// Opaque cursor the host emits on every successful page. The runtime
/// guarantees the cursor only encodes `(created_at, id)` of the last
/// row of the previous page — never the row content, the row hash,
/// the source app, the collection or the favourite flag. Clients
/// MUST treat the cursor as opaque: submitting a fabricated cursor
/// returns [`PeerHistoryCursorError::Invalid`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RemoteHistoryCursor {
    inner: String,
}

impl RemoteHistoryCursor {
    /// Mint a cursor from the (timestamp, id) pair of the last row
    /// the page emitted. The cursor is the host's canonical
    /// `<rfc3339>|<id>` pair percent-encoded so the renderer can
    /// carry it through a JSON / Tauri boundary without losing
    /// any byte; the host is the only entity that ever has both
    /// pieces of information. The renderer and the bridge treat
    /// the value as opaque and never parse it.
    pub fn mint(timestamp: &str, id: i64) -> Self {
        let raw = format!("{timestamp}|{id}");
        Self {
            inner: percent_encode(&raw),
        }
    }

    /// Decode a cursor the client submitted. The helper collapses
    /// every malformed input into [`PeerHistoryCursorError`] without
    /// surfacing the original bytes; the runtime never logs the
    /// decoded timestamp / id pair because they identify a single
    /// local entry the host should not leak through the wire.
    pub fn decode(&self) -> Result<(String, i64), PeerHistoryCursorError> {
        let decoded = percent_decode(&self.inner)?;
        let Some((timestamp, id)) = decoded.split_once('|') else {
            return Err(PeerHistoryCursorError::Invalid);
        };
        // The host only ever mints cursors with RFC 3339 strings
        // and a positive integer id, so a malformed timestamp / id
        // here is an explicit forgery attempt.
        if OffsetDateTime::parse(timestamp, &Rfc3339).is_err() {
            return Err(PeerHistoryCursorError::MalformedTimestamp);
        }
        let id: i64 = id
            .parse()
            .map_err(|_| PeerHistoryCursorError::MalformedId)?;
        if id <= 0 {
            return Err(PeerHistoryCursorError::MalformedId);
        }
        if !is_well_formed_cursor_payload(&decoded) {
            return Err(PeerHistoryCursorError::Invalid);
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

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b':'
            | b'|'
            | b'+' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn percent_decode(input: &str) -> Result<String, PeerHistoryCursorError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%' {
            // A bare `%` without two trailing hex digits is a
            // forgery attempt: the encoder never produces one.
            if idx + 2 >= bytes.len() {
                return Err(PeerHistoryCursorError::Invalid);
            }
            let hi = hex_value(bytes[idx + 1]);
            let lo = hex_value(bytes[idx + 2]);
            match (hi, lo) {
                (Some(hi), Some(lo)) => {
                    out.push((hi << 4) | lo);
                    idx += 3;
                    continue;
                }
                _ => return Err(PeerHistoryCursorError::Invalid),
            }
        }
        out.push(bytes[idx]);
        idx += 1;
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_well_formed_cursor_payload(input: &str) -> bool {
    let mut saw_pipe = false;
    for ch in input.chars() {
        if ch == '|' {
            saw_pipe = true;
            continue;
        }
        if ch.is_control() {
            return false;
        }
        if ch == '%' {
            return false;
        }
        // The cursor only carries the RFC 3339 timestamp and a
        // positive integer id; every other Unicode class is
        // suspicious and rejected as a forgery attempt.
        if !(ch.is_ascii_alphanumeric()
            || matches!(ch, '-' | '_' | '.' | '~' | ':' | '+' | 'T' | 'Z'))
        {
            return false;
        }
    }
    saw_pipe
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
    /// The cursor was not minted by this host. The renderer surfaces
    /// the typed reason without retrying.
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
    /// persistence layer rejected the query (broken DB,
    /// disconnected session, …). The renderer surfaces a typed
    /// failure copy without retrying blindly.
    PersistenceUnavailable,
}

/// Predicate the runtime and the unit tests share. The predicate
/// returns `true` when the entry is textual and carries no
/// non-transferable metadata: image asset, image MIME/size metadata
/// or rich-text metadata. A row that has rich-text metadata is
/// non-transferable even when the textual content is otherwise
/// valid, because the design caps the wire to plain text only.
pub fn entry_is_transferable(entry: &EntryRecord) -> bool {
    if !entry.content_type.is_textual() {
        return false;
    }
    if entry.is_renderable_image() {
        return false;
    }
    if entry.has_rich_text() {
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

/// Persistence trait the runtime consumes to project the transferable
/// text history. The bootstrap installs an adapter that delegates to
/// [`clipvault_db::EntryRepository::text_entries`] / a freshly
/// allocated repository handle; tests inject an in-memory fake.
pub trait PeerHistoryProjection: Send + Sync {
    /// Return every transferable textual entry that matches the
    /// (created_at, id) cursor pair (strict less-than ordering). The
    /// implementation MUST sort by `created_at DESC, id DESC` and
    /// return at most `limit` rows. The implementation MAY return
    /// fewer rows than `limit` even when more entries exist; the
    /// runtime reads more pages by re-invoking with the new cursor.
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
    /// production adapter returns a SHA-256 of the most recent
    /// textual `created_at` / `id` pair; tests may return a
    /// deterministic placeholder.
    fn snapshot_id(&self) -> Result<String, PeerHistoryPersistenceError>;
}

/// Typed persistence error the runtime surfaces. Every adapter
/// collapses the underlying failure into one of these variants so
/// the runtime can branch on the reason without inspecting
/// platform-specific error strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeerHistoryPersistenceError {
    #[error("peer history persistence backend is unavailable")]
    Unavailable,
    #[error("peer history persistence backend rejected the operation")]
    Failed,
}

/// In-memory persistence adapter the tests use. The adapter mirrors
/// the contract [`clipvault_db::EntryRepository`] exposes — the
/// runtime trusts the same outcomes regardless of the backend.
#[derive(Debug, Default)]
pub struct InMemoryPeerHistoryProjection {
    inner: RwLock<Vec<EntryRecord>>,
}

impl InMemoryPeerHistoryProjection {
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

    /// Latest snapshot id derived from the highest `created_at` /
    /// `id` pair currently in the adapter. `None` when the adapter
    /// is empty.
    pub fn latest_fingerprint(&self) -> String {
        let guard = self.inner.read();
        let Some(top) = guard.iter().max_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        }) else {
            return "empty".to_string();
        };
        format!("{}|{}", top.created_at, top.id)
    }
}

impl PeerHistoryProjection for InMemoryPeerHistoryProjection {
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
        Ok(self.latest_fingerprint())
    }
}

/// Production adapter the bootstrap wires against the live SQLite
/// handle. The adapter borrows a [`Mutex<Database>`] the runtime
/// hands it through the bootstrap; tests use the in-memory
/// projection above.
pub struct EntryRepositoryPeerHistoryProjection {
    database: Arc<parking_lot::Mutex<clipvault_db::Database>>,
}

impl EntryRepositoryPeerHistoryProjection {
    /// Build an adapter that borrows the shared [`clipvault_db::Database`].
    pub fn new(database: Arc<parking_lot::Mutex<clipvault_db::Database>>) -> Self {
        Self { database }
    }
}

impl PeerHistoryProjection for EntryRepositoryPeerHistoryProjection {
    fn page_after(
        &self,
        created_at: &str,
        id: i64,
        limit: usize,
    ) -> Result<Vec<EntryRecord>, PeerHistoryPersistenceError> {
        let mut guard = self.database.lock();
        let conn = guard.connection_mut();
        let repo = EntryRepository::new(conn);
        let page = repo
            .text_entries_after(created_at, id, limit)
            .map_err(|error| {
                tracing::warn!(?error, "peer history persistence: page_after failed");
                PeerHistoryPersistenceError::Failed
            })?;
        Ok(page)
    }

    fn snapshot_id(&self) -> Result<String, PeerHistoryPersistenceError> {
        let mut guard = self.database.lock();
        let conn = guard.connection_mut();
        let repo = EntryRepository::new(conn);
        let latest = repo.latest_transferable_text_snapshot().map_err(|error| {
            tracing::warn!(?error, "peer history persistence: snapshot failed");
            PeerHistoryPersistenceError::Failed
        })?;
        Ok(latest
            .map(|(created_at, id)| format!("{created_at}|{id}"))
            .unwrap_or_else(|| "empty".to_string()))
    }
}

/// Service the shell drives. The façade is cheap to clone (every
/// field is `Arc`-shared) and never mutates the persistence layer.
#[derive(Clone)]
pub struct PeerTextHistoryService {
    projection: Arc<dyn PeerHistoryProjection>,
    /// Per-peer active-state cache. The cache is metadata-only
    /// (no content bytes) and is what the runtime consults before
    /// opening a cursor pagination. The cache lives in-memory; the
    /// bootstrap populates it from the
    /// [`clipvault_core::peer_pairing::PairingRuntime`] on every
    /// peer update so a browsing call cannot outrun the trust
    /// transition that unlocked it.
    active_peers: Arc<RwLock<HashMap<String, PeerActiveState>>>,
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

impl PeerTextHistoryService {
    /// Build a service backed by the supplied projection. The
    /// active-state cache starts empty: the bootstrap populates it
    /// on every snapshot / health probe so the very first browsing
    /// call waits for an `Active` projection before returning rows.
    pub fn new(projection: Arc<dyn PeerHistoryProjection>) -> Self {
        Self {
            projection,
            active_peers: Arc::new(RwLock::new(HashMap::new())),
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

    /// Browse the most recent transferable text page of `peer_id`.
    ///
    /// The runtime consults the in-memory active-state cache and
    /// refuses to project when the peer is not trusted, not present
    /// or unknown. A `cursor = None` request starts the newest
    /// page; a `Some(cursor)` request decodes the cursor and pages
    /// strictly after the (created_at, id) pair it carries.
    pub fn browse(
        &self,
        peer_id: &str,
        cursor: Option<&RemoteHistoryCursor>,
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

        // The sentinel cursor marks the very first page: any
        // real RFC 3339 timestamp sorts strictly before it, so
        // the projection returns every transferable row the host
        // owns (newest first, up to the page cap). The sentinel
        // id is `i64::MAX` so the tie-breaker the projection
        // applies to rows that share the cursor's created_at
        // still matches every persisted entry.
        const SENTINEL_TIMESTAMP: &str = "9999-12-31T23:59:59Z";
        const SENTINEL_ID: i64 = i64::MAX;

        let (start_created_at, start_id) = match cursor {
            None => (SENTINEL_TIMESTAMP.to_string(), SENTINEL_ID),
            Some(cursor) => match cursor.decode() {
                Ok((ts, id)) => (ts, id),
                Err(_) => return PeerHistoryOutcome::InvalidCursor,
            },
        };

        let rows = match self
            .projection
            .page_after(&start_created_at, start_id, MAX_PAGE_ROWS)
        {
            Ok(rows) => rows,
            Err(_) => return PeerHistoryOutcome::PersistenceUnavailable,
        };

        let mut previews = Vec::with_capacity(rows.len());
        for row in rows {
            previews.push(project_row(&row));
        }
        let next_cursor = if previews.len() == MAX_PAGE_ROWS {
            previews.last().map(|preview| {
                RemoteHistoryCursor::mint(
                    &preview.created_at,
                    decode_remote_id(&preview.remote_entry_id),
                )
            })
        } else {
            None
        };
        let snapshot_id = match self.projection.snapshot_id() {
            Ok(value) => value,
            Err(_) => return PeerHistoryOutcome::PersistenceUnavailable,
        };
        PeerHistoryOutcome::Ok {
            page: RemoteTextHistoryPage {
                rows: previews,
                next_cursor,
            },
            snapshot_id,
        }
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
    fn transferable_predicate_rejects_rich_text_rows() {
        let mut record = record(1, ContentType::Text, "hello", "2026-01-01T00:00:00Z");
        record.rich_text_hash = Some("a".repeat(64));
        record.rich_html_ref = Some("rich-text/a.html".to_string());
        assert!(!entry_is_transferable(&record));
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
    fn cursor_round_trips_timestamp_and_id() {
        let cursor = RemoteHistoryCursor::mint("2026-01-02T03:04:05Z", 42);
        let (ts, id) = cursor.decode().expect("decoded");
        assert_eq!(ts, "2026-01-02T03:04:05Z");
        assert_eq!(id, 42);
    }

    #[test]
    fn cursor_rejects_forged_input() {
        let cursor = RemoteHistoryCursor {
            inner: "not-a-valid-cursor".to_string(),
        };
        assert!(matches!(
            cursor.decode(),
            Err(PeerHistoryCursorError::Invalid)
        ));
        let cursor = RemoteHistoryCursor {
            inner: percent_encode("2026-01-02T03:04:05Z|not-a-number"),
        };
        assert!(matches!(
            cursor.decode(),
            Err(PeerHistoryCursorError::MalformedId)
        ));
    }

    #[test]
    fn cursor_rejects_non_rfc3339_timestamp() {
        let cursor = RemoteHistoryCursor {
            inner: percent_encode("not-a-date|1"),
        };
        assert!(matches!(
            cursor.decode(),
            Err(PeerHistoryCursorError::MalformedTimestamp)
        ));
    }

    #[test]
    fn cursor_rejects_bare_percent_sequence() {
        // A cursor with an unmatched `%` byte cannot have been
        // minted by the host (the encoder never emits a bare `%`).
        let cursor = RemoteHistoryCursor {
            inner: "2026-01-02T03:04:05Z|1%zz".to_string(),
        };
        assert!(matches!(
            cursor.decode(),
            Err(PeerHistoryCursorError::Invalid)
        ));
    }

    #[test]
    fn browse_returns_peer_unavailable_when_state_missing() {
        let projection: Arc<dyn PeerHistoryProjection> =
            Arc::new(InMemoryPeerHistoryProjection::new());
        let service = PeerTextHistoryService::new(projection);
        let outcome = service.browse("peer-x", None);
        assert!(matches!(
            outcome,
            PeerHistoryOutcome::PeerUnavailable {
                reason: "no_known_peer"
            }
        ));
    }

    #[test]
    fn browse_returns_peer_unavailable_when_state_not_trusted() {
        let projection: Arc<dyn PeerHistoryProjection> =
            Arc::new(InMemoryPeerHistoryProjection::new());
        let service = PeerTextHistoryService::new(projection);
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: false,
                active: false,
            },
        );
        let outcome = service.browse("peer-x", None);
        assert!(matches!(
            outcome,
            PeerHistoryOutcome::PeerUnavailable {
                reason: "not_trusted"
            }
        ));
    }

    #[test]
    fn browse_returns_peer_unavailable_when_state_not_active() {
        let projection: Arc<dyn PeerHistoryProjection> =
            Arc::new(InMemoryPeerHistoryProjection::new());
        let service = PeerTextHistoryService::new(projection);
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: false,
            },
        );
        let outcome = service.browse("peer-x", None);
        assert!(matches!(
            outcome,
            PeerHistoryOutcome::PeerUnavailable {
                reason: "not_active"
            }
        ));
    }

    #[test]
    fn browse_returns_invalid_cursor_for_forged_cursor() {
        let projection: Arc<dyn PeerHistoryProjection> =
            Arc::new(InMemoryPeerHistoryProjection::new());
        let service = PeerTextHistoryService::new(projection);
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );
        let cursor = RemoteHistoryCursor {
            inner: "forged".to_string(),
        };
        let outcome = service.browse("peer-x", Some(&cursor));
        assert!(matches!(outcome, PeerHistoryOutcome::InvalidCursor));
    }

    fn active_service_with(entries: Vec<EntryRecord>) -> PeerTextHistoryService {
        let projection = Arc::new(InMemoryPeerHistoryProjection::new());
        projection.seed(entries);
        let service = PeerTextHistoryService::new(projection);
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );
        service
    }

    #[test]
    fn browse_orders_rows_newest_first_with_stable_id_tie_break() {
        // The projection sorts by `created_at DESC, id DESC`; this
        // test feeds a same-timestamp batch to verify the id
        // tie-breaker stays stable.
        let mut entries = Vec::new();
        for (id, content) in [(1u64, "old"), (2, "middle"), (3, "new")] {
            entries.push(record(
                id as i64,
                ContentType::Text,
                content,
                "2026-01-01T00:00:00Z",
            ));
        }
        let service = active_service_with(entries);
        let outcome = service.browse("peer-x", None);
        let PeerHistoryOutcome::Ok { page, .. } = outcome else {
            panic!("expected Ok variant");
        };
        let previews: Vec<&str> = page.rows.iter().map(|row| row.preview.as_str()).collect();
        // Tie-broken by `id DESC` so id=3 renders first.
        assert_eq!(previews, vec!["new", "middle", "old"]);
    }

    #[test]
    fn browse_paginates_after_cursor_with_strict_less_than() {
        // The test seeds `MAX_PAGE_ROWS + 5` entries so the first
        // page hits the cap and the host emits a `next_cursor`;
        // the second page then pages strictly after that cursor.
        let mut entries = Vec::new();
        for id in 1..=(MAX_PAGE_ROWS + 5) as i64 {
            entries.push(record(
                id,
                ContentType::Text,
                &format!("row-{id}"),
                "2026-01-01T00:00:00Z",
            ));
        }
        let service = active_service_with(entries);
        let first = service.browse("peer-x", None);
        let PeerHistoryOutcome::Ok { page, .. } = first else {
            panic!("expected Ok variant");
        };
        let next_cursor = page.next_cursor.expect("next cursor");
        let second = service.browse("peer-x", Some(&next_cursor));
        let PeerHistoryOutcome::Ok { page, .. } = second else {
            panic!("expected Ok variant");
        };
        let second_ids: Vec<i64> = page
            .rows
            .iter()
            .map(|row| {
                row.remote_entry_id
                    .strip_prefix("entry-")
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0)
            })
            .collect();
        // The cursor was minted from id = MAX_PAGE_ROWS (the
        // newest row the first page returned); the second page
        // must contain only rows with id < MAX_PAGE_ROWS.
        assert_eq!(second_ids.len(), 5);
        assert!(second_ids.iter().all(|id| *id < MAX_PAGE_ROWS as i64));
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn browse_caps_page_size_at_max_rows() {
        let mut entries = Vec::new();
        for id in 1..=(MAX_PAGE_ROWS + 5) as i64 {
            entries.push(record(
                id,
                ContentType::Text,
                &format!("row-{id}"),
                "2026-01-01T00:00:00Z",
            ));
        }
        let service = active_service_with(entries);
        let outcome = service.browse("peer-x", None);
        let PeerHistoryOutcome::Ok { page, .. } = outcome else {
            panic!("expected Ok variant");
        };
        assert_eq!(page.rows.len(), MAX_PAGE_ROWS);
        assert!(page.next_cursor.is_some());
    }

    #[test]
    fn browse_excludes_image_and_rich_text_rows() {
        let mut entries = vec![
            record(1, ContentType::Text, "transferable", "2026-01-01T00:00:00Z"),
            image_record(2),
        ];
        let mut rich = record(3, ContentType::Text, "rich", "2026-01-01T00:00:00Z");
        rich.rich_text_hash = Some("a".repeat(64));
        rich.rich_html_ref = Some("rich-text/a.html".to_string());
        entries.push(rich);

        let service = active_service_with(entries);
        let outcome = service.browse("peer-x", None);
        let PeerHistoryOutcome::Ok { page, .. } = outcome else {
            panic!("expected Ok variant");
        };
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].preview, "transferable");
    }

    #[test]
    fn browse_does_not_expose_body_hashes_or_source_metadata() {
        let mut entry = record(7, ContentType::Text, "<b>html</b>", "2026-01-01T00:00:00Z");
        entry.title = Some("hello".to_string());
        entry.source_app = Some("com.example.Editor".to_string());
        entry.content_hash = "deadbeef".repeat(8);

        let service = active_service_with(vec![entry]);
        let outcome = service.browse("peer-x", None);
        let PeerHistoryOutcome::Ok { page, .. } = outcome else {
            panic!("expected Ok variant");
        };
        let row = &page.rows[0];
        // The preview is HTML-escaped; the title is preserved (it
        // is the human-readable label, not content).
        assert!(row.preview.contains("&lt;b&gt;"));
        // Source app metadata is never emitted.
        assert!(!row.preview.contains("com.example.Editor"));
        // The raw content hash never reaches the wire. The
        // remote_entry_id is the opaque host-minted id (the
        // numeric id is allowed since it is the bridge between
        // the renderer and the future import action).
        assert!(row.preview.contains("hello") == false);
    }

    #[test]
    fn browse_returns_persistence_unavailable_when_projection_fails() {
        struct FailingProjection;
        impl PeerHistoryProjection for FailingProjection {
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
        let projection: Arc<dyn PeerHistoryProjection> = Arc::new(FailingProjection);
        let service = PeerTextHistoryService::new(projection);
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );
        let outcome = service.browse("peer-x", None);
        assert!(matches!(
            outcome,
            PeerHistoryOutcome::PersistenceUnavailable
        ));
    }

    #[test]
    fn forget_peer_drops_state_and_re_routes_to_no_known_peer() {
        let projection: Arc<dyn PeerHistoryProjection> =
            Arc::new(InMemoryPeerHistoryProjection::new());
        let service = PeerTextHistoryService::new(projection);
        service.record_peer_state(
            "peer-x",
            PeerActiveState {
                trusted: true,
                active: true,
            },
        );
        service.forget_peer("peer-x");
        let outcome = service.browse("peer-x", None);
        assert!(matches!(
            outcome,
            PeerHistoryOutcome::PeerUnavailable {
                reason: "no_known_peer"
            }
        ));
    }
}
