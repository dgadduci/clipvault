## OpenSpec

- [x] 1.1 Specify uppercase and lowercase picker variants for every current
  supported image extension without expanding the decoder allowlist.

## Implementation

- [x] 2.1 Pass lowercase and uppercase variants of every supported extension to
  the native file picker through one named constant.
- [x] 2.2 Add a focused Rust regression test for the exact picker extension
  list.

## Verification

- [x] 3.1 Run the focused Rust regression, Tauri crate check, strict OpenSpec
  validation and `git diff --check`.

The focused picker test passed (1 test), `cargo check -p clipvault-app
--locked --offline` passed, `cargo fmt --all -- --check` passed, strict OpenSpec
validation passed, and `git diff --check` passed. The Rust test and check used
an isolated target directory under `/tmp` to avoid the shared build target.
