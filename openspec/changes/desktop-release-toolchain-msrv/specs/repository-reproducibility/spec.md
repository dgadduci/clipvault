## MODIFIED Requirements

### Requirement: Canonical Rust toolchain

The repository SHALL declare Rust `1.90.0` as its exact local-development and
CI toolchain, based on the minimum required by the locked desktop release
dependencies, and the workspace SHALL declare Rust `1.90` as its compatible
minimum version.

#### Scenario: macOS and Ubuntu use the same Rust baseline

- **WHEN** a developer enters the repository on macOS or Ubuntu and runs
  `rustc --version` through the repository toolchain
- **THEN** the selected compiler is Rust `1.90.0`
- **AND** `rustfmt` and `clippy` are available without a machine-local
  override
- **AND** the workspace `rust-version` is not lower than `1.90`

#### Scenario: Toolchain is reproducible in CI

- **WHEN** the desktop release workflow builds a macOS or Linux target
- **THEN** it uses the repository-declared Rust `1.90.0` toolchain
- **AND** the compiler satisfies the minimum Rust version of every locked
  release dependency
- **AND** it does not rewrite `Cargo.lock` to work around an obsolete
  compiler pin
