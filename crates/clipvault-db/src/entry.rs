//! Persistent representation of captured clipboard entries.
//!
//! The struct is intentionally small and serialisable so the Tauri shell
//! can hand it to the frontend without leaking the SQLite row layout.

use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Concrete content type stored in [`EntryRecord::content_type`].
///
/// The MVP introduced [`ContentType::Text`] as the only variant; the
/// `clipboard-type-detection` change extends the enum with every
/// textual category the detector can produce while keeping `Text` as
/// the stable value for legacy data. The `clipboard-rich-content`
/// change adds the first non-textual variant, [`ContentType::Image`].
///
/// All variants serialize as snake_case strings (e.g.
/// [`ContentType::ShellCommand`] -> `"shell_command"`,
/// [`ContentType::Ipv4`] -> `"ipv4"`, [`ContentType::Image`] ->
/// `"image"`). The string form is the authoritative wire format used
/// by SQLite and the Tauri shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    /// Free-form text or any payload the detector could not classify
    /// into a more specific variant. This is also the historical
    /// default for rows inserted before the detector shipped.
    Text,
    /// Absolute URL with an allowed scheme (`http`, `https`, `ftp`,
    /// `sftp`, `ssh`, `file`, `mailto`, `tel`).
    Url,
    /// Email address matching the documented `local@domain.tld` shape.
    Email,
    /// Structured JSON document (non-empty object or array).
    Json,
    /// Three-segment base64url JWT starting with `eyJ`.
    Jwt,
    /// UUID in any of the documented forms.
    Uuid,
    /// IPv4 address parseable via [`std::net::Ipv4Addr::from_str`].
    Ipv4,
    /// IPv6 address parseable via [`std::net::Ipv6Addr::from_str`].
    Ipv6,
    /// Hexadecimal color in `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA`
    /// form.
    HexColor,
    /// HTML snippet with at least one recognizable paired tag.
    Html,
    /// File path on macOS or Linux.
    FilePath,
    /// Shell command detected via shebang or strong shell syntax.
    ShellCommand,
    /// SQL statement with a DML/DDL keyword and a structural keyword.
    Sql,
    /// Code block detected via a fenced block or a shebang with a
    /// recognized language.
    Code,
    /// Static raster image persisted as a local PNG asset by the
    /// `clipboard-rich-content` capability.
    ///
    /// An image row is **not** textual: it never participates in the
    /// local text search and its [`EntryRecord::content`] column holds
    /// the documented empty sentinel instead of a payload. The
    /// renderable bytes live under `<data_dir>/assets/clipboard/` and
    /// are referenced through [`EntryRecord::asset_ref`].
    Image,
}

impl ContentType {
    /// Stable snake_case string used in SQLite and the Tauri shell.
    pub fn as_str(self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Url => "url",
            ContentType::Email => "email",
            ContentType::Json => "json",
            ContentType::Jwt => "jwt",
            ContentType::Uuid => "uuid",
            ContentType::Ipv4 => "ipv4",
            ContentType::Ipv6 => "ipv6",
            ContentType::HexColor => "hex_color",
            ContentType::Html => "html",
            ContentType::FilePath => "file_path",
            ContentType::ShellCommand => "shell_command",
            ContentType::Sql => "sql",
            ContentType::Code => "code",
            ContentType::Image => "image",
        }
    }

    /// True when the variant represents textual content that should
    /// participate in local search. The list mirrors the textual
    /// categories the detector can produce.
    ///
    /// [`ContentType::Image`] is deliberately excluded: the search
    /// stays textual and never inspects image bytes.
    pub fn is_textual(self) -> bool {
        matches!(
            self,
            ContentType::Text
                | ContentType::Url
                | ContentType::Email
                | ContentType::Json
                | ContentType::Jwt
                | ContentType::Uuid
                | ContentType::Ipv4
                | ContentType::Ipv6
                | ContentType::HexColor
                | ContentType::Html
                | ContentType::FilePath
                | ContentType::ShellCommand
                | ContentType::Sql
                | ContentType::Code
        )
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// MIME type persisted for every image entry in this phase. The
/// `clipboard-rich-content` capability normalises every supported
/// bitmap to a canonical PNG, so a single value is enough and the
/// column stays forward-compatible for a future format.
pub const IMAGE_MIME_PNG: &str = "image/png";

/// Sentinel stored in [`EntryRecord::content`] for an image row.
///
/// The `content` column stays `NOT NULL` so the historical table does
/// not have to be rebuilt. An image row therefore stores the empty
/// string: it carries no textual payload, it must never be rendered as
/// text by the frontend, and it is only a valid image when
/// [`EntryRecord::asset_ref`] and the rest of the image metadata are
/// present and coherent (see [`EntryRecord::is_renderable_image`]).
pub const IMAGE_CONTENT_SENTINEL: &str = "";

/// Stored row. `source_app` is `None` when the operating system did not
/// report the originator, as permitted by the spec. `is_pinned` is the
/// favorite flag added by the `clipboard-management` change; it is
/// always `false` for rows inserted through [`NewEntry`].
///
/// The presentation metadata fields added by the `history-card-layout`
/// change are nullable on purpose: a `None` means the row predates the
/// change (or the user never customised it). The frontend uses the
/// nullable shape to drive the fallback rendering (title derived from
/// `content_type`, source name derived from `source_app`, generic
/// application icon).
///
/// The payload metadata fields added by the `clipboard-rich-content`
/// change are nullable for the same reason: every pre-existing textual
/// row keeps `None` for all four and renders exactly as before.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EntryRecord {
    pub id: i64,
    pub content: String,
    pub content_type: ContentType,
    pub content_size: i64,
    pub content_hash: String,
    pub source_app: Option<String>,
    pub is_pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_seen_at: String,
    /// User-defined card title. `None` (or an empty string) means the
    /// card uses the localised `content_type` label as the title.
    pub title: Option<String>,
    /// User-visible source-application name (e.g. "Terminal",
    /// "Safari"). `None` when the platform adapter could not resolve
    /// a name or when the row predates the `history-card-layout`
    /// migration; the frontend falls back to `source_app`.
    pub source_app_name: Option<String>,
    /// Opaque, locally-controlled reference to the source
    /// application icon (for example
    /// `application-icons/com.apple.textedit.png`). `None` when no
    /// icon was extracted or the reference was cleared.
    pub source_app_icon_ref: Option<String>,
    /// Opaque, locally-controlled **relative** reference to the
    /// persisted payload asset, always of the form
    /// `clipboard/<lowercase-sha256>.png`. `None` for every textual
    /// row. An absolute path is never stored here and never returned
    /// to the frontend.
    pub asset_ref: Option<String>,
    /// MIME type of the persisted asset. `Some("image/png")` for an
    /// image row in this phase, `None` for textual rows.
    pub mime_type: Option<String>,
    /// Original pixel width of the captured image. `None` for textual
    /// rows.
    pub payload_width: Option<u32>,
    /// Original pixel height of the captured image. `None` for textual
    /// rows.
    pub payload_height: Option<u32>,
    /// Deterministic SHA-256 of the canonical rich-text representation.
    /// `None` for every textual and image row. Acts as the secondary
    /// dedupe key so two captures with identical plain text but
    /// different styles do not collapse to a single row.
    pub rich_text_hash: Option<String>,
    /// Relative reference to the original HTML representation,
    /// always of the form `rich-text/<sha256>.html`. `None` when the
    /// source did not expose HTML.
    pub rich_html_ref: Option<String>,
    /// Relative reference to the original RTF representation, always
    /// of the form `rich-text/<sha256>.rtf`. `None` when the source
    /// did not expose RTF.
    pub rich_rtf_ref: Option<String>,
    /// Relative reference to the sanitised rich-text preview, always
    /// of the form `rich-text/<sha256>.preview.html`. The frontend
    /// renders this asset through the validated bridge; the original
    /// HTML and RTF never cross the Tauri boundary.
    pub rich_preview_ref: Option<String>,
    /// Byte length of the original HTML representation. `None` when
    /// the source did not expose HTML.
    pub rich_html_size: Option<i64>,
    /// Byte length of the original RTF representation. `None` when the
    /// source did not expose RTF.
    pub rich_rtf_size: Option<i64>,
    /// Canonical programming-language identifier the
    /// `code-language-detection` detector accepted for the capture.
    /// `None` for every row predating the change, every textual
    /// variant other than `code`, every image / rich-text row and
    /// every payload the detector could not classify with sufficient
    /// confidence. The value is the canonical, normalised identifier
    /// from the allowlist (never an alias) so the frontend, the SQL
    /// index and the UI label stay byte-for-byte aligned.
    pub code_language: Option<String>,
}

impl EntryRecord {
    /// Convenience used by tests and callers that already hold the
    /// parsed timestamp.
    pub fn parsed_created_at(&self) -> Option<OffsetDateTime> {
        OffsetDateTime::parse(&self.created_at, &Rfc3339).ok()
    }

    /// Whether this row is a *coherent* image entry.
    ///
    /// An `image` row is only renderable when it also carries an asset
    /// reference, a MIME type and non-zero dimensions. A row that
    /// claims `content_type = image` without them is treated as
    /// broken: the frontend shows its accessible fallback and never
    /// falls back to rendering the `content` sentinel as text.
    pub fn is_renderable_image(&self) -> bool {
        self.content_type == ContentType::Image
            && self
                .asset_ref
                .as_deref()
                .is_some_and(|reference| !reference.is_empty())
            && self.mime_type.is_some()
            && self.payload_width.is_some_and(|value| value > 0)
            && self.payload_height.is_some_and(|value| value > 0)
    }

    /// Whether this row carries any rich-text metadata. A row is
    /// "rich" when the original capture exposed at least one rich
    /// representation: the row then carries a `rich_text_hash` and
    /// at least one of `rich_html_ref` / `rich_rtf_ref`. The
    /// `rich_preview_ref` is the asset the card renders.
    pub fn has_rich_text(&self) -> bool {
        self.rich_text_hash.is_some()
            && (self.rich_html_ref.is_some() || self.rich_rtf_ref.is_some())
    }
}

/// Payload supplied by [`crate::EntryRepository::insert_or_touch`]. The
/// timestamps are filled by the service so the storage layer never has to
/// decide "now".
#[derive(Debug, Clone)]
pub struct NewEntry {
    pub content: String,
    pub content_type: ContentType,
    pub content_size: i64,
    pub content_hash: String,
    pub source_app: Option<String>,
    pub created_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
    /// Relative asset reference for a non-textual payload. `None` for
    /// every textual capture, which keeps the existing insert path
    /// byte-for-byte compatible.
    pub asset_ref: Option<String>,
    pub mime_type: Option<String>,
    pub payload_width: Option<u32>,
    pub payload_height: Option<u32>,
    /// Canonical rich-text hash for a textual capture with rich
    /// representations. `None` for plain text and image captures.
    pub rich_text_hash: Option<String>,
    pub rich_html_ref: Option<String>,
    pub rich_rtf_ref: Option<String>,
    pub rich_preview_ref: Option<String>,
    pub rich_html_size: Option<i64>,
    pub rich_rtf_size: Option<i64>,
    /// Canonical programming-language identifier for a textual
    /// capture the detector accepted. `None` for every other row.
    pub code_language: Option<String>,
}

impl NewEntry {
    /// Build a textual entry. Keeps the four payload-metadata fields
    /// `None` so existing callers do not have to spell them out.
    pub fn text(
        content: String,
        content_type: ContentType,
        content_size: i64,
        content_hash: String,
        source_app: Option<String>,
        created_at: OffsetDateTime,
        last_seen_at: OffsetDateTime,
    ) -> Self {
        Self {
            content,
            content_type,
            content_size,
            content_hash,
            source_app,
            created_at,
            last_seen_at,
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

    /// Build a textual `code` entry with the canonical language the
    /// detector accepted. `code_language` MUST be the canonical,
    /// normalised identifier from the allowlist; the helper does not
    /// validate it because the canonical-language helpers in
    /// `clipvault-core` already enforce the rule at the bridge.
    pub fn code(
        content: String,
        content_size: i64,
        content_hash: String,
        source_app: Option<String>,
        code_language: Option<String>,
        created_at: OffsetDateTime,
        last_seen_at: OffsetDateTime,
    ) -> Self {
        Self::text(
            content,
            ContentType::Code,
            content_size,
            content_hash,
            source_app,
            created_at,
            last_seen_at,
        )
        .with_code_language(code_language)
    }

    /// Attach a canonical language to an existing textual entry.
    /// Returns `self` so callers can chain the helper without a
    /// dedicated intermediate variable. The helper intentionally
    /// trusts the caller; the canonical-language helpers in the
    /// core layer own the validation.
    pub fn with_code_language(mut self, language: Option<String>) -> Self {
        self.code_language = language.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_returns_snake_case_for_every_variant() {
        let cases = [
            (ContentType::Text, "text"),
            (ContentType::Url, "url"),
            (ContentType::Email, "email"),
            (ContentType::Json, "json"),
            (ContentType::Jwt, "jwt"),
            (ContentType::Uuid, "uuid"),
            (ContentType::Ipv4, "ipv4"),
            (ContentType::Ipv6, "ipv6"),
            (ContentType::HexColor, "hex_color"),
            (ContentType::Html, "html"),
            (ContentType::FilePath, "file_path"),
            (ContentType::ShellCommand, "shell_command"),
            (ContentType::Sql, "sql"),
            (ContentType::Code, "code"),
        ];
        for (variant, expected) in cases {
            assert_eq!(variant.as_str(), expected, "variant = {variant:?}");
            assert_eq!(variant.to_string(), expected);
            assert!(variant.is_textual(), "{variant:?} should be textual");
        }
    }

    #[test]
    fn serde_serialises_every_variant_as_snake_case() {
        let cases = [
            (ContentType::Text, "\"text\""),
            (ContentType::Url, "\"url\""),
            (ContentType::Email, "\"email\""),
            (ContentType::Json, "\"json\""),
            (ContentType::Jwt, "\"jwt\""),
            (ContentType::Uuid, "\"uuid\""),
            (ContentType::Ipv4, "\"ipv4\""),
            (ContentType::Ipv6, "\"ipv6\""),
            (ContentType::HexColor, "\"hex_color\""),
            (ContentType::Html, "\"html\""),
            (ContentType::FilePath, "\"file_path\""),
            (ContentType::ShellCommand, "\"shell_command\""),
            (ContentType::Sql, "\"sql\""),
            (ContentType::Code, "\"code\""),
            (ContentType::Image, "\"image\""),
        ];
        for (variant, expected) in cases {
            let serialised = serde_json::to_string(&variant).expect("serialise");
            assert_eq!(serialised, expected, "variant = {variant:?}");
        }
    }

    // -----------------------------------------------------------------
    // `clipboard-rich-content` contracts.
    // -----------------------------------------------------------------

    fn image_record() -> EntryRecord {
        EntryRecord {
            id: 1,
            content: IMAGE_CONTENT_SENTINEL.to_string(),
            content_type: ContentType::Image,
            content_size: 128,
            content_hash: "a".repeat(64),
            source_app: Some("com.example.App".into()),
            is_pinned: false,
            created_at: "2026-01-02T03:04:05Z".into(),
            updated_at: "2026-01-02T03:04:05Z".into(),
            last_seen_at: "2026-01-02T03:04:05Z".into(),
            title: None,
            source_app_name: None,
            source_app_icon_ref: None,
            asset_ref: Some(format!("clipboard/{}.png", "a".repeat(64))),
            mime_type: Some(IMAGE_MIME_PNG.to_string()),
            payload_width: Some(4),
            payload_height: Some(2),
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
    fn image_wire_format_is_stable() {
        // The `image` literal is the persisted wire format shared by
        // SQLite, the Tauri boundary and the frontend. It must not
        // drift.
        assert_eq!(ContentType::Image.as_str(), "image");
        assert_eq!(ContentType::Image.to_string(), "image");
    }

    #[test]
    fn image_is_not_textual_so_search_never_sees_it() {
        assert!(!ContentType::Image.is_textual());
    }

    #[test]
    fn image_mime_and_sentinel_constants_are_stable() {
        assert_eq!(IMAGE_MIME_PNG, "image/png");
        assert_eq!(IMAGE_CONTENT_SENTINEL, "");
    }

    #[test]
    fn coherent_image_row_is_renderable() {
        assert!(image_record().is_renderable_image());
    }

    #[test]
    fn incoherent_image_row_is_not_renderable() {
        // Each missing piece of metadata independently disqualifies the
        // row, so the frontend renders its accessible fallback instead
        // of showing the empty `content` sentinel as text.
        let mut without_ref = image_record();
        without_ref.asset_ref = None;
        assert!(!without_ref.is_renderable_image());

        let mut empty_ref = image_record();
        empty_ref.asset_ref = Some(String::new());
        assert!(!empty_ref.is_renderable_image());

        let mut without_mime = image_record();
        without_mime.mime_type = None;
        assert!(!without_mime.is_renderable_image());

        let mut zero_width = image_record();
        zero_width.payload_width = Some(0);
        assert!(!zero_width.is_renderable_image());

        let mut without_height = image_record();
        without_height.payload_height = None;
        assert!(!without_height.is_renderable_image());
    }

    #[test]
    fn textual_row_is_never_treated_as_an_image() {
        let mut textual = image_record();
        textual.content_type = ContentType::Text;
        textual.content = "hello".into();
        assert!(!textual.is_renderable_image());
    }

    #[test]
    fn new_entry_text_helper_leaves_payload_metadata_empty() {
        let entry = NewEntry::text(
            "hello".into(),
            ContentType::Text,
            5,
            "hash".into(),
            None,
            OffsetDateTime::UNIX_EPOCH,
            OffsetDateTime::UNIX_EPOCH,
        );
        assert!(entry.asset_ref.is_none());
        assert!(entry.mime_type.is_none());
        assert!(entry.payload_width.is_none());
        assert!(entry.payload_height.is_none());
        assert!(entry.code_language.is_none());
    }

    #[test]
    fn new_entry_code_helper_sets_content_type_and_code_language() {
        let entry = NewEntry::code(
            "print('hi')".into(),
            11,
            "hash".into(),
            None,
            Some("python".into()),
            OffsetDateTime::UNIX_EPOCH,
            OffsetDateTime::UNIX_EPOCH,
        );
        assert_eq!(entry.content_type, ContentType::Code);
        assert_eq!(entry.code_language.as_deref(), Some("python"));
    }

    #[test]
    fn with_code_language_trims_and_drops_empty_strings() {
        let entry = NewEntry::text(
            "x".into(),
            ContentType::Code,
            1,
            "hash".into(),
            None,
            OffsetDateTime::UNIX_EPOCH,
            OffsetDateTime::UNIX_EPOCH,
        )
        .with_code_language(Some("  python  ".into()));
        assert_eq!(entry.code_language.as_deref(), Some("python"));

        let empty = entry.clone().with_code_language(Some("   ".into()));
        assert!(empty.code_language.is_none());

        let cleared = entry.with_code_language(None);
        assert!(cleared.code_language.is_none());
    }

    #[test]
    fn serialised_image_record_exposes_only_a_relative_reference() {
        // Privacy contract: the record that crosses the Tauri boundary
        // must never carry an absolute filesystem path.
        let json = serde_json::to_string(&image_record()).expect("serialise");
        assert!(json.contains("\"asset_ref\":\"clipboard/"), "got {json}");
        assert!(json.contains("\"mime_type\":\"image/png\""), "got {json}");
        assert!(!json.contains("/Users/"), "absolute path leaked: {json}");
        assert!(!json.contains("/home/"), "absolute path leaked: {json}");
        assert!(
            !json.contains("\"asset_ref\":\"/"),
            "absolute path leaked: {json}"
        );
    }
}
