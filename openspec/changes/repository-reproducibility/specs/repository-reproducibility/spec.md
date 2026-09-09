## ADDED Requirements

### Requirement: Canonical Rust toolchain

The repository SHALL declare one exact Rust toolchain for local development
and CI, based on the Rust 1.89.0 toolchain validated on Ubuntu, and the
workspace SHALL declare a compatible minimum Rust version.

#### Scenario: macOS and Ubuntu use the same Rust baseline

- **WHEN** a developer enters the repository on macOS or Ubuntu and runs
  `rustc --version` through the repository toolchain
- **THEN** the selected compiler is Rust 1.89.0
- **AND** `rustfmt` and `clippy` are available without a machine-local
  override
- **AND** the workspace `rust-version` does not claim compatibility with a
  version older than the declared dependency baseline

#### Scenario: Toolchain is reproducible in CI

- **WHEN** a CI job builds the Rust workspace
- **THEN** it uses the repository-declared toolchain rather than the runner's
  ambient stable compiler
- **AND** it does not rewrite dependencies merely because the runner differs
  from the developer machine

### Requirement: Canonical frontend toolchain and dependency graph

The frontend SHALL use Node.js 20 LTS with npm 10 as its declared development
baseline, and `app/tauri/frontend/package-lock.json` SHALL remain the single
tracked npm dependency lockfile.

#### Scenario: Clean frontend installation

- **WHEN** a developer installs frontend dependencies
- **THEN** the developer runs `npm ci` from `app/tauri/frontend`
- **AND** the command consumes the tracked `package-lock.json`
- **AND** no root, `app/` or `app/tauri/` npm/pnpm lockfile is generated

#### Scenario: Frontend verification uses one dependency graph

- **WHEN** macOS, Ubuntu and CI run frontend checks
- **THEN** they use the same tracked lockfile and declared Node/npm baseline
- **AND** `npm run check`, `npm run build` and `npm test` run from the
  frontend directory without an implicit package-manager migration

### Requirement: Canonical development commands

The repository SHALL document one working directory and one command sequence
for Rust, frontend and Tauri development.

#### Scenario: Rust verification from repository root

- **WHEN** a developer validates the Rust workspace
- **THEN** the commands are executed from the repository root
- **AND** the documented commands include `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace`

#### Scenario: Tauri development from the shell directory

- **WHEN** a developer starts the Tauri app in development mode
- **THEN** the canonical command is `cd app/tauri && cargo tauri dev`
- **AND** the workflow does not pass an unsupported `--manifest-path` option
  to `cargo tauri dev`
- **AND** the configured frontend `beforeDevCommand` resolves its package
  from the intended frontend directory

#### Scenario: One active development instance

- **WHEN** a developer recompiles or manually validates a new commit
- **THEN** the previous Tauri instance is closed before the new one starts
- **AND** stale bundles or processes are not presented as validation of the
  current commit

### Requirement: Cross-platform validation workflow

The repository SHALL provide non-interactive CI checks for macOS and Ubuntu
that cover compilation, tests, frontend validation and the Linux shell feature
combination without requiring a graphical clipboard session.

#### Scenario: Ubuntu Linux build checks

- **WHEN** the Ubuntu CI job runs
- **THEN** it installs only the documented Tauri system prerequisites
- **AND** it runs Rust checks and
  `cargo check -p clipvault-app --no-default-features --features clipboard-arboard,hotkey-global`
- **AND** it runs the frontend checks from `app/tauri/frontend`

#### Scenario: macOS build checks

- **WHEN** the macOS CI job runs
- **THEN** it uses the repository toolchain and the declared Node/npm baseline
- **AND** it runs Rust and frontend checks without opening a GUI
- **AND** it preserves the macOS-specific feature compilation path

#### Scenario: Manual platform checks remain explicit

- **WHEN** CI completes
- **THEN** it does not claim to have verified global hotkeys, active-app
  detection, tray behavior, synthetic paste, permissions, X11 or Wayland
- **AND** those checks remain documented as manual host validation

### Requirement: Safe Git and OpenSpec handoff

The repository SHALL document a branch-isolated handoff in which a change is
specified, implemented, committed and pushed before the same commit is
validated on the other operating system.

#### Scenario: macOS to Ubuntu handoff

- **WHEN** a feature is ready for cross-platform validation
- **THEN** the developer pushes the feature branch with its OpenSpec and
  implementation commit
- **AND** Ubuntu checks out the same branch and commit before testing
- **AND** fixes found on Ubuntu are committed to that branch rather than
  copied manually between worktrees

#### Scenario: Integration to main

- **WHEN** macOS and Ubuntu validation is complete
- **THEN** the branch is reviewed and merged into `main` through the existing
  repository workflow
- **AND** OpenSpec synchronization/archive is performed only after the
  change is validated and the implementation is committed
- **AND** unrelated active changes are not archived or modified as part of
  this handoff

### Requirement: Protect local runtime data

Builds, tests, CI, OpenSpec commands and Git operations SHALL NOT delete,
move, rename or regenerate the user's local ClipVault database or assets.

#### Scenario: Existing image history survives validation

- **WHEN** a developer builds, tests, validates OpenSpec or updates Git
- **THEN** `~/.clipvault/clipvault.db` and `~/.clipvault/assets/` are outside
  the operation's write/cleanup scope
- **AND** previously stored image files remain available for the application

#### Scenario: CI has no personal runtime state

- **WHEN** CI runs on a hosted runner
- **THEN** it uses isolated temporary test data or in-memory fakes
- **AND** it does not access a developer's home directory, clipboard content,
  secrets, absolute local paths or asset bytes

### Requirement: Generated-file hygiene

The repository SHALL keep generated build artifacts and accidental package
manager files out of commits while preserving required lockfiles.

#### Scenario: Git status after frontend installation

- **WHEN** a developer runs `npm ci` and the documented checks
- **THEN** `app/tauri/frontend/package-lock.json` remains the tracked npm
  lockfile
- **AND** `node_modules`, `dist`, `target`, `pnpm-lock.yaml` and
  `pnpm-workspace.yaml` are not added to the change

#### Scenario: Sensitive and local files remain ignored

- **WHEN** a developer inspects `git status` after local development
- **THEN** local databases, logs, environment overrides, IDE state and
  agent/tool state remain ignored
- **AND** no ignore rule hides source files, OpenSpec artifacts or the tracked
  npm/Cargo lockfiles
