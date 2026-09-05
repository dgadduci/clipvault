# Tareas de implementación

## 1. Diagnóstico

- [x] 1.1 Confirmar la causa raíz del decoder:
  `decode_png` rechaza `(ColorType, BitDepth)` distintos de
  `Rgb`+`Eight` y `Rgba`+`Eight`, lo que descarta cualquier PNG
  capturado por una build anterior o por una herramienta externa.
- [x] 1.2 Confirmar la causa raíz del colector:
  `HistoryManagementService::collect_unreferenced_assets` colapsa
  un `Err` de `referenced_asset_refs` (y de
  `referenced_rich_asset_refs`) en un `BTreeSet::new()`, lo que
  provoca que `ClipboardAssetStore::collect_unreferenced` borre
  todos los PNG que SQLite sigue referenciando.
- [x] 1.3 Confirmar la causa raíz del aislamiento: el binario
  de tests `organization.rs` ejecutó `unlink` contra
  `~/.clipvault/assets/clipboard/*.png` (auditoría 11:20:44) por
  un `AppBootstrap::finish()` que recurría a
  `DefaultPlatform::detect()` y resolvía `data_dir` a
  `~/.clipvault` aunque la base SQLite viviera en un
  `tempfile::TempDir`.
- [x] 1.4 Documentar las tres causas y la separación de
  responsabilidades en `proposal.md`.

## 2. Decoder ampliado

- [x] 2.1 Refactorizar `decode_png` para delegar la conversión del
  frame a un helper `expand_png_frame_to_rgba8`.
- [x] 2.2 Implementar la rama `Rgba` (8/16 bits) y `Rgb` (8/16 bits)
  en el helper.
- [x] 2.3 Implementar la rama `Grayscale` (1/2/4/8/16 bits) incluyendo
  el caso sub-byte con escalado estándar del PNG.
- [x] 2.4 Implementar la rama `GrayscaleAlpha` (8/16 bits).
- [x] 2.5 Implementar la rama `Indexed` (1/2/4/8 bits) consumiendo
  la paleta `PLTE` y el chunk `tRNS` desde `reader.info()`.
- [x] 2.6 Mantener `AssetError::NotPng` como rechazo tipado para
  combinaciones ilegales bajo la especificación PNG.

## 3. Diagnóstico metadata-only del decoder

- [x] 3.1 Definir `AssetDiagnosticKind` con las variantes `Loaded`,
  `InvalidReference`, `NotFound`, `WrongDataDir`, `WrongNamespace`,
  `TooLarge`, `InvalidPng`, `InvalidDimensions` y `Io`.
- [x] 3.2 Exponer `AssetDiagnosticKind::kind_str()` con
  identificadores snake_case estables.
- [x] 3.3 Implementar `ClipboardAssetStore::diagnose` que ejecuta la
  pipeline completa de validación y devuelve el `AssetDiagnostic`
  sin exponer rutas absolutas, bytes ni hashes.
- [x] 3.4 Exportar el diagnóstico desde `clipvault_core::lib`.

## 4. Colector seguro

- [x] 4.1 Reescribir
  `HistoryManagementService::collect_unreferenced_assets` para
  separar `Ok(set)` y `Err(error)` antes de llamar al colector.
- [x] 4.2 Aplicar la misma separación a la consulta
  `referenced_rich_asset_refs()`.
- [x] 4.3 Definir y exportar `AssetCollectionOutcome` con campos
  metadata-only snake_case:
  `image_reference_query_succeeded`,
  `image_reference_query_failed`, `image_collection_skipped`,
  `rich_reference_query_succeeded`,
  `rich_reference_query_failed`, `rich_collection_skipped`,
  `assets_removed_count`.
- [x] 4.4 Verificar que el struct serializado nunca contiene
  rutas absolutas, hashes, bytes ni el `asset_ref`.
- [x] 4.5 Sustituir el `warn!(error = %error, ...)` (que filtra
  contenido del error de SQLite) por `warn!(error_kind =
  "image_reference_query_failed", ...)` y su equivalente para
  rich text.

## 5. Protección arquitectónica del bootstrap (regresión 11:20:44)

- [x] 5.1 Añadir `BootstrapError::MissingPlatformAdapters` con un
  mensaje que guíe al caller hacia `with_platform_adapters` o
  `with_default_platform_adapters`.
- [x] 5.2 Modificar `AppBootstrap::finish()` para devolver el nuevo
  error cuando no se inyectaron `PlatformAdapters` (en lugar de
  recurrir a `DefaultPlatform::detect()`).
- [x] 5.3 Exponer `AppBootstrap::with_default_platform_adapters()`,
  una ruta explícita para que el shell de Tauri siga pudiendo
  usar el host detection sin reabrir la regresión.
- [x] 5.4 Crear `crates/clipvault-core/src/test_support.rs` con
  `IsolatedTestHarness`, `build_isolated_adapters`, `FixedClock`
  y los helpers `isolated_harness` / `isolated_harness_with_clock`
  / `isolated_harness_at`.
- [x] 5.5 Re-exportar el helper desde `lib.rs` para que las
  suites de integración (`crates/clipvault-core/tests/`) y los
  tests internos (`crates/clipvault-core/src/**/tests.rs`)
  consuman la misma fuente.
- [x] 5.6 Auditar y actualizar todas las llamadas a
  `AppBootstrap::bootstrap_at` y `bootstrap_with_database` que
  viven en `crates/clipvault-core/tests/`,
  `crates/clipvault-core/src/` y `app/tauri/src-tauri/tests/` para
  inyectar `PlatformAdapters` aislados.
- [x] 5.7 Comprobar que `app/tauri/src-tauri/src/bootstrap.rs`
  sigue construyendo el bundle contra la detección del host
  (producción) sin cambios funcionales.

## 6. Tests de regresión Rust / core — decoder y diagnóstico

- [x] 6.1 Tests de decoder para `Rgba` 8-bit, `Rgb` 8-bit,
  `Indexed` 8-bit, `Grayscale` 8-bit, `GrayscaleAlpha` 8-bit,
  `Grayscale` 4/2/1 bits, `Rgba` 16-bit.
- [x] 6.2 Tests de decoder para oversized y corrupt PNG.
- [x] 6.3 Test de round-trip "el asset persiste a través de un
  reinicio y se sirve por su `asset_ref` original".
- [x] 6.4 Test de "el garbage collector no borra un asset legacy
  que una fila sigue referenciando".
- [x] 6.5 Tests del helper `diagnose` para cada variante del enum.
- [x] 6.6 Test de "el `AssetDiagnostic` nunca contiene rutas
  absolutas, bytes ni el `asset_ref`".

## 7. Tests de regresión Rust / core — colector seguro

- [x] 7.1 `failing_image_reference_query_keeps_every_image_asset_on_disk`.
- [x] 7.2 `failing_rich_text_reference_query_keeps_every_rich_asset_on_disk`.
- [x] 7.3 `failing_reference_query_reports_metadata_only_diagnostics`.
- [x] 7.4 `successful_query_with_referenced_assets_keeps_every_file`.
- [x] 7.5 `successful_query_with_no_references_removes_only_orphans`.
- [x] 7.6 `shared_asset_survives_every_pass_including_failing_query`.
- [x] 7.7 `apply_retention_with_a_failing_reference_query_keeps_every_asset`.
- [x] 7.8 `startup_retention_pass_with_a_failing_query_keeps_every_asset`.
- [x] 7.9 `shutdown_retention_pass_with_a_failing_query_keeps_every_asset`.
- [x] 7.10 `fresh_capture_survives_a_previous_failed_collection_pass`.
- [x] 7.11 `asset_collection_outcome_serialises_with_snake_case_fields`.
- [x] 7.12 `asset_collection_outcome_default_reports_no_diagnostics`.

## 8. Tests de regresión Rust / core — aislamiento de assets (regresión 11:20:44)

- [x] 8.1 `bootstrap_at_without_platform_adapters_is_rejected`.
- [x] 8.2 `bootstrap_with_database_without_platform_adapters_is_rejected`.
- [x] 8.3 `isolated_harness_keeps_asset_root_inside_the_tempdir`.
- [x] 8.4 `delete_entry_does_not_touch_external_directories`.
- [x] 8.5 `clear_non_favorites_does_not_touch_external_directories`.
- [x] 8.6 `apply_retention_does_not_touch_external_directories`.
- [x] 8.7 `collect_unreferenced_can_reap_a_sentinel_inside_the_tempdir`.
- [x] 8.8 `a_sentinel_outside_the_context_is_never_modified`.
- [x] 8.9 `temp_database_with_zero_references_never_deletes_real_assets`.
- [x] 8.10 `audit_no_test_root_points_at_real_clipvault_home`.

## 9. Tests de regresión frontend

- [x] 9.1 Tests de `legacyImageAssets.test.ts` que cubren RGBA8,
  indexed, grayscale 8-bit, grayscale + alpha 8-bit, RGBA 16-bit.
- [x] 9.2 Test del token guard: una resolución stale nunca
  sobrescribe el estado de una card recién comprometida.
- [x] 9.3 Test de "el resolver reutiliza el mismo blob URL tras un
  remount para un asset legacy".
- [x] 9.4 Test de "el bridge devuelve los bytes correctos sin
  inspección ni parsing".

## 10. Verificación

- [x] 10.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 10.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 10.3 Ejecutar `cargo test --workspace`.
- [x] 10.4 Ejecutar `npm run check`.
- [x] 10.5 Ejecutar `npm run build`.
- [x] 10.6 Ejecutar `npm test`.
- [x] 10.7 Ejecutar `openspec validate --changes --strict` y
  revisar el diff.
- [x] 10.8 Auditar las llamadas a `unlink` / `remove_file` durante
  los tests y confirmar que ninguna apunta a
  `~/.clipvault/assets/clipboard`. El test
  `audit_no_test_root_points_at_real_clipvault_home` es la red
  de seguridad estática.

## 11. Verificación manual

- [ ] 11.1 **No ejecutar clear history, delete ni retention hasta
  confirmar que el colector es seguro.** La verificación manual
  se hace después de los tests automatizados.
- [ ] 11.2 Capturar una imagen nueva y confirmar que aparece un
  PNG en `~/.clipvault/assets/clipboard/`.
- [ ] 11.3 Cerrar la aplicación.
- [ ] 11.4 Volver a abrirla y recompilar con:
  ```text
  cd /Users/diegoadducilagreca/Documents/ClipVault/app/tauri
  cargo clean -p clipvault-app
  cargo tauri dev
  ```
- [ ] 11.5 Repetir el ciclo dos o tres veces y confirmar que el
  PNG no desaparece.
- [ ] 11.6 Confirmar que la nueva imagen sigue visible después de
  reiniciar.
- [ ] 11.7 Simular una falla de consulta (renombrando la tabla
  `clipboard_entries` desde una sesión `sqlite3` con la app
  cerrada) y confirmar que ningún PNG es eliminado al volver a
  arrancar.
- [ ] 11.8 Documentar explícitamente qué assets faltan en el
  entorno del usuario (los ya borrados por el bug) y cuáles
  siguen presentes (los nuevos, que el fix protege).
- [ ] 11.9 Confirmar que, tras el guard, una ejecución del test
  suite sigue sin tocar `~/.clipvault` (revisión manual del
  filesystem o strace puntual si es necesario).

La verificación manual debe completarse antes de archivar el
cambio. La validación manual de `platform-permission-guidance`
permanece separada y no debe marcarse como completada por este
cambio.

## 12. Corrección visual del icono de favoritos (chincheta)

> **Alcance:** este bloque NO pertenece al fix del colector de
> assets; es una corrección visual aislada del icono de favoritos
> de la `HistoryCard`. Se documenta aquí porque la verificación
> manual (sección 9) y los chequeos automatizados (sección 8)
> todavía no se habían ejecutado, y la revisión visual del icono
> acompañaba al refresh del rail. La corrección es
> presentación-only: no toca el bridge, `EntryRecord`, SQLite, los
> comandos Tauri, la lógica de favoritos, tags, colecciones,
> imágenes, búsqueda ni el drag-and-drop.

### 10.a Causa raíz de la corrección

La revisión manual del icono implementado en una iteración anterior
confirmó que la composición `<circle>` + `<line>` + `<polygon>`
(cabeza circular pequeña en el cuadrante superior derecho, línea
larga diagonal hacia abajo-izquierda, triángulo diminuto en la
punta) **se leía como una llave / lupa** y no como una chincheta
clásica de pizarrón. La proporción entre el diámetro de la cabeza
(~8 unidades), la longitud del cuerpo (~8.5 unidades) y el tamaño
del triángulo (~2.5 unidades) reproducía exactamente el contorno
de una lupa: cabeza = lente, línea = mango, triángulo = punta del
mango. La corrección reemplaza esa composición por **un único
`<path>` que traza la silueta completa** (cabeza ancha y redondeada
+ cuello corto + cuerpo afilado + punta claramente visible), y
luego lo rota 45° alrededor del centro del viewport.

### 10.b Implementación

- [x] 10.1 Sustituir la composición `<circle>` + `<line>` +
  `<polygon>` (que se leía como lupa / llave) por una chincheta
  inclinada ~45° trazada por **un único `<path>`** dentro de un
  grupo `<g transform="rotate(45 12 12)">`:
  - `app/tauri/frontend/src/HistoryCard.svelte` declara las dos
    variantes (`history-card-pin-filled` /
    `history-card-pin-outline`) con `viewBox="0 0 24 24"` y
    `width="18" height="18"`.
  - El `<path>` se dibuja verticalmente (cabeza en `y≈2-11`, punta
    en `y≈22`) y el grupo `rotate(45 12 12)` lo inclina 45° en
    sentido horario para que la cabeza ancha termine en el
    cuadrante superior derecho y la punta en el inferior
    izquierdo.
  - Las coordenadas del `d` permanecen dentro del viewport
    `0..24` (`minX ≥ 3`, `maxX ≤ 21`, `minY ≥ 1`, `maxY ≤ 22`).
  - **No queda ningún `<circle>`, `<line>`, `<polygon>` ni
    `<polyline>`** dentro del SVG del pin: el invariante lo
    verifican los tests `diagonalChincheta.test.ts` y
    `desktopDndCardVisualCorrections.test.ts`.
- [x] 10.2 Mantener el invariante de no-glifo de
  `desktop-dnd-card-visual-corrections`: la chincheta sigue sin
  ser una estrella, marcador, corazón, alfiler vertical, emoji o
  glifo textual. La cobertura explícita en
  `diagonalChincheta.test.ts` recorre los caracteres Unicode
  prohibidos (`★`, `☆`, `⭐`, `🌟`, `✦`, `✧`, `❋`, `✱`, `♥`,
  `❤`, `♦`, `♣`, `♠`, `►`, `▾`, `📌`, `📍`).
- [x] 10.3 Estado no fijado: silueta outline (`fill="none"` +
  `stroke="currentColor"` sobre el `<path>`) en tono
  gris/lavender `#AAB4C8`. La transición visual outline → filled
  usa exactamente el mismo `d`; sólo cambian `fill` / `stroke`.
- [x] 10.4 Estado fijado: silueta rellena
  (`fill="currentColor"` + `stroke="currentColor"` con
  `stroke-width="0.6"` para que el borde del path no produzca un
  contorno doble cuando se combina con el `fill`) en amarillo
  `#F5C542`.
- [x] 10.5 Fondo del botón: se mantiene `#1F2937` (oscuro) en
  ambos estados para que el relleno amarillo siga siendo visible;
  el override de `aria-pressed="true"` sólo cambia `color`, no
  `background`.
- [x] 10.6 Conservar `aria-pressed={entry.is_pinned}`,
  `aria-label`, `title`, `focus-visible`, `data-pinned`,
  `data-testid` (`history-card-pin`, `history-card-pin-filled`,
  `history-card-pin-outline`) y el callback `handlePinClick` →
  `onTogglePin`. **No se agregan listeners ni handlers nuevos:**
  el click sigue invocando exactamente una vez
  `onTogglePin(entry)`.
- [x] 10.7 Ajustar el botón para alojar el SVG de 18×18 sin
  desbordar el rail. La regla compartida
  `.card-actions :global(.pin), .card-actions :global(.menu-trigger)`
  ahora declara `min-height: 1.65rem` y `box-sizing: border-box`
  para que el botón del pin y el del menú mantegan la misma
  altura visible cuando el SVG es más grande que el line-box del
  texto. El padding horizontal (0.55rem) y el fondo (`#1F2937`)
  se conservan.
- [x] 10.8 Pin/unpin sigue siendo presentación-only: no modifica
  `asset_ref`, `mime_type`, `payload_width`, `payload_height`,
  `content_size`, `created_at`, `tags`, `collections` ni el
  `entryOrganizationHydration`. El cambio de estado sigue
  propagándose por `applyPinUpdate` →
  `{ ...existing, is_pinned }`, sin reventar el snapshot.
- [x] 10.9 Tests actualizados:
  - `app/tauri/frontend/tests/diagonalChincheta.test.ts` cubre:
    - viewBox `0 0 24 24` y `width="18"`/`height="18"`
      compartidos entre las dos variantes;
    - **ningún** `<circle>`, `<line>`, `<polygon>` ni
      `<polyline>` dentro del SVG del pin (verificación
      negativa: la lupa/llave nunca vuelve);
    - presencia del grupo `<g transform="rotate(45 12 12)">`;
    - `<path>` único con `d` no vacío, coordenadas dentro del
      viewport, `minY < 8` (cabeza en la mitad superior del
      frame vertical) y `maxY > 18` (punta en la mitad
      inferior);
    - outline: `fill="none"` + `stroke="currentColor"` en el
      `<path>`;
    - filled: `fill="currentColor"` + `stroke="currentColor"`
      sin `fill="none"` en todo el SVG;
    - CSS: fondo `#1F2937` constante, color base `#AAB4C8`,
      color pinned `#F5C542`, focus-visible y disabled;
    - preservación de `aria-pressed`, `aria-label`, `title`,
      `data-testid`, `data-pinned` y `handlePinClick`;
    - ausencia de estrellas, corazones, marcadores, alfileres
      verticales, glifos Unicode o emojis.
  - `app/tauri/frontend/tests/desktopDndCardVisualCorrections.test.ts`
    deja de exigir `<circle>` + `<line>` + `<polygon>` y ahora
    exige `<path>` único + `<g transform="rotate(45 12 12)">`,
    anclando el match por `data-testid` para no sangrar a
    otros SVG del componente.

## 11. Verificación visual manual del icono

> **No** se ejecuta hasta que las tareas 8.1–8.7 hayan pasado.
> La validación manual de `platform-permission-guidance`
> permanece separada y no debe marcarse como completada por este
> cambio.

- [ ] 11.1 Abrir una card no favorita y confirmar que aparece la
  chincheta contorneada gris/lavanda **diagonal** (cabeza en el
  cuadrante superior derecho, punta en el inferior izquierdo) y
  que ya no se lee como lupa / llave.
- [ ] 11.2 Pulsar el botón y confirmar que aparece la misma
  chincheta rellena amarilla (silueta idéntica, sólo cambia el
  relleno).
- [ ] 11.3 Reiniciar la aplicación y confirmar que el icono
  refleja el estado persistido.
- [ ] 11.4 Repetir la verificación en una card de texto y en una
  card de imagen.
- [ ] 11.5 Confirmar que las imágenes persistidas siguen
  visibles tras el reinicio y que el icono no las oculta.
- [ ] 11.6 Confirmar que los tags, las colecciones y el menú
  contextual no cambian.
- [ ] 11.7 Confirmar que el botón del pin y el del menú
  mantienen la misma altura visible (sin desalineación por el
  nuevo `min-height: 1.65rem`).
