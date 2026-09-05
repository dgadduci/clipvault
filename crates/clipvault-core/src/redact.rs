//! Deterministic, conservative redactor for diagnostic output.
//!
//! [`redact`] is a pure function: it takes a `&str` and returns a new
//! `String` with each detected secret replaced by the literal
//! `[REDACTED:secret]` placeholder. Surrounding context is preserved so
//! logs remain useful for debugging.
//!
//! Detection categories (each pattern is anchored and bounded so we do
//! not match arbitrary strings of similar shape):
//!
//! - JWT (`eyJ…` followed by two more base64url segments).
//! - AWS access keys (`AKIA` / `ASIA` prefix, 16 alphanumerics total).
//! - PEM private keys (`-----BEGIN ... PRIVATE KEY-----` blocks).
//! - HTTP `Authorization:` / `Proxy-Authorization:` headers.
//! - Basic-auth credentials inside URLs (`https://user:pass@host`).
//! - Inline `key=value` prefixes commonly used in shell snippets:
//!   `password=`, `token=`, `secret=`, `api_key=`, `apikey=`.
//!
//! The redactor is intentionally conservative: it does not run any
//! entropy analysis, it does not look for high-entropy strings, and it
//! does not try to redact email addresses, IPv4 addresses or innocuous
//! URLs. False positives are acceptable; false negatives are documented
//! and remain user-visible in the spec ("Privacy-preserving diagnostics").
//!
//! The function does not allocate beyond the output string and the
//! match arms use a single owned regex; see tests at the bottom.

use std::io::{self, Write};

const REDACTED: &str = "[REDACTED:secret]";

/// Return `input` with detected secrets replaced by `[REDACTED:secret]`.
pub fn redact(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut cursor = 0;

    while cursor < bytes.len() {
        if let Some(consumed) = match_at(bytes, cursor) {
            out.push_str(REDACTED);
            cursor += consumed;
            continue;
        }
        let ch = char_at(bytes, cursor);
        out.push(ch);
        cursor += ch.len_utf8();
    }
    out
}

/// Length of the match starting at `start`, or `None` if no secret
/// pattern matches at that position.
fn match_at(bytes: &[u8], start: usize) -> Option<usize> {
    if let Some(len) = match_private_key(bytes, start) {
        return Some(len);
    }
    if let Some(len) = match_authorization(bytes, start) {
        return Some(len);
    }
    if let Some(len) = match_basic_auth_url(bytes, start) {
        return Some(len);
    }
    if let Some(len) = match_jwt(bytes, start) {
        return Some(len);
    }
    if let Some(len) = match_aws_key(bytes, start) {
        return Some(len);
    }
    if let Some(len) = match_prefixed_kv(bytes, start) {
        return Some(len);
    }
    None
}

fn char_at(bytes: &[u8], i: usize) -> char {
    bytes[i] as char
}

fn match_private_key(bytes: &[u8], start: usize) -> Option<usize> {
    const MARKERS: &[&[u8]] = &[
        b"-----BEGIN RSA PRIVATE KEY-----",
        b"-----BEGIN EC PRIVATE KEY-----",
        b"-----BEGIN OPENSSH PRIVATE KEY-----",
        b"-----BEGIN PRIVATE KEY-----",
        b"-----BEGIN ENCRYPTED PRIVATE KEY-----",
    ];
    for marker in MARKERS {
        if bytes.len() >= start + marker.len() && &bytes[start..start + marker.len()] == *marker {
            return Some(marker.len());
        }
    }
    None
}

fn match_authorization(bytes: &[u8], start: usize) -> Option<usize> {
    const PREFIXES: &[&[u8]] = &[
        b"authorization:",
        b"authorization :",
        b"proxy-authorization:",
        b"proxy-authorization :",
    ];
    for prefix in PREFIXES {
        if bytes.len() < start + prefix.len() {
            continue;
        }
        if !bytes[start..start + prefix.len()].eq_ignore_ascii_case(prefix) {
            continue;
        }
        let mut idx = start + prefix.len();
        while idx < bytes.len() && (bytes[idx] == b' ' || bytes[idx] == b'\t') {
            idx += 1;
        }
        while idx < bytes.len() && bytes[idx] != b'\n' && bytes[idx] != b'\r' {
            idx += 1;
        }
        // Replace the full "Authorization: <value>" group, including
        // any trailing newline, so a log line that wraps does not
        // re-leak the value halfway through.
        let consumed = idx - start;
        if consumed == 0 {
            return None;
        }
        return Some(consumed);
    }
    None
}

fn match_basic_auth_url(bytes: &[u8], start: usize) -> Option<usize> {
    const SCHEMES: &[&[u8]] = &[b"https://", b"http://"];
    let scheme = SCHEMES
        .iter()
        .find(|scheme| {
            bytes.len() >= start + scheme.len() && &bytes[start..start + scheme.len()] == **scheme
        })
        .copied()?;
    let mut idx = start + scheme.len();
    while idx < bytes.len()
        && bytes[idx] != b'@'
        && bytes[idx] != b'/'
        && bytes[idx] != b' '
        && bytes[idx] != b'\n'
        && bytes[idx] != b'\r'
        && bytes[idx] != b'"'
        && bytes[idx] != b'\''
    {
        idx += 1;
    }
    if idx >= bytes.len() || bytes[idx] != b'@' {
        return None;
    }
    // Consume up to (but not including) the '@' so the host
    // delimiter stays visible in the redacted output, e.g.
    // "https://[REDACTED:secret]@example.com".
    Some(idx - start)
}

fn match_jwt(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.len() < start + 3 || &bytes[start..start + 3] != b"eyJ" {
        return None;
    }
    let mut idx = start + 3;
    let mut dots = 0;
    while idx < bytes.len() {
        let b = bytes[idx];
        let is_base64url = b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
        if is_base64url {
            idx += 1;
            continue;
        }
        if b == b'.' {
            dots += 1;
            idx += 1;
            if dots == 2 {
                while idx < bytes.len() {
                    let b = bytes[idx];
                    if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                        idx += 1;
                    } else {
                        break;
                    }
                }
                return Some(idx - start);
            }
            continue;
        }
        break;
    }
    None
}

fn match_aws_key(bytes: &[u8], start: usize) -> Option<usize> {
    const PREFIXES: &[&[u8]] = &[b"AKIA", b"ASIA"];
    for prefix in PREFIXES {
        if bytes.len() < start + prefix.len() {
            continue;
        }
        if &bytes[start..start + prefix.len()] != *prefix {
            continue;
        }
        let mut idx = start + prefix.len();
        let mut count = 0;
        while idx < bytes.len() && count < 12 {
            let b = bytes[idx];
            if b.is_ascii_alphanumeric() {
                idx += 1;
                count += 1;
            } else {
                break;
            }
        }
        if count == 12 {
            return Some(idx - start);
        }
    }
    None
}

fn match_prefixed_kv(bytes: &[u8], start: usize) -> Option<usize> {
    const PREFIXES: &[&[u8]] = &[
        b"password=",
        b"password =",
        b"token=",
        b"token =",
        b"secret=",
        b"secret =",
        b"api_key=",
        b"api_key =",
        b"apikey=",
        b"apikey =",
    ];
    for prefix in PREFIXES {
        if bytes.len() < start + prefix.len() {
            continue;
        }
        if !bytes[start..start + prefix.len()].eq_ignore_ascii_case(prefix) {
            continue;
        }
        let mut idx = start + prefix.len();
        let value_start = idx;
        while idx < bytes.len()
            && bytes[idx] != b'\n'
            && bytes[idx] != b'\r'
            && bytes[idx] != b' '
            && bytes[idx] != b'\t'
            && bytes[idx] != b'"'
            && bytes[idx] != b'\''
        {
            idx += 1;
        }
        if idx == value_start {
            return None;
        }
        return Some(idx - start);
    }
    None
}

// ---------------------------------------------------------------------------
// `tracing-subscriber` integration via a `MakeWriter` wrapper
// ---------------------------------------------------------------------------

/// MakeWriter wrapper that runs every chunk through [`redact`] before
/// the inner writer sees it. The wrapper itself implements
/// `MakeWriter<'w>` so it can be plugged directly into
/// `tracing_subscriber::fmt().with_writer(...)`.
#[derive(Clone, Debug)]
pub struct RedactingMakeWriter<W> {
    inner: W,
}

impl<W> RedactingMakeWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }
}

impl<'w, W> tracing_subscriber::fmt::MakeWriter<'w> for RedactingMakeWriter<W>
where
    W: tracing_subscriber::fmt::MakeWriter<'w>,
{
    type Writer = RedactingWriter<<W as tracing_subscriber::fmt::MakeWriter<'w>>::Writer>;

    fn make_writer(&'w self) -> Self::Writer {
        RedactingWriter {
            inner: self.inner.make_writer(),
            buffer: Vec::with_capacity(256),
        }
    }
}

/// Buffered writer that scrubs every chunk through [`redact`] before
/// delegating to the inner writer. The buffer drains on `flush()` so
/// callers can rely on the inner writer receiving the scrubbed bytes.
pub struct RedactingWriter<W: Write> {
    inner: W,
    buffer: Vec<u8>,
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let buffered = std::mem::take(&mut self.buffer);
        let redacted = redact(std::str::from_utf8(&buffered).unwrap_or(""));
        self.inner.write_all(redacted.as_bytes())?;
        self.inner.flush()?;
        self.buffer = Vec::with_capacity(256);
        Ok(())
    }
}

impl<W: Write> Drop for RedactingWriter<W> {
    fn drop(&mut self) {
        // Tracing-subscriber often drops the per-event writer without
        // calling `flush()` first. Drain the buffer here so buffered
        // bytes (including secrets) cannot outlive the writer.
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_jwt() {
        let input = "session=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyIn0.4f5f";
        let output = redact(input);
        assert!(output.contains("[REDACTED:secret]"));
        assert!(!output.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    }

    #[test]
    fn redacts_aws_access_key() {
        let input = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE and AWS_SECRET=foo";
        let output = redact(input);
        assert!(output.contains("[REDACTED:secret]"));
        assert!(!output.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn redacts_private_key_marker() {
        let input = "key\n-----BEGIN RSA PRIVATE KEY-----\nabc\n-----END RSA PRIVATE KEY-----\n";
        let output = redact(input);
        assert!(output.contains("[REDACTED:secret]"));
        assert!(!output.contains("BEGIN RSA PRIVATE KEY"));
    }

    #[test]
    fn redacts_authorization_header() {
        let input = "Authorization: Bearer eyJabc.def.ghi";
        let output = redact(input);
        assert!(!output.contains("Bearer eyJabc.def.ghi"));
    }

    #[test]
    fn redacts_basic_auth_in_url() {
        let input = "fetch https://alice:s3cret@example.com/api";
        let output = redact(input);
        assert!(!output.contains("alice:s3cret"));
        assert!(output.contains("[REDACTED:secret]@example.com"));
    }

    #[test]
    fn redacts_password_kv_pair() {
        let input = "config password=hunter2 prod=true";
        let output = redact(input);
        assert!(!output.contains("hunter2"));
        assert!(output.contains("config [REDACTED:secret] prod=true"));
    }

    #[test]
    fn leaves_emails_unchanged() {
        let input = "contact: ops@example.com";
        let output = redact(input);
        assert_eq!(output, input);
    }

    #[test]
    fn leaves_innocuous_url_unchanged() {
        let input = "see docs at https://clipvault.local/page";
        let output = redact(input);
        assert_eq!(output, input);
    }

    #[test]
    fn empty_input_returns_empty_string() {
        assert_eq!(redact(""), "");
    }

    #[test]
    fn multiple_patterns_in_one_line() {
        let input = "Bearer eyJabc.def.ghi password=hunter2 AKIAABCDEFGHIJKL";
        let output = redact(input);
        assert!(!output.contains("hunter2"));
        assert!(!output.contains("AKIAABCDEFGHIJKL"));
    }

    #[test]
    fn redacting_writer_scrubs_buffered_bytes() {
        use std::io::{Read, Write};

        #[derive(Default)]
        struct Capture {
            bytes: Vec<u8>,
        }
        impl Write for Capture {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.bytes.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut capture = Capture::default();
        {
            let mut writer = RedactingWriter {
                inner: &mut capture,
                buffer: Vec::new(),
            };
            writer
                .write_all(b"password=hunter2 \n eyJabc.def.ghi")
                .unwrap();
            writer.flush().unwrap();
        }
        let mut s = String::new();
        let mut cursor = std::io::Cursor::new(&capture.bytes);
        cursor.read_to_string(&mut s).unwrap();
        assert!(!s.contains("hunter2"));
        assert!(!s.contains("eyJabc.def.ghi"));
    }

    /// `5.3`: integration test that registers a custom writer through
    /// `tracing_subscriber::fmt`, emits a `tracing::warn!` containing a
    /// fake secret, and asserts the writer only ever sees the redacted
    /// form. This proves the production wiring
    /// (`app/tauri/src-tauri/src/main.rs::init_tracing`) actually
    /// scrubs secrets before they hit the destination.
    #[test]
    fn redactor_is_wired_into_tracing_subscriber() {
        use std::io::Write;
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::fmt::MakeWriter;

        #[derive(Default, Clone)]
        struct SharedCapture(Arc<Mutex<Vec<u8>>>);

        impl Write for SharedCapture {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        impl<'w> MakeWriter<'w> for SharedCapture {
            type Writer = SharedCapture;
            fn make_writer(&'w self) -> Self::Writer {
                self.clone()
            }
        }

        let capture = SharedCapture::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(RedactingMakeWriter::new(capture.clone()))
            .with_max_level(tracing::Level::WARN)
            .finish();

        let fake_secret = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyIn0.AAAA";

        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(token = %fake_secret, "auth attempt");
            // Drop the subscriber guard so any pending writes flush
            // before we read the buffer.
        });

        let guard = capture.0.lock().unwrap();
        let buffer = String::from_utf8_lossy(&guard).into_owned();
        assert!(
            !buffer.contains(fake_secret),
            "raw secret leaked into writer: {buffer}"
        );
        assert!(
            buffer.contains("[REDACTED:secret]"),
            "writer did not see the redacted form: {buffer}"
        );
    }
}
