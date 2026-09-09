# Cross-platform development

This guide is the canonical reference for building, testing and running
ClipVault on macOS and Linux. It exists to make sure both hosts compile
the same code with the same toolchain and to keep the local user data
outside the build/test pipeline.

## Required toolchains

| Layer     | Tool          | Pinned version        | Where it is declared                |
| --------- | ------------- | --------------------- | ----------------------------------- |
| Rust      | `rustc`       | `1.89.0`              | `rust-toolchain.toml`               |
| Rust      | MSRV          | `1.89`                | `Cargo.toml` (`workspace.package`)  |
| Frontend  | Node.js       | 20 LTS                | `app/tauri/frontend/.nvmrc`         |
| Frontend  | npm           | 10                    | `app/tauri/frontend/package.json` (`engines`) |
| Frontend  | npm lockfile  | tracked               | `app/tauri/frontend/package-lock.json` |
| Tauri CLI | `cargo-tauri` | 2.x compatible with Tauri 2 | Install with `cargo install tauri-cli --version "^2"` |

The Rust toolchain is installed automatically by `rustup` the first time
a `cargo` command runs inside the workspace, because the
`rust-toolchain.toml` file pins the channel. CI mirrors the same pin by
calling `dtolnay/rust-toolchain@master` with `toolchain: 1.89.0`
explicitly; the `stable` channel of the GitHub Actions image does NOT
honour `rust-toolchain.toml` and would otherwise pick whatever Rust
release the runner considers current.

Node and npm must be provisioned so that the version reported by
`node --version` matches `app/tauri/frontend/.nvmrc` (Node 20 LTS).
When the host already has a different Node line (for example Node 22 or
26) the install is allowed to print an `EBADENGINE` warning, but CI and
fresh checkouts must run with Node 20.

## Canonical working directories

ClipVault intentionally runs each layer from its own root. Do not
shorten the commands by changing the working directory at random or by
passing `--manifest-path` to `cargo tauri dev`: the `beforeDevCommand`
in `app/tauri/src-tauri/tauri.conf.json` is configured relative to
`app/tauri/frontend`.

```text
crates/                 -> clipvault-core, clipvault-db, clipvault-platform, clipvault-search
app/tauri/src-tauri/    -> Tauri 2 desktop shell (clipvault-app)
app/tauri/frontend/     -> Svelte + TypeScript frontend
```

## Rust commands (run from the repository root)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The Linux shell feature combination used by the Ubuntu build is checked
locally on macOS to prove the same source compiles with the same
features that ship on Linux:

```sh
cargo check -p clipvault-app \
  --no-default-features \
  --features clipboard-arboard,hotkey-global
```

## Frontend commands (run from app/tauri/frontend)

```sh
npm ci
npm run check
npm run build
npm test
```

`npm ci` consumes `app/tauri/frontend/package-lock.json` exactly as it
is tracked in Git. It must never be replaced by `npm install` in a
change, because that would rewrite the lockfile and drift the
dependency graph between machines.

Do not run `npm audit fix --force` as part of the reproducible install:
it mutates the dependency tree and breaks the lockfile guarantee. If
`npm audit` reports vulnerabilities, open a follow-up change that
treats them as a deliberate upgrade.

## Tauri development command (run from app/tauri)

```sh
cd app/tauri
cargo tauri dev
```

This command resolves the frontend `beforeDevCommand`
(`npm run dev` in `app/tauri/frontend`) through the directory the
project already configures. **Do not** append `--manifest-path` to
`cargo tauri dev`: the `tauri` subcommand does not accept it, and even
if it did, the relative path of the `beforeDevCommand` would point at
the wrong location.

### One active instance

Before starting a new build of the desktop app, close the previous
Tauri instance. A stale bundle left running on the host will display
out-of-date strings and may capture new clipboard content into the
local SQLite, which would otherwise be attributed to the current
commit.

Concretely, on macOS quit ClipVault from the menu bar tray icon and
confirm there are no `clipvault-app` processes left:

```sh
pkill -x clipvault-app || true
```

On Linux/X11 quit from the system tray and verify no `clipvault-app`
processes remain:

```sh
pkill -x clipvault-app || true
```

Wayland does not expose the same global hotkey and active-app surface
as X11; the same command applies but the manual smoke tests in
`docs/manual-flows.md` document which flows are validated on which
session type.

## Local user data is never a build artifact

ClipVault stores its database and assets under the user's home
directory:

```text
~/.clipvault/clipvault.db
~/.clipvault/assets/
```

These paths are owned by the running application and must never be
treated as build artifacts. The Rust workspace never writes to them;
the tests use `tempfile` and in-memory fakes; CI runs in ephemeral
containers with no `$HOME` history; and the `.gitignore` keeps them
out of Git.

The following commands are forbidden during normal development,
testing, validation and OpenSpec operations, because they can move,
rename or regenerate the local database or assets:

- `cargo clean` when a release build has already populated
  `~/.clipvault` (only use `cargo clean` if you have already verified
  the user data lives elsewhere);
- `rm -rf ~/.clipvault`;
- `npm ci --clean-cache` followed by operations that target
  `~/.clipvault`;
- any custom script that points at `~/.clipvault` to "reset" state.

If a clean rebuild is required, move `~/.clipvault` aside first, build
the project, then decide explicitly whether to copy it back.

## Git + OpenSpec flow across hosts

```text
main is up to date
  -> branch feat/fix/chore (one change at a time)
  -> OpenSpec change with proposal/design/specs/tasks
  -> implement on macOS
  -> commit + push
  -> pull the same branch on Ubuntu
  -> run the documented checks + manual Linux smoke test
  -> push fixes back to the same branch
  -> PR + merge into main
  -> OpenSpec sync + archive (only after manual validation)
```

Every report must include:

- the branch name and the exact commit SHA;
- the platform (macOS / Linux X11 / Linux Wayland);
- the toolchain versions reported by `rustc --version`,
  `node --version`, `npm --version` and `cargo tauri --version`;
- the commands that were run;
- the limitations of the validation (for example, Wayland not available
  on the runner, no display server in CI, permissions not granted).

## What CI covers and what stays manual

The CI workflow (`.github/workflows/ci.yml`) runs non-interactive
checks for both macOS and Ubuntu:

- `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cargo test --workspace`;
- `cargo check -p clipvault-app --no-default-features --features clipboard-arboard,hotkey-global`
  on Ubuntu (proves the Linux feature combination compiles);
- `npm ci`, `npm run check`, `npm run build` and `npm test` from
  `app/tauri/frontend`.

CI never:

- runs `cargo tauri dev`;
- opens a GUI window;
- uses the real clipboard, simulates hotkeys or queries the active
  application;
- reads or writes `~/.clipvault`;
- publishes clipboard content, hashes, absolute paths or asset bytes
  in logs or artifacts.

Manual checks that remain out of CI scope are documented in
`docs/manual-flows.md`. They include global hotkey activation, system
tray, synthetic paste, macOS Privacy permissions, X11 and Wayland
session behaviour. They must be executed on the host before a change
that touches them is archived.
