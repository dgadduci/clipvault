//! Local PDF generation for an ordered selection of clipboard entries.
//!
//! This module contains no Tauri or frontend types. It reads entry records and
//! validated image assets through the core's existing persistence boundary,
//! then returns one transient PDF byte buffer to the shell for explicit save.

use std::collections::{BTreeMap, HashSet};

use clipvault_db::{ContentType, EntryRecord, EntryRepository, EntryRepositoryError};
use printpdf::{
    Base64OrRaw, GeneratePdfOptions, PdfDocument, PdfParseErrorSeverity, PdfSaveOptions,
};
use thiserror::Error;

use crate::bootstrap::AppContext;
use crate::clipboard_assets::{AssetError, ClipboardAssetStore};

const PAGE_WIDTH_MM: f32 = 210.0;
const PAGE_HEIGHT_MM: f32 = 297.0;
const PAGE_MARGIN_MM: f32 = 12.0;
const PRINTABLE_WIDTH_MM: f32 = PAGE_WIDTH_MM - (PAGE_MARGIN_MM * 2.0);
const PRINTABLE_HEIGHT_MM: f32 = PAGE_HEIGHT_MM - (PAGE_MARGIN_MM * 2.0);
const CSS_PIXELS_PER_MM: f32 = 96.0 / 25.4;
const PDF_FONT_NAME: &str = "ClipVaultSans";
const PDF_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");

#[derive(Debug, Error)]
pub enum PdfExportError {
    #[error("PDF export selection is empty")]
    EmptySelection,
    #[error("a selected capture no longer exists")]
    MissingCapture,
    #[error("a selected image has no persisted asset")]
    MissingImageAsset,
    #[error("entry repository error: {0}")]
    Repository(#[from] EntryRepositoryError),
    #[error("image asset error: {0}")]
    Asset(#[from] AssetError),
    #[error("PDF rendering failed")]
    Render,
}

#[derive(Debug)]
enum PdfCapture {
    Text(String),
    Png {
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
}

/// Builds one local PDF from the selected entry IDs in the supplied order.
pub struct PdfExportService;

impl PdfExportService {
    pub fn generate(
        context: &AppContext,
        entry_ids: &[i64],
        page_break_between_captures: bool,
    ) -> Result<Vec<u8>, PdfExportError> {
        if entry_ids.is_empty() {
            return Err(PdfExportError::EmptySelection);
        }

        let records = {
            let mut db = context.database().lock();
            let repository = EntryRepository::new(db.connection_mut());
            let mut records = Vec::with_capacity(entry_ids.len());
            for entry_id in deduplicate_ids(entry_ids) {
                records.push(
                    repository
                        .find_by_id(entry_id)?
                        .ok_or(PdfExportError::MissingCapture)?,
                );
            }
            records
        };

        if records.is_empty() {
            return Err(PdfExportError::EmptySelection);
        }

        let assets = ClipboardAssetStore::new(context.platform().data_dir.clone());
        let captures = records
            .into_iter()
            .map(|record| load_capture(record, &assets))
            .collect::<Result<Vec<_>, _>>()?;

        let document = render_captures(&captures, page_break_between_captures)?;

        let mut warnings = Vec::new();
        let bytes = document.save(
            &PdfSaveOptions {
                subset_fonts: true,
                ..Default::default()
            },
            &mut warnings,
        );
        if warnings
            .iter()
            .any(|warning| warning.severity == PdfParseErrorSeverity::Error)
            || bytes.is_empty()
        {
            return Err(PdfExportError::Render);
        }
        Ok(bytes)
    }
}

fn deduplicate_ids(entry_ids: &[i64]) -> Vec<i64> {
    let mut seen = HashSet::with_capacity(entry_ids.len());
    entry_ids
        .iter()
        .copied()
        .filter(|entry_id| seen.insert(*entry_id))
        .collect()
}

fn render_captures(
    captures: &[PdfCapture],
    page_break_between_captures: bool,
) -> Result<PdfDocument, PdfExportError> {
    if page_break_between_captures {
        let mut combined = PdfDocument::new("ClipVault captures");
        for (selection_index, capture) in captures.iter().enumerate() {
            combined.append_document(render_document(
                std::slice::from_ref(capture),
                selection_index,
            )?);
        }
        if combined.page_count() == 0 {
            return Err(PdfExportError::Render);
        }
        Ok(combined)
    } else {
        render_document(captures, 0)
    }
}

fn load_capture(
    record: EntryRecord,
    assets: &ClipboardAssetStore,
) -> Result<PdfCapture, PdfExportError> {
    if record.content_type == ContentType::Image {
        if !record.is_renderable_image() {
            return Err(PdfExportError::MissingImageAsset);
        }
        let asset_ref = record
            .asset_ref
            .as_deref()
            .ok_or(PdfExportError::MissingImageAsset)?;
        return Ok(PdfCapture::Png {
            bytes: assets.read_bytes(asset_ref)?,
            width: record
                .payload_width
                .ok_or(PdfExportError::MissingImageAsset)?,
            height: record
                .payload_height
                .ok_or(PdfExportError::MissingImageAsset)?,
        });
    }

    // Rich-text records already retain their plain-text representation in
    // `content`. Exporting it does not rewrite the original capture or assets.
    Ok(PdfCapture::Text(record.content))
}

fn render_document(
    captures: &[PdfCapture],
    first_selection_index: usize,
) -> Result<PdfDocument, PdfExportError> {
    let mut images = BTreeMap::new();
    let mut body = String::new();

    for (index, capture) in captures.iter().enumerate() {
        match capture {
            PdfCapture::Text(text) => body.push_str(&render_text_block(text)),
            PdfCapture::Png {
                bytes,
                width,
                height,
            } => {
                let name = format!("capture-{}.png", first_selection_index + index);
                images.insert(name.clone(), Base64OrRaw::Raw(bytes.clone()));
                let (width_css_px, height_css_px) =
                    fit_image_to_printable_area(*width, *height).ok_or(PdfExportError::Render)?;
                body.push_str(&format!(
                    "<div style=\"width:{width_css_px:.3}px; height:{height_css_px:.3}px; margin:0;\"><img src=\"{name}\" style=\"display:block; width:{width_css_px:.3}px; height:{height_css_px:.3}px;\"></div>"
                ));
            }
        }
    }

    let html = format!(
        "<html><body style=\"margin:0; padding:0; font-family:'{PDF_FONT_NAME}'; font-size:11pt; line-height:1.35;\">{body}</body></html>"
    );
    let mut fonts = BTreeMap::new();
    fonts.insert(
        PDF_FONT_NAME.to_string(),
        Base64OrRaw::Raw(PDF_FONT_BYTES.to_vec()),
    );
    let options = GeneratePdfOptions {
        page_width: Some(PAGE_WIDTH_MM),
        page_height: Some(PAGE_HEIGHT_MM),
        margin_top: Some(PAGE_MARGIN_MM),
        margin_right: Some(PAGE_MARGIN_MM),
        margin_bottom: Some(PAGE_MARGIN_MM),
        margin_left: Some(PAGE_MARGIN_MM),
        ..Default::default()
    };
    let mut warnings = Vec::new();
    let document = PdfDocument::from_html(&html, &images, &fonts, &options, &mut warnings)
        .map_err(|_| PdfExportError::Render)?;
    if document.page_count() == 0
        || warnings
            .iter()
            .any(|warning| warning.severity == PdfParseErrorSeverity::Error)
    {
        return Err(PdfExportError::Render);
    }
    Ok(document)
}

/// Returns the largest CSS-pixel dimensions that fit inside the printable
/// A4 area while preserving the source image's aspect ratio.
fn fit_image_to_printable_area(width: u32, height: u32) -> Option<(f32, f32)> {
    if width == 0 || height == 0 {
        return None;
    }

    let max_width_css_px = PRINTABLE_WIDTH_MM * CSS_PIXELS_PER_MM;
    let max_height_css_px = PRINTABLE_HEIGHT_MM * CSS_PIXELS_PER_MM;
    let scale = (max_width_css_px / width as f32).min(max_height_css_px / height as f32);
    Some((width as f32 * scale, height as f32 * scale))
}

fn render_text_block(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = String::new();
    for line in normalized.split('\n') {
        let escaped = escape_html_text(line);
        let visible = if escaped.is_empty() {
            "&#160;"
        } else {
            escaped.as_str()
        };
        output.push_str("<div style=\"min-height:5mm;\">");
        output.push_str(visible);
        output.push_str("</div>");
    }
    output
}

fn escape_html_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\0' => escaped.push('\u{FFFD}'),
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> PdfCapture {
        PdfCapture::Text(value.to_string())
    }

    fn parsed(bytes: &[u8]) -> PdfDocument {
        let mut warnings = Vec::new();
        PdfDocument::parse(bytes, &Default::default(), &mut warnings).expect("valid PDF")
    }

    fn png(width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("PNG header");
            let row = color.repeat(width as usize);
            let pixels = row.repeat(height as usize);
            writer.write_image_data(&pixels).expect("PNG pixels");
        }
        bytes
    }

    fn image(bytes: Vec<u8>, width: u32, height: u32) -> PdfCapture {
        PdfCapture::Png {
            bytes,
            width,
            height,
        }
    }

    #[test]
    fn text_export_preserves_order_and_line_breaks_in_one_document() {
        let document = render_captures(
            &[text("first line\nsecond line"), text("later capture")],
            false,
        )
        .expect("rendered document");
        let mut warnings = Vec::new();
        let bytes = document.save(&PdfSaveOptions::default(), &mut warnings);
        let parsed = parsed(&bytes);
        let extracted = parsed
            .extract_text()
            .into_iter()
            .flatten()
            .collect::<String>();

        assert_eq!(parsed.page_count(), 1);
        let first = extracted.find("first line").expect("first capture text");
        let second = extracted.find("second line").expect("line after break");
        let later = extracted.find("later capture").expect("next capture text");
        assert!(first < second && second < later);
    }

    #[test]
    fn text_is_escaped_and_unicode_font_is_embedded() {
        let document = render_document(&[text("área <x> & Ω")], 0).expect("rendered document");
        let mut warnings = Vec::new();
        let bytes = document.save(&PdfSaveOptions::default(), &mut warnings);
        let parsed = parsed(&bytes);
        let extracted = parsed
            .extract_text()
            .into_iter()
            .flatten()
            .collect::<String>();

        assert!(extracted.contains("área <x> & Ω"));
    }

    #[test]
    fn page_break_preference_controls_capture_boundaries_in_one_pdf() {
        let captures = [text("first capture"), text("second capture")];
        let continuous = render_captures(&captures, false).expect("continuous PDF");
        let separated = render_captures(&captures, true).expect("page-break PDF");

        assert_eq!(continuous.page_count(), 1);
        assert_eq!(separated.page_count(), 2);
        let mut warnings = Vec::new();
        let bytes = separated.save(&PdfSaveOptions::default(), &mut warnings);
        let extracted = parsed(&bytes)
            .extract_text()
            .into_iter()
            .flatten()
            .collect::<String>();
        let first = extracted.find("first capture").expect("first capture text");
        let second = extracted
            .find("second capture")
            .expect("second capture text");
        assert!(first < second);
    }

    #[test]
    fn png_capture_is_embedded_as_an_image_xobject() {
        let png = include_bytes!("../../../app/tauri/src-tauri/icons/32x32.png");
        let document = render_captures(&[image(png.to_vec(), 32, 32)], false).expect("image PDF");
        assert_eq!(document.page_count(), 1);
        assert!(!document.resources.xobjects.map.is_empty());
    }

    #[test]
    fn image_fitting_preserves_aspect_ratio_and_uses_the_limiting_page_dimension() {
        let (landscape_width, landscape_height) =
            fit_image_to_printable_area(1600, 900).expect("landscape dimensions");
        let max_width = PRINTABLE_WIDTH_MM * CSS_PIXELS_PER_MM;
        assert!((landscape_width - max_width).abs() < 0.01);
        assert!((landscape_width / landscape_height - (1600.0 / 900.0)).abs() < 0.001);

        let (portrait_width, portrait_height) =
            fit_image_to_printable_area(900, 1600).expect("portrait dimensions");
        let max_height = PRINTABLE_HEIGHT_MM * CSS_PIXELS_PER_MM;
        assert!((portrait_height - max_height).abs() < 0.01);
        assert!(portrait_width <= max_width);
        assert!((portrait_width / portrait_height - (900.0 / 1600.0)).abs() < 0.001);

        let (upright_width, upright_height) =
            fit_image_to_printable_area(900, 1000).expect("upright dimensions");
        assert!(upright_width <= max_width);
        assert!(upright_height <= max_height);
        assert!((upright_width / upright_height - 0.9).abs() < 0.001);
        assert!(fit_image_to_printable_area(0, 1).is_none());
    }

    #[test]
    fn page_breaks_keep_each_selected_image_on_its_own_page() {
        let captures = [
            image(png(160, 90, [255, 0, 0]), 160, 90),
            image(png(60, 120, [0, 255, 0]), 60, 120),
        ];
        let document = render_captures(&captures, true).expect("image PDF");

        assert_eq!(document.page_count(), 2);
        let first_ids = document.pages[0].get_xobject_ids();
        let second_ids = document.pages[1].get_xobject_ids();
        assert_eq!(first_ids.len(), 1);
        assert_eq!(second_ids.len(), 1);
        assert_ne!(first_ids[0], second_ids[0]);

        let dimensions = |id: &printpdf::XObjectId| match document
            .resources
            .xobjects
            .map
            .get(id)
            .expect("image resource for page")
        {
            printpdf::XObject::Image(image) => (image.width, image.height),
            other => panic!("expected image resource, got {other:?}"),
        };
        assert_eq!(dimensions(&first_ids[0]), (160, 90));
        assert_eq!(dimensions(&second_ids[0]), (60, 120));

        for page in &document.pages {
            let transform = page
                .ops
                .iter()
                .find_map(|operation| match operation {
                    printpdf::Op::UseXobject { transform, .. } => Some(transform),
                    _ => None,
                })
                .expect("image placement operation");
            let scale_x = transform.scale_x.expect("horizontal image scale");
            let scale_y = transform.scale_y.expect("vertical image scale");
            assert!((scale_x - scale_y).abs() < 0.01);
        }
    }

    #[test]
    fn duplicate_capture_ids_keep_the_first_selection_position() {
        assert_eq!(deduplicate_ids(&[7, 3, 7, 9, 3]), vec![7, 3, 9]);
    }

    #[test]
    fn empty_or_missing_capture_selection_fails_before_rendering() {
        let (_dir, context) = crate::test_support::isolated_harness();

        assert!(matches!(
            PdfExportService::generate(&context, &[], false),
            Err(PdfExportError::EmptySelection)
        ));
        assert!(matches!(
            PdfExportService::generate(&context, &[i64::MAX], false),
            Err(PdfExportError::MissingCapture)
        ));
    }
}
