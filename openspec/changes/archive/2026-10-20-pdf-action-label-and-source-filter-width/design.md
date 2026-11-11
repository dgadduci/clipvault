# Design: PDF text label and source filter width

## PDF action

- Render the translated short label `PDF` inside the existing export button
  on Desktop and QuickPaste instead of the inline document SVG.
- Keep the detailed translated `aria-label` and tooltip so assistive
  technology and hover users retain the full action description.
- Style the visible label red and keep the existing red semi-transparent
  button surface, placement, selection gating and click handler.
- Add `pdf_export.short_label` to the `en`, `es`, `pt`, `de` and `fr` catalogs;
  the file-format acronym is `PDF` in each locale.

## Source-application filter

- Change the shared source-filter width token from 187 px to 168.3 px
  (187 × 0.9).
- Change its minimum from 9.35rem to 8.415rem (9.35 × 0.9), and keep the
  component's CSS declarations and test metadata aligned with those tokens.
- Leave compact tag, collection and content-type filter widths unchanged.

## Verification

- Update targeted frontend regressions for the PDF label/color and exact
  source-filter dimensions.
- Run frontend checks/build, the PDF and toolbar regression tests, and the
  protected drag-and-drop regression files because toolbar sizing changes.
- Validate this OpenSpec change strictly and review `git diff --check`.
