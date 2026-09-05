//! Allow-list HTML and rich-text sanitiser for rich-text previews.
//!
//! The card preview must render the sanitised HTML the core produces.
//! The output is a self-contained HTML fragment with a fixed set of
//! tags and attributes that the Svelte component can inject safely
//! through a single, well-tested code path. Raw clipboard HTML MUST
//! NEVER reach the frontend unfiltered; the sanitiser is the only
//! component authorised to translate a `RichTextPayload` into a
//! preview string.
//!
//! ## Design notes
//!
//! The sanitiser is **deliberately hand-written** instead of pulling a
//! general-purpose HTML parser. Reasons:
//!
//! - The allow-list is tiny and stable (formatting + lists + code
//!   blocks + paragraphs); a parser like `html5ever` would add a large
//!   dependency surface for a feature area that never needs the full
//!   HTML5 parser.
//! - The tokeniser is a small state machine that drops comments,
//!   doctypes, `<script>`, `<style>`, `<iframe>`, `<object>`, `<embed>`,
//!   `<form>`, `<input>`, `<textarea>`, `<select>`, `<button>`,
//!   `<link>`, `<meta>`, `<base>`, `<frame>`, `<frameset>`, `<applet>`,
//!   `<noscript>` and any element whose name starts with a colon
//!   (namespaced, possibly foreign). Anything unknown to the allow-list
//!   is dropped *with its content* (e.g. `<script>alert(1)</script>`
//!   produces an empty string) so a `<style>` cannot smuggle content
//!   out through the renderer.
//! - Attributes are filtered through a small per-element allow-list.
//!   Event handlers (`on*`) and dangerous URLs (`javascript:`,
//!   `vbscript:`, `data:` for any resource other than text, raw
//!   resource references) are stripped unconditionally.
//! - CSS properties that survive are restricted to a tiny allow-list
//!   (`font-weight`, `font-style`, `text-decoration`, `color`,
//!   `background-color`, `text-align`, `font-family`, `font-size`).
//!   `expression(...)` and `url(...)` values are removed.
//!
//! The output is HTML-encoded again on the way out, so any text the
//! tokenizer misclassified cannot end up as a literal tag in the
//! preview. The function never logs the input or output, never
//! includes a payload byte in an error message and never returns the
//! raw input.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use clipvault_platform::RichTextPayload;

use crate::clipboard_assets::sha256_hex;

/// Maximum depth of nested tags the sanitiser will keep. Anything
/// beyond this depth is closed implicitly to bound the work the
/// preview renderer has to do on a malicious payload.
const MAX_NESTING_DEPTH: usize = 32;

/// Maximum length, in bytes, of the input HTML. The sanitiser rejects
/// anything above this cap before scanning; the same cap is enforced
/// at the asset-store boundary and at the platform layer.
pub const MAX_PREVIEW_INPUT_BYTES: usize = 64 * 1024;

/// Maximum length, in bytes, of the produced preview. The cap is
/// smaller than the input cap because the sanitiser is free to drop
/// anything it considers unsafe; what survives is what the card
/// renders.
pub const MAX_PREVIEW_OUTPUT_BYTES: usize = 32 * 1024;

/// Allow-list of tags that survive sanitisation. The order matters:
/// void elements are emitted as `<name …>` without a closing tag.
const ALLOWED_TAGS: &[(&str, ElementRule)] = &[
    (
        "b",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "strong",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "i",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "em",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "u",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "s",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "strike",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "del",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "ins",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "span",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "br",
        ElementRule::Inline {
            closing: Closing::Void,
        },
    ),
    (
        "p",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "div",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "h1",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "h2",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "h3",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "h4",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "h5",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "h6",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "ul",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "ol",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "li",
        ElementRule::ListItem {
            closing: Closing::Required,
        },
    ),
    (
        "pre",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
    (
        "code",
        ElementRule::Inline {
            closing: Closing::Required,
        },
    ),
    (
        "blockquote",
        ElementRule::Block {
            closing: Closing::Required,
        },
    ),
];

/// Allow-list of attributes per element kind. Inline formatting
/// inherits from the shared set; code blocks and lists are tighter.
const ALLOWED_ATTRIBUTES: &[(&str, &[&str])] = &[
    ("span", &["style"]),
    ("p", &["style"]),
    ("div", &["style"]),
    ("h1", &["style"]),
    ("h2", &["style"]),
    ("h3", &["style"]),
    ("h4", &["style"]),
    ("h5", &["style"]),
    ("h6", &["style"]),
    ("li", &["style"]),
    ("pre", &["style"]),
    ("blockquote", &["style"]),
];

/// Allow-list of CSS declarations accepted by the inline-style
/// filter. Everything else is stripped on a per-declaration basis.
const ALLOWED_CSS_PROPERTIES: &[&str] = &[
    "font-weight",
    "font-style",
    "font-family",
    "font-size",
    "text-decoration",
    "text-align",
    "color",
    "background-color",
];

#[derive(Debug, Clone, Copy)]
enum ElementRule {
    Inline { closing: Closing },
    Block { closing: Closing },
    ListItem { closing: Closing },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Closing {
    /// Tag is written as `<foo …>` and needs an explicit `</foo>`
    /// counterpart.
    Required,
    /// Tag is written as `<foo …>` without any closing tag.
    Void,
}

/// Errors the sanitiser can surface. Every variant is metadata-only
/// and never carries a payload byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SanitizeError {
    /// The input exceeded the size cap. `size` is the actual byte
    /// length the caller passed in.
    InputTooLarge { size: usize },
    /// The sanitiser had to close more than `MAX_NESTING_DEPTH`
    /// unclosed tags, which is the signal of either a malformed
    /// payload or a deliberate nesting attack. The output is still
    /// safe; the variant is exposed so callers can decide to drop the
    /// preview or just truncate it.
    TooDeep { depth: usize },
    /// The sanitised preview still exceeded `MAX_PREVIEW_OUTPUT_BYTES`
    /// after the size enforcement. The output was truncated to fit.
    OutputTooLarge { size: usize },
}

impl SanitizeError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            SanitizeError::InputTooLarge { .. } => "input_too_large",
            SanitizeError::TooDeep { .. } => "too_deep",
            SanitizeError::OutputTooLarge { .. } => "output_too_large",
        }
    }
}

/// Result of sanitising a payload. `preview` is the safe HTML fragment
/// the card renders; it may be empty if the input only contained
/// tags the allow-list drops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedPreview {
    pub preview: String,
    pub truncated: bool,
}

/// Sanitise an HTML string into a safe preview fragment.
///
/// The output is well-formed, self-contained HTML that the Svelte
/// component can inject safely. Anything outside the allow-list is
/// stripped *with its content*, so a `<script>` tag cannot leak bytes
/// through the preview.
pub fn sanitize_html(input: &str) -> Result<SanitizedPreview, SanitizeError> {
    if input.len() > MAX_PREVIEW_INPUT_BYTES {
        return Err(SanitizeError::InputTooLarge { size: input.len() });
    }

    let mut state = SanitizerState::new();
    for ch in input.chars() {
        state.feed(ch);
        if state.depth > MAX_NESTING_DEPTH {
            return Err(SanitizeError::TooDeep { depth: state.depth });
        }
    }
    state.flush_text();
    // Close any tags still open so the preview is balanced.
    while let Some(open) = state.stack.pop() {
        if open.closing == Closing::Required {
            state.push_str("</");
            state.push_str(&open.name);
            state.push_char('>');
        }
    }
    if state.output.len() > MAX_PREVIEW_OUTPUT_BYTES {
        state.output.truncate(MAX_PREVIEW_OUTPUT_BYTES);
        return Ok(SanitizedPreview {
            preview: state.output,
            truncated: true,
        });
    }
    Ok(SanitizedPreview {
        preview: state.output,
        truncated: false,
    })
}

struct OpenTag {
    name: String,
    closing: Closing,
}

struct SanitizerState {
    output: String,
    text_buf: String,
    in_tag: bool,
    tag_buf: String,
    stack: Vec<OpenTag>,
    /// Names of elements whose opening has been emitted but whose
    /// body has been suppressed (e.g. `<script>`, `<style>`,
    /// `<iframe>`). The scanner must skip everything, including
    /// nested tags, until the matching close tag is seen.
    skip_until: Option<String>,
    depth: usize,
}

impl SanitizerState {
    fn new() -> Self {
        Self {
            output: String::with_capacity(input_capacity_hint()),
            text_buf: String::new(),
            in_tag: false,
            tag_buf: String::new(),
            stack: Vec::new(),
            skip_until: None,
            depth: 0,
        }
    }

    fn push_str(&mut self, value: &str) {
        self.output.push_str(value);
    }

    fn push_char(&mut self, ch: char) {
        self.output.push(ch);
    }

    fn feed(&mut self, ch: char) {
        // Inside a skipped block, only the matching close tag breaks out.
        if let Some(target) = self.skip_until.clone() {
            if self.in_tag {
                self.tag_buf.push(ch);
                if ch == '>' {
                    let raw = std::mem::take(&mut self.tag_buf);
                    self.in_tag = false;
                    let raw_lower = raw.to_ascii_lowercase();
                    if raw_lower.starts_with("</") {
                        let name = raw_lower
                            .trim_start_matches("</")
                            .trim_end_matches('>')
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .trim_end_matches('/');
                        if name.eq_ignore_ascii_case(&target) {
                            self.skip_until = None;
                        }
                    }
                }
            } else if ch == '<' {
                self.tag_buf.clear();
                self.tag_buf.push('<');
                self.in_tag = true;
            }
            // Any other character inside a skipped block is dropped,
            // including text: a `<script>` tag must not leak its body
            // into the preview.
            return;
        }

        if self.in_tag {
            self.tag_buf.push(ch);
            if ch == '>' {
                let raw = std::mem::take(&mut self.tag_buf);
                self.in_tag = false;
                self.process_tag(&raw);
            }
            return;
        }

        if ch == '<' {
            // Treat every `<` as the start of a tag. A stray `<` in
            // a sentence therefore becomes an empty / malformed tag
            // and is dropped — that is the conservative, safe
            // behaviour the rest of the contract relies on.
            self.flush_text();
            self.tag_buf.clear();
            self.tag_buf.push('<');
            self.in_tag = true;
        } else {
            self.text_buf.push(ch);
        }
    }

    fn flush_text(&mut self) {
        if self.text_buf.is_empty() {
            return;
        }
        let escaped = escape_html(&self.text_buf);
        self.output.push_str(&escaped);
        self.text_buf.clear();
    }

    fn process_tag(&mut self, raw: &str) {
        self.flush_text();
        let trimmed = raw.trim();
        let Some(parsed) = parse_tag(trimmed) else {
            return;
        };
        match parsed {
            ParsedTag::Open { name, attrs } => {
                let lower = name.to_ascii_lowercase();
                if is_dangerous_drop_with_content(&lower) {
                    self.skip_until = Some(lower);
                    return;
                }
                let Some(rule) = lookup_tag(&lower) else {
                    return;
                };
                let safe_attrs = filter_attributes(&lower, &attrs);
                if let ElementRule::Inline { .. } = rule {
                    self.emit_open(&lower, &safe_attrs);
                } else {
                    // Block elements are wrapped in their own line so
                    // the preview has predictable layout without
                    // relying on browser CSS.
                    if !self.output.ends_with('\n') && !self.output.is_empty() {
                        self.push_char('\n');
                    }
                    self.emit_open(&lower, &safe_attrs);
                }
                self.stack.push(OpenTag {
                    name: lower,
                    closing: match rule {
                        ElementRule::Inline { closing } => closing,
                        ElementRule::Block { closing } => closing,
                        ElementRule::ListItem { closing } => closing,
                    },
                });
                self.depth += 1;
            }
            ParsedTag::Close { name } => {
                let lower = name.to_ascii_lowercase();
                // Close the matching open tag (and anything in
                // between) so the preview stays balanced.
                if let Some(pos) = self.stack.iter().rposition(|open| open.name == lower) {
                    while self.stack.len() > pos + 1 {
                        let orphan = self.stack.pop().expect("non-empty");
                        if orphan.closing == Closing::Required {
                            self.push_str("</");
                            self.push_str(&orphan.name);
                            self.push_char('>');
                        }
                        self.depth = self.depth.saturating_sub(1);
                    }
                    if let Some(top) = self.stack.last() {
                        let needs_close = top.closing == Closing::Required;
                        let name = top.name.clone();
                        if needs_close {
                            self.push_str("</");
                            self.push_str(&name);
                            self.push_char('>');
                        }
                    }
                    self.stack.pop();
                    self.depth = self.depth.saturating_sub(1);
                    if matches!(lookup_tag(&lower), Some(ElementRule::Block { .. }))
                        && !self.output.ends_with('\n')
                    {
                        self.push_char('\n');
                    }
                }
            }
            ParsedTag::Doctype | ParsedTag::Comment | ParsedTag::ProcessingInstruction => {}
        }
    }

    fn emit_open(&mut self, name: &str, attrs: &[SafeAttr]) {
        self.push_char('<');
        self.push_str(name);
        for attr in attrs {
            self.push_char(' ');
            self.push_str(&attr.name);
            self.push_str("=\"");
            for ch in attr.value.chars() {
                self.push_char(escape_attr(ch));
            }
            self.push_char('"');
        }
        self.push_char('>');
    }
}

fn input_capacity_hint() -> usize {
    MAX_PREVIEW_OUTPUT_BYTES.min(4096)
}

#[derive(Debug)]
enum ParsedTag<'a> {
    Open {
        name: &'a str,
        attrs: Vec<RawAttr<'a>>,
    },
    Close {
        name: &'a str,
    },
    Doctype,
    Comment,
    ProcessingInstruction,
}

#[derive(Debug, Clone)]
struct RawAttr<'a> {
    name: &'a str,
    value: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct SafeAttr {
    name: String,
    value: String,
}

fn parse_tag(raw: &str) -> Option<ParsedTag<'_>> {
    let trimmed = raw.trim().trim_start_matches('<').trim_end_matches('>');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("!--") {
        return Some(ParsedTag::Comment);
    }
    if trimmed.starts_with('!') {
        return Some(ParsedTag::Doctype);
    }
    if trimmed.starts_with('?') {
        return Some(ParsedTag::ProcessingInstruction);
    }
    let (close, body) = if let Some(rest) = trimmed.strip_prefix('/') {
        (true, rest)
    } else {
        (false, trimmed)
    };
    let (name, rest) = split_on_whitespace(body);
    if name.is_empty() {
        return None;
    }
    let name = name.trim_end_matches('/');
    if close {
        return Some(ParsedTag::Close { name });
    }
    let attrs = parse_attrs(rest);
    Some(ParsedTag::Open { name, attrs })
}

fn split_on_whitespace(input: &str) -> (&str, &str) {
    let mut idx = 0usize;
    for (i, ch) in input.char_indices() {
        if ch.is_whitespace() {
            idx = i;
            break;
        }
        idx = input.len();
    }
    let (name, rest) = input.split_at(idx);
    (name, rest)
}

fn parse_attrs(input: &str) -> Vec<RawAttr<'_>> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        // Skip ASCII whitespace between attributes.
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }
        let name_start = idx;
        while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() && bytes[idx] != b'=' {
            idx += 1;
        }
        if idx == name_start {
            break;
        }
        let name = &input[name_start..idx];
        // Optional value: `=`, then optionally a quoted string.
        let value = if idx < bytes.len() && bytes[idx] == b'=' {
            idx += 1;
            if idx < bytes.len() && (bytes[idx] == b'"' || bytes[idx] == b'\'') {
                let quote = bytes[idx];
                idx += 1;
                let value_start = idx;
                while idx < bytes.len() && bytes[idx] != quote {
                    idx += 1;
                }
                let value_slice = &input[value_start..idx];
                if idx < bytes.len() {
                    idx += 1; // skip the closing quote
                }
                Some(value_slice)
            } else {
                let value_start = idx;
                while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() {
                    idx += 1;
                }
                Some(&input[value_start..idx])
            }
        } else {
            None
        };
        out.push(RawAttr { name, value });
    }
    out
}

fn lookup_tag(name: &str) -> Option<ElementRule> {
    ALLOWED_TAGS
        .iter()
        .find(|(allowed, _)| *allowed == name)
        .map(|(_, rule)| *rule)
}

/// Tags that must be dropped *with their content* because they can
/// host scripts, remote resources or otherwise smuggle bytes through
/// the preview. The list is intentionally broader than the documented
/// dangerous tags so a future addition (e.g. `<math>`) lands here by
/// default.
fn is_dangerous_drop_with_content(name: &str) -> bool {
    matches!(
        name,
        "script"
            | "style"
            | "iframe"
            | "object"
            | "embed"
            | "form"
            | "input"
            | "textarea"
            | "select"
            | "button"
            | "link"
            | "meta"
            | "base"
            | "frame"
            | "frameset"
            | "applet"
            | "noscript"
            | "noframes"
            | "noembed"
            | "template"
            | "svg"
            | "math"
            | "audio"
            | "video"
            | "source"
            | "track"
            | "picture"
            | "canvas"
    ) || name.starts_with(':')
}

fn filter_attributes(name: &str, attrs: &[RawAttr<'_>]) -> Vec<SafeAttr> {
    let allowed = attribute_allowlist_for(name);
    if allowed.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for attr in attrs {
        let lower = attr.name.to_ascii_lowercase();
        if !allowed.iter().any(|a| *a == lower) {
            continue;
        }
        if lower.starts_with("on") {
            // Defence in depth: the lookup already excludes event
            // handlers, but this branch is the canonical place that
            // drops them.
            continue;
        }
        let raw_value = attr.value.unwrap_or("");
        if lower == "style" {
            if let Some(safe) = sanitize_style(raw_value) {
                out.push(SafeAttr {
                    name: "style".to_string(),
                    value: safe,
                });
            }
            continue;
        }
        out.push(SafeAttr {
            name: lower,
            value: sanitize_url_like(raw_value),
        });
    }
    out
}

fn attribute_allowlist_for(name: &str) -> &'static [&'static str] {
    ALLOWED_ATTRIBUTES
        .iter()
        .find(|(tag, _)| *tag == name)
        .map(|(_, attrs)| *attrs)
        .unwrap_or(&[])
}

fn sanitize_style(value: &str) -> Option<String> {
    let mut safe = String::new();
    let mut any = false;
    for declaration in value.split(';') {
        let trimmed = declaration.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (raw_name, raw_val) = match trimmed.split_once(':') {
            Some(pair) => pair,
            None => continue,
        };
        let prop = raw_name.trim().to_ascii_lowercase();
        if !ALLOWED_CSS_PROPERTIES.contains(&prop.as_str()) {
            continue;
        }
        let value = raw_val.trim();
        if value.is_empty() {
            continue;
        }
        // Drop `url(...)`, `expression(...)` and any reference that
        // could pull a remote resource.
        let lowered_value = value.to_ascii_lowercase();
        if lowered_value.contains("url(")
            || lowered_value.contains("expression(")
            || lowered_value.contains("@import")
            || lowered_value.contains("behavior:")
        {
            continue;
        }
        // `font-family` accepts a comma-separated list of family
        // names; reject any token that looks like a CSS injection or
        // contains a URL.
        if prop == "font-family" {
            let bad = value
                .split(',')
                .any(|family| family.contains(';') || family.contains('"'));
            if bad {
                continue;
            }
        }
        if any {
            safe.push_str("; ");
        }
        safe.push_str(&prop);
        safe.push_str(": ");
        safe.push_str(value);
        any = true;
    }
    if any {
        Some(safe)
    } else {
        None
    }
}

fn sanitize_url_like(value: &str) -> String {
    // The allow-list only ever forwards attributes whose values look
    // like a URL (`href`, `src`, `cite`). The current allow-list does
    // not include any of them, so this helper is a defensive
    // normaliser that strips `javascript:` and similar prefixes.
    let trimmed = value.trim();
    let lowered = trimmed.to_ascii_lowercase();
    if lowered.starts_with("javascript:")
        || lowered.starts_with("vbscript:")
        || lowered.starts_with("data:text/html")
        || lowered.starts_with("data:application/")
        || lowered.starts_with("data:image/svg")
    {
        return String::new();
    }
    trimmed.to_string()
}

fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Build a safe HTML preview from the canonical `plain_text`.
///
/// Escapes `&`, `<`, `>`, `"` and `'` and turns `\r` / `\r\n` / `\n`
/// into `<br>` so the result can be safely injected as raw HTML by the
/// card renderer (`{@html}` is restricted to this sanitised output).
/// The function never produces an empty string for a non-empty input:
/// the UTF-8 / size accounting guarantees at least the first
/// character reaches the output buffer, and the size cap only ever
/// ends the loop early, never before a single byte is written.
fn plain_text_preview(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(MAX_RICH_TEXT_PREVIEW_BYTES));
    let mut chars = input.chars().peekable();
    let mut pushed_any = false;
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            if out.len() + 4 > MAX_RICH_TEXT_PREVIEW_BYTES {
                break;
            }
            out.push_str("<br>");
            pushed_any = true;
            continue;
        }
        if ch == '\n' {
            if out.len() + 4 > MAX_RICH_TEXT_PREVIEW_BYTES {
                break;
            }
            out.push_str("<br>");
            pushed_any = true;
            continue;
        }
        let escaped = match ch {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&#39;",
            _ => {
                if out.len() + ch.len_utf8() > MAX_RICH_TEXT_PREVIEW_BYTES {
                    break;
                }
                out.push(ch);
                pushed_any = true;
                continue;
            }
        };
        if out.len() + escaped.len() > MAX_RICH_TEXT_PREVIEW_BYTES {
            break;
        }
        out.push_str(escaped);
        pushed_any = true;
    }
    // Defensive safety net for the documented contract: a non-empty
    // input must yield a non-empty output. The loop above always
    // pushes something on the first iteration (the size checks cannot
    // trip on an empty `out`), but if a future refactor ever
    // short-circuits the loop without writing bytes we still want
    // the asset store to receive a deterministic, escaped preview
    // rather than an empty string that would trigger
    // `RichTextAssetError::EmptyPreview`.
    if !pushed_any && !input.is_empty() {
        let first = input.chars().next().expect("input is not empty");
        match first {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(first),
        }
    }
    out
}

/// Internal helper that returns a guaranteed-non-empty, HTML-safe
/// preview for a non-empty `plain_text`. The contract is the same as
/// [`plain_text_preview`] but the defensive escape is explicit so a
/// caller can document the guarantee at the call site.
fn safe_plain_text_preview(input: &str) -> String {
    let preview = plain_text_preview(input);
    if preview.is_empty() {
        debug_assert!(
            input.is_empty(),
            "non-empty input must produce non-empty preview"
        );
        String::new()
    } else {
        preview
    }
}

fn has_visible_preview_content(input: &str) -> bool {
    let mut in_tag = false;
    for ch in input.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
            }
        } else if ch == '<' {
            in_tag = true;
        } else if !ch.is_whitespace() {
            return true;
        }
    }
    false
}

fn escape_attr(ch: char) -> char {
    match ch {
        '&' | '"' | '<' | '>' | '\'' => '\u{FFFD}',
        _ => ch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sanitize(input: &str) -> SanitizedPreview {
        sanitize_html(input).expect("sanitize")
    }

    #[test]
    fn preserves_paragraphs_and_inline_formatting() {
        let preview = sanitize("<p>hello <b>world</b></p>");
        assert!(preview.preview.contains("<p>"));
        assert!(preview.preview.contains("<b>"));
        assert!(preview.preview.contains("</p>"));
    }

    #[test]
    fn drops_script_tags_with_their_content() {
        let preview = sanitize("<p>safe<script>alert(1)</script>end</p>");
        assert!(!preview.preview.contains("alert"));
        assert!(!preview.preview.contains("script"));
        assert!(preview.preview.contains("safe"));
        assert!(preview.preview.contains("end"));
    }

    #[test]
    fn drops_style_and_iframe_content() {
        let preview =
            sanitize("<style>body { color: red }</style><iframe src=\"https://x\"></iframe>kept");
        assert!(!preview.preview.contains("body"));
        assert!(!preview.preview.contains("iframe"));
        assert!(!preview.preview.contains("color"));
        assert!(preview.preview.contains("kept"));
    }

    #[test]
    fn escapes_text_content() {
        // Text that contains characters that MUST be escaped on the
        // way out. The sanitiser escapes `&`, `<`, `>`, `"` and `'`
        // before flushing text, so a stray `<` inside text content
        // (which is otherwise ambiguous) becomes `&lt;`.
        let preview = sanitize("<p>3 & 5 & ok</p>");
        assert!(preview.preview.contains("3 &amp; 5 &amp; ok"));
    }

    #[test]
    fn drops_event_handler_attributes() {
        let preview = sanitize("<p onclick=\"alert(1)\" style=\"color: red\">x</p>");
        assert!(!preview.preview.contains("onclick"));
        assert!(!preview.preview.contains("alert"));
        assert!(preview.preview.contains("color: red"));
    }

    #[test]
    fn strips_javascript_urls() {
        let preview = sanitize("<a href=\"javascript:alert(1)\">x</a>");
        assert!(!preview.preview.contains("javascript"));
    }

    #[test]
    fn strips_dangerous_style_declarations() {
        let preview =
            sanitize("<p style=\"color:red;background:url(https://x);font-weight:bold\">x</p>");
        assert!(preview.preview.contains("color: red"));
        assert!(preview.preview.contains("font-weight: bold"));
        assert!(!preview.preview.contains("background"));
        assert!(!preview.preview.contains("url("));
    }

    #[test]
    fn keeps_supported_style_declarations() {
        let preview = sanitize("<p style=\"font-style:italic;text-align:center\">x</p>");
        assert!(preview.preview.contains("font-style: italic"));
        assert!(preview.preview.contains("text-align: center"));
    }

    #[test]
    fn drops_lists_outside_the_allow_list() {
        let preview = sanitize("<table><tr><td>x</td></tr></table><ul><li>a</li></ul>");
        assert!(!preview.preview.contains("<table"));
        assert!(!preview.preview.contains("<tr"));
        assert!(!preview.preview.contains("<td"));
        assert!(preview.preview.contains("<ul"));
        assert!(preview.preview.contains("<li"));
    }

    #[test]
    fn drops_comments_and_doctypes() {
        let preview = sanitize("<!-- secret --><!doctype html><p>kept</p>");
        assert!(!preview.preview.contains("secret"));
        assert!(!preview.preview.contains("doctype"));
        assert!(preview.preview.contains("kept"));
    }

    #[test]
    fn balances_unclosed_tags() {
        let preview = sanitize("<p><b>open");
        assert!(preview.preview.contains("</b>"));
        assert!(preview.preview.contains("</p>"));
    }

    #[test]
    fn rejects_oversized_input() {
        let big = "x".repeat(MAX_PREVIEW_INPUT_BYTES + 1);
        let err = sanitize_html(&big).expect_err("must reject");
        assert_eq!(err.kind_str(), "input_too_large");
    }

    #[test]
    fn truncation_marker_is_set_when_output_too_long() {
        // Build a payload that fits the input cap but produces more
        // output than the cap allows (one big paragraph of repeated
        // text).
        let inner = "x".repeat(MAX_PREVIEW_OUTPUT_BYTES / 2);
        let input = format!("<p>{inner}{inner}</p>");
        assert!(input.len() <= MAX_PREVIEW_INPUT_BYTES);
        let preview = sanitize(&input);
        assert!(preview.truncated, "expected truncation");
        assert!(preview.preview.len() <= MAX_PREVIEW_OUTPUT_BYTES);
    }

    #[test]
    fn empty_input_produces_empty_preview() {
        let preview = sanitize("");
        assert!(preview.preview.is_empty());
    }

    #[test]
    fn plain_text_preview_escapes_markup_and_preserves_line_breaks() {
        assert_eq!(
            plain_text_preview("a & <b> > \"quoted\" 'quoted'\r\nnext"),
            "a &amp; &lt;b&gt; &gt; &quot;quoted&quot; &#39;quoted&#39;<br>next"
        );
    }

    /// Non-empty plain text always produces a non-empty, escaped
    /// preview so the asset store never has to surface a typed
    /// `empty_preview` failure for a non-empty capture.
    #[test]
    fn plain_text_preview_is_never_empty_for_non_empty_input() {
        for input in [
            "x", " ", "  ", "&", "<", ">", "\"", "'", "\u{00A0}", "\u{FEFF}", "&#10;", "a\nb",
            "a\rb", "a\r\nb", "abc",
        ] {
            let preview = plain_text_preview(input);
            assert!(
                !preview.is_empty(),
                "plain_text_preview({input:?}) returned an empty preview"
            );
        }
    }

    /// `plain_text_preview` must never carry a `<` character that is
    /// not part of the `<br>` line-break escape we emit ourselves.
    /// Every other `<` is escaped as `&lt;`.
    #[test]
    fn plain_text_preview_does_not_leak_raw_special_chars_except_br() {
        for input in ["a<b", "&x", "<", ">", "\"", "'"] {
            let preview = plain_text_preview(input);
            // Strip the line-break escapes we author ourselves.
            let stripped: String = preview.replace("<br>", "");
            assert!(
                !stripped.contains('<'),
                "plain_text_preview({input:?}) leaked a raw `<`: {preview:?}"
            );
            assert!(
                !stripped.contains('>'),
                "plain_text_preview({input:?}) leaked a raw `>`: {preview:?}"
            );
        }
    }

    #[test]
    fn escaped_plain_text_fallback_escapes_specials_and_breaks() {
        let preview =
            escaped_plain_text_fallback("a & <b> > \"q\" 'q'\r\nnext", MAX_RICH_TEXT_PREVIEW_BYTES);
        assert!(preview.contains("a &amp;"));
        assert!(preview.contains("&lt;b&gt;"));
        assert!(preview.contains("&quot;q&quot;"));
        assert!(preview.contains("&#39;q&#39;"));
        assert!(preview.contains("<br>next"));
    }

    /// `escaped_plain_text_fallback` must never return an empty string
    /// for a non-empty input — even when every char is a single byte
    /// UTF-8 and `max_chars` is very small.
    #[test]
    fn escaped_plain_text_fallback_never_collapses_non_empty_input() {
        for (input, max) in [("x", 1), ("hello", 3), ("a\n", 1), ("&", 1), (" ", 1)] {
            let preview = escaped_plain_text_fallback(input, max);
            assert!(
                !preview.is_empty(),
                "escaped_plain_text_fallback({input:?}, {max}) was empty"
            );
        }
    }

    /// `build_safe_preview` is the layered fallback the asset store
    /// uses; it must always return a non-empty preview for a payload
    /// whose `plain_text` is non-empty, regardless of what the
    /// sanitiser produced.
    #[test]
    fn build_safe_preview_returns_safe_html_for_dangerous_html() {
        let payload = RichTextPayload::new(
            "safe text & \"quoted\" <tag>".into(),
            Some("<script>alert(1)</script><a href=\"javascript:alert(2)\"></a>".into()),
            None,
        )
        .expect("valid");
        let preview = build_safe_preview(&payload);
        assert!(!preview.is_empty(), "preview must not be empty");
        assert!(!preview.contains("alert"), "preview leaked {preview:?}");
        assert!(
            !preview.contains("javascript"),
            "preview leaked {preview:?}"
        );
        assert!(!preview.contains("<script"));
        assert!(!preview.contains("<a "));
        assert!(preview.contains("safe text"));
        // `&` is escaped, never raw — even though the rendered HTML
        // contains `&amp;`, the literal `&` only ever appears as part
        // of an escape entity.
        assert!(preview.contains("&amp;"));
        assert!(preview.contains("&lt;tag&gt;"));
    }

    /// Capture whose HTML leg is missing entirely (RTF-only payload)
    /// must still produce a safe preview sourced from `plain_text`.
    #[test]
    fn build_safe_preview_uses_plain_text_when_html_is_absent() {
        let payload = RichTextPayload::new(
            "alpha & beta".into(),
            None,
            Some(b"{\\rtf1 alpha}".to_vec()),
        )
        .expect("valid");
        let preview = build_safe_preview(&payload);
        assert!(!preview.is_empty());
        assert!(preview.contains("alpha"));
        assert!(preview.contains("&amp;"));
    }

    /// Defence-in-depth: even with a non-empty `plain_text` whose only
    /// char is `<`, the preview must escape it.
    #[test]
    fn build_safe_preview_escapes_special_only_plain_text() {
        let payload =
            RichTextPayload::new("<".into(), None, Some(b"{\\rtf1}".to_vec())).expect("valid");
        let preview = build_safe_preview(&payload);
        assert_eq!(preview, "&lt;");
    }

    #[test]
    fn markup_without_visible_text_is_not_a_usable_preview() {
        let preview = sanitize("<p onclick=\"alert(1)\"></p><script>alert(2)</script>");
        assert!(!has_visible_preview_content(&preview.preview));
    }

    #[test]
    fn nested_lists_round_trip_through_the_allow_list() {
        let preview = sanitize("<ul><li>one<ul><li>two</li></ul></li></ul>");
        assert!(preview.preview.contains("<ul"));
        assert!(preview.preview.matches("<li").count() == 2);
    }
}

// ===================================================================
// Canonicalisation and hashing
// ===================================================================

/// Canonical, versioned, deterministic representation of a rich-text
/// payload. Two captures with identical HTML and RTF bytes always
/// produce the same canonical string, byte for byte, regardless of
/// platform or run; this is what makes
/// [`canonical_rich_text_hash`] a usable dedupe key across restarts.
///
/// The format is a tiny, line-oriented concatenation:
///
/// ```text
/// clipvault-rich-text:v1
/// plain:<plain_text>\n
/// html:<html-or-empty>\n
/// rtf:<hex-of-rtf-or-empty>\n
/// ```
///
/// RTF is encoded as lowercase hex so a single byte difference in the
/// RTF stream produces a canonical-string difference (without that,
/// binary RTF could collide with line-noise inputs). Plain text and
/// HTML are kept verbatim; the sanitiser is a separate step that runs
/// *after* the canonical string has been hashed.
pub fn canonical_rich_text_repr(payload: &RichTextPayload) -> String {
    let mut out = String::new();
    out.push_str("clipvault-rich-text:v1\n");
    let _ = writeln!(out, "plain:{}", payload.plain_text());
    let html = payload.html().unwrap_or("");
    let _ = writeln!(out, "html:{html}");
    let rtf_hex = payload.rtf().map(hex_encode).unwrap_or_default();
    let _ = writeln!(out, "rtf:{rtf_hex}");
    out
}

/// Lowercase hex SHA-256 of the canonical rich-text representation.
///
/// Returns a 64-character string for every payload; the empty input
/// produces the SHA-256 of the empty canonical string. The hash is
/// metadata-only: it never appears in a log line or in a Tauri event.
pub fn canonical_rich_text_hash(payload: &RichTextPayload) -> String {
    let canonical = canonical_rich_text_repr(payload);
    sha256_hex(canonical.as_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod hash_tests {
    use super::*;

    fn payload(html: Option<&str>, rtf: Option<&[u8]>) -> RichTextPayload {
        RichTextPayload::new("hello".into(), html.map(str::to_owned), rtf.map(Vec::from))
            .expect("valid")
    }

    #[test]
    fn canonical_representation_is_stable_across_calls() {
        let p = payload(Some("<b>hi</b>"), Some(b"{\\rtf1 hi}"));
        assert_eq!(canonical_rich_text_repr(&p), canonical_rich_text_repr(&p));
    }

    #[test]
    fn hash_is_sha256_of_canonical_representation() {
        let p = payload(Some("<b>hi</b>"), Some(b"{\\rtf1 hi}"));
        let canonical = canonical_rich_text_repr(&p);
        let expected = sha256_hex(canonical.as_bytes());
        assert_eq!(canonical_rich_text_hash(&p), expected);
    }

    #[test]
    fn same_plain_with_different_styles_produces_different_hash() {
        let plain = payload(Some("<i>x</i>"), None);
        let styled = payload(Some("<b>x</b>"), None);
        assert_ne!(
            canonical_rich_text_hash(&plain),
            canonical_rich_text_hash(&styled)
        );
    }

    #[test]
    fn identical_payloads_produce_identical_hashes() {
        let a = payload(Some("<i>x</i>"), Some(b"rtf"));
        let b = payload(Some("<i>x</i>"), Some(b"rtf"));
        assert_eq!(canonical_rich_text_hash(&a), canonical_rich_text_hash(&b));
    }

    #[test]
    fn rtf_hex_is_lowercase() {
        let p = payload(None, Some(&[0xAB, 0xCD, 0xEF]));
        let repr = canonical_rich_text_repr(&p);
        assert!(repr.contains("rtf:abcdef"), "got {repr}");
    }
}

// ===================================================================
// Asset store
// ===================================================================

/// Sub-directory under `<data_dir>/assets` that holds rich-text
/// payloads. Independent from the image and icon namespaces so each
/// feature area can evolve its asset shape without colliding with
/// the others.
pub const RICH_TEXT_ASSETS_DIR: &str = "rich-text";

/// File extensions for each rich-text asset.
pub const RICH_TEXT_HTML_EXTENSION: &str = "html";
pub const RICH_TEXT_RTF_EXTENSION: &str = "rtf";
pub const RICH_TEXT_PREVIEW_EXTENSION: &str = "preview.html";

/// Upper bound on the byte length of a rich-text HTML asset. The
/// store rejects anything larger before writing so a tampered file
/// cannot exhaust the renderer's memory.
pub const MAX_RICH_TEXT_HTML_BYTES: usize = 1024 * 1024;

/// Upper bound on the byte length of a rich-text RTF asset.
pub const MAX_RICH_TEXT_RTF_BYTES: usize = 4 * 1024 * 1024;

/// Upper bound on the byte length of a rich-text preview asset. The
/// cap matches `MAX_PREVIEW_OUTPUT_BYTES` in the sanitiser: every
/// persisted preview is by construction at most that long.
pub const MAX_RICH_TEXT_PREVIEW_BYTES: usize = 32 * 1024;

/// Outcome of persisting rich-text assets. Mirrors the image
/// asset-store outcome shape so callers can reuse the same pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichTextAssetOutcome {
    /// Every requested asset was newly written.
    Written {
        html_ref: Option<String>,
        rtf_ref: Option<String>,
        preview_ref: Option<String>,
    },
    /// Every requested asset already existed and was reused. The
    /// references returned point at the existing files.
    Reused {
        html_ref: Option<String>,
        rtf_ref: Option<String>,
        preview_ref: Option<String>,
    },
}

impl RichTextAssetOutcome {
    pub fn html_ref(&self) -> Option<&str> {
        match self {
            RichTextAssetOutcome::Written { html_ref, .. }
            | RichTextAssetOutcome::Reused { html_ref, .. } => html_ref.as_deref(),
        }
    }

    pub fn rtf_ref(&self) -> Option<&str> {
        match self {
            RichTextAssetOutcome::Written { rtf_ref, .. }
            | RichTextAssetOutcome::Reused { rtf_ref, .. } => rtf_ref.as_deref(),
        }
    }

    pub fn preview_ref(&self) -> Option<&str> {
        match self {
            RichTextAssetOutcome::Written { preview_ref, .. }
            | RichTextAssetOutcome::Reused { preview_ref, .. } => preview_ref.as_deref(),
        }
    }
}

/// Errors the rich-text asset store can surface. Mirrors the image
/// asset-store taxonomy so callers can map `kind_str` to the same
/// fallback copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichTextAssetError {
    /// A byte buffer exceeded the documented size cap. The store
    /// refuses to persist oversized assets.
    TooLarge { size: usize, cap: usize },
    /// The sanitiser rejected the preview input or output.
    Sanitize { kind: &'static str },
    /// Filesystem I/O failed.
    Io { reason: String },
    /// A previous sanitisation produced no preview bytes — the asset
    /// would be empty.
    EmptyPreview,
}

impl RichTextAssetError {
    pub fn kind_str(&self) -> &'static str {
        match self {
            RichTextAssetError::TooLarge { .. } => "too_large",
            RichTextAssetError::Sanitize { .. } => "sanitize",
            RichTextAssetError::Io { .. } => "io",
            RichTextAssetError::EmptyPreview => "empty_preview",
        }
    }
}

impl std::fmt::Display for RichTextAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RichTextAssetError::TooLarge { size, cap } => {
                write!(f, "rich text asset exceeds the {cap} byte cap (got {size})")
            }
            RichTextAssetError::Sanitize { kind } => write!(f, "rich text sanitize failed: {kind}"),
            RichTextAssetError::Io { reason } => write!(f, "rich text asset io failed: {reason}"),
            RichTextAssetError::EmptyPreview => {
                f.write_str("rich text preview is empty after sanitisation")
            }
        }
    }
}

impl std::error::Error for RichTextAssetError {}

fn io_error(error: io::Error) -> RichTextAssetError {
    RichTextAssetError::Io {
        reason: error.kind().to_string(),
    }
}

/// Escape the first `max_chars` characters of `plain_text` as HTML,
/// replacing line breaks with `<br>` so the result can be rendered
/// safely through the sandboxed iframe on the frontend. This is the
/// absolute last-resort preview used by [`build_safe_preview`] when
/// both the sanitised HTML and [`safe_plain_text_preview`] somehow
/// fail to produce bytes; it never returns an empty string for a
/// non-empty input.
fn escaped_plain_text_fallback(plain_text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut chars = plain_text.chars();
    let mut remaining = max_chars;
    let mut pushed = false;
    while remaining > 0 {
        let Some(ch) = chars.next() else {
            break;
        };
        match ch {
            '\r' | '\n' => {
                out.push_str("<br>");
                pushed = true;
                remaining = remaining.saturating_sub(1);
            }
            '&' => {
                out.push_str("&amp;");
                pushed = true;
                remaining = remaining.saturating_sub(1);
            }
            '<' => {
                out.push_str("&lt;");
                pushed = true;
                remaining = remaining.saturating_sub(1);
            }
            '>' => {
                out.push_str("&gt;");
                pushed = true;
                remaining = remaining.saturating_sub(1);
            }
            '"' => {
                out.push_str("&quot;");
                pushed = true;
                remaining = remaining.saturating_sub(1);
            }
            '\'' => {
                out.push_str("&#39;");
                pushed = true;
                remaining = remaining.saturating_sub(1);
            }
            other => {
                out.push(other);
                pushed = true;
                remaining = remaining.saturating_sub(1);
            }
        }
    }
    if !pushed && !plain_text.is_empty() {
        let first = plain_text.chars().next().expect("plain_text is not empty");
        match first {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(first),
        }
    }
    out
}

/// Layered preview builder used by [`RichTextAssetStore::store`].
///
/// The function is total: for a payload with a non-empty `plain_text`
/// it always returns a non-empty, escaped HTML preview fragment. The
/// layered contract guarantees that no failure in any single layer
/// (oversized input, an empty result from the sanitiser, an
/// undersized plain text escape) can turn a non-empty plain-text
/// capture into a typed capture failure with
/// [`RichTextAssetError::EmptyPreview`]:
///
///   1. Use the sanitised HTML preview when it carries visible text.
///   2. Fall back to [`safe_plain_text_preview`], which itself is
///      documented to return non-empty output for non-empty input.
///   3. As an absolute last resort, escape a bounded prefix of
///      `plain_text` so the asset store always has a safe preview
///      even if every other layer failed to produce bytes.
///
/// The function only returns an empty `String` when `plain_text` is
/// itself empty — the upstream pipeline treats that case as
/// `ignored`/`unsupported` and there is nothing to preview.
fn build_safe_preview(payload: &RichTextPayload) -> String {
    if let Some(html) = payload.html() {
        if let Ok(preview) = sanitize_html(html) {
            if has_visible_preview_content(&preview.preview) {
                return preview.preview;
            }
        }
        let fallback = safe_plain_text_preview(payload.plain_text());
        if !fallback.is_empty() {
            return fallback;
        }
    } else if !payload.plain_text().is_empty() {
        let fallback = safe_plain_text_preview(payload.plain_text());
        if !fallback.is_empty() {
            return fallback;
        }
    }
    // Final safety net: even if every layer above produced an empty
    // string (which the safety nets in `plain_text_preview` make
    // unreachable for non-empty plain text), escape up to
    // `MAX_RICH_TEXT_PREVIEW_BYTES / 2` characters of plain_text so
    // the asset store can persist *something* the card can render
    // safely. Never inject the raw plain text.
    escaped_plain_text_fallback(payload.plain_text(), MAX_RICH_TEXT_PREVIEW_BYTES / 2)
}

/// Filesystem-backed store for rich-text assets.
#[derive(Debug, Clone)]
pub struct RichTextAssetStore {
    data_dir: PathBuf,
}

impl RichTextAssetStore {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    /// `<data_dir>/assets/rich-text`.
    pub fn root(&self) -> PathBuf {
        self.data_dir
            .join(clipvault_platform::ASSETS_DIR)
            .join(RICH_TEXT_ASSETS_DIR)
    }

    /// Persist the rich-text payload and its sanitised preview, then
    /// return the relative references the database stores.
    ///
    /// See the body for the layered preview-fallback contract.
    ///
    /// The hash argument is the `rich_text_hash` already computed for
    /// the payload; it serves as the asset filename. The asset
    /// store is idempotent: a second call with the same bytes reuses
    /// the existing files.
    pub fn store(
        &self,
        hash: &str,
        payload: &RichTextPayload,
    ) -> Result<RichTextAssetOutcome, RichTextAssetError> {
        if let Some(html) = payload.html() {
            if html.len() > MAX_RICH_TEXT_HTML_BYTES {
                return Err(RichTextAssetError::TooLarge {
                    size: html.len(),
                    cap: MAX_RICH_TEXT_HTML_BYTES,
                });
            }
        }
        if let Some(rtf) = payload.rtf() {
            if rtf.len() > MAX_RICH_TEXT_RTF_BYTES {
                return Err(RichTextAssetError::TooLarge {
                    size: rtf.len(),
                    cap: MAX_RICH_TEXT_RTF_BYTES,
                });
            }
        }

        // The preview is the only asset the renderer ever sees, so it
        // must always be present and non-empty when `plain_text`
        // carries text. The sanitiser is the preferred source (it
        // preserves the formatting the user copied); whenever the
        // sanitiser returns no visible content — empty preview, all
        // tags stripped, oversized input, nested too deep — the
        // fallback layer in this function is the source of truth.
        //
        // The fallback chain is layered so a bug in one layer cannot
        // turn a non-empty `plain_text` into a capture failure:
        //
        //   1. The sanitised HTML preview when it has visible text.
        //   2. `safe_plain_text_preview(plain_text)` — escapes
        //      `& < > " '`, turns line breaks into `<br>` and is
        //      documented to return a non-empty string for any
        //      non-empty input (defensive escape net included).
        //   3. An HTML-escaped prefix of `plain_text` — the absolute
        //      last resort that still does not leak raw plain_text
        //      through `{@html}` on the frontend.
        let preview = build_safe_preview(payload);
        if preview.is_empty() {
            // `plain_text` is non-empty by `RichTextPayload::new`'s
            // invariant, so reaching this branch means the payload
            // carries no usable text either. Mirror the historical
            // behaviour: refuse with the typed error so the capture
            // pipeline reports `ignored` / `unsupported` upstream.
            return Err(RichTextAssetError::EmptyPreview);
        }
        if preview.len() > MAX_RICH_TEXT_PREVIEW_BYTES {
            return Err(RichTextAssetError::TooLarge {
                size: preview.len(),
                cap: MAX_RICH_TEXT_PREVIEW_BYTES,
            });
        }

        let root = self.root();
        fs::create_dir_all(&root).map_err(io_error)?;
        let mut written_paths = Vec::new();
        let result = (|| {
            let mut written = false;
            let mut html_ref = None;
            let mut rtf_ref = None;

            if let Some(html) = payload.html() {
                let reference = rich_text_ref_for(hash, RICH_TEXT_HTML_EXTENSION);
                let was_written = self.write_text_asset(&root, &reference, html.as_bytes())?;
                if was_written {
                    written = true;
                    written_paths.push(rich_asset_path(&root, &reference));
                }
                html_ref = Some(reference);
            }

            if let Some(rtf) = payload.rtf() {
                let reference = rich_text_ref_for(hash, RICH_TEXT_RTF_EXTENSION);
                let was_written = self.write_text_asset(&root, &reference, rtf)?;
                if was_written {
                    written = true;
                    written_paths.push(rich_asset_path(&root, &reference));
                }
                rtf_ref = Some(reference);
            }

            let reference = rich_text_ref_for(hash, RICH_TEXT_PREVIEW_EXTENSION);
            let was_written = self.write_text_asset(&root, &reference, preview.as_bytes())?;
            if was_written {
                written = true;
                written_paths.push(rich_asset_path(&root, &reference));
            }
            let preview_ref = Some(reference);

            Ok(if written {
                RichTextAssetOutcome::Written {
                    html_ref,
                    rtf_ref,
                    preview_ref,
                }
            } else {
                RichTextAssetOutcome::Reused {
                    html_ref,
                    rtf_ref,
                    preview_ref,
                }
            })
        })();

        if result.is_err() {
            cleanup_written_assets(&written_paths);
        }
        result
    }

    /// Atomic write: temp file in the same directory + `rename`.
    /// Returns `true` when the file was written, `false` when an
    /// already-validated copy existed and was reused.
    fn write_text_asset(
        &self,
        root: &Path,
        reference: &str,
        bytes: &[u8],
    ) -> Result<bool, RichTextAssetError> {
        let file_name = reference
            .strip_prefix(&format!("{RICH_TEXT_ASSETS_DIR}/"))
            .unwrap_or(reference);
        let target = root.join(file_name);
        if target.exists() {
            let existing = fs::read(&target).map_err(io_error)?;
            if existing == bytes {
                return Ok(false);
            }
        }
        let temp = root.join(format!(".{hash}.tmp", hash = file_name));
        let _ = fs::remove_file(&temp);
        if let Err(error) = fs::write(&temp, bytes) {
            let _ = fs::remove_file(&temp);
            return Err(io_error(error));
        }
        if let Err(error) = fs::rename(&temp, &target) {
            let _ = fs::remove_file(&temp);
            return Err(io_error(error));
        }
        Ok(true)
    }

    /// Resolve a relative reference into a canonical absolute path
    /// inside the rich-text namespace. Mirrors the image
    /// asset-store validator.
    pub fn resolve(&self, reference: &str) -> Result<PathBuf, RichTextAssetError> {
        if reference.is_empty() {
            return Err(RichTextAssetError::Io {
                reason: "empty reference".into(),
            });
        }
        let candidate = Path::new(reference);
        if candidate.is_absolute() {
            return Err(RichTextAssetError::Io {
                reason: "absolute reference".into(),
            });
        }
        if candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(RichTextAssetError::Io {
                reason: "traversal reference".into(),
            });
        }
        let prefix = format!("{RICH_TEXT_ASSETS_DIR}/");
        if !reference.starts_with(&prefix) {
            return Err(RichTextAssetError::Io {
                reason: "out_of_scope reference".into(),
            });
        }
        let allowed_root = self.root();
        let canonical_root =
            fs::canonicalize(&allowed_root).map_err(|_| RichTextAssetError::Io {
                reason: "namespace missing".into(),
            })?;
        let relative =
            candidate
                .strip_prefix(RICH_TEXT_ASSETS_DIR)
                .map_err(|_| RichTextAssetError::Io {
                    reason: "out_of_scope reference".into(),
                })?;
        if relative.components().count() != 1 {
            return Err(RichTextAssetError::Io {
                reason: "out_of_scope reference".into(),
            });
        }
        let full_path = allowed_root.join(relative);
        let metadata = fs::symlink_metadata(&full_path).map_err(|_| RichTextAssetError::Io {
            reason: "missing file".into(),
        })?;
        if metadata.file_type().is_symlink() {
            return Err(RichTextAssetError::Io {
                reason: "symlink reference".into(),
            });
        }
        if !metadata.is_file() {
            return Err(RichTextAssetError::Io {
                reason: "missing file".into(),
            });
        }
        let canonical = fs::canonicalize(&full_path).map_err(|_| RichTextAssetError::Io {
            reason: "missing file".into(),
        })?;
        if !canonical.starts_with(&canonical_root) {
            return Err(RichTextAssetError::Io {
                reason: "escaped reference".into(),
            });
        }
        Ok(canonical)
    }

    /// Read the bytes behind a relative reference after validation.
    /// The function applies a size cap that matches the asset type
    /// (HTML, RTF or preview).
    pub fn read_bytes(&self, reference: &str) -> Result<Vec<u8>, RichTextAssetError> {
        let path = self.resolve(reference)?;
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if metadata.file_type().is_symlink() {
            return Err(RichTextAssetError::Io {
                reason: "symlink reference".into(),
            });
        }
        if !metadata.is_file() {
            return Err(RichTextAssetError::Io {
                reason: "missing file".into(),
            });
        }
        let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        let cap = cap_for_reference(reference);
        if size > cap {
            return Err(RichTextAssetError::TooLarge { size, cap });
        }
        let bytes = fs::read(&path).map_err(io_error)?;
        if bytes.len() > cap {
            return Err(RichTextAssetError::TooLarge {
                size: bytes.len(),
                cap,
            });
        }
        Ok(bytes)
    }

    /// Reclaim every asset in the rich-text namespace whose name is
    /// not referenced by `referenced`. Returns the number of files
    /// removed.
    pub fn collect_unreferenced(
        &self,
        referenced: &BTreeSet<String>,
    ) -> Result<usize, RichTextAssetError> {
        let root = self.root();
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(io_error(error)),
        };
        let mut removed = 0usize;
        for entry in entries {
            let entry = entry.map_err(io_error)?;
            let file_type = entry.file_type().map_err(io_error)?;
            if !file_type.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.starts_with('.') && name.ends_with(".tmp") {
                if fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                }
                continue;
            }
            let candidate_ref = format!("{RICH_TEXT_ASSETS_DIR}/{name}");
            if referenced.contains(&candidate_ref) {
                continue;
            }
            if fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn cap_for_reference(reference: &str) -> usize {
    if reference.ends_with(RICH_TEXT_PREVIEW_EXTENSION) {
        MAX_RICH_TEXT_PREVIEW_BYTES
    } else if reference.ends_with(RICH_TEXT_HTML_EXTENSION) {
        MAX_RICH_TEXT_HTML_BYTES
    } else if reference.ends_with(RICH_TEXT_RTF_EXTENSION) {
        MAX_RICH_TEXT_RTF_BYTES
    } else {
        MAX_RICH_TEXT_HTML_BYTES
    }
}

fn rich_text_ref_for(hash: &str, extension: &str) -> String {
    format!("{RICH_TEXT_ASSETS_DIR}/{hash}.{extension}")
}

fn rich_asset_path(root: &Path, reference: &str) -> PathBuf {
    let prefix = format!("{RICH_TEXT_ASSETS_DIR}/");
    root.join(reference.strip_prefix(&prefix).unwrap_or(reference))
}

fn cleanup_written_assets(paths: &[PathBuf]) {
    for path in paths.iter().rev() {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod asset_tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn fresh_store() -> (tempfile::TempDir, RichTextAssetStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = RichTextAssetStore::new(dir.path());
        (dir, store)
    }

    fn unique_hash(label: &str) -> String {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let raw = format!("{label}-{n}");
        sha256_hex(raw.as_bytes())
    }

    fn html_payload() -> RichTextPayload {
        RichTextPayload::new(
            "hello world".into(),
            Some("<p>hello <b>world</b></p>".into()),
            None,
        )
        .expect("valid")
    }

    fn rtf_only_payload() -> RichTextPayload {
        RichTextPayload::new("hello rtf".into(), None, Some(b"{\\rtf1 hello}".to_vec()))
            .expect("valid")
    }

    #[test]
    fn writes_escaped_plain_preview_for_rtf_only_payload() {
        let (_dir, store) = fresh_store();
        let payload = RichTextPayload::new(
            "a <b> & \"quoted\"\r\nnext".into(),
            None,
            Some(b"{\\rtf1 original}".to_vec()),
        )
        .expect("valid");
        let hash = unique_hash("rtf-escaped");
        let outcome = store.store(&hash, &payload).expect("store");
        let preview_ref = outcome.preview_ref().expect("preview ref");
        let preview =
            String::from_utf8(store.read_bytes(preview_ref).expect("preview")).expect("utf8");
        assert_eq!(preview, "a &lt;b&gt; &amp; &quot;quoted&quot;<br>next");
        let rtf_ref = outcome.rtf_ref().expect("rtf ref");
        assert_eq!(
            store.read_bytes(rtf_ref).expect("rtf"),
            b"{\\rtf1 original}"
        );
    }

    #[test]
    fn uses_plain_fallback_when_sanitized_html_has_no_visible_text() {
        let (_dir, store) = fresh_store();
        let payload = RichTextPayload::new(
            "plain <text>\nnext".into(),
            Some("<a href=\"javascript:alert(3)\"></a><p onclick=\"alert(1)\"></p><script>alert(2)</script>".into()),
            None,
        )
        .expect("valid");
        let hash = unique_hash("empty-preview");
        let outcome = store.store(&hash, &payload).expect("store");
        let preview_ref = outcome.preview_ref().expect("preview ref");
        let preview =
            String::from_utf8(store.read_bytes(preview_ref).expect("preview")).expect("utf8");
        assert_eq!(preview, "plain &lt;text&gt;<br>next");
        let html_ref = outcome.html_ref().expect("html ref");
        assert_eq!(
            store.read_bytes(html_ref).expect("html"),
            b"<a href=\"javascript:alert(3)\"></a><p onclick=\"alert(1)\"></p><script>alert(2)</script>"
        );
    }

    #[test]
    fn uses_plain_fallback_when_sanitizer_rejects_html() {
        let (_dir, store) = fresh_store();
        let html = "<b>".repeat(33);
        let payload =
            RichTextPayload::new("safe fallback".into(), Some(html.clone()), None).expect("valid");
        let hash = unique_hash("sanitize-error");
        let outcome = store.store(&hash, &payload).expect("store");
        let preview_ref = outcome.preview_ref().expect("preview ref");
        assert_eq!(
            store.read_bytes(preview_ref).expect("preview"),
            b"safe fallback"
        );
        let html_ref = outcome.html_ref().expect("html ref");
        assert_eq!(store.read_bytes(html_ref).expect("html"), html.as_bytes());
    }

    #[test]
    fn rolls_back_assets_when_a_later_asset_write_fails() {
        let (_dir, store) = fresh_store();
        let hash = unique_hash("rollback");
        fs::create_dir_all(store.root()).expect("mkdir");
        fs::create_dir_all(
            store
                .root()
                .join(format!("{hash}.{RICH_TEXT_PREVIEW_EXTENSION}")),
        )
        .expect("block preview");
        let payload = RichTextPayload::new(
            "plain".into(),
            Some("<p>original</p>".into()),
            Some(b"{\\rtf1 original}".to_vec()),
        )
        .expect("valid");

        let error = store.store(&hash, &payload).expect_err("write must fail");
        assert_eq!(error.kind_str(), "io");
        assert!(!store
            .root()
            .join(format!("{hash}.{RICH_TEXT_HTML_EXTENSION}"))
            .exists());
        assert!(!store
            .root()
            .join(format!("{hash}.{RICH_TEXT_RTF_EXTENSION}"))
            .exists());
        let temporary_count = fs::read_dir(store.root())
            .expect("read root")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(temporary_count, 0);
    }

    #[test]
    fn writes_html_and_preview_when_html_is_present() {
        let (_dir, store) = fresh_store();
        let payload = html_payload();
        let hash = unique_hash("html");
        let outcome = store.store(&hash, &payload).expect("store");
        assert!(outcome.html_ref().is_some());
        assert!(outcome.rtf_ref().is_none());
        assert!(outcome.preview_ref().is_some());
    }

    #[test]
    fn writes_rtf_and_plain_preview_for_rtf_only_payload() {
        let (_dir, store) = fresh_store();
        let payload = rtf_only_payload();
        let hash = unique_hash("rtf");
        let outcome = store.store(&hash, &payload).expect("store");
        assert!(outcome.html_ref().is_none());
        assert!(outcome.rtf_ref().is_some());
        assert!(outcome.preview_ref().is_some());
    }

    #[test]
    fn store_reuses_existing_assets() {
        let (_dir, store) = fresh_store();
        let payload = html_payload();
        let hash = unique_hash("reuse");
        let first = store.store(&hash, &payload).expect("first");
        let second = store.store(&hash, &payload).expect("second");
        assert!(matches!(first, RichTextAssetOutcome::Written { .. }));
        assert!(matches!(second, RichTextAssetOutcome::Reused { .. }));
        assert_eq!(first.html_ref(), second.html_ref());
    }

    #[test]
    fn collect_unreferenced_removes_only_orphan_files() {
        let (_dir, store) = fresh_store();
        let payload = html_payload();
        let kept_hash = unique_hash("kept");
        let dropped_hash = unique_hash("dropped");
        store.store(&kept_hash, &payload).expect("kept");
        store.store(&dropped_hash, &payload).expect("dropped");
        let mut referenced = BTreeSet::new();
        if let Some(reference) = store
            .store(&kept_hash, &payload)
            .ok()
            .and_then(|o| o.html_ref().map(str::to_owned))
        {
            referenced.insert(reference);
        }
        let removed = store.collect_unreferenced(&referenced).expect("collect");
        assert!(removed >= 1);
        // Kept asset must still resolve.
        let kept_ref = format!("{RICH_TEXT_ASSETS_DIR}/{kept_hash}.{RICH_TEXT_HTML_EXTENSION}");
        assert!(store.read_bytes(&kept_ref).is_ok());
    }

    #[test]
    fn rejects_oversized_html() {
        let (_dir, store) = fresh_store();
        let oversized = "x".repeat(MAX_RICH_TEXT_HTML_BYTES + 1);
        let payload = RichTextPayload::new("hello".into(), Some(oversized), None).expect("ok");
        let error = store
            .store(&unique_hash("big"), &payload)
            .expect_err("fail");
        assert_eq!(error.kind_str(), "too_large");
    }

    #[test]
    fn resolve_rejects_absolute_paths() {
        let (_dir, store) = fresh_store();
        let error = store.resolve("/etc/passwd").expect_err("absolute");
        assert!(matches!(error, RichTextAssetError::Io { .. }));
    }

    #[test]
    fn resolve_rejects_traversal() {
        let (_dir, store) = fresh_store();
        let error = store
            .resolve(&format!("{RICH_TEXT_ASSETS_DIR}/../etc/passwd"))
            .expect_err("traversal");
        assert!(matches!(error, RichTextAssetError::Io { .. }));
    }
}
