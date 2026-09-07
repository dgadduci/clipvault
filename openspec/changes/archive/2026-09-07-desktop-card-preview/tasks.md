# Tareas de implementación

## 1. Relevamiento y límites

- [x] 1.1 Leer `project.md`, `AGENTS.md`, este cambio y los contratos de
  `quick-paste-preview-ui`, `quick-paste-compact-ui`,
  `clipboard-rich-content`, `clipboard-rich-text`, `desktop-shell-layout`,
  `search-title-matching` y `card-title-editing-regression`.
- [x] 1.2 Auditar `QuickPaste.svelte`, `HistoryCard.svelte`,
  `HistoryCardRail.svelte`, `App.svelte`, `clipboardAsset.ts`,
  `iconResolver.ts`, `contentTypeIcons.ts` y sus tests.
- [x] 1.3 Identificar exactamente dónde vive hoy la preview de Quick Paste,
  su matcher Cmd/Ctrl+Enter, su carga de imágenes, escape y cleanup.
- [x] 1.4 Ejecutar `git status --short` y `git diff --check`; preservar cambios
  pendientes de otros specs.
- [x] 1.5 Documentar en `design.md` cualquier contradicción detectada antes de
  resolverla. No mantener dos implementaciones por conveniencia.

## 2. Extracción compartida

- [x] 2.1 Extraer la lógica pura de preview a un módulo compartido, conservando
  `entryFullPreviewText`, `escapeForPreview` y las políticas actuales.
- [x] 2.2 Extraer a un helper común el matcher de `Cmd/Ctrl+Enter`, con rechazo
  de Shift/Alt y contexto interactivo.
- [x] 2.3 Crear un componente compartido para header, body, footer, overlay,
  foco, Escape, click exterior y estados de preview.
- [x] 2.4 Hacer que el componente use el resolver de asset existente, con
  loading/loaded/error, guard anti-stale y revocación completa de Blob URLs.
- [x] 2.5 Mantener la preview completa de texto y la política segura vigente;
  no reutilizar el preview truncado de las cards/filas.
- [x] 2.6 Verificar que la extracción no cambia visual ni funcionalmente la
  preview de Quick Paste.

## 3. Integración Desktop

- [x] 3.1 Agregar `Previsualizar` al menú común de `HistoryCard` para texto,
  rich text, imágenes y tipos actuales.
- [x] 3.2 Hacer las cards enfocables sin romper `draggable="false"`, pointer
  capture, fallback WebKit/Tauri, bloqueo de selección ni hit-testing.
- [x] 3.3 Conectar `Cmd+Enter`/`Ctrl+Enter` sólo al contexto de card enfocado y
  excluir input, editor, botones, menú y otros controles interactivos.
- [x] 3.4 Coordinar una única preview Desktop desde App/rail o equivalente;
  no montar una overlay por card.
- [x] 3.5 Resolver la entrada desde el scope visible actual y cerrar la
  preview si la entrada desaparece por búsqueda, colección, borrado o refresh.
- [x] 3.6 Cerrar el menú al abrir la preview exactamente una vez y aislar el
  evento de pin, paste, delete, título, tags, colecciones y drag-and-drop.
- [x] 3.7 Mantener el Desktop fijo, sin crecimiento del body, scroll horizontal
  adicional ni cambios en la altura de las cards.

## 4. Integración Quick Paste

- [x] 4.1 Reemplazar el markup/lógica local de preview por el componente y
  helpers compartidos.
- [x] 4.2 Mantener selección, scroll, `Cmd/Ctrl+Enter`, Escape, click exterior,
  focus-loss close, menú y copy-only exactamente como estaban.
- [x] 4.3 Confirmar que el componente compartido no agrega listeners globales
  duplicados ni cambia la ventana fija de 720×520.

## 5. Tests de no-regresión

- [x] 5.1 Tests unitarios del helper compartido: texto completo, escape,
  imágenes, tipos inválidos y matcher por plataforma.
- [x] 5.2 Tests del componente compartido: texto, rich text, imagen,
  loading/error, stale response, scroll interno, Escape, click exterior y
  cleanup de Blob URLs.
- [x] 5.3 Tests Desktop: menú `Previsualizar`, shortcut de card enfocada,
  contexto inválido, foco de retorno y una sola preview.
- [x] 5.4 Tests Quick Paste: mismo helper/componente, shortcut, selección,
  menú, preview y ausencia de cambios funcionales.
- [x] 5.5 Tests de privacidad: ningún log/evento/DOM nuevo contiene contenido,
  bytes, hashes, paths absolutos o referencias opacas visibles.
- [x] 5.6 Ejecutar regresiones de imágenes tras reinicio y remount, rich text,
  título/contenido search, favoritos, tags, colecciones y source-app filter.
- [x] 5.7 Ejecutar regresiones de drag-and-drop, edición de títulos, paste,
  focus-loss de Quick Paste y listeners idempotentes.

## 6. Verificación automática

- [x] 6.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 6.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 6.3 Ejecutar `cargo test --workspace`.
- [x] 6.4 Ejecutar `npm run check` dentro de `app/tauri/frontend`.
- [x] 6.5 Ejecutar `npm run build` dentro de `app/tauri/frontend`.
- [x] 6.6 Ejecutar `npm test` dentro de `app/tauri/frontend`.
- [x] 6.7 Ejecutar `openspec validate desktop-card-preview --strict --type
  change`.
- [x] 6.8 Revisar el diff y confirmar que no hay dependencias, red,
  telemetría, archivos generados ni escrituras en `~/.clipvault`.

## 7. Verificación manual

- [x] 7.1 En macOS, cerrar instancias anteriores y abrir una build actual.
- [x] 7.2 Abrir `Previsualizar` desde el menú de una card de texto y una de
  imagen; verificar contenido completo, imagen completa y ausencia de
  mutaciones.
- [x] 7.3 Enfocar cards distintas y probar `Cmd+Enter`; confirmar que el
  shortcut sólo abre la card enfocada.
- [x] 7.4 Probar Escape, click exterior, apertura repetida y retorno de foco.
- [x] 7.5 Cambiar colección, buscar, hacer pin, editar título y usar tags sin
  que la preview muestre una entrada obsoleta.
- [x] 7.6 Reiniciar y confirmar imágenes antiguas, títulos, tags, favoritos,
  búsqueda y drag-and-drop.
- [ ] 7.7 Repetir en Linux X11/Wayland cuando estén disponibles usando
  `Ctrl+Enter` y respetando capacidades existentes.

> Las tareas 1.1–6.8 quedan verificadas a través del trabajo documentado.
> El nuevo módulo `app/tauri/frontend/src/lib/clipboardPreview.ts`
> consolida `entryFullPreviewText`, `escapeForPreview`,
> `isImageEntry`, `hasRenderableImage` y el matcher
> `matchesPreviewShortcut` (consumido por Desktop y Quick Paste);
> el componente compartido vive en
> `app/tauri/frontend/src/ClipboardPreview.svelte` y es importado
> por `QuickPaste.svelte` y `App.svelte`. Los tests nuevos
> (`clipboardPreview.test.ts`, `clipboardPreviewComponent.test.ts`,
> `desktopCardPreview.test.ts`,
> `desktopCardPreviewNoMutations.test.ts` y
> `desktopCardPreviewPrivacy.test.ts`) ejecutan 73 casos sin
> fallos. `cargo fmt --all -- --check`, `cargo clippy -D warnings`,
> `cargo test --workspace` (suite Rust, 0 fallos), `npm run check`
> (0 errores, 13 warnings preexistentes), `npm run build` (verde)
> y `npm test` (852 tests frontend, 0 fallos) están en verde.
> `openspec validate desktop-card-preview --strict --type change`
> responde `Change 'desktop-card-preview' is valid`.
> Las tareas 7.1–7.6 fueron verificadas manualmente por el usuario en macOS.
> La tarea 7.7 permanece pendiente hasta disponer de un host Linux X11/Wayland
> para ejecutar esa comprobación específica.

La validación manual de `platform-permission-guidance` permanece separada y no
debe marcarse como completada por este cambio. Este cambio tampoco debe
sincronizarse ni archivarse automáticamente.
