# Design: PDF image layout and resource identity

## Image sizing

- Read the validated image's stored pixel width and height from its entry
  record.
- Compute a single scale factor that fits the original dimensions inside the
  A4 printable box (page dimensions minus the existing 12 mm margins on each
  side). Apply the same factor to width and height.
- Pass the resulting explicit CSS width and height to `printpdf`; do not use
  `width: 100%` or independently clamp either dimension. Landscape captures
  therefore use the available width, while portrait captures use the
  available height when that is the limiting dimension. Very wide portrait
  images remain inside the printable width.
- Keep the original image bytes and rendering pipeline unchanged.

## Resource identity across page breaks

- Give each capture document an image source key containing its index in the
  original ordered selection.
- `printpdf` derives its image XObject ID from that key. Distinct keys prevent
  `append_document` from merging different images under one resource ID.
- In continuous-flow mode, use the same global selection indices so the
  identity rule stays consistent.

## Verification

- Unit tests assert that landscape and portrait bounds fit the printable area
  and preserve the image aspect ratio.
- A multi-image page-break regression asserts each output page references a
  distinct image XObject with its own dimensions.
- Run the focused core PDF export tests, OpenSpec strict validation, and
  `git diff --check`. No frontend, collection, or drag-and-drop code changes
  are expected.
