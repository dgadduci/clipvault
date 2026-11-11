# Proposal: Accept uppercase image extensions in the import picker

## Why

The native image import picker lists supported file extensions only in
lowercase. On systems whose picker matches extension patterns case-sensitively,
files such as `PHOTO.PNG` are hidden and cannot be selected even though the
image decoder supports their contents.

## What Changes

- Add uppercase variants for every currently supported image extension to the
  native file picker filter.
- Keep the supported image formats and decoding behavior unchanged.
- Add a focused regression test that pins lowercase and uppercase variants for
  every supported extension.

## Non-Goals

- Add image formats or change image decoding, normalization, deduplication, or
  import ordering.
- Change file-picker labels, import progress, or image metadata handling.
