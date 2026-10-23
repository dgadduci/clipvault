# Design: Case variants in the local image picker

## Picker filter

The native Tauri dialog currently receives a literal list of lowercase file
extensions. Keep that list and add each ASCII uppercase variant alongside it:
`png/PNG`, `jpg/JPG`, `jpeg/JPEG`, `webp/WEBP`, `gif/GIF`, `bmp/BMP`,
`tif/TIF` and `tiff/TIFF`.

Use a named constant as the single source passed to `add_filter`. This only
changes which filenames the operating-system picker displays. The import
decoder, format allowlist, image normalization, and error handling remain
unchanged.

## Verification

Add a Rust unit test that verifies every supported lowercase extension and its
uppercase form are present in the exact list used by the picker. Run the
focused Rust test, the Tauri crate check, strict OpenSpec validation and
`git diff --check`.
