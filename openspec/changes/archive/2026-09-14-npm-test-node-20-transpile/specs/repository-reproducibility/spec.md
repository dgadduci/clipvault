## MODIFIED Requirements

### Requirement: Canonical frontend toolchain and dependency graph

The frontend SHALL use Node.js 20 LTS with npm 10 as its declared development
baseline, and `app/tauri/frontend/package-lock.json` SHALL remain the single
tracked npm dependency lockfile. The frontend SHALL ship its tests as a
TypeScript transpile + Node test runner pipeline so `npm test` succeeds on
Node 20 LTS without depending on Node 22-only flags or third-party
TypeScript runners.

#### Scenario: Clean frontend installation

- **WHEN** a developer installs frontend dependencies
- **THEN** the developer runs `npm ci` from `app/tauri/frontend`
- **AND** the command consumes the tracked `package-lock.json`
- **AND** no root, `app/` or `app/tauri/` npm/pnpm lockfile is generated
- **AND** `git diff app/tauri/frontend/package-lock.json` remains empty

#### Scenario: Frontend verification uses one dependency graph

- **WHEN** macOS, Ubuntu and CI run frontend checks
- **THEN** they use the same tracked lockfile and declared Node/npm baseline
- **AND** `npm run check`, `npm run build` and `npm test` run from the
  frontend directory without an implicit package-manager migration

#### Scenario: Frontend test pipeline runs on Node 20 LTS

- **WHEN** a developer runs `npm test` from `app/tauri/frontend`
- **THEN** the script invokes `tsc --noCheck -p tsconfig.test.json`
  followed by `node --test` against the transpiled output under
  `app/tauri/frontend/node_modules/.cache/clipvault-test-build/tests/`
- **AND** the script does NOT depend on `--experimental-strip-types`,
  `--experimental-transform-types` or any Node 22-only flag
- **AND** no third-party TypeScript runner (`tsx`, `ts-node`, `swc-node`)
  is added to `devDependencies`
- **AND** no `@types/node` is added to `devDependencies`
- **AND** the cache directory is recreated on every `npm ci` /
  `npm test` invocation and is matched by `**/node_modules` in
  `.gitignore`, so it is never committed
