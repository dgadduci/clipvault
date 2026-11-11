# Proposal: Correct image layout in PDF exports

## Problem

Manual verification found two image-export regressions. Images are stretched
to the printable width even when their original proportions are different,
and page-separated image captures can all resolve to the first image selected.

## Scope

- Fit each image inside the printable page area while preserving its original
  aspect ratio.
- Give every selected image a unique PDF resource identity so page-separated
  captures retain their own image bytes and selection order.
- Add focused regression coverage for sizing and multi-page image resources.

The export still produces one local PDF and keeps the existing page-break
setting and selection behavior.

## Motivation

The current renderer assigns every image the full content width, which lets
the PDF image placement stretch portrait captures. When captures are rendered
as separate documents and appended, each document also names its image
`capture-0.png`; `printpdf` merges same-named XObjects by retaining the first
resource. Both issues are in the core renderer and can be corrected without
changing the export UI or saved settings.
