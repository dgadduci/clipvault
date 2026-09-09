# Development workflow

This document pins the canonical command sequence a contributor runs on
macOS or Linux to bring ClipVault to a green state from a clean checkout.
The goal is reproducibility: a single command sequence produces the same
outcome on both hosts, with no implicit package-manager migration, no
ambient toolchain override, and no accidental writes to
`~/.clipvault/`.

## Required toolchains

| Layer | Tool | Version | Pin location |
| --- | --- | --- | --- |
| Shell | Rust | 1.89.0 | `rust-toolchain.toml` |
| Shell | Cargo | 1.89.0 | ships with the Rust toolchain |
| Shell | Tauri CLI | 2.x | `cargo install tauri-cli --version "^2"` |
| Frontend | Node.js | 20 LTS | `app/tauri/frontend/.nvmrc` |
| Frontend | npm | 10 | `app/tauri/frontend/package.json` (`engines`) |

`node --version` MUST report `v20.x` and `npm --version` MUST report
`10.x` from `app/tauri/frontend`. Running `npm ci` on any other
Node major version emits an `EBADENGINE` warning and is treated as
a local-host problem, not a project configuration issue.

## Canonical command sequence

Run every step from the repository root unless the section explicitly
states otherwise.

### 1. Rust checks

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The Rust workspace declares `rust-version = "1.85"` and the toolchain
file pins `1.89.0`. Both checks run on the pinned toolchain; CI
asserts `rustc --version` reports exactly `1.89.0` so an ambient
stable compiler cannot silently pass.

### 2. Frontend checks (run from `app/tauri/frontend`)

```text
npm ci
npm run check
npm run build
npm test
```

- `npm ci` consumes the tracked `app/tauri/frontend/package-lock.json`
  and MUST NOT rewrite it. `git diff package-lock.json` stays empty.
- `npm run check` invokes `svelte-check` against `tsconfig.json` and
  surfaces type errors in `src/`.
- `npm run build` runs `svelte-check` and then `vite build`. The
  bundle output lands in `app/tauri/frontend/dist/`.
- `npm test` runs the Node 20 test pipeline documented below.

### 3. Tauri shell (run from `app/tauri`)

```text
cargo tauri dev
```

The supported entry point is `cd app/tauri && cargo tauri dev`. The
unsupported form `cargo tauri dev --manifest-path ...` MUST NOT be
used; the project does not pass `--manifest-path` to `cargo tauri
dev` and the documented flow does the working-directory switch
instead.

## `npm test` on Node 20 LTS

`npm test` runs the Node 20 built-in test runner against TypeScript
sources through a `tsc` transpile step. The script:

```text
"test": "tsc --noCheck -p tsconfig.test.json && node --test --test-reporter=spec node_modules/.cache/clipvault-test-build/tests/"
```

- `tsconfig.test.json` extends the regular `tsconfig.json` and
  enables `noEmit: false`, `declaration: false`, `sourceMap: false`,
  `rewriteRelativeImportExtensions: true`, `rootDir: "./"` and
  `outDir: "./node_modules/.cache/clipvault-test-build"`.
- `--noCheck` is intentional: the test pipeline does not add
  `@types/node` as a `devDependency`. The user-facing type contract
  for `src/` is covered by `npm run check` (`svelte-check`); the test
  pipeline keeps Node 20 as a runtime dependency.
- The transpiled cache directory lives under
  `app/tauri/frontend/node_modules/.cache/clipvault-test-build/`,
  which is matched by `**/node_modules` in `.gitignore`. It is
  recreated on every `npm ci` / `npm test` invocation and is never
  committed.

The pipeline is intentionally **not** `--experimental-strip-types`
(removed) and **not** `tsx` / `ts-node` (no new dependency). Both
options would either split the toolchain from Node 20 LTS or add an
unnecessary devDependency for a single command.

## Generated-file hygiene

`git status --short` after the canonical sequence MUST NOT include:

- `app/tauri/frontend/node_modules/` (anywhere)
- `app/tauri/frontend/dist/`
- `app/tauri/frontend/node_modules/.cache/clipvault-test-build/`
- `pnpm-lock.yaml`, `pnpm-workspace.yaml` (anywhere)
- `~/.clipvault/clipvault.db`, `~/.clipvault/assets/`

The `.gitignore` already covers every entry above. If `git status`
shows any of them after a clean run, the run did something the
canonical sequence does not document — stop and investigate.

## One active instance

Before rebuilding or restarting the Tauri shell manually, close the
previous instance. A stale bundle from the previous build can pass
visual checks that look like validation of the current commit; the
canonical command sequence assumes a single Tauri process per host.

## What CI covers and what stays manual

CI on macOS and Ubuntu runs:

- `cargo fmt`, `cargo clippy`, `cargo test`, the Linux shell feature
  combination (`cargo check -p clipvault-app --no-default-features
  --features clipboard-arboard,hotkey-global`).
- `npm ci`, `npm run check`, `npm run build`, `npm test` from
  `app/tauri/frontend`.

CI does NOT exercise the global hotkey, the active-app probe, the
tray / menu bar, the synthetic paste path, the macOS Privacy prompts
or the X11 / Wayland session behaviour. Those checks remain manual
and are documented in `docs/manual-flows.md`.

## Local user data is never a build artifact

The host's `~/.clipvault/clipvault.db` and `~/.clipvault/assets/`
are personal data the application writes at runtime. None of the
commands above — `cargo test`, `npm ci`, `npm run check`,
`npm run build`, `npm test`, `cargo tauri dev`, `cargo tauri build`,
CI or OpenSpec validation — touch that directory. If a build script
ever proposes to do so, stop and treat it as a regression.
