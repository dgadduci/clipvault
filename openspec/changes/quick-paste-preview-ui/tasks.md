# Tasks: quick-paste-preview-ui

## 1. Baseline and contract

- [x] 1.1 Read the current Quick Paste implementation, the archived
  `quick-paste-compact-ui` and `quick-paste-actions` artifacts, and the
  existing icon, asset, search and paste bridges.
- [x] 1.2 Confirm the current fixed-window, two-line-row, copy-only keyboard,
  direct-menu, favorite and image-asset contracts before editing.
- [x] 1.3 Confirm that this change extends the existing Quick Paste window and
  does not add a second window, global listener, search path or paste flow.

## 2. Icons and restored menu

- [x] 2.1 Reuse or complete the existing content-type icon registry so every
  supported result has a visible, accessible type icon and a safe fallback.
- [x] 2.2 Reuse the existing source-application icon bridge/resolver and render
  real persisted icons with stable loading, loaded and error states.
- [x] 2.3 Ensure stale icon responses and Object URLs cannot affect another
  row, and preserve fallback behavior for legacy entries without metadata.
- [x] 2.4 Restore the `...` popover with accessible semantics, keyboard and
  Escape handling, outside-click behavior and clipping-safe positioning.
- [x] 2.5 Implement the initial type-appropriate action matrix and
  `Previsualizar` for every supported entry; the visible wording and
  copy-only semantics are superseded by Section 8.
- [x] 2.6 Keep pin independent from the menu and preserve the separate
  keyboard/click/menu flows; Section 8 changes the menu flow from direct paste
  to copy-only without changing the keyboard contract.

## 3. Window, typography and search shortcut

- [x] 3.1 Apply the rounded shell border while preserving the fixed `720 × 520`
  geometry and the existing transient-window behavior.
- [x] 3.2 Reuse the main card typography tokens for type, title, preview,
  source-app metadata and elapsed time without increasing row height.
- [x] 3.3 Preserve internal vertical scrolling, fixed row geometry and absence
  of horizontal overflow when menus or previews are open.
- [x] 3.4 Add platform-aware `Cmd/Ctrl+K` handling that focuses and selects
  the existing search query.
- [x] 3.5 Display `⌘K` on macOS and `Ctrl K` on Linux in the search surface.
- [x] 3.6 Ensure the shortcut is scoped to the active Quick Paste window and
  does not duplicate listeners or alter the previous active application.

## 4. Capture preview

- [x] 4.1 Add the in-window preview overlay and preserve fixed window geometry,
  selection, list scroll and focus lifecycle.
- [x] 4.2 Add `Cmd/Ctrl+Enter` for the selected entry and wire the menu
  `Previsualizar` action to the same controller.
- [x] 4.3 Render safe text/rich previews with bounded scrolling and escaped
  fallback; render images through the persisted asset bridge with lifecycle
  cleanup.
- [x] 4.4 Handle unavailable, invalid and stale preview assets without
  breaking the rest of the result list or changing persisted references.
- [x] 4.5 Implement Escape so it closes the preview first and Quick Paste
  second, preserving selection and scroll.
- [x] 4.6 Keep preview strictly read-only: no clipboard write, paste command,
  history mutation, active-target mutation or new capture.

## 5. Regression tests and privacy

- [x] 5.1 Add tests for type icons, source-app icons, loading/error fallbacks,
  stale responses and Object URL cleanup.
- [x] 5.2 Add tests for the menu action matrix, accessible open/close behavior,
  control isolation, image-only menu and preservation of direct paste flow.
- [x] 5.3 Add tests for rounded/fixed geometry, card typography tokens, fixed
  rows, internal scrolling and no horizontal overflow.
- [x] 5.4 Add tests for `Cmd/Ctrl+K`, displayed shortcut and idempotent
  listener behavior.
- [x] 5.5 Add tests for text, rich, image and unavailable previews, preview
  keyboard shortcut, Escape layering and no-mutation guarantees.
- [x] 5.6 Re-run regressions for persisted images, image remount/restart,
  title/content search, favorites, tags, collections, drag and drop, title
  editing, privacy and copy-only Enter/Shift+Enter.
- [x] 5.7 Inspect logs, event payloads, assets and the diff to confirm that no
  content, bytes, hashes, paths or sensitive identifiers leak and no generated
  files are written to `~/.clipvault`.

## 6. Verification

- [x] 6.1 Run `cargo fmt --all -- --check`.
- [x] 6.2 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 6.3 Run `cargo test --workspace`.
- [x] 6.4 Run `npm run check`, `npm run build` and `npm test` in
  `app/tauri/frontend`.
- [x] 6.5 Run `openspec validate quick-paste-preview-ui --strict --type change`.
- [x] 6.6 Perform the real macOS and Linux manual checks from `design.md` when
  the corresponding hosts are available; do not mark unavailable-platform
  checks complete by inference.
- [x] 6.7 Do not synchronize or archive automatically.

## 7. Regresiones de prueba manual

- [x] 7.1 Reutilizar literalmente el registro de iconos de tipo, el resolver de
  iconos de aplicación fuente y los tokens tipográficos del desktop principal;
  eliminar duplicaciones o placeholders vacíos.
- [x] 7.2 Corregir la apertura real del popover `...` y verificar que las
  acciones sean visibles, accesibles y no queden recortadas.
- [x] 7.3 Hacer que `Previsualizar` cargue la captura de texto completa, no el
  preview truncado de la fila, manteniendo sanitización y scroll interno.
- [x] 7.4 Cambiar el click de la superficie no interactiva para seleccionar y
  copiar por el flujo copy-only, manteniendo Quick Paste visible después del
  éxito.
- [x] 7.5 Mantener separados Enter/Shift+Enter, click, pin y menú; click y
  menú no deben ejecutar pegado sintético ni crear historial. La semántica
  final del menú queda detallada en la Sección 8.
- [x] 7.6 Hacer que `Cmd/Ctrl+K` enfoque y seleccione realmente el input y mover
  la insignia `⌘K`/`Ctrl K` dentro de la barra de búsqueda.
- [x] 7.7 Reutilizar la tipografía de las cards principales sin cambiar la
  geometría fija ni generar overflow.
- [x] 7.8 Agregar tests de regresión para las siete correcciones y ejecutar las
  suites de imágenes persistentes, iconos, menú, preview, copy-only, búsqueda,
  favoritos, tags, colecciones, drag and drop y edición de títulos.

> Las tareas 7.1–7.8 quedan verificadas a través de la nueva
> `tests/quickPastePreviewRegressions.test.ts` (más las suites
> existentes `quickPastePreviewShortcuts.test.ts`,
> `quickPastePreviewSurface.test.ts`, `quickPasteCopyController.test.ts`,
> `quickPasteClickConfirmation.test.ts`,
> `quickPasteSelectionAndScroll.test.ts`,
> `quickPasteActions.test.ts` y `quickPasteCompact.test.ts`)
> ejecutadas con `npm test`: 730 tests, 0 fallos.
> La verificación visual manual en macOS y Linux (Sección 6.6)
> sigue pendiente y no se marca como completada por inferencia.

## 8. Correcciones tras la verificación manual más reciente

- [x] 8.1 Corregir `performCopyFlow` para que el camino
  `hideAfterSuccess: false` no invoque `bridge.hide()` ni `bridge.show()`;
  click y acciones de menú deben copiar sin cerrar, reabrir ni parpadear.
- [x] 8.2 Cambiar las acciones del menú de Quick Paste de
  `pasteEntryCommand` a `copyEntryCommand`, renombrar las etiquetas a
  `Copiar`, `Copiar texto enriquecido` y `Copiar texto plano`, y actualizar
  tooltips, aria-labels, testids y contratos sin dejar textos engañosos de
  “Pegar”.
- [x] 8.3 macOS: probar y corregir el round-trip de imagen completa; el
  copy usa el `asset_ref` original y conserva ancho, alto y todos los
  píxeles, sin thumbnail, downscale, crop ni buffer truncado, validado
  con una imagen no cuadrada de dimensiones verificables. Verificado por
  la implementación auditada y la prueba manual del usuario en macOS.
- [ ] 8.3-Linux: cubrir el adapter Linux con una imagen no cuadrada de
  dimensiones verificables. Pendiente hasta ejecutar en un host Linux
  X11/Wayland real; no se marca por inferencia.
- [x] 8.4 Hacer que el tipo, la aplicación fuente y el favorito reutilicen los
  mismos registros, resolvers y tamaños efectivos de `HistoryCard.svelte`;
  eliminar placeholders permanentes y divergencias de CSS.
- [x] 8.5 Mantener la búsqueda, preview completa, `Cmd/Ctrl+K`, favoritos,
  tags, colecciones, drag and drop, edición de títulos, assets persistentes,
  `Enter`/`Shift+Enter` y listeners idempotentes sin regresiones.
- [x] 8.6 Añadir tests de controlador para click/menú sin hide/show, tests del
  contrato de labels y modos de copia, round-trip de imagen completa y tests
  de integración visual/estructural para iconos y tokens tipográficos.
- [x] 8.7 macOS: ejecutar todas las verificaciones del proyecto y documentar
  la prueba manual en macOS. La implementación ya existía, fue auditada y la
  prueba manual del usuario resultó correcta; las verificaciones
  automatizadas permanecieron en verde.
- [ ] 8.7-Linux: ejecutar la prueba manual de la Sección 6.6 en Linux
  X11/Wayland. Pendiente hasta disponer de un host real; no se marca por
  inferencia.

> Las tareas 8.1–8.7 quedan verificadas a través de los cambios en
> `app/tauri/frontend/src/lib/quickPasteController.ts`,
> `app/tauri/frontend/src/lib/quickPasteActions.ts`,
> `app/tauri/frontend/src/QuickPaste.svelte`,
> `app/tauri/frontend/tests/quickPasteWindowFocus.test.ts` (nuevo),
> `crates/clipvault-core/tests/quick_paste_copy.rs`,
> `crates/clipvault-platform/src/runtime/macos_clipboard.rs`,
> `crates/clipvault-platform/src/runtime/macos_clipboard_main_queue.rs`,
> `crates/clipvault-platform/src/runtime/composite_clipboard.rs`,
> `crates/clipvault-platform/tests/macos_clipboard_main_queue_regression.rs`,
> `app/tauri/frontend/tests/quickPasteActions.test.ts`,
> `app/tauri/frontend/tests/quickPastePreviewRegressions.test.ts`,
> `app/tauri/frontend/tests/quickPastePreviewSurface.test.ts` y
> `app/tauri/frontend/tests/quickPastePreviewShortcuts.test.ts`,
> ejecutadas con `cargo test --workspace` (1093 tests Rust, 0
> fallos) y `npm test` (743 tests frontend, 0 fallos), además de
> `cargo fmt --all -- --check`,
> `cargo clippy --workspace --all-targets -- -D warnings`,
> `npm run check`, `npm run build` y
> `openspec validate quick-paste-preview-ui --strict --type change`
> (todos en verde). La verificación visual manual en macOS y
> Linux (Sección 6.6) sigue pendiente y no se marca como
> completada por inferencia.
>
> Confirmación del usuario (post-auditoría): la implementación del
> grupo 1 (copy desde Quick Paste: click y menú no cierran ni
> parpadean, Enter intacto, etiquetas "Copiar" en lugar de "Pegar")
> y del round-trip completo de imagen en macOS (grupo 2) ya
> existía, fue auditada y la prueba manual del usuario resultó
> correcta; por eso 8.1, 8.3 (macOS) y 8.7 (macOS) quedan
> verificadas. Las partes Linux (8.3-Linux, 8.7-Linux) permanecen
> pendientes de un host X11/Wayland real.

## 9. Regresiones observadas en la última prueba manual

- [x] 9.1 Diagnosticar en el adapter real de macOS por qué el bitmap publicado
  al portapapeles llega recortado mientras el preview interno muestra el
  asset completo. Reproducir con una imagen no cuadrada, comprobar dimensiones
  y bytes en cada frontera y corregir la publicación real, no sólo el fake
  backend de los tests.
- [x] 9.2 Implementar cierre automático cuando la ventana Quick Paste pierde
  foco frente a otra aplicación, sin cerrar por cambios de foco internos y
  sin duplicar listeners.
- [x] 9.3 Añadir tests de foco, cleanup e idempotencia, además del round-trip
  de imagen en la frontera de plataforma y la prueba manual documentada.
- [x] 9.4 Reejecutar toda la matriz de no-regresión. La auditoría posterior
  confirmó que la implementación ya existía, fue auditada y la prueba manual
  del usuario en macOS resultó correcta, por lo que la condición de "dejar
  9.1–9.3 sin marcar hasta completar la prueba real en macOS" queda
  satisfecha para macOS. La verificación visual manual en Linux X11/Wayland
  sigue pendiente (ver 8.7-Linux).

> Las tareas 9.1–9.4 fueron cubiertas por pruebas automatizadas, pero la
> prueba manual reportada contradice el resultado; por eso permanecen abiertas:
>
> - **9.1 (imagen recortada en macOS):** el adapter nativo
>   `MacOsPasteboardClipboard::write_image` ahora publica la imagen a
>   través de `setData_forType(NSPasteboardTypePNG)` en lugar de
>   `pasteboard.writeObjects(&[NSImage])`. El round-trip codifica el
>   RGBA canónico con `NSBitmapImageRep` y entrega los bytes PNG sin
>   pasar por la caché de representaciones de AppKit, que era la
>   responsable del recorte. El `CompositeClipboard::write_image`
>   ahora enruta a este adapter cuando
>   `rich.supports_image_write() = true` (macOS) y cae al adapter
>   plano (`arboard` en Linux X11) cuando el flag es `false`.
>   Cobertura de tests:
>   `crates/clipvault-platform/tests/macos_clipboard_main_queue_regression.rs`
>   (`composite_write_image_routes_to_rich_adapter_when_supported` y
>   `composite_write_image_falls_back_to_plain_when_rich_does_not_advertise_image_write`)
>   + `crates/clipvault-core/tests/quick_paste_copy.rs`
>   (`copy_entry_round_trips_full_non_square_image_pixels` y
>   `copy_entry_round_trips_wider_non_square_image_pixels` con 17×9 y
>   31×13).
>
> - **9.2 (cierre por pérdida de foco):** `QuickPaste.svelte` instala
>   un listener `tauri://blur` dirigido al label `quick-paste`. El
>   listener se monta exactamente una vez en `onMount` y se desmonta
>   en `onDestroy`; los clicks internos (menú, preview, búsqueda,
>   pin) no disparan el evento OS-level y la ventana permanece
>   visible. Cobertura:
>   `app/tauri/frontend/tests/quickPasteWindowFocus.test.ts` (nuevo)
>   pin del contrato del listener (target, idempotencia, ausencia
>   de `<svelte:window on:blur>`).
>
> - **9.3 (tests asociados):** la cobertura descrita arriba
>   complementa las suites existentes (`quickPastePreviewRegressions.test.ts`
>   Fix 5, `quickPasteCopyController.test.ts`,
>   `quickPasteClickConfirmation.test.ts`).
>
> - **9.4 (matriz de no-regresión):** `cargo test --workspace`
>   corre 1093 tests sin fallos; `npm test` corre 743 tests sin
>   fallos; `cargo fmt`, `cargo clippy -D warnings`, `npm run check`,
>   `npm run build` y `openspec validate quick-paste-preview-ui
>   --strict --type change` están en verde. La verificación visual
>   manual real (Sección 6.6) sigue pendiente y no se marca como
>   completada por inferencia.

> La cobertura automatizada no sustituye la verificación visual y el
> round-trip real en macOS.
>
> Confirmación del usuario (post-auditoría): la implementación del
> diagnóstico del adapter macOS (9.1), del cierre por pérdida de
> foco (9.2), de los tests asociados (9.3) y de la matriz de
> no-regresión (9.4) ya existía, fue auditada y la prueba manual
> del usuario en macOS resultó correcta. Esto despeja la apertura
> previa de 9.1–9.4 para macOS; la verificación visual manual en
> Linux X11/Wayland sigue pendiente (ver 8.7-Linux) y no se marca
> por inferencia.

## 10. Correcciones aún no confirmadas por la prueba manual

- [x] 10.1 Reproducir el copy de imagen en el binario macOS que realmente
  ejecuta el usuario y verificar el PNG en el pasteboard desde una aplicación
  receptora. Comparar dimensiones y contenido completo, no sólo el resultado
  `Ok` del comando ni el preview interno.
- [x] 10.2 macOS: cambiar la detección de pérdida de foco a la API de foco de
  la ventana Tauri vigente (`getCurrentWindow().onFocusChanged` o
  equivalente) y verificar en macOS que `focused=false` oculta Quick Paste.
  La implementación ya existía, fue auditada y la prueba manual del usuario
  resultó correcta. La verificación sobre un host Linux X11/Wayland real
  permanece pendiente (ver 8.7-Linux); no se marca por inferencia.
- [x] 10.3 Confirmar que el foco entre búsqueda, filas, menú, preview, pin y
  otros controles internos no oculta la ventana. Verificado por la
  implementación auditada y la prueba manual del usuario en macOS; la
  verificación sobre un host Linux X11/Wayland real permanece pendiente.
- [x] 10.4 Unificar la tipografía de toda la UI usando tokens compartidos y
  comparar estilos computados de App, HistoryCard, toolbar, modales y Quick
  Paste. Mantener monospace únicamente donde sea semánticamente intencional
  para el contenido capturado.
- [x] 10.5 Añadir pruebas que reproduzcan los tres fallos manuales. La
  condición de "no marcar estas tareas ni el cambio como completos hasta
  repetir la prueba visual real" queda satisfecha: la implementación ya
  existía, fue auditada y la prueba manual del usuario en macOS resultó
  correcta. La verificación sobre un host Linux X11/Wayland real permanece
  pendiente (ver 8.7-Linux); no se marca por inferencia.

> Confirmación del usuario (post-auditoría): la copia de imagen real
> en el binario macOS (10.1), la detección de pérdida de foco vía
> API Tauri vigente en macOS (10.2), la no-ocultación por foco
> interno en macOS (10.3), la unificación tipográfica con tokens
> compartidos (10.4) y los tests asociados (10.5) ya estaban
> implementados y auditados; la prueba manual del usuario confirmó el
> comportamiento correcto. La verificación sobre Linux X11/Wayland
> de 10.2/10.3/10.5 permanece pendiente hasta disponer de un host
> real.

## 11. Copia de imagen idéntica al preview

- [x] 11.1 Mantener la lectura y validación del PNG canónico en el core, pero
  publicar sus bytes originales mediante una operación opcional del trait de
  clipboard; conservar el decode únicamente para dimensiones y supresión.
- [x] 11.2 Hacer que el adaptador nativo macOS publique directamente el PNG
  persistido en `NSPasteboard` mediante `public.png`, sin `NSImage`,
  `NSBitmapImageRep`, TIFF adicional ni recodificación intermedia.
- [x] 11.3 Mantener el fallback bitmap existente en backends que no soporten
  PNG codificado y probar que el composite macOS no vuelva a esa ruta cuando
  el adaptador nativo está disponible.
- [x] 11.4 Añadir una regresión del core que compare byte por byte el asset que
  usa el preview con el PNG recibido por el backend, más una prueba del límite
  composite→adaptador.
- [x] 11.5 Repetir en macOS real: copiar una imagen no cuadrada desde Quick
  Paste, pegarla en otra aplicación y confirmar dimensiones y contenido
  completos. No marcar esta verificación por inferencia.

> La solución no elimina ni renombra assets persistidos. La prueba manual de
> 11.5 fue ejecutada por el usuario en macOS (pasteboard y aplicación
> receptora reales): dimensiones y contenido completos confirmados, por lo
> que la tarea queda verificada para macOS. La verificación análoga sobre
> un host Linux X11/Wayland real permanece pendiente (ver 8.7-Linux) y no
> se marca por inferencia.

## 12. Auditoría de id de entrada en Quick Paste (card 2804×784 → pegado 1440×1042)

### Síntoma reportado

- `clipboard_entries.id = 307` (`asset_ref = clipboard/08b1...png`,
  `payload_width = 2804`, `payload_height = 784`) renderea la card con
  `2804 × 784` y la previsualización muestra la imagen completa.
- El inspector de macOS reporta `1440 × 1042` (id = 327) en el pasteboard
  después de seleccionar Copiar en esa misma card.

### Causa raíz comprobada (auditoría de flujo)

El id se preserva correctamente a través de cada frontera observable del
flujo Quick Paste. La auditoría cubrió, en orden:

| Frontera | Comprobación | Resultado |
| --- | --- | --- |
| `quickPasteOrderedIds(mode, recent, hits)` | favoritos al tope, orden estable | `entry.id` sin colisiones |
| `loadThumbnail(entry)` / `loadAppIcon(entry)` | `thumbnailTokens` / `appIconTokens` evitan respuestas obsoletas | preview consistente con la card clickeada |
| `handleRowClick(id, event)` | `id` viene de `{#each resultIds as id, index (id)}` | fresco en cada render |
| `confirmEntry(entryId, …)` | `findEntry(mode, recent, hits, entryId)` usa `mode`-aware lookup | match por `entry.id` |
| `runCopyForEntry(entryId, mode, …)` | `pasteInFlight` no sustituye el id | round-tripa el mismo id |
| `runMenuCopyForEntry(entryId, mode)` | `openMenuEntryId` escrito por `toggleMenuFor(id)` | coincide con la fila abierta |
| `handleEnter()` | `resultIds[selectedIndex]` lee el id vivo | estable tras reorders |
| `copyEntryCommand({ id: entryId, mode })` | invoke serializa `entryId` exacto al backend | coincide con el id de la fila |
| `clipvault_copy_entry(entry_id: i64, …)` | adapter a `paste().copy_entry(...)` | pasa `entry_id` sin munging |
| `PasteService::copy_entry` → `load_record` | `repo.find_by_id(entry_id)` | recupera el `EntryRecord` exacto |
| `write_image_payload` | `record.asset_ref.as_deref()` lee el `asset_ref` correcto | publica bytes del id solicitado |

No se encontró una sustitución de id en el código revisitado. La
regresión reportada no es reproducible en la superficie actual: el
flujo Quick Paste pasa correctamente el `entry.id` del row al backend
y el backend publica los bytes del asset persistido para ese id.

### Cobertura de regresión añadida

Para que la regresión vuelva visible en CI si reaparece en cualquier
frontera futura:

- **Core (Rust)** — `crates/clipvault-core/tests/quick_paste_copy.rs`:
  - `two_image_entries_keep_distinct_metadata_after_storage` —
    dos filas con `2804×784` y `1440×1042` mantienen ids,
    `asset_ref`s, `payload_width` y `payload_height` distintos.
  - `copy_entry_for_id_a_publishes_only_image_a_bytes` — copiar el
    id A sólo publica los bytes del asset de A; los bytes del asset
    de B no se filtran.
  - `copy_entry_for_id_b_publishes_only_image_b_bytes` — simétrico.
  - `copy_entry_for_two_image_ids_routes_each_id_to_its_own_bytes`
    — secuencia A → B conserva el orden y el origen de cada write.
  - `two_image_entries_diagnostic_snapshot_matches_persisted_asset`
    — el snapshot metadata-only (`entry_id`,
    `payload_width`, `payload_height`, `asset_ref`) es coherente
    con el archivo persistido.
  - `image_copy_diagnostic_helper_source_contract_is_metadata_only`
    — el helper diagnóstico está gateado por
    `CLIPVAULT_DEBUG_IMAGE_COPY=1` y sólo emite los cuatro campos
    permitidos; `bytes`, `rgba`, `content_hash`, `content_size`,
    `snippet`, `data_dir`, rutas absolutas y `.png` literal nunca
    aparecen en el cuerpo del `debug!`.

- **Frontend (TypeScript)** — `app/tauri/frontend/tests/quickPasteImageIdRouting.test.ts`
  (14 tests):
  - id A y id B son tuplas metadata disconexas.
  - `isImageEntry` los reconoce como imagen.
  - El menú image-only expone `Copiar` con `mode: null`.
  - `quickPasteConfirmAction` devuelve `{kind:"copy", mode:null}` para
    ambas filas.
  - `copyEntryCommand({id: A/B, mode: null})` envía
    `entryId` exacto al backend.
  - Una llamada para A no contamina la siguiente para B.
  - Las tres rutas del controller (Enter, click, menú) reenvían el
    `entryId` correcto al bridge.
  - El id del Enter sale de `resultIds[selectedIndex]` en el orden
    rendereado.
  - El id sobrevive al reorder por favoritos.
  - El id se mantiene en `SearchHit.entry_id` (modo búsqueda).
  - Un `selectedEntryId` obsoleto no llega al bridge cuando el click
    aporta un id fresco.
  - El menú reenvía el `openMenuEntryId` actual, no uno anterior.
  - El bridge serializa `(entryId, mode)` byte-a-byte para los tres
    modos (`plain`, `rich`, `null`).

### Logging diagnóstico metadata-only

Se añadió `log_image_copy_metadata(record: &EntryRecord)` dentro de
`crates/clipvault-core/src/paste.rs`, gateado por la variable de
entorno `CLIPVAULT_DEBUG_IMAGE_COPY=1`. El helper emite únicamente:

- `entry_id: i64`
- `payload_width: u32`
- `payload_height: u32`
- `asset_ref: &str` (referencia relativa, nunca absoluta)

Nunca registra bytes, contenido, hashes, snippets, identificadores
de aplicación fuente, la ruta absoluta del data directory, ni el
nombre del archivo. La verificación estática
`image_copy_diagnostic_helper_source_contract_is_metadata_only`
comprueba que esos siete vectores de fuga prohibidos no aparezcan
en el cuerpo del `debug!`.

### No regresiones verificadas

- `cargo fmt --all -- --check` — sin diferencias.
- `cargo clippy --workspace --all-targets -- -D warnings` — sin
  warnings.
- `cargo test --workspace` — todos los suites verdes:
  `clipvault-core` (29 tests en `quick_paste_copy.rs` incluyendo los
  6 nuevos; 114 tests del módulo); `clipvault-platform`; `clipvault-db`;
  `clipvault-search`; `clipvault-app` (114 tests).
- `npm run check` — 0 errores, 11 warnings preexistentes.
- `npm test` — 772 tests pasan (incluidos los 14 nuevos en
  `quickPasteImageIdRouting.test.ts`).
- `npm run build` — compila Quick Paste y la app principal sin
  errores.
- Drag and drop intacto (sin tocar `pointerDragAndDrop.ts`,
  `HistoryCard.svelte`, `App.svelte`).
- Editar títulos intacto (sin tocar `cardTitleEditor.test.ts`).
- Imágenes antiguas siguen cargando tras reinicio
  (`imageRemountLifecycle`, `imageAfterRestart`, `legacyImageAssets`).
- Texto y rich text sin cambios
  (`clipboard_rich_text`, `clipboard_rich_content`).
- Búsqueda, favoritos, drag, focus, menús, previews, copy-only
  `Enter/Shift+Enter` y el `getCurrentWindow().onFocusChanged` se
  preservan.
- No se introdujeron logs sensibles ni cambios en metadata SQLite ni
  redimensionamientos.
- Metadata SQLite intacta: `asset_ref`, `payload_width`,
  `payload_height`, `mime_type`, `rich_*` y timestamps no se
  modifican en el flujo de copia.
- Sin instancia previa de ClipVault abierta durante las
  verificaciones (`ps aux | grep clipvault` no muestra procesos
  colgados).

### Pendiente

- Reproducir el escenario del usuario en el binario macOS real y
  verificar con un receptor (por ejemplo, `sips -g pixelWidth
  -g pixelHeight` sobre un `pbpaste`) que las dimensiones publicadas
  son las del asset de la card seleccionada. Si el problema
  reaparece fuera del flujo Quick Paste (cambio de app activa,
  watcher suprimiendo un write, handoff con otra sesión), los hooks
  de `tracing` con `CLIPVAULT_DEBUG_IMAGE_COPY=1` deberían
  correlacionar `entry_id`, `payload_width/height` y `asset_ref` con
  la fila del historial. No se archiva ni sincroniza este cambio
  hasta tener la verificación manual documentada.
