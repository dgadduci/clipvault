# Tasks: code-language-detection

## 1. Relevamiento y contrato

- [x] 1.1 Leer `AGENTS.md`, `project.md`, este cambio y los contratos de
  `clipboard-type-detection`, `clipboard-rich-content`,
  `clipboard-rich-text`, `desktop-card-preview` y `quick-paste-preview-ui`.
- [x] 1.2 Auditar `ContentType`, `EntryRecord`, `NewEntry`, las migraciones
  vigentes, `detect_content_type`, el pipeline de captura y los bridges de
  Desktop/Quick Paste.
- [x] 1.3 Confirmar que la nueva metadata no altera imágenes, rich text,
  títulos, tags, colecciones, favoritos, búsqueda ni drag and drop.
- [x] 1.4 Fijar la versión de `highlight.js` (`^11.10.0`), el allowlist,
  los aliases y los umbrales de detección en
  `app/tauri/frontend/src/lib/codeLanguageDetector.ts` y los tests.

## 2. Persistencia y core

- [x] 2.1 Agregar la migración `0011` (la `0010` ya está ocupada por
  `tags-and-collections`) con `code_language TEXT NULL` e índice
  aditivo, up/down reversibles. Migración aditiva en
  `crates/clipvault-db/src/registry.rs` (`MIGRATION_0011_CODE_LANGUAGE`).
- [x] 2.2 Extender `EntryRecord`, `NewEntry`, row mappings y serialización
  sin romper filas antiguas. El campo `code_language: Option<String>`
  es nullable y los insert/touch lo propagan. `ENTRY_COLUMNS`,
  `row_to_record` y `parse_code_language` actualizados en
  `crates/clipvault-db/src/entry_repository.rs`.
- [x] 2.3 Crear validación canónica de lenguaje y un servicio de persistencia
  idempotente que sólo acepte entradas textuales y no sobrescriba una
  clasificación existente con `null`. Ver `code_language.rs` y
  `code_language_service.rs` en `crates/clipvault-core/src/`.
- [x] 2.4 Mantener intacta la precedencia de JSON, HTML, SQL, shell y demás
  tipos existentes; `ContentType::Code` sigue siendo la categoría común.

## 3. Bridge Tauri y actualización

- [x] 3.1 Exponer un comando delgado `clipvault_code_language_set` con
  `entry_id` y lenguaje canónico, respuesta metadata-only y errores
  tipados. Registrado en `app/tauri/src-tauri/src/commands.rs` y
  `app/tauri/src-tauri/src/main.rs`.
- [x] 3.2 Implementar la hidratación desde Desktop/Quick Paste para filas
  sin `code_language`, con deduplicación por id y respuestas obsoletas
  ignoradas. `app/tauri/frontend/src/lib/codeLanguageHydration.ts`.
- [x] 3.3 Actualizar recent entries, search, history-updated y tipos
  frontend sin transportar contenido adicional en eventos.
  `App.svelte` y `QuickPaste.svelte` invocan el helper tras
  `history-updated` y tras `loadRecent`.

## 4. Detector y resaltado frontend

- [x] 4.1 Agregar `highlight.js/lib/core` con sólo las gramáticas del
  allowlist. `package.json` actualizado con `highlight.js@^11.10.0` y el
  helper `ensureHighlightRegistered` registra los lenguajes canónicos
  una sola vez.
- [x] 4.2 Implementar helper puro para aliases, fences, shebangs, señales
  de código, relevancia, empate, límite de tamaño y fallback a texto.
  Constantes `MIN_DETECTION_LENGTH`, `MIN_LINE_COUNT`,
  `RELEVANCE_THRESHOLD`, `RELEVANCE_MARGIN`, `MAX_DETECTION_BYTES`
  cubiertas por tests deterministas.
- [x] 4.3 Reutilizar el helper desde Desktop, HistoryCard,
  ClipboardPreview y Quick Paste. `ClipboardPreview.svelte` y
  `HistoryCard.svelte` consumen `codeLanguageDetector.ts` y muestran
  `Código · Python` con resaltado local.
- [x] 4.4 Renderizar la preview completa con el lenguaje persistido y
  fallback seguro a texto escapado; no persistir HTML generado. La
  sanitización defensiva (`sanitiseHighlightedHTML`) descarta
  scripts, event handlers y URLs activas.
- [x] 4.5 Mostrar `Código` más el nombre canónico del lenguaje sin
  cambiar la geometría existente de cards o filas.

## 5. Tests y no-regresiones

- [x] 5.1 Tests del detector para JavaScript, TypeScript, Java, C, C++,
  Python, Rust y el resto del allowlist.
  `app/tauri/frontend/tests/codeLanguageDetector.test.ts`.
- [x] 5.2 Tests de alias, fences, shebangs, snippets cortos, prosa ambigua,
  empates, umbral, límites y prioridad de tipos existentes. Mismo test
  file.
- [x] 5.3 Tests de migración, filas legacy, validación, persistencia,
  idempotencia y supervivencia tras reinicio.
  `crates/clipvault-db/tests/code_language_migration.rs` (8 tests).
- [x] 5.4 Tests de bridge, hidratación, búsqueda, Desktop, Quick Paste y
  preview compartida. `crates/clipvault-core/tests/code_language_bridge.rs`
  (6 tests) + `app/tauri/frontend/tests/codeLanguageHydration.test.ts`
  (11 tests).
- [x] 5.5 Tests de privacidad, sin contenido/HTML generado/hashes/paths en
  logs, eventos, DOM o errores.
  `crates/clipvault-core/tests/code_language_privacy.rs` (7 tests).
- [x] 5.6 Reejecutar regresiones de imágenes, rich text, título, tags,
  colecciones, favoritos, búsqueda, drag and drop, foco y copy/paste.
  Los tests de `clipvault-db`, `clipvault-core` (incluyendo
  `clipboard_rich_content`, `desktop_dnd_card_visual_corrections`,
  `desktop_header_card_dnd`, `organization`, `pasteboard_png_metadata`)
  y los 889 tests de frontend pasan tras la migración.

## 6. Verificación automática

- [x] 6.1 `cargo fmt --all -- --check` → 0 diffs.
- [x] 6.2 `cargo clippy --workspace --all-targets -- -D warnings` →
  Finished sin warnings.
- [x] 6.3 `cargo test --workspace` → 0 fallos. Resumen capturado:
  69, 61, 5, 14, 10, 4, 9, 286, 10, 9, 6, 47, 47, 6, 7, 5, 6, 14, 14,
  56, 19, 8, 15, 18, 21, 32, 29, 12, 11, 9 tests por binario/test (todos
  pasan). Ejecutado en macOS Darwin (host actual).
- [x] 6.4 `npm run check` → 0 errors (13 warnings preexistentes
  no introducidos por este cambio). `npm run build` → ✓ built.
  `npm test` → 889 / 889 passing.
- [x] 6.5 `openspec validate code-language-detection --strict --type
  change` → ejecutado tras la implementación; ver sección final.
- [x] 6.6 Revisar el diff y confirmar que no hay red, telemetría,
  archivos generados ni escrituras fuera de los assets permitidos.
  `git status --short` y `git diff --stat` revisados.

## 7. Verificación manual

- [x] 7.1 Copiar snippets reales de JavaScript, TypeScript, Java, C/C++,
  Python y Rust; confirmar tipo, lenguaje y resaltado.
  **Verificación manual realizada por el usuario en macOS; resultado correcto.**
- [x] 7.2 Probar prosa ambigua, texto corto, JSON, HTML, SQL y shell;
  confirmar que no hay falsos positivos ni regresiones de tipo.
  **Verificación manual realizada por el usuario en macOS; resultado correcto.**
- [x] 7.3 Reiniciar ClipVault y confirmar que `code_language`, títulos,
  tags, colecciones, favoritos e imágenes persisten.
  **Verificación manual realizada por el usuario en macOS; resultado correcto.**
- [x] 7.4 Confirmar la misma etiqueta y preview en Desktop, card,
  preview y Quick Paste. **Verificación manual realizada por el usuario en macOS; resultado correcto.**
- [x] 7.5 Confirmar que búsqueda, drag and drop, copy/paste y rich text
  siguen funcionando. **Verificación manual realizada por el usuario en macOS; resultado correcto.**
- [ ] 7.6 Repetir en Linux X11/Wayland cuando estén disponibles; no
  marcar capacidades de plataforma por inferencia. **No ejecutado
  en este host (macOS Darwin). No se infieren capacidades.**

El cambio no debe sincronizarse ni archivarse automáticamente. Las tareas
manuales sólo se marcan con evidencia real del host correspondiente.