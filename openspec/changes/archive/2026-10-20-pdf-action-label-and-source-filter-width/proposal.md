# Proposal: Refine PDF action and source filter sizing

## Problem

The PDF export action currently uses a document icon on Desktop and QuickPaste.
The source-application filter also needs a further 10% width reduction from
its current 187 px width.

## Scope

- Replace the PDF document icon with the localized visible label `PDF` in red
  text on both selection surfaces.
- Reduce the source-application filter width and minimum width by 10%, from
  187 px / 9.35rem to 168.3 px / 8.415rem.
- Update frontend regression checks and all five locale catalogs.

The export action's position, enabled state, save flow, selection behavior and
semi-transparent red background remain as currently specified.
