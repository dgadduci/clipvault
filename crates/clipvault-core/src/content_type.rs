//! Deterministic, conservative content-type detector for textual
//! clipboard captures.
//!
//! [`detect_content_type`] is a pure function: it takes a `&str` and
//! returns a [`ContentType`] from the textual taxonomy. The function
//! does not depend on Tauri, SQLite, the clipboard adapter, the
//! filesystem, the network or any logger; it can be unit-tested
//! without any I/O.
//!
//! # Detection precedence
//!
//! When a payload could match several categories the detector picks
//! the most specific one in this fixed order:
//!
//! 1. [`ContentType::Jwt`] — three base64url segments starting with
//!    `eyJ`.
//! 2. [`ContentType::Uuid`] — standard / braced / parenthesized /
//!    `urn:uuid:` / no-hyphen forms.
//! 3. [`ContentType::Ipv6`] — strict parse via
//!    [`std::net::Ipv6Addr::from_str`].
//! 4. [`ContentType::Ipv4`] — strict parse via
//!    [`std::net::Ipv4Addr::from_str`].
//! 5. [`ContentType::Email`] — `local@domain.tld`, no whitespace, one
//!    `@`, at least one `.` in the domain.
//! 6. [`ContentType::Url`] — allowed scheme (`http`, `https`, `ftp`,
//!    `sftp`, `ssh`, `file`, `mailto`, `tel`) followed by `://` and a
//!    non-empty host.
//! 7. [`ContentType::HexColor`] — `#RGB`, `#RGBA`, `#RRGGBB` or
//!    `#RRGGBBAA` with valid hex digits.
//! 8. [`ContentType::Json`] — [`serde_json::from_str`] succeeds and the
//!    result is a non-empty `Object` or `Array`.
//! 9. [`ContentType::Html`] — at least one recognizable paired HTML
//!    tag (`<html>…</html>`, `<div>…</div>`, `<p>…</p>`,
//!    `<a …>…</a>`, `<script …>…</script>`, `<style …>…</style>`).
//! 10. [`ContentType::Sql`] — starts with a DML/DDL keyword
//!     (`SELECT`, `INSERT`, `UPDATE`, `DELETE`, `CREATE`, `DROP`,
//!     `ALTER`, `WITH`) and the same or following line contains a
//!     structural keyword.
//! 11. [`ContentType::ShellCommand`] — shebang (`#!…`) or strong shell
//!     operators (`|`, `&&`, `||`, `>`, `<`, `2>&1`) combined with a
//!     shell keyword.
//! 12. [`ContentType::Code`] — fenced block (` ``` ` or `~~~`) with a
//!     recognized language tag, or a shebang with a recognized
//!     language.
//! 13. [`ContentType::FilePath`] — macOS / Linux path pattern that is
//!     NOT a URL with a scheme and contains no whitespace.
//! 14. [`ContentType::Text`] — fallback for everything else, including
//!     empty inputs, whitespace-only inputs and prose that doesn't
//!     satisfy any of the rules above.
//!
//! The function is intentionally conservative: only strong signals
//! trigger a structured category; ambiguous prose always falls back to
//! [`ContentType::Text`].

use clipvault_db::ContentType;

const JWT_HEADER_PREFIX: &[u8] = b"eyJ";

/// Classify `input` into one of the textual [`ContentType`] variants.
///
/// Empty inputs and whitespace-only inputs are classified as
/// [`ContentType::Text`]; the function never panics and never returns
/// an error.
pub fn detect_content_type(input: &str) -> ContentType {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ContentType::Text;
    }

    // 1) Single-line detectors (URL, Email, IP, JWT, UUID, hex color,
    //    file path). These categories only make sense for a single
    //    line of input and would yield false positives on multi-line
    //    prose.
    if let Some(kind) = detect_single_line(trimmed) {
        return kind;
    }

    // 2) Structural detectors (JSON, HTML, SQL, shell, code). These
    //    work on both single-line and multi-line payloads; running
    //    them unconditionally avoids missing a single-line JSON
    //    document that the single-line pass did not claim.
    if let Some(kind) = detect_structural(trimmed) {
        return kind;
    }

    ContentType::Text
}

fn detect_single_line(trimmed: &str) -> Option<ContentType> {
    if is_jwt(trimmed) {
        return Some(ContentType::Jwt);
    }
    if is_uuid(trimmed) {
        return Some(ContentType::Uuid);
    }
    if is_ipv6(trimmed) {
        return Some(ContentType::Ipv6);
    }
    if is_ipv4(trimmed) {
        return Some(ContentType::Ipv4);
    }
    if is_email(trimmed) {
        return Some(ContentType::Email);
    }
    if is_url(trimmed) {
        return Some(ContentType::Url);
    }
    if is_hex_color(trimmed) {
        return Some(ContentType::HexColor);
    }
    if is_file_path(trimmed) {
        return Some(ContentType::FilePath);
    }
    None
}

fn detect_structural(trimmed: &str) -> Option<ContentType> {
    // Order matches the documented precedence. Code is checked
    // before HTML because a fenced HTML block is a stronger signal
    // than the presence of raw HTML tags; a fenced block always wins.
    if is_json(trimmed) {
        return Some(ContentType::Json);
    }
    if is_code(trimmed) {
        return Some(ContentType::Code);
    }
    if is_html(trimmed) {
        return Some(ContentType::Html);
    }
    if is_sql(trimmed) {
        return Some(ContentType::Sql);
    }
    if is_shell_command(trimmed) {
        return Some(ContentType::ShellCommand);
    }
    None
}

// ---------------------------------------------------------------------------
// JWT
// ---------------------------------------------------------------------------

fn is_jwt(input: &str) -> bool {
    let bytes = input.as_bytes();
    if bytes.len() < 4 || &bytes[0..3] != JWT_HEADER_PREFIX {
        return false;
    }
    let mut idx = 3;
    let mut dots = 0;
    let mut last_segment_len = 0usize;
    while idx < bytes.len() {
        let b = bytes[idx];
        let is_base64url = b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
        if is_base64url {
            idx += 1;
            last_segment_len += 1;
            continue;
        }
        if b == b'.' {
            // A segment must have at least one base64url character.
            if idx == 3 || bytes[idx - 1] == b'.' {
                return false;
            }
            dots += 1;
            last_segment_len = 0;
            idx += 1;
            continue;
        }
        // Any other byte terminates the scan.
        break;
    }
    if dots != 2 || last_segment_len == 0 {
        return false;
    }
    // The loop must have ended because idx reached the end of the
    // input (no trailing garbage).
    idx == bytes.len()
}

// ---------------------------------------------------------------------------
// UUID
// ---------------------------------------------------------------------------

const UUID_HYPHEN_POSITIONS: [usize; 4] = [8, 13, 18, 23];

fn is_uuid(input: &str) -> bool {
    if let Some(inner) = input.strip_prefix("urn:uuid:") {
        return is_uuid_hyphenated(inner);
    }
    if let Some(inner) = input.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        return is_uuid_hyphenated(inner);
    }
    if let Some(inner) = input.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        return is_uuid_hyphenated(inner);
    }
    if input.len() == 32 {
        return is_uuid_no_hyphens(input);
    }
    is_uuid_hyphenated(input)
}

fn is_uuid_hyphenated(input: &str) -> bool {
    is_uuid_hyphenated_with_offsets(input, &UUID_HYPHEN_POSITIONS)
}

fn is_uuid_hyphenated_with_offsets(input: &str, offsets: &[usize; 4]) -> bool {
    let bytes = input.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    let positions = *offsets;
    for (i, b) in bytes.iter().enumerate() {
        if positions.contains(&i) {
            if *b != b'-' {
                return false;
            }
        } else if !is_hex_digit(*b) {
            return false;
        }
    }
    // The version digit is the first hex digit of the third group,
    // i.e. the byte right after the second hyphen.
    let version_offset = positions[1] + 1;
    match bytes[version_offset] {
        b'1'..=b'8' => {}
        _ => return false,
    }
    true
}

fn is_uuid_no_hyphens(input: &str) -> bool {
    let bytes = input.as_bytes();
    for b in bytes {
        if !is_hex_digit(*b) {
            return false;
        }
    }
    // Version digit (offset 12 in the no-hyphen form).
    match bytes[12] {
        b'1'..=b'8' => {}
        _ => return false,
    }
    true
}

#[inline]
fn is_hex_digit(b: u8) -> bool {
    b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || (b'A'..=b'F').contains(&b)
}

// ---------------------------------------------------------------------------
// IPv4 / IPv6
// ---------------------------------------------------------------------------

fn is_ipv4(input: &str) -> bool {
    // `from_str` rejects values containing whitespace or surrounding
    // garbage, which matches our conservative policy.
    input.parse::<std::net::Ipv4Addr>().is_ok()
}

fn is_ipv6(input: &str) -> bool {
    input.parse::<std::net::Ipv6Addr>().is_ok()
}

// ---------------------------------------------------------------------------
// Email
// ---------------------------------------------------------------------------

fn is_email(input: &str) -> bool {
    if input.is_empty() || input.len() > 254 {
        return false;
    }
    let bytes = input.as_bytes();
    if bytes.iter().any(|b| b.is_ascii_whitespace()) {
        return false;
    }
    let mut at_count = 0;
    let mut at_idx = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'@' {
            at_count += 1;
            at_idx = i;
        }
    }
    if at_count != 1 || at_idx == 0 || at_idx == bytes.len() - 1 {
        return false;
    }
    let local = &input[..at_idx];
    let domain = &input[at_idx + 1..];
    if !is_email_local(local) || !is_email_domain(domain) {
        return false;
    }
    true
}

fn is_email_local(local: &str) -> bool {
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    let mut first = true;
    for b in local.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' | b'+' => {}
            _ => return false,
        }
        if first && b == b'.' {
            return false;
        }
        first = false;
    }
    if local.ends_with('.') {
        return false;
    }
    true
}

fn is_email_domain(domain: &str) -> bool {
    if domain.is_empty() || domain.len() > 253 {
        return false;
    }
    let mut last_dot_idx = None;
    for (i, b) in domain.bytes().enumerate() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' => {}
            b'.' => {
                if i == 0 || i == domain.len() - 1 {
                    return false;
                }
                if domain.as_bytes()[i - 1] == b'.' {
                    return false;
                }
                last_dot_idx = Some(i);
            }
            _ => return false,
        }
    }
    let Some(idx) = last_dot_idx else {
        return false;
    };
    let tld = &domain[idx + 1..];
    if tld.len() < 2 {
        return false;
    }
    for b in tld.bytes() {
        if !b.is_ascii_alphabetic() {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// URL
// ---------------------------------------------------------------------------

const URL_SCHEMES: &[&str] = &[
    "http://", "https://", "ftp://", "sftp://", "ssh://", "file://", "mailto:", "tel:",
];

fn is_url(input: &str) -> bool {
    let bytes = input.as_bytes();
    if bytes.iter().any(|b| {
        let c = *b;
        c.is_ascii_whitespace() || c == b'\0'
    }) {
        return false;
    }
    for scheme in URL_SCHEMES {
        if let Some(remainder) = input.strip_prefix(scheme) {
            // The remainder must contain at least one non-whitespace
            // byte and must not include forbidden control characters.
            if remainder.is_empty() {
                return false;
            }
            if !remainder.bytes().all(is_url_body_byte) {
                return false;
            }
            // For schemes that include `://` the host part must be
            // non-empty. For `mailto:` / `tel:` the address part
            // serves as the equivalent.
            if matches!(*scheme, "mailto:" | "tel:") {
                return true;
            }
            // Skip an optional extra slash so `file:///path` is
            // accepted (the host is implicit).
            let remainder = remainder.trim_start_matches('/');
            if remainder.is_empty() {
                return false;
            }
            let host_end = remainder
                .bytes()
                .position(|b| b == b'/' || b == b'?' || b == b'#')
                .unwrap_or(remainder.len());
            let host = &remainder[..host_end];
            return !host.is_empty() && host.bytes().all(is_url_host_byte);
        }
    }
    false
}

fn is_url_body_byte(b: u8) -> bool {
    !b.is_ascii_whitespace() && b != b'\0'
}

fn is_url_host_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b':' | b'[' | b']' | b'@' | b'%')
}

// ---------------------------------------------------------------------------
// Hex color
// ---------------------------------------------------------------------------

fn is_hex_color(input: &str) -> bool {
    let bytes = input.as_bytes();
    if bytes.len() != 4 && bytes.len() != 5 && bytes.len() != 7 && bytes.len() != 9 {
        return false;
    }
    if bytes[0] != b'#' {
        return false;
    }
    for b in &bytes[1..] {
        if !is_hex_digit(*b) {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

fn is_json(input: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(input) {
        Ok(value) => match value {
            serde_json::Value::Object(map) => !map.is_empty(),
            serde_json::Value::Array(items) => !items.is_empty(),
            _ => false,
        },
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// HTML
// ---------------------------------------------------------------------------

const HTML_PAIRED_TAGS: &[&str] = &[
    "html",
    "div",
    "p",
    "span",
    "section",
    "article",
    "main",
    "header",
    "footer",
    "nav",
    "ul",
    "ol",
    "li",
    "table",
    "tr",
    "td",
    "th",
    "form",
    "label",
    "fieldset",
    "pre",
    "blockquote",
];

fn is_html(input: &str) -> bool {
    // The detector requires at least one recognizable paired tag. A
    // single `<` or `>` does not qualify.
    let lower = input.to_ascii_lowercase();
    for tag in HTML_PAIRED_TAGS {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        if lower.contains(&open) && lower.contains(&close) {
            return true;
        }
    }
    // Allow void tags that look like `<a href="…">` paired with `</a>`.
    if lower.contains("<a ") && lower.contains("</a>") {
        return true;
    }
    if lower.contains("<script") && lower.contains("</script>") {
        return true;
    }
    if lower.contains("<style") && lower.contains("</style>") {
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// SQL
// ---------------------------------------------------------------------------

const SQL_DML_KEYWORDS: &[&str] = &[
    "SELECT", "INSERT", "UPDATE", "DELETE", "CREATE", "DROP", "ALTER", "WITH",
];

const SQL_STRUCTURAL_KEYWORDS: &[&str] = &[
    "FROM", "INTO", "TABLE", "VALUES", "SET", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER",
    "GROUP", "ORDER", "HAVING", "LIMIT", "OFFSET", "AS", "ON",
];

fn is_sql(input: &str) -> bool {
    let upper = input.to_ascii_uppercase();
    let lines: Vec<&str> = upper.lines().collect();
    let Some(first_line) = lines.first() else {
        return false;
    };
    let first_token = first_line.split_whitespace().next();
    let Some(first_token) = first_token else {
        return false;
    };
    if !SQL_DML_KEYWORDS.contains(&first_token) {
        return false;
    }
    let body = upper.lines().take(3).collect::<Vec<_>>().join(" ");
    SQL_STRUCTURAL_KEYWORDS
        .iter()
        .any(|kw| body.split_whitespace().any(|token| token == *kw))
}

// ---------------------------------------------------------------------------
// Shell command
// ---------------------------------------------------------------------------

const SHELL_KEYWORDS: &[&str] = &[
    "ls", "cd", "grep", "cat", "echo", "mkdir", "rm", "cp", "mv", "chmod", "chown", "awk", "sed",
    "curl", "wget", "git", "npm", "cargo", "psql", "ssh", "docker", "kubectl", "make", "yarn",
    "pnpm", "brew", "apt", "sudo", "export", "source", "tar", "gzip", "ssh-add", "gh", "kubectl",
    "jq",
];

const SHELL_OPERATORS: &[&str] = &["&&", "||", "|", ">>", ">", "<<", "<", "2>&1", "2>"];

fn is_shell_command(input: &str) -> bool {
    let trimmed = input.trim_start();
    if let Some(body) = trimmed.strip_prefix("#!") {
        // Any shebang qualifies; the keyword check is unnecessary.
        let _ = body;
        return !body.is_empty();
    }
    let lower = input.to_ascii_lowercase();
    let has_operator = SHELL_OPERATORS.iter().any(|op| lower.contains(op));
    if !has_operator {
        return false;
    }
    let first_token = lower
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next());
    let Some(first_token) = first_token else {
        return false;
    };
    SHELL_KEYWORDS.contains(&first_token)
        || lower
            .split_whitespace()
            .any(|token| SHELL_KEYWORDS.contains(&token))
}

// ---------------------------------------------------------------------------
// Code
// ---------------------------------------------------------------------------

const CODE_LANGUAGES: &[&str] = &[
    "python",
    "py",
    "rust",
    "rs",
    "javascript",
    "js",
    "typescript",
    "ts",
    "tsx",
    "jsx",
    "go",
    "golang",
    "java",
    "c",
    "cpp",
    "c++",
    "ruby",
    "rb",
    "php",
    "sh",
    "shell",
    "bash",
    "zsh",
    "sql",
    "html",
    "css",
    "json",
    "yaml",
    "yml",
    "toml",
    "markdown",
    "md",
    "kt",
    "kotlin",
    "swift",
    "scala",
    "dart",
    "lua",
    "pl",
    "perl",
    "hs",
    "haskell",
    "ex",
    "exs",
    "elixir",
    "elm",
    "vue",
    "svelte",
    "dockerfile",
    "makefile",
    "groovy",
    "r",
    "matlab",
    "node",
    "deno",
    "nim",
];

fn is_code(input: &str) -> bool {
    let trimmed_start = input.trim_start();
    if let Some(body) = trimmed_start.strip_prefix("#!") {
        // Shebang must point at a recognized interpreter. The first
        // whitespace-separated token is the executable (which may
        // include a path like `/usr/bin/env`); we extract the last
        // path segment, then if it is `env` we read the next token
        // as the actual interpreter. Trailing version digits are
        // stripped so `python3` and `python2` both resolve to
        // `python`.
        let mut tokens = body.split_whitespace();
        let first = tokens.next().unwrap_or("");
        let interpreter_path = first.rsplit('/').next().unwrap_or(first);
        let raw_interpreter = if interpreter_path == "env" {
            tokens.next().unwrap_or("")
        } else {
            interpreter_path
        };
        let interpreter = strip_trailing_digits(raw_interpreter.trim());
        if interpreter.is_empty() {
            return false;
        }
        return CODE_LANGUAGES.contains(&interpreter);
    }
    if is_fenced_block(input) {
        return true;
    }
    false
}

fn strip_trailing_digits(token: &str) -> &str {
    token.trim_end_matches(|c: char| c.is_ascii_digit())
}

fn is_fenced_block(input: &str) -> bool {
    let bytes = input.as_bytes();
    if bytes.len() < 6 {
        return false;
    }
    let first = bytes[0];
    if first != b'`' && first != b'~' {
        return false;
    }
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx] == first {
        idx += 1;
    }
    if idx < 3 {
        return false;
    }
    let fence = &bytes[..idx];
    // The opening fence must be followed by an optional language tag
    // (alphanumeric / `-` / `_` / `+`) and a newline / EOF.
    let after_fence = &bytes[idx..];
    let after_fence_str = match std::str::from_utf8(after_fence) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let mut lang_chars = 0usize;
    for b in after_fence_str.bytes() {
        if b == b'\n' || b == b'\r' {
            break;
        }
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'+' | b'.') {
            lang_chars += 1;
            if lang_chars > 32 {
                break;
            }
            continue;
        }
        // Non-language character before the newline: allow whitespace
        // only.
        if b.is_ascii_whitespace() {
            continue;
        }
        return false;
    }
    let language = after_fence_str
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '+');
    if language.is_empty() {
        return false;
    }
    if !CODE_LANGUAGES.contains(&language) {
        return false;
    }
    // The closing fence must appear later in the string.
    let rest = &input[idx + lang_chars..];
    let _ = fence;
    contains_closing_fence(rest, first)
}

fn contains_closing_fence(rest: &str, ch: u8) -> bool {
    let mut run = 0usize;
    for b in rest.bytes() {
        if b == ch {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// File path
// ---------------------------------------------------------------------------

const FILE_PATH_PREFIXES: &[&str] = &[
    "/Users/",
    "/home/",
    "/etc/",
    "/usr/",
    "/var/",
    "/tmp/",
    "/Volumes/",
    "/private/",
    "/opt/",
    "/bin/",
    "/sbin/",
    "~/",
    "./",
    "../",
];

const FILE_PATH_RELATIVE_TOKENS: &[&str] = &["~", "."];

fn is_file_path(input: &str) -> bool {
    if input.is_empty() {
        return false;
    }
    let bytes = input.as_bytes();
    if bytes.iter().any(|b| {
        let c = *b;
        c.is_ascii_whitespace() || c == b'\0'
    }) {
        return false;
    }
    for prefix in FILE_PATH_PREFIXES {
        if input.starts_with(prefix) {
            return true;
        }
    }
    // Root path like `/foo/bar.txt`.
    if input.starts_with('/') && input.len() > 1 && !input.contains(':') {
        return true;
    }
    // Relative tokens like `~`, `.` alone are too ambiguous; require
    // at least one separator.
    FILE_PATH_RELATIVE_TOKENS.contains(&input) && false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_type(input: &str, expected: ContentType) {
        assert_eq!(detect_content_type(input), expected, "input = {input:?}");
    }

    // ---------- URL ----------

    #[test]
    fn url_cases() {
        let positives = [
            "https://example.com",
            "http://foo.bar/path?x=1",
            "ftp://files.example.org",
            "sftp://host.example",
            "ssh://user@host.example",
            "file:///etc/hosts",
            "mailto:user@example.com",
            "tel:+15551234567",
        ];
        for input in positives {
            assert_type(input, ContentType::Url);
        }

        let negatives = [
            "example.com",
            "hello world",
            "https://",
            "ftp://",
            "tel:",
            "look at https://example.com for more", // embedded in prose
            "my favorite is https://example.com.",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- Email ----------

    #[test]
    fn email_cases() {
        let positives = [
            "user@example.com",
            "first.last@sub.example.co.uk",
            "alias+tag@example.io",
            "x_y-z@a.example",
        ];
        for input in positives {
            assert_type(input, ContentType::Email);
        }

        let negatives = [
            "foo@",
            "@bar.com",
            "foo bar@example.com",
            "foo@@bar.com",
            "user@example",
            "user@example.c",
            "contact me at user@example.com", // embedded
            "no-at-sign",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- JSON ----------

    #[test]
    fn json_cases() {
        let positives = [
            r#"{"k":"v"}"#,
            r#"{"a":1,"b":[1,2,3]}"#,
            "[1,2,3]",
            r#"{"nested":{"ok":true}}"#,
        ];
        for input in positives {
            assert_type(input, ContentType::Json);
        }

        let negatives = [
            "",
            "{}",
            "[]",
            "null",
            "true",
            "42",
            "\"string\"",
            "{not valid",
            "[unterminated",
            r#"{"key":}"#,
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- JWT ----------

    #[test]
    fn jwt_cases() {
        let positives = [
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyIn0.4f5f",
            "eyJabc.def.ghi",
            "eyJaaa.bbbb.cccc",
        ];
        for input in positives {
            assert_type(input, ContentType::Jwt);
        }

        let negatives = [
            "eyJ",
            "eyJabc",          // missing segments
            "eyJabc.def",      // only two segments
            "eyJ!@#.def.ghi",  // non base64url
            "eyJabc.def.ghi!", // trailing garbage
            "",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- UUID ----------

    #[test]
    fn uuid_cases() {
        let positives = [
            "550e8400-e29b-41d4-a716-446655440000",
            "{550e8400-e29b-41d4-a716-446655440000}",
            "(550e8400-e29b-41d4-a716-446655440000)",
            "urn:uuid:550e8400-e29b-41d4-a716-446655440000",
            "550E8400E29B41D4A716446655440000", // no-hyphen, uppercase
            "550e8400e29b41d4a716446655440000", // no-hyphen, lowercase
        ];
        for input in positives {
            assert_type(input, ContentType::Uuid);
        }

        let negatives = [
            "550e8400-e29b-41d4-a716-44665544000",   // too short
            "550e8400-e29b-41d4-a716-4466554400000", // too long
            "550e8400e29b41d4a71644665544000",       // too short no hyphen
            "550e8400-e29b-41d4-a716-44665544000Z",  // invalid last char
            "00000000-0000-0000-0000-000000000000",  // version 0
            "hello world",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- IPv4 ----------

    #[test]
    fn ipv4_cases() {
        let positives = ["127.0.0.1", "0.0.0.0", "255.255.255.255", "192.168.1.1"];
        for input in positives {
            assert_type(input, ContentType::Ipv4);
        }

        let negatives = [
            "1.2.3",
            "256.0.0.1",
            "1.2.3.4.5",
            "1.2.3.a",
            "01.02.03.04",
            "",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- IPv6 ----------

    #[test]
    fn ipv6_cases() {
        let positives = [
            "::1",
            "2001:db8::1",
            "2001:0db8:85a3:0000:0000:8a2e:0370:7334",
            "fe80::1",
            "::",
        ];
        for input in positives {
            assert_type(input, ContentType::Ipv6);
        }

        let negatives = ["2001:db8:", "2001:db8:::1", "not::an::addr::ss", ""];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- Hex color ----------

    #[test]
    fn hex_color_cases() {
        let positives = [
            "#fff",
            "#ffff",
            "#ffffff",
            "#ffffffff",
            "#ABC",
            "#ABCDEF",
            "#ABCDEF12",
        ];
        for input in positives {
            assert_type(input, ContentType::HexColor);
        }

        let negatives = [
            "#ff", "#fffff", "#fffffff", "#1234567", "#gggggg", "fff", "#", "",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- HTML ----------

    #[test]
    fn html_cases() {
        let positives = [
            "<html><body>hi</body></html>",
            "<div>hello</div>",
            "<p>x</p>",
            "<a href=\"https://example.com\">link</a>",
            "<script>alert(1)</script>",
            "<style>body { color: red; }</style>",
            "<section><h1>title</h1></section>",
        ];
        for input in positives {
            assert_type(input, ContentType::Html);
        }

        let negatives = [
            "use < and > carefully",
            "1 < 2 and 3 > 2",
            "<3",
            "iOS 14 < Android",
            "a < b",
            "",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- File path ----------

    #[test]
    fn file_path_cases() {
        let positives = [
            "/Users/foo/bar.txt",
            "/home/user/documents/report.pdf",
            "/etc/hosts",
            "/usr/local/bin",
            "/var/log/system.log",
            "/tmp/notes",
            "/Volumes/External/backups",
            "/private/etc/hosts",
            "~/Documents",
            "./relative/path",
            "../sibling/file",
            "/opt/app/config.toml",
            "/single-segment", // matches root + non-empty + no colon
        ];
        for input in positives {
            assert_type(input, ContentType::FilePath);
        }

        let negatives = [
            "just-a-word",
            "hello world",
            "with spaces in path",
            "look at /etc/hosts for details", // embedded in prose → text
            "",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- SQL ----------

    #[test]
    fn sql_cases() {
        let positives = [
            "SELECT * FROM users",
            "select id, name from accounts",
            "INSERT INTO logs (msg) VALUES ('x')",
            "UPDATE users SET active = 1 WHERE id = 1",
            "DELETE FROM sessions",
            "CREATE TABLE notes (id INTEGER PRIMARY KEY)",
            "DROP TABLE old",
            "ALTER TABLE foo ADD COLUMN bar INTEGER",
            "WITH cte AS (SELECT 1) SELECT * FROM cte",
        ];
        for input in positives {
            assert_type(input, ContentType::Sql);
        }

        let negatives = [
            "SELECT",
            "select",
            "FROM",
            "I love to select things",
            "select your favorite item",
            "this update is great",
            "",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- Shell command ----------

    #[test]
    fn shell_command_cases() {
        let positives = [
            "ls -la | grep foo",
            "cd /tmp && rm -rf foo",
            "cat file.txt > /tmp/x",
            "curl https://example.com | jq .",
            "npm run build && echo done",
            "docker ps | grep api",
            "kubectl get pods 2>&1",
        ];
        for input in positives {
            assert_type(input, ContentType::ShellCommand);
        }

        let negatives = [
            // Shebangs are detected as Code (the more specific
            // signal), so they don't reach the shell detector and
            // are NOT classified as Text here.
            "ls",
            "grep is a word I use a lot",
            "I love to use the && symbol in prose",
            "single && operator",
            "no pipe here",
            "git status",
            "npm run build",
            "cargo build",
            "kubectl get pods",
            "docker ps",
            "make test",
            "yarn install",
            "brew install foo",
            "sudo apt-get update",
            "",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- Code ----------

    #[test]
    fn code_cases() {
        let positives = [
            "```python\nprint('hi')\n```",
            "```rust\nfn main() {}\n```",
            "```js\nconsole.log(1)\n```",
            "```ts\nlet x = 1;\n```",
            "```bash\necho hi\n```",
            "```sql\nSELECT 1;\n```",
            "```html\n<p>x</p>\n```",
            "~~~go\npackage main\n~~~",
            "#!/usr/bin/env python3\nprint('hi')",
            "#!/usr/bin/env node\nconsole.log(1)",
            "#!/bin/bash\necho hi",
            "#!/usr/bin/env bash\necho hi",
        ];
        for input in positives {
            assert_type(input, ContentType::Code);
        }

        let negatives = [
            "```\nunfenced\n```",
            "```made-up-language\nx\n```",
            "print('hello')", // no fence / no shebang
            "fn main() {}",
            "echo hi",
            "",
        ];
        for input in negatives {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- Text fallback ----------

    #[test]
    fn text_fallback_cases() {
        let inputs = [
            "",
            " ",
            "\n\n",
            "\t",
            "hello world",
            "lorem ipsum dolor sit amet",
            "🦀 rustlang is great",
            "café résumé",
            "中文 测试",
            "مرحبا بالعالم",
            "🚀🎉",
            "single word",
            "one two three",
        ];
        for input in inputs {
            assert_type(input, ContentType::Text);
        }
    }

    // ---------- Precedence ----------

    #[test]
    fn jwt_wins_over_url_when_both_match() {
        // A JWT can look like a URL with three segments separated by dots.
        assert_eq!(
            detect_content_type("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signaturehere"),
            ContentType::Jwt
        );
    }

    #[test]
    fn uuid_wins_over_text_when_both_match() {
        assert_eq!(
            detect_content_type("550e8400-e29b-41d4-a716-446655440000"),
            ContentType::Uuid
        );
    }

    #[test]
    fn ipv4_wins_over_text_when_both_match() {
        assert_eq!(detect_content_type("192.168.1.1"), ContentType::Ipv4);
    }

    #[test]
    fn email_wins_over_text_when_both_match() {
        assert_eq!(detect_content_type("user@example.com"), ContentType::Email);
    }

    #[test]
    fn url_wins_over_file_path() {
        assert_eq!(
            detect_content_type("https://example.com/path"),
            ContentType::Url
        );
    }

    #[test]
    fn json_wins_over_html_for_unambiguous_payload() {
        let payload = "{\"k\":\"v\"}";
        assert_eq!(detect_content_type(payload), ContentType::Json);
    }

    // ---------- Determinism ----------

    #[test]
    fn repeated_calls_return_identical_results() {
        let cases = [
            "https://example.com",
            "user@example.com",
            r#"{"k":"v"}"#,
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signaturehere",
            "550e8400-e29b-41d4-a716-446655440000",
            "127.0.0.1",
            "::1",
            "#fff",
            "<div>x</div>",
            "/Users/foo/bar.txt",
            "SELECT 1 FROM dual",
            "ls -la | grep foo",
            "```python\nprint(1)\n```",
            "hello world",
            "",
        ];
        for input in cases {
            let first = detect_content_type(input);
            for _ in 0..5 {
                assert_eq!(detect_content_type(input), first, "input = {input:?}");
            }
        }
    }

    // ---------- Unicode ----------

    #[test]
    fn unicode_prose_falls_back_to_text() {
        let inputs = [
            "café résumé",
            "中文 测试",
            "🚀🎉✨",
            "Ω≈ç√∫˜µ",
            "Привет мир",
        ];
        for input in inputs {
            assert_type(input, ContentType::Text);
        }
    }
}
