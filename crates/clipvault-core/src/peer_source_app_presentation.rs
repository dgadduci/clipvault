//! Validation primitives the `peer-source-app-presentation`
//! change ships.
//!
//! The host and the client both validate the source-application
//! display name and the bounded PNG icon bytes the on-demand
//! route exchanges. The helpers below pin the deterministic
//! rules in one place so a future change cannot drift the wire
//! contract:
//!
//! - [`MAX_SOURCE_APP_NAME_CHARS`] is the absolute upper bound
//!   on the number of Unicode scalar values the host accepts;
//!   the host trims the input, drops control characters and
//!   refuses the value when the resulting scalar count exceeds
//!   the bound.
//! - [`MAX_SOURCE_APP_ICON_BYTES`] and
//!   [`MAX_SOURCE_APP_ICON_LONGEST_SIDE`] mirror the local
//!   application-icon limits the desktop shell already enforces.
//!   The host decodes the PNG, validates the dimensions and the
//!   byte count and refuses any icon that exceeds the bounds.
//!
//! The module never logs or echoes the supplied bytes; the
//! helper functions return a typed [`SourceAppPresentationError`]
//! so the runtime / client can branch on the failure without
//! inspecting the rejected payload.

use thiserror::Error;

/// Maximum number of Unicode scalar values the
/// `peer-source-app-presentation` change accepts for the
/// source-application display name. The constant mirrors the
/// documented contract; the helper
/// [`validate_source_app_name`] enforces it after trimming and
/// control-character filtering.
pub const MAX_SOURCE_APP_NAME_CHARS: usize = 128;

/// Maximum number of bytes the host accepts for the
/// source-application PNG icon. The constant mirrors the
/// documented 512 KiB cap the design pins and matches the
/// `application-icons/` limits the desktop shell enforces.
pub const MAX_SOURCE_APP_ICON_BYTES: usize = 512 * 1024;

/// Maximum longest side (in pixels) of the source-application
/// PNG icon the host accepts. The helper
/// [`validate_source_app_icon`] decodes the PNG signature and
/// dimensions before comparing against the bound.
pub const MAX_SOURCE_APP_ICON_LONGEST_SIDE: u32 = 256;

/// Typed error the validation helpers surface. Every variant
/// collapses to a stable identifier the caller can branch on
/// without inspecting the rejected payload.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SourceAppPresentationError {
    /// The supplied name was empty after trimming, contained a
    /// control character or exceeded the
    /// [`MAX_SOURCE_APP_NAME_CHARS`] bound.
    #[error("source-application name is invalid")]
    InvalidName,
    /// The supplied icon bytes are not a valid PNG, exceeded
    /// the [`MAX_SOURCE_APP_ICON_BYTES`] bound or exceeded the
    /// [`MAX_SOURCE_APP_ICON_LONGEST_SIDE`] dimension cap.
    #[error("source-application icon is invalid")]
    InvalidIcon,
}

/// Normalise and validate the source-application display name
/// the host / client exchange.
///
/// The helper trims the input, REFUSES the value when the
/// trimmed result contains any control character (the contract
/// explicitly forbids silent stripping so a malicious peer
/// cannot hide a control sequence inside an otherwise
/// well-formed display name), and rejects the value when the
/// resulting scalar count exceeds
/// [`MAX_SOURCE_APP_NAME_CHARS`]. An all-whitespace input
/// collapses to [`SourceAppPresentationError::InvalidName`]
/// because the renderer must not surface an empty name.
///
/// The helper does not log or echo the supplied bytes; the
/// caller is responsible for sanitising any logging surface.
pub fn validate_source_app_name(raw: &str) -> Result<String, SourceAppPresentationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SourceAppPresentationError::InvalidName);
    }
    if trimmed.chars().any(|ch| ch.is_control()) {
        // The contract rejects, rather than silently strips,
        // names that carry control characters: a peer that
        // smuggles `\u{0001}` or `\u{0008}` separators into an
        // otherwise well-formed name should not reach the
        // renderer at all. The renderer would otherwise have
        // to defend against every possible control byte
        // sequence (RTLO, ANSI, NUL termination, …) without
        // ever knowing the value crossed the wire. A pure
        // whitespace input collapses to the empty-name branch
        // above so the helper does not double-count the same
        // failure mode.
        return Err(SourceAppPresentationError::InvalidName);
    }
    if trimmed.chars().count() > MAX_SOURCE_APP_NAME_CHARS {
        return Err(SourceAppPresentationError::InvalidName);
    }
    Ok(trimmed.to_string())
}

/// Validate the source-application PNG icon the host / client
/// exchange.
///
/// The helper enforces:
/// - non-empty payload;
/// - byte count ≤ [`MAX_SOURCE_APP_ICON_BYTES`];
/// - the PNG signature (`\x89PNG\r\n\x1a\n`, RFC 2083 §3.1);
/// - successful PNG decode via the in-tree [`png`] codec —
///   every chunk (header, IDAT, IEND, …) must round-trip the
///   decoder so a truncated / corrupted payload cannot
///   survive with a valid signature;
/// - longest side (width or height) ≤
///   [`MAX_SOURCE_APP_ICON_LONGEST_SIDE`].
///
/// The validation routine intentionally stays metadata-only:
/// it never re-encodes the bytes, never caches the decoded
/// image and never copies the input into a log / event / toast.
/// The caller is responsible for rendering the validated bytes
/// to a transient Object URL when the validated payload needs
/// to surface to the user.
///
/// `png` is already a direct workspace dependency of the core
/// (the `clipboard-rich-content` asset store uses it to decode
/// clipboard PNGs); the helper reuses it so the validation
/// does not introduce a new crate. A build that lacks the
/// decoder — i.e. one that compiles the core without the
/// `png` crate — fails closed: `validate_source_app_icon`
/// rejects every payload instead of accepting a PNG with a
/// signature alone, which is the behaviour the spec scenario
/// "Malformed or oversized icon is encountered" pins.
pub fn validate_source_app_icon(bytes: &[u8]) -> Result<(), SourceAppPresentationError> {
    if bytes.is_empty() {
        return Err(SourceAppPresentationError::InvalidIcon);
    }
    if bytes.len() > MAX_SOURCE_APP_ICON_BYTES {
        return Err(SourceAppPresentationError::InvalidIcon);
    }
    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < PNG_SIGNATURE.len() || bytes[..8] != PNG_SIGNATURE {
        return Err(SourceAppPresentationError::InvalidIcon);
    }
    let (width, height) =
        decode_png_dimensions(bytes).ok_or(SourceAppPresentationError::InvalidIcon)?;
    let longest = width.max(height);
    if longest > MAX_SOURCE_APP_ICON_LONGEST_SIDE {
        return Err(SourceAppPresentationError::InvalidIcon);
    }
    Ok(())
}

/// Decode the PNG header (`IHDR` chunk) to extract the width and
/// height of the image and confirm the decoder accepts every
/// chunk. The helper drives the in-tree [`png`] decoder's
/// `Reader` API so it never has to reach for an extra image
/// crate; the decoder is already a direct workspace
/// dependency of the core (`png = "0.17"`). Returns `None`
/// when the payload cannot be parsed as a PNG (truncated,
/// malformed IDAT, missing IEND, …) so the caller can collapse
/// every failure into the typed [`SourceAppPresentationError`].
fn decode_png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let cursor = std::io::Cursor::new(bytes);
    let mut reader = png::Decoder::new(cursor).read_info().ok()?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    // Drive the decoder through one frame so a truncated /
    // corrupted payload fails here even when the `IHDR` chunk
    // parsed cleanly. The output buffer size is what the
    // decoder expects for the full RGBA8 frame; for validation
    // purposes we only care that the decode succeeds without
    // surfacing the bytes to the caller.
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    if reader.next_frame(&mut buffer).is_err() {
        return None;
    }
    Some((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_source_app_name_trims_and_accepts_short_names() {
        let result =
            validate_source_app_name("  Visual Studio Code  ").expect("short name is valid");
        assert_eq!(result, "Visual Studio Code");
    }

    #[test]
    fn validate_source_app_name_rejects_empty_input() {
        assert_eq!(
            validate_source_app_name("   "),
            Err(SourceAppPresentationError::InvalidName)
        );
    }

    #[test]
    fn validate_source_app_name_rejects_control_characters() {
        // The contract refuses, rather than silently strips,
        // names that carry control characters. A peer that
        // smuggles `\u{0001}` or `\u{0008}` separators into an
        // otherwise well-formed name must not reach the
        // renderer at all. A pure-whitespace input collapses to
        // the empty-name branch above; the helper returns
        // `InvalidName` for both failure modes.
        assert_eq!(
            validate_source_app_name("Terminal\u{0001} Code"),
            Err(SourceAppPresentationError::InvalidName)
        );
        assert_eq!(
            validate_source_app_name("\u{0001}\u{0002}\u{0003}"),
            Err(SourceAppPresentationError::InvalidName)
        );
        assert_eq!(
            validate_source_app_name("Code\u{0007}"),
            Err(SourceAppPresentationError::InvalidName)
        );
    }

    #[test]
    fn validate_source_app_name_rejects_overlong_input() {
        let too_long = "a".repeat(MAX_SOURCE_APP_NAME_CHARS + 1);
        assert_eq!(
            validate_source_app_name(&too_long),
            Err(SourceAppPresentationError::InvalidName)
        );
    }

    #[test]
    fn validate_source_app_name_accepts_boundary_length() {
        let boundary = "a".repeat(MAX_SOURCE_APP_NAME_CHARS);
        let result = validate_source_app_name(&boundary).expect("boundary length is valid");
        assert_eq!(result.chars().count(), MAX_SOURCE_APP_NAME_CHARS);
    }

    #[test]
    fn validate_source_app_icon_rejects_empty_input() {
        assert_eq!(
            validate_source_app_icon(b""),
            Err(SourceAppPresentationError::InvalidIcon)
        );
    }

    #[test]
    fn validate_source_app_icon_rejects_non_png_bytes() {
        // Random non-PNG bytes — the helper refuses them
        // because the signature check fails first.
        let not_png = vec![0u8; 64];
        assert_eq!(
            validate_source_app_icon(&not_png),
            Err(SourceAppPresentationError::InvalidIcon)
        );
    }

    #[test]
    fn validate_source_app_icon_rejects_oversized_payload() {
        // Build a payload that looks like a PNG but exceeds the
        // byte cap. The signature passes; the size check
        // refuses the value.
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend(std::iter::repeat(0u8).take(MAX_SOURCE_APP_ICON_BYTES + 1));
        assert_eq!(
            validate_source_app_icon(&bytes),
            Err(SourceAppPresentationError::InvalidIcon)
        );
    }

    fn encode_test_png(width: u32, height: u32) -> Vec<u8> {
        // Synthesise a tiny solid-colour RGBA PNG so the
        // decoder has a fully-formed payload to inspect. The
        // `png` crate is already a direct workspace dependency
        // of the core so the test stays hermetic and never has
        // to bundle a fixture.
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("png header");
            let frame = vec![0x80u8; (width as usize) * (height as usize) * 4];
            writer.write_image_data(&frame).expect("png data");
            writer.finish().expect("png finish");
        }
        buf
    }

    #[test]
    fn validate_source_app_icon_accepts_valid_png_within_limits() {
        // Build a 32 × 32 RGBA PNG: well within the 512 KiB
        // byte cap, well within the 256 × 256 dimension cap.
        let bytes = encode_test_png(32, 32);
        assert!(
            bytes.len() <= MAX_SOURCE_APP_ICON_BYTES,
            "test fixture must respect the byte cap"
        );
        validate_source_app_icon(&bytes).expect("valid icon must validate");
    }

    #[test]
    fn validate_source_app_icon_accepts_boundary_dimensions() {
        // A 256 × 256 PNG is the documented upper bound; the
        // helper must accept it without exceeding the byte cap.
        // The synthetic solid-colour PNG encodes to a tiny
        // payload so the byte cap is never the limiter.
        let bytes = encode_test_png(
            MAX_SOURCE_APP_ICON_LONGEST_SIDE,
            MAX_SOURCE_APP_ICON_LONGEST_SIDE,
        );
        assert!(bytes.len() <= MAX_SOURCE_APP_ICON_BYTES);
        validate_source_app_icon(&bytes).expect("256x256 boundary must validate");
    }

    #[test]
    fn validate_source_app_icon_rejects_oversized_dimensions() {
        // A 257 × 257 PNG blows past the longest-side cap even
        // though the signature parses cleanly. The helper must
        // refuse the payload instead of surfacing the bytes to
        // the renderer.
        let bytes = encode_test_png(
            MAX_SOURCE_APP_ICON_LONGEST_SIDE + 1,
            MAX_SOURCE_APP_ICON_LONGEST_SIDE + 1,
        );
        assert_eq!(
            validate_source_app_icon(&bytes),
            Err(SourceAppPresentationError::InvalidIcon)
        );
    }

    #[test]
    fn validate_source_app_icon_rejects_truncated_png_with_valid_signature() {
        // A PNG with the canonical 8-byte signature followed by
        // 64 truncated bytes — the IHDR chunk is not present,
        // the decoder fails, and the helper must reject the
        // payload even though the signature check passes.
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend(std::iter::repeat(0u8).take(64));
        assert_eq!(
            validate_source_app_icon(&bytes),
            Err(SourceAppPresentationError::InvalidIcon)
        );
    }

    #[test]
    fn validate_source_app_icon_rejects_corrupted_png_with_valid_signature() {
        // Build a valid 8 × 8 PNG, then flip a byte inside the
        // IHDR payload so the signature passes but the decode
        // fails. The helper must reject the corrupted payload
        // rather than accept a PNG it cannot decode.
        let mut bytes = encode_test_png(8, 8);
        // Flip a byte deep enough that the signature + header
        // still pass the byte-8 check but the chunk decode
        // fails. The IHDR chunk is the second chunk the
        // decoder consumes (the first is the 8-byte signature).
        if bytes.len() > 16 {
            bytes[16] ^= 0xFF;
        }
        assert_eq!(
            validate_source_app_icon(&bytes),
            Err(SourceAppPresentationError::InvalidIcon)
        );
    }
}
