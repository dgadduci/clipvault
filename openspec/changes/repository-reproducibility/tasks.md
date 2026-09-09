# Tareas de implementación

## 1. Auditoría previa y aislamiento

- [x] 1.1 Leer `AGENTS.md`, `project.md`, `Cargo.toml`, `Cargo.lock`,
  `rust-toolchain.toml`, `app/tauri/frontend/package.json`, el lockfile del
  frontend y `app/tauri/src-tauri/tauri.conf.json`.
- [x] 1.2 Confirmar que el cambio se ejecuta desde una rama dedicada y
  registrar rama, commit base y estado inicial sin incluir los lockfiles
  accidentales `pnpm` ni ejemplos locales no relacionados. Rama actual:
  `chore/repository-reproducibility`; commit base `31b9932`; los lockfiles
  `pnpm` y `crates/clipvault-core/examples/debug_png.rs` no se modifican.
- [x] 1.3 Inventariar, sin modificar datos del usuario, las versiones de Rust,
  Cargo, Node, npm y cargo-tauri disponibles en el host implementador y
  compararlas con la base Ubuntu validada (`rustc 1.89.0`). Host actual
  (macOS): `rustc 1.98.0` (auto-instala `1.89.0` desde `rust-toolchain.toml`),
  `cargo 1.98.0`, `node v26.8.1`, `npm 11.19.0`, `cargo-tauri 2.11.4`.
  La base Ubuntu validada es `rustc 1.89.0`; el engine warning de `npm ci`
  por Node 26 es informativo, CI usa Node 20 LTS desde `.nvmrc`.
- [x] 1.4 Confirmar que la prueba manual Ubuntu 10/10 de
  `linux-x11-compatibility` se documentará en su propio change y no se
  mezclará con este cambio.

## 2. Toolchain Rust común

- [x] 2.1 Cambiar `rust-toolchain.toml` de `stable` a `1.89.0`, conservando
  `rustfmt`, `clippy` y el perfil mínimo. También se elimina la sección
  `[toolchain.target]` específica de la máquina de desarrollo, ya que los
  targets deben añadirse en el pipeline de release y no en el repositorio.
- [x] 2.2 Cambiar el `workspace.package.rust-version` de `1.85` a `1.89`
  y documentar la razón en este change. Razón: el lockfile actual requiere
  Rust ≥ 1.89 para resolver `tauri 2.11.x` y sus dependencias, y la prueba
  Ubuntu validada fue sobre `1.89.0`.
- [x] 2.3 Verificar que no se agregan targets específicos de una máquina ni
  dependencias nuevas por este ajuste. `git diff --stat Cargo.lock` quedó
  vacío; no se añadieron crates.
- [x] 2.4 Ejecutar fmt, clippy y tests Rust con la toolchain declarada y
  revisar que `Cargo.lock` sólo cambie si existe una causa directa y
  documentada. `cargo fmt --all -- --check`, `cargo clippy --workspace
  --all-targets -- -D warnings` y `cargo test --workspace` pasan con
  `1.89.0`; `Cargo.lock` no cambia.

  Nota de scope: dos lints preexistentes en
  `crates/clipvault-core/src/content_type.rs` (`overly_complex_bool_expr` y
  `dead_code`) impedían que `cargo clippy -D warnings` pasara con la
  toolchain declarada. Se aplicó una corrección mínima sin cambio de
  comportamiento: reemplazar `FILE_PATH_RELATIVE_TOKENS.contains(&input)
  && false` por `false` y marcar la constante con `#[allow(dead_code)]`.
  El comportamiento de `is_file_path` es idéntico.

## 3. Toolchain frontend y lockfiles

- [x] 3.1 Declarar Node.js 20 LTS y npm 10 mediante los archivos mínimos y
  estables que el proyecto necesite, registrando la versión exacta usada por
  CI sin inventar una versión incompatible con los hosts. Se añadió
  `app/tauri/frontend/.nvmrc` con `20` (usado por `actions/setup-node` en
  CI) y un campo `engines` en `app/tauri/frontend/package.json`
  declarando `node >=20.0.0 <21.0.0` y `npm >=10.0.0 <11.0.0`. CI consume
  la versión declarada vía `.nvmrc`; el host implementador ya tiene
  Node 26 y `npm ci` emite un `EBADENGINE` informativo, sin alterar el
  lockfile ni la instalación.
- [x] 3.2 Conservar `app/tauri/frontend/package-lock.json` como único
  lockfile npm oficial y no ejecutar `npm install` para resolver dependencias
  durante la validación. `git diff app/tauri/frontend/package-lock.json`
  queda vacío tras `npm ci`, `npm run check`, `npm run build` y `npm test`.
- [x] 3.3 Ajustar `.gitignore` para ignorar `pnpm-lock.yaml` y
  `pnpm-workspace.yaml` accidentales en cualquier subdirectorio, sin borrar
  archivos locales ni ignorar el lockfile npm rastreado. Patrones añadidos:
  `/pnpm-lock.yaml`, `**/pnpm-lock.yaml`, `**/pnpm-workspace.yaml`.
  `git check-ignore` confirma que `pnpm-lock.yaml` y
  `pnpm-workspace.yaml` quedan ignorados y `package-lock.json` permanece
  rastreado.
- [x] 3.4 Ejecutar `npm ci`, `npm run check`, `npm run build` y `npm test`
  desde `app/tauri/frontend` y verificar que `npm ci` no modifica el
  lockfile. Los cuatro comandos pasan; 1192 tests frontend verdes;
  `package-lock.json` no cambia.

## 4. Documentación operativa

- [x] 4.1 Crear o actualizar una guía de desarrollo multiplataforma con las
  versiones requeridas, las raíces correctas de cada comando y el flujo
  macOS → commit/push → Ubuntu → prueba → PR/merge. Se creó
  `docs/development.md` con la tabla de toolchains, las raíces canónicas y
  el flujo Git/OpenSpec.
- [x] 4.2 Documentar `cd app/tauri && cargo tauri dev` como comando canónico
  y explicar que `cargo tauri dev --manifest-path ...` no es válido para este
  proyecto. Documentado en `docs/development.md` § "Tauri development
  command".
- [x] 4.3 Documentar el cierre de una instancia Tauri anterior antes de
  recompilar y la prohibición de usar `npm audit fix --force` como solución
  automática de la instalación reproducible. Documentado en
  `docs/development.md` § "One active instance" y en la sección de
  comandos frontend.
- [x] 4.4 Documentar la separación entre checks automatizados y pruebas
  manuales de hotkeys, captura, foco, tray, pegado, X11 y Wayland.
  Documentado en `docs/development.md` § "What CI covers and what stays
  manual" y reforzado en el workflow.
- [x] 4.5 Documentar que `~/.clipvault/clipvault.db` y
  `~/.clipvault/assets/` son datos locales protegidos y nunca artefactos de
  compilación o limpieza. Documentado en `docs/development.md` § "Local user
  data is never a build artifact" con la lista de comandos prohibidos.

## 5. CI multiplataforma

- [x] 5.1 Crear workflow(s) de GitHub Actions para macOS y Ubuntu sin
  ejecutar una GUI ni tocar el portapapeles real. Workflow creado en
  `.github/workflows/ci.yml` con dos jobs (`rust` y `frontend`) en matriz
  `macos-latest` / `ubuntu-latest`.
- [x] 5.2 Configurar setup de Rust según `rust-toolchain.toml` y setup de
  Node según la declaración del frontend, usando cache sin regenerar
  lockfiles. `dtolnay/rust-toolchain@master` se invoca con
  `toolchain: 1.89.0` y `components: rustfmt, clippy` (el canal `stable`
  de la acción NO consulta `rust-toolchain.toml`, por eso la versión se
  fija explícitamente); `actions/setup-node@v4` usa `node-version-file:
  app/tauri/frontend/.nvmrc`. La cache de Cargo y npm está indexada por
  `Cargo.lock` + `rust-toolchain.toml` y por `package-lock.json`
  respectivamente, y `CARGO_HOME`/`RUSTUP_HOME` se redirigen a
  `${{ runner.temp }}`. CI añade además un paso que imprime
  `rustc --version` / `cargo --version` y falla si `rustc` no reporta
  exactamente `1.89.0`, de modo que una build verde no se consiga con un
  canal estable distinto.
- [x] 5.3 Instalar en Ubuntu únicamente las dependencias de sistema
  necesarias para compilar Tauri y documentarlas en el workflow. Paso
  "Install Linux Tauri system prerequisites" instala
  `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`,
  `librsvg2-dev`, `libssl-dev`, `patchelf`, `build-essential`, `curl`,
  `wget`, `file`.
- [x] 5.4 Ejecutar en CI fmt, clippy, tests Rust y el check Linux del shell
  con `--no-default-features --features clipboard-arboard,hotkey-global`.
  Pasos `fmt`, `clippy`, `test` y `Linux shell feature combination`
  (este último condicional a Linux).
- [x] 5.5 Ejecutar en CI `npm ci`, `npm run check`, `npm run build` y
  `npm test` desde `app/tauri/frontend`. Job `frontend` con `working-
  directory: app/tauri/frontend` para los cuatro comandos.
- [x] 5.6 Asegurar que los jobs no llaman `cargo tauri dev`, no requieren
  `DISPLAY`, no usan `~/.clipvault` y no publican contenido, rutas, hashes o
  bytes de clipboard en logs o artefactos. El workflow no invoca `cargo
  tauri dev`; el comentario al inicio del archivo declara la política
  "headless". `CARGO_HOME`, `RUSTUP_HOME` y la cache npm se redirigen a
  `runner.temp`, lo que aísla la sesión del `$HOME` del runner.
- [x] 5.7 Mantener las pruebas GUI reales como checklist manual; no marcar
  `platform-permission-guidance` como validado desde este workflow.
  Documentado en `docs/development.md` y en el comentario del workflow; el
  change `platform-permission-guidance` no fue tocado.

## 6. No-regresiones

- [x] 6.1 Verificar que los adapters macOS, Linux X11 y Wayland conservan sus
  features y sus contratos; no reimplementar lógica de plataforma.
  `git diff` no toca `crates/clipvault-platform/`. Los archivos de adapters
  y contratos permanecen intactos.
- [x] 6.2 Verificar que el historial SQLite y los assets existentes no se
  leen, mueven, borran ni regeneran durante CI, OpenSpec o los scripts
  nuevos. Ningún script añadido apunta a `~/.clipvault`. El job `rust`
  corre `cargo test --workspace`, que ya usa `tempfile` y fakes in-memory.
- [x] 6.3 Verificar que las imágenes guardadas, tags, colecciones, favoritos,
  búsqueda, Quick Paste y drag-and-drop no se modifican por este cambio.
  `git diff` no toca `crates/clipvault-core/src/image*`, `crates/clipvault-
  db`, `crates/clipvault-search`, `crates/clipvault-core/src/quick*`,
  `crates/clipvault-core/src/drag*` ni
  `app/tauri/frontend/src/lib/pointerDragAndDrop.ts`. La única edición en
  `content_type.rs` es semánticamente neutra.
- [x] 6.4 Revisar el diff para confirmar que no hay dependencias funcionales
  nuevas, red, telemetría, LLM, secretos, contenido del clipboard ni rutas
  absolutas personales. `Cargo.toml` y `package.json` no añaden dependencias;
  la CI no envía telemetría; el workflow no publica secretos. Diff resumido:
  5 archivos modificados (`.gitignore`, `Cargo.toml`, `package.json`,
  `content_type.rs`, `rust-toolchain.toml`), 4 archivos nuevos
  (`.github/workflows/ci.yml`, `app/tauri/frontend/.nvmrc`,
  `docs/development.md`, `openspec/changes/repository-reproducibility/*`).

## 7. Verificación

- [x] 7.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 7.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 7.3 Ejecutar `cargo test --workspace`. 42 grupos de tests verdes
  (`cargo test --workspace | grep -c "test result: ok\."` = 42; ninguna
  línea `FAILED`).
- [x] 7.4 Ejecutar `npm run check` desde `app/tauri/frontend`. 0 errors,
  15 warnings preexistentes (no introducidas por este change).
- [x] 7.5 Ejecutar `npm run build` desde `app/tauri/frontend`. Build OK;
  artefactos en `app/tauri/frontend/dist/`.
- [x] 7.6 Ejecutar `npm test` desde `app/tauri/frontend`. 1192 tests
  verdes, 0 fallos.
- [x] 7.7 Ejecutar `openspec validate repository-reproducibility --strict
  --type change`. Salida: `Change 'repository-reproducibility' is valid`.
- [x] 7.8 Revisar `git status --short`, `git diff --check` y el diff final;
  dejar fuera lockfiles `pnpm` y artefactos locales no relacionados. Diff
  limpio (`git diff --check` sin avisos), `Cargo.lock` y
  `app/tauri/frontend/package-lock.json` sin cambios, `pnpm-lock.yaml` y
  `pnpm-workspace.yaml` ignorados.

## 8. Verificación manual y cierre

- [ ] 8.1 En macOS, clonar/actualizar la rama, confirmar la toolchain
  declarada y ejecutar el flujo documentado con una sola instancia Tauri.
  Pendiente para una corrida manual en macOS con Node 20 LTS instalado
  (`fnm`/`nvm`) y `cargo tauri dev` validado como comando canónico. En este
  host el host implementador tiene Node 26 y el flujo con Node 20 LTS se
  delega a CI (`node-version-file`).
- [ ] 8.2 En Ubuntu, hacer checkout del mismo commit, confirmar Rust 1.89.0,
  ejecutar los checks documentados y conservar el resultado manual 10/10 de
  la validación Linux. Pendiente de re-validar con `rust-toolchain.toml`
  fijado a `1.89.0` (Ubuntu ya validó 10/10 con `1.89.0` cuando era override
  local). El job Linux del workflow confirma la combinación
  `clipboard-arboard,hotkey-global`; el smoke test GUI depende de la sesión
  X11/Wayland y permanece como checklist manual.
- [ ] 8.3 Confirmar que las pruebas de ambos hosts usan el mismo commit,
  `Cargo.lock` y `app/tauri/frontend/package-lock.json`. Garantizado por
  Git + los lockfiles versionados; CI lo refuerza al cachear por sus hashes.
  Validación manual final pendiente del merge.
- [ ] 8.4 Confirmar que `~/.clipvault` y sus assets permanecen intactos antes
  y después de compilar, probar, validar OpenSpec y actualizar Git.
  Ningún script añadido toca esa ruta. Pendiente de inspección manual del
  usuario en macOS y Ubuntu tras el merge.
- [ ] 8.5 Registrar limitaciones de sesión Wayland/X11, permisos macOS y
  cualquier check no ejecutable en CI sin presentarlo como soporte verificado.
  Documentado en `docs/development.md` § "What CI covers and what stays
  manual" y en el comentario inicial del workflow; sigue siendo checklist
  manual (`docs/manual-flows.md`).
- [ ] 8.6 No archivar automáticamente este change; esperar validación manual,
  commit/push y la orden explícita de sincronización y archive.
