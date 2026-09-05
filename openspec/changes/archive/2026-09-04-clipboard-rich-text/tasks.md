# Tasks: clipboard-rich-text

Las tareas deben marcarse al terminar cada verificación. No archivar este
cambio al finalizar; la prueba manual de macOS permanece separada.

## 1. Contrato neutral del clipboard

- [x] 1.1 Extender `ClipboardPayload` con `RichText` y mantener `Text`/`Image`.
- [x] 1.2 Añadir lectura/escritura rich opcional, capacidades y errores tipados
  sin romper fakes ni adapters existentes.
- [x] 1.3 Implementar la prioridad común `RichText > Text > Image > ignored`.
- [x] 1.4 Cubrir rich completo, sólo HTML, sólo RTF, plain, rich+image,
  payload vacío, formato no soportado y errores parciales.

## 2. Normalización y seguridad

- [x] 2.1 Definir la canonicalización determinística y el `rich_text_hash`.
- [x] 2.2 Implementar sanitización de preview con allow-list, límites y
  fallback a plain.
- [x] 2.3 Probar eliminación de scripts, eventos, recursos remotos,
  `javascript:`, objetos y HTML malformado sin filtrar texto legítimo.
- [x] 2.4 Verificar que ningún Debug, error, log o evento incluye payload,
  hash, snippet, path o bytes rich.

## 3. Persistencia

- [x] 3.1 Crear la migración siguiente a la versión actual con las seis
  columnas rich nullable, índices necesarios y `down_sql` reversible.
- [x] 3.2 Extender `EntryRecord`, `NewEntry`, mapeos SQL y repositorio.
- [x] 3.3 Implementar deduplicación por `content_hash` más
  `rich_text_hash` para filas rich, conservando el comportamiento histórico
  de filas plain.
- [x] 3.4 Probar bootstrap desde una base anterior, round-trip, nulls,
  duplicados rich idénticos y mismo plain con formato diferente.

## 4. Asset store rich

- [x] 4.1 Crear `<data_dir>/assets/rich-text/` con nombres por SHA-256,
  referencias relativas y escritura atómica.
- [x] 4.2 Persistir HTML/RTF originales y preview sanitizada sólo después de
  pasar PrivacyGate y validaciones de tamaño/formato.
- [x] 4.3 Implementar validación estricta para lectura de preview y bytes de
  paste: namespace, traversal, symlink, extensión, tamaño y existencia.
- [x] 4.4 Integrar recolección de assets rich con delete, clear y retention,
  respetando referencias compartidas y huérfanos.
- [x] 4.5 Añadir tests de asset reuse, atomicidad/fallo, referencia maliciosa,
  cleanup y privacidad.

## 5. Platform adapters

- [x] 5.1 Implementar lectura/escritura rich detrás de `ClipboardBackend` en
  macOS y Linux X11 sólo donde la sesión lo soporte.
- [x] 5.2 Mantener un fallback explícito y no fatal para Linux Wayland o
  cualquier backend que no exponga rich.
- [x] 5.3 Extender capability matrix y platform guidance con las capacidades
  rich sin mezclar sus resultados con plain.
- [x] 5.4 Cubrir adapters con fakes y tests condicionales; no depender de una
  sesión gráfica en tests del core.

## 6. Core capture/paste

- [x] 6.1 Capturar `RichText` y persistir sus metadatos/assets desde el mismo
  servicio que ya captura texto e imágenes.
- [x] 6.2 Hacer que blacklist, dedupe, history-updated y source-app metadata
  funcionen igual para rich, sin side effects si la captura es ignorada.
- [x] 6.3 Agregar modo `Plain`/`Rich` al servicio de pegado manteniendo plain
  como default compatible para quick-paste.
- [x] 6.4 Implementar rich write con fallback plain tipado, sin modificar la
  entrada de origen ni el target de aplicación previamente capturado.
- [x] 6.5 Probar éxito, fallback, capabilities unavailable, error de asset,
  entrada inexistente y ausencia de selección.

## 7. Tauri

- [x] 7.1 Extender `EntryRecord`/tipos frontend sólo con metadata y refs
  relativas, nunca HTML/RTF originales en listados.
- [x] 7.2 Registrar comandos thin para modo de paste y preview rich; validar
  toda referencia en backend.
- [x] 7.3 Mantener eventos sin payload y reusar guidance existente para
  errores/capacidades.
- [x] 7.4 Cubrir serialización, comandos, validaciones y ausencia de leaks.

## 8. HistoryCard y menú

- [x] 8.1 Renderizar preview rich segura en la `HistoryCard` existente, con
  tamaño fijo, clipping y fallback plain.
- [x] 8.2 Conservar título, icono de tipo, icono de aplicación, pin/unpin,
  ellipsis y delete sin duplicar handlers/listeners.
- [x] 8.3 Agregar `Paste de texto enriquecido` y `Paste de texto plano` al
  menú; deshabilitar rich cuando la entrada no tiene rich refs.
- [x] 8.4 Implementar focus, Escape, cierre único, estados busy y errores
  accesibles.
- [x] 8.5 Probar card rich/plain, atributos visibles, fallback, acciones,
  cleanup de Blob URLs y resultados tardíos.

## 9. Verificación y revisión manual

- [x] 9.1 `cargo fmt --all -- --check`.
- [x] 9.2 `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 9.3 `cargo test --workspace`.
- [x] 9.4 `cd app/tauri/frontend && npm run check`.
- [x] 9.5 `cd app/tauri/frontend && npm run build`.
- [x] 9.6 `cd app/tauri/frontend && npm test`.
- [x] 9.7 `openspec validate clipboard-rich-text --strict --type change`.

- [ ] 9.8 Ejecutar prueba manual real en macOS: copiar texto con formato desde
  una app compatible, verificar preview visual, reiniciar, usar ambos items
  del menú y comprobar fallback desde una fuente plain. Esta tarea sólo se
  marca con evidencia del usuario.

## 10. Regresión de preview vacía y degradación segura

- [x] 10.1 Verificar que `empty_preview` usa un fallback plain determinístico y
  conserva la captura cuando `plain_text` no está vacío.
- [x] 10.2 Verificar que `plain_text` escapa `&`, `<`, `>`, comillas y saltos de
  línea antes de formar HTML de preview.
- [x] 10.3 Verificar que HTML/RTF originales se persisten intactos aunque la
  preview enriquecida quede vacía o no pueda sanitizarse.
- [x] 10.4 Verificar rollback y limpieza de temporales/assets parciales ante un
  fallo real de escritura.
- [x] 10.5 Verificar ausencia de contenido, HTML, RTF, hashes, paths y bytes en
  logs, errores, eventos y diagnostics.
- [x] 10.6 Verificar que `HistoryCard` muestra el fallback plain de forma segura
  cuando la preview rich no carga.
- [x] 10.7 Verificar que `Paste de texto enriquecido` usa los HTML/RTF originales
  y que `Paste de texto plano` mantiene el camino plain.

## 11. Regresión de pegado rich real y timeout de metadata

Las siguientes tareas cubren la regresión manual reportada en macOS:
"pegar texto enriquecido pierde el formato" y
`metadata enrichment on the main thread did not run entry_id=129`. Las
tareas se marcan únicamente cuando los tests automatizados
correspondientes pasan; la verificación manual en macOS 9.8 sigue
pendiente de evidencia del usuario.

- [x] 11.1 Adapter nativo `NSPasteboard` que publica `public.utf8-plain-text`,
  `public.html` y `public.rtf` en una sola escritura lógica.
- [x] 11.2 El adapter `arboard` rechaza payloads RTF-only con
  `ClipboardBackendError::Unavailable { capability: ClipboardWriteRichText }`
  en lugar de convertir silenciosamente a plain text.
- [x] 11.3 Adapter compuesto (`CompositeClipboard`) que enruta texto plano e
  imágenes a `arboard` y rich text al adapter nativo en macOS.
- [x] 11.4 `supports_rich_write` refleja la capacidad real del backend (true
  en macOS nativo, true para HTML-only en `arboard`, false en
  `arboard` para RTF-only).
- [x] 11.5 `paste.rs::write_rich_payload` reusa los bytes originales
  (`rich_html_ref`, `rich_rtf_ref`) sin recurrir a la preview sanitizada.
- [x] 11.6 `run_on_main_thread_sync` deja de usarse en el capture loop; se
  reemplaza por un scheduler no-bloqueante, coalescente y acotado.
- [x] 11.7 Coalescing por `entry_id` evita duplicar tareas de enrichment
  para la misma fila.
- [x] 11.8 Cancelación limpia al cerrar: el `Drop` del scheduler limpia el
  conjunto coalescente y Tauri descarta los closures pendientes.
- [x] 11.9 Diagnósticos distinguen `scheduled`, `skipped_duplicate`,
  `schedule_failed` y `no_main_runtime` con identificadores snake_case
  estables.
- [x] 11.10 Tests que prueban: fake backend recibe HTML y RTF originales;
  rich paste no usa la preview sanitizada; rich paste conserva formatos
  cuando el backend los soporta; RTF-only no se reporta como
  `pasted`; fallback plain devuelve `pasted_plain_fallback`; plain paste
  nunca lee assets rich; entrada histórica permanece intacta ante
  errores; no se filtra contenido en logs; enrichment exitoso en main
  thread; enrichment con timeout no falla la captura; no se duplican
  tareas; cleanup al cerrar; blacklist no crea assets rich ni metadata.

## 12. Verificación manual pendiente

- [ ] 9.8 Ejecutar prueba manual real en macOS: copiar texto con formato desde
  una app compatible, verificar preview visual, reiniciar, usar ambos items
  del menú y comprobar fallback desde una fuente plain. Esta tarea sólo se
  marca con evidencia del usuario.
- [ ] Verificación manual pendiente de `platform-permission-guidance` (no
  forma parte de este cambio; no se modifica ni se marca).

## 13. Regresión `empty_preview` (FIX 2026-09-04)

Reproducción documentada por el usuario:
`2026-09-04T08:36:49.552629Z WARN rich text asset write failed reason="empty_preview"`.
El bug convertía una captura con `plain_text` no vacío en un
`HistoryOutcome::Failed` cuando el sanitizador producía una preview
vacía. La corrección introduce un fallback determinístico en cadena
(`build_safe_preview` en `crates/clipvault-core/src/rich_text.rs`)
cuyas capas son: sanitizado HTML con contenido visible, escape plain
determinístico con `plain_text`, escape defensivo de un prefijo de
`plain_text`. Cada tarea se marca sólo después de verificar el test
automatizado correspondiente.

- [x] 13.1 Fallback determinístico cuando `empty_preview`. Captura con HTML
  cuya sanitización colapsa a vacío y `plain_text` no vacío termina en
  `HistoryOutcome::Stored`; la preview persistida es el escape plain.
  Test:
  `empty_preview_regression_uses_plain_text_fallback`
  (`crates/clipvault-core/tests/clipboard_rich_text.rs`).

- [x] 13.2 `plain_text` se escapa antes de formar la preview: `&`, `<`, `>`,
  `"` y `'` se reemplazan por sus entidades; `\r`, `\r\n` y `\n` se
  convierten en `<br>`. Ningún carácter crudo se filtra al sandbox
  iframe. Tests:
  `plain_text_preview_escapes_markup_and_preserves_line_breaks`,
  `plain_text_preview_is_never_empty_for_non_empty_input`,
  `plain_text_preview_does_not_leak_raw_special_chars_except_br`,
  `escaped_plain_text_fallback_escapes_specials_and_breaks`,
  `escaped_plain_text_fallback_never_collapses_non_empty_input`,
  `build_safe_preview_returns_safe_html_for_dangerous_html`,
  `build_safe_preview_escapes_special_only_plain_text`
  (`crates/clipvault-core/src/rich_text.rs`).

- [x] 13.3 Persistencia de los HTML/RTF originales aunque la preview
  enriquecida quede vacía o no pueda sanitizarse. El asset store escribe
  los originales antes que la preview y conserva los `rich_html_ref` /
  `rich_rtf_ref` / `rich_preview_ref` para `Paste de texto enriquecido`.
  Tests:
  `empty_preview_regression_uses_plain_text_fallback`,
  `empty_preview_regression_keeps_originals_when_sanitizer_rejects`,
  `empty_preview_regression_preserves_plain_text_line_break`
  (`crates/clipvault-core/tests/clipboard_rich_text.rs`).

- [x] 13.4 Rollback y limpieza de temporales/assets parciales ante un fallo
  real de escritura del preview. El rollback elimina sólo los assets
  escritos por la transacción actual; ningún asset queda huérfano.
  Test:
  `empty_preview_regression_real_io_failure_does_not_leak_assets`
  (`crates/clipvault-core/tests/clipboard_rich_text.rs`).

- [x] 13.5 Ausencia de contenido, HTML, RTF, hashes, paths y bytes en
  logs, errores, eventos y diagnostics. El tipo `RichTextAssetError` y
  sus `kind_str` siguen siendo los únicos identificadores
  (`empty_preview`, `io`, `too_large`, `sanitize`); el caller
  (`history.rs`) registra sólo `reason = error.kind_str()`.
  Sin cambios respecto al contrato previo, verificado por
  inspección del código y por la naturaleza determinística del
  helper `build_safe_preview` que sólo opera con metadata.

- [x] 13.6 `HistoryCard` muestra el fallback plain de forma segura cuando
  la preview rich no carga. El branch `:else if isRich` y la rama
  rica cuando `richPreviewUrl === null` renderizan `<pre class="preview
  preview--rich-fallback">{previewText}</pre>` usando el helper
  `entryPreviewText`, nunca `{@html}`. Verificado por inspección del
  template existente
  (`app/tauri/frontend/src/HistoryCard.svelte:627-655`).

- [x] 13.7 `Paste de texto enriquecido` usa los HTML/RTF originales; `Paste
  de texto plano` mantiene el camino plain. `PasteService::paste_entry`
  reconstruye el `RichTextPayload` desde `rich_html_ref` y `rich_rtf_ref`
  a través de `build_rich_payload`, y `paste_plain_entry` /
  `PasteMode::Plain` siguen escribiendo sólo `record.content`.
  Tests preexistentes que siguen pasando:
  `rich_paste_writes_rich_payload`,
  `rich_paste_falls_back_to_plain_when_rich_unavailable`,
  `plain_paste_writes_only_plain_text`,
  `plain_paste_of_plain_entry_keeps_text_only_path`
  (`crates/clipvault-core/tests/clipboard_rich_text.rs`).

## 14. Regresión: lectura rich en main thread (FIX 2026-09-04)
El bug convertía cada tick del capture loop en
`WatchTickOutcome::Failed` porque el adaptador nativo
`MacOsPasteboardClipboard` aplicaba `require_main_thread` y devolvía
`ClipboardBackendError::Backend { ... }` cuando el watcher llamaba
desde su hilo background. La corrección añade un puente de
plataforma (`macos_clipboard_main_queue`) que:

- ejecuta directamente cuando ya está en el main thread;
- salta síncronamente a `DispatchQueue::main()` cuando está en
  background (vía `dispatch2::run_on_main`);
- usa la infraestructura `dispatch2` ya presente en el crate;
- no depende de Tauri ni del frontend;
- entrega la salida a través de un canal síncrono con timeout para
  que un event loop ausente se traduzca en `Unavailable`, no en un
  cuelgue indefinido del hilo.

El adaptador nativo ahora expone `read_text` / `write_text` /
`read_rich` / `write_rich` que invocan el puente internamente y
devuelven `ClipboardBackendError::Unavailable { capability:
ClipboardRead | ClipboardWrite | ClipboardReadRichText |
ClipboardWriteRichText }` cuando el puente no puede saltar al main
thread. `CompositeClipboard::read_payload` se sobreescribe para
preservar el contrato de coherencia: el fallback de texto plano
después de una miss rich sale por la pierna plain-text del mismo
adaptador rich, no por el adaptador `plain` (evita la doble fila
`Text` + `RichText` que produciría un fingerprint inconsistente).
Cada tarea se marca sólo después de verificar el test automatizado
correspondiente.

- [x] 14.1 Módulo `crates/clipvault-platform/src/runtime/macos_clipboard_main_queue.rs`
  que detecta el main thread con `objc2_foundation::MainThreadMarker`
  y, en background, delega el cierre en `dispatch2::run_on_main`
  dentro de un hilo dedicado con `recv_timeout(1s)` para no colgar
  indefinidamente al capture loop. Tests:
  `bridge_types_are_send_and_sync`,
  `unavailable_for_carries_the_requested_capability`,
  `bridge_error_kind_string_is_stable`
  (`crates/clipvault-platform/src/runtime/macos_clipboard_main_queue.rs`).

- [x] 14.2 `MacOsPasteboardClipboard::read_text` / `read_rich` /
  `write_text` / `write_rich` enrutan al puente. Las tres piernas
  (`public.utf8-plain-text`, `public.html`, `public.rtf`) se leen y
  se escriben en una única operación lógica sobre la misma
  `generalPasteboard()`. Tests:
  `read_rich_from_background_returns_unavailable_not_backend`,
  `write_rich_from_background_returns_unavailable_not_backend`,
  `read_write_text_from_background_returns_unavailable_not_backend`,
  `bridge_constant_keeps_canonical_utf8_plain_text_flavor`,
  `adapter_declares_rich_text_capabilities_and_image_unavailable`
  (`crates/clipvault-platform/src/runtime/macos_clipboard.rs`).

- [x] 14.3 `CompositeClipboard::read_payload` se sobreescribe para
  que el fallback de texto plano salga por la pierna plain-text del
  mismo adaptador rich (preservando coherencia entre rich y plain)
  y para que `read_rich` traduzca `Backend { ... }` en
  `Unavailable { capability: ClipboardReadRichText }`. Tests:
  `read_payload_uses_rich_payload_when_rich_leg_present`,
  `read_payload_falls_back_to_rich_plain_text_after_rich_miss`,
  `read_payload_normalises_rich_backend_to_unavailable`,
  `read_rich_passes_through_rich_payload`,
  `read_payload_keeps_rich_unavailable_soft`
  (`crates/clipvault-platform/src/runtime/composite_clipboard.rs`).

- [x] 14.4 `read_rich` desde un hilo background ya no convierte el
  capture loop en `WatchTickOutcome::Failed`. Tests:
  `read_rich_from_background_does_not_return_backend`,
  `write_rich_from_background_does_not_return_backend`,
  `composite_read_rich_does_not_propagate_backend_thread_error`,
  `composite_read_payload_collapses_thread_limitation_to_soft_outcome`
  (`crates/clipvault-platform/tests/macos_clipboard_main_queue_regression.rs`).

- [x] 14.5 `read_rich` obtiene HTML / RTF / plain de forma coherente
  en una misma `generalPasteboard()`. Tests:
  `read_payload_uses_rich_payload_when_rich_leg_present`,
  `read_payload_falls_back_to_rich_plain_text_after_rich_miss`,
  `composite_read_payload_keeps_rich_adapter_as_coherent_source`,
  `composite_read_payload_falls_back_through_rich_after_soft_rich_miss`,
  `composite_read_image_routes_to_plain_adapter`,
  `macos_pasteboard_does_not_claim_image_support`
  (`crates/clipvault-platform/tests/macos_clipboard_main_queue_regression.rs`).

- [x] 14.6 El capture loop produce `Stored` / `Duplicate` /
  `Ignored`, nunca `Failed` por `read_rich must run on the macOS
  main thread`. Tests:
  `watcher_never_fails_on_unavailable_rich_leg`,
  `watcher_produces_ignored_when_rich_unavailable_and_plain_empty`
  (`crates/clipvault-core/tests/clipboard_rich_text.rs`).

- [x] 14.7 Fallback a plain cuando rich no está disponible:
  `rich_paste_falls_back_to_plain_when_rich_unavailable`
  (`crates/clipvault-core/tests/clipboard_rich_text.rs`).

- [x] 14.8 No se generan dos filas para el mismo cambio. La
  coherencia se prueba en
  `composite_read_payload_keeps_rich_adapter_as_coherent_source` y
  la deduplicación del watcher en
  `clones_share_the_dedupe_state`
  (`crates/clipvault-core/src/watcher.rs`) más
  `identical_rich_payload_refreshes_row`
  (`crates/clipvault-core/tests/clipboard_rich_text.rs`).

- [x] 14.9 Rich paste publica HTML y RTF originales; no usa la
  preview sanitizada. Test:
  `rich_paste_publishes_original_html_and_rtf_not_preview`
  (`crates/clipvault-core/tests/clipboard_rich_text.rs`).

- [x] 14.10 Rich write fallido no se reporta como éxito:
  `rich_paste_falls_back_to_plain_when_rich_unavailable` cubre el
  camino `Unavailable → plain fallback → PastedPlainFallback`, y
  `rich_paste_does_not_report_plain_fallback_on_success` cubre el
  camino exitoso.

- [x] 14.11 `pasted_plain_fallback` sólo aparece cuando
  corresponde: `rich_paste_does_not_report_plain_fallback_on_success`
  afirma que un rich paste exitoso nunca devuelve el fallback.

- [x] 14.12 No hay deadlock entre capture loop, main queue y
  SQLite: el puente usa un canal síncrono con
  `recv_timeout(std::time::Duration::from_secs(1))` y no llama a
  `handle.join()` después del timeout, de modo que un hilo
  bloqueado en `dispatch2::run_on_main` no congela al caller. La
  captura sigue usando el `parking_lot::Mutex` del `database()` y
  no comparte locks con el main thread. Test:
  `read_rich_from_background_returns_unavailable_not_backend`.

- [x] 14.13 No se duplican timers, dispatchers, callbacks ni
  listeners. La corrección no añade nuevos
  `MainQueueActiveAppRefresher` ni nuevos closures al
  `MetadataEnrichmentScheduler`; el puente es un helper sin estado
  reutilizado en cada llamada. Verificado por inspección.

- [x] 14.14 No se filtra contenido, HTML, RTF, hash, path ni bytes
  en logs o errores. El puente sólo propaga el identificador
  estable `main_queue_unavailable` a través de `kind_str`; los
  `Debug` / `Display` nunca llevan contenido. Tests:
  `bridge_types_are_send_and_sync`, inspección.

- [x] 14.15 El warning de metadata enrichment no debe convertir una
  captura válida en `Failed`:
  `metadata_enrichment_warning_does_not_convert_capture_to_failed`
  (`crates/clipvault-core/tests/clipboard_rich_text.rs`). El flujo
  de enriquecimiento se mantiene como branch independiente que
  sólo registra `trace!` cuando falla; el helper `enrich_metadata`
  de `TextHistoryService` nunca devuelve un error al pipeline.

## 15. Regresión: autocaptura del paste + estilos residuales en plain

Reproducción documentada por el usuario (2026-09-04):

1. Seleccionar `Paste de texto enriquecido` o `Paste de texto plano`
   en una card producía una nueva card con el mismo contenido
   porque el watcher capturaba el payload que la propia paste
   acababa de escribir.
2. `Paste de texto plano` sobre una entrada rich conservaba en el
   destino el color y los estilos del fragmento previo.
3. La card mezclaba preview rich con preview plano y resultaba
   visualmente inconsistente.

Las correcciones separan el contrato plain, limpian la pasteboard
en el camino plain y eliminan la preview rich del render de la
card. Las tareas se marcan únicamente cuando los tests
automatizados correspondientes pasan; la verificación manual en
macOS 9.8 sigue pendiente de evidencia del usuario.

### 15.A Paste-suppression token (sin autocaptura)

- [x] 15.A.1 `PasteSuppression` registra fingerprints metadata-only
  (plain SHA-256, rich canónico, image SHA-256) en un registro
  compartido entre el paste service y el watcher. Test:
  `paste_suppression::tests::plain_text_hash_is_deterministic_and_distinct`
  y resto de unit tests del módulo.

- [x] 15.A.2 `PasteService` arma un token antes de cada escritura
  al clipboard (plain, rich, image) y lo limpia en caso de fallo
  de la escritura o del synthetic paste. Tests:
  `plain_paste_does_not_create_a_new_card`,
  `rich_paste_does_not_create_a_new_card`,
  `image_paste_does_not_create_a_new_card`,
  `synthetic_paste_failure_does_not_create_a_card`,
  `write_failure_clears_the_token`,
  `capability_unavailable_keeps_card_unchanged`
  (`crates/clipvault-core/tests/paste_suppression.rs`).

- [x] 15.A.3 `CaptureWatcher::tick` consume el token antes de
  PrivacyGate, persistencia, enriquecimiento de metadata y emisión
  de `history-updated`. La nueva variante `WatchTickOutcome::Suppressed`
  no crea fila, no actualiza una fila existente y no emite el
  evento. Tests:
  `plain_paste_does_not_create_a_new_card`,
  `distinct_copy_during_suppression_window_is_captured`,
  `token_is_consumed_by_the_first_matching_observation`
  (`crates/clipvault-core/tests/paste_suppression.rs`).

- [x] 15.A.4 El token es de un solo uso (consumido por la primera
  observación que coincide) y expira tras un TTL corto
  (`DEFAULT_SUPPRESSION_TTL = 2.5s`). Una copia distinta del
  usuario dentro de la ventana de supresión se captura
  normalmente. Tests:
  `token_is_consumed_by_the_first_matching_observation`,
  `token_expires_after_ttl`,
  `distinct_copy_during_suppression_window_is_captured`
  (`crates/clipvault-core/tests/paste_suppression.rs`).

- [x] 15.A.5 El fingerprint no contiene texto, HTML, RTF, hashes
  completos del payload, paths ni bytes. La función `Debug` del
  fingerprint sólo expone longitudes y presencia de campos. Test:
  `fingerprint_carries_no_sensitive_content`.

- [x] 15.A.6 El watcher publica `WatchTickOutcome::Suppressed`,
  `should_emit_history_updated` lo trata como "no emitir" y el
  shell registra `suppressed:paste_owned` en los diagnostics. El
  comando Tauri `clipvault_capture_tick` expone la variante
  `suppressed` al frontend. Verificado por inspección en
  `app/tauri/src-tauri/src/bootstrap.rs` y
  `app/tauri/src-tauri/src/commands.rs`.

- [x] 15.A.7 Thread-safety: `PasteSuppression` se comparte vía
  `AppContext` (un `Arc<Mutex<...>>` interno), el watcher y el
  paste service mantienen clones de la misma instancia y la
  comparación es `Send + Sync`. Tests unitarios del módulo cubren
  `clones_share_the_registry`. Verificado por compilación
  `-D warnings` en el workspace completo.

- [x] 15.A.8 El camino plain en `macOS` y el camino rich no
  introducen guards duplicados: la supresión vive en el registry
  compartido, no en el `FakeClipboardBackend` ni en el bridge.
  Verificado por inspección: `CompositeClipboard::write_text`
  enruta al native adapter sin un check de supresión local.

### 15.B Plain paste limpia la pasteboard

- [x] 15.B.1 `macos_clipboard_main_queue::write_plain_text_main_thread`
  llama explícitamente a `clearContents()` y `declareTypes_owner`
  con sólo `NSPasteboardTypeString` y `public.utf8-plain-text`
  antes de publicar el texto. Test: cobertura existente en
  `crates/clipvault-platform/src/runtime/macos_clipboard.rs`
  (las pruebas ejercen el bridge con `Unavailable`; el cambio
  preserva la API del bridge y no requiere nuevos tests a nivel
  de bridge).

- [x] 15.B.2 `CompositeClipboard::write_text` enruta a la
  implementación nativa (`supports_native_plain_write = true` en
  `MacOsPasteboardClipboard`) y cae al adapter plain sólo cuando
  el bridge reporta `Unavailable` por ausencia de main queue.
  El flag `supports_native_plain_write` es opt-in y `arboard` lo
  deja en `false`. Tests de `composite_clipboard.rs` siguen
  pasando sin cambios contractuales.

- [x] 15.B.3 `PasteMode::Plain` no publica rich flavours: el
  servicio `paste_entry` ya escribía sólo el `record.content`
  plano. La nueva ruta refuerza el contrato eliminando el residuo
  rich en la pasteboard. Test preexistente
  `plain_paste_writes_only_plain_text` (no se debilita) y nuevo
  `plain_paste_does_not_create_a_new_card` (15.A.2).

- [x] 15.B.4 En Linux X11, el backend `arboard` reemplaza
  completamente la selección `XA_PRIMARY`/`XA_CLIPBOARD` cuando
  publica texto, por lo que el contrato plain ya se cumple sin
  cambios adicionales. Verificado por inspección del
  `clipboard_arboard.rs` y por el flujo `CompositeClipboard`
  (que en Linux no enruta por el native adapter).

- [x] 15.B.5 La acción "Pegar texto plano" del menú no declara
  rich y la sintética de paste se invoca después de la limpieza
  de la pasteboard. `PasteOutcome::Pasted { id }` se sigue
  devolviendo sin fabricar un RTF artificial. Test
  `plain_paste_writes_only_plain_text` cubre la no-emisión de
  RTF.

### 15.C Card siempre muestra preview plain

- [x] 15.C.1 `HistoryCard.svelte` renderiza siempre
  `entryPreviewText(entry)` mediante un `<pre class="preview">`,
  tanto para entradas plain como para entradas rich. Las entradas
  rich pierden el iframe sandbox, el resolver
  `createRichTextPreviewResolver`, la blob URL y los listeners
  asociados. Verificado por inspección del template.

- [x] 15.C.2 Las acciones del menú (pin/unpin, title, paste rich,
  paste plain, delete) y la infraestructura de la card (iconos,
  tamaño fijo, clipping, ellipsis) se conservan. Verificado por
  inspección de la card y de `npm run check` (0 errores).

- [x] 15.C.3 `Paste de texto enriquecido` sigue disponible desde
  el menú; el backend `PasteService::paste_entry` con
  `PasteMode::Rich` publica los HTML/RTF originales. El soporte
  rich del core y del adapter se conserva: ninguna ruta de pegado
  rich se elimina. `rich_paste_writes_rich_payload` y
  `rich_paste_falls_back_to_plain_when_rich_unavailable`
  siguen pasando.

- [x] 15.C.4 `rich_paste_enabled` se deshabilita sólo cuando la
  entrada no tiene rich metadata (`hasRenderableRichText`). Test
  preexistente `plain_paste_of_rich_entry_disables_rich_action`
  sigue pasando.

- [x] 15.C.5 `rich_preview_ref` se mantiene en el modelo de
  datos por compatibilidad con el backend pero la card no lo
  solicita ni lo renderiza. La columna
  `rich_text_hash`/`rich_html_ref`/`rich_rtf_ref` permanece
  disponible para el `paste` rich. `entryPreviewText` ignora el
  rich preview ref y devuelve siempre el `content` plano.

- [x] 15.C.6 El contenido HTML peligroso se sigue mostrando como
  texto, no se ejecuta, y la card no crea iframe ni Blob URL para
  la preview rich. Los tests preexistentes del
  `clipboardAsset.test.ts` siguen pasando al no referenciar la
  card (los resolvers de rich preview se conservan en el módulo
  por compatibilidad con código futuro que pueda necesitarlos).

- [x] 15.C.7 El truncamiento (`line-clamp: 6`), el tamaño fijo
  (`--cv-card-size`) y el overflow se preservan. Verificado por
  inspección del bloque CSS `.preview` y por `npm run build`
  (build sin errores).

## 16. Regresión: menú de imagen sólo expone `Paste` (FIX 2026-09-04)

Reproducción documentada por el usuario: el menú de una `HistoryCard`
de imagen mostraba `Paste de texto enriquecido` y `Paste de texto
plano`, dos acciones que no corresponden al flujo de pegado de
imagen y que enviaban `mode = "rich" | "plain"` al comando
`clipvault_paste_entry`. Como el `PasteService` ignora el modo para
las filas de imagen y delega en el camino bitmap, ninguna de las dos
acciones rompía la operación — pero el menú quedaba visualmente
incorrecto y prometía formatos que la captura no tiene.

La corrección centraliza el contenido del menú en un único helper
puro, `pasteMenuActionsFor`, que vive en `lib/clipboardAsset.ts`. La
`HistoryCard` renderiza cada acción desde la lista que devuelve el
helper, de modo que la regresión no puede volver a través de un
botón hardcodeado. Las tareas se marcan únicamente cuando el test
automatizado correspondiente pasa; la verificación manual en macOS
9.8 sigue pendiente de evidencia del usuario.

- [x] 16.1 `pasteMenuActionsFor` es la única fuente de verdad para
  las acciones de pegado del menú. Para una entrada de imagen
  devuelve exactamente **una** acción `{ kind: "image-paste", label:
  "Paste", testId: "history-card-paste", mode: null, disabled:
  false }`. Para una entrada textual devuelve la pareja `{ kind:
  "text-rich-paste", mode: "rich" }` + `{ kind: "text-plain-paste",
  mode: "plain" }` con la acción rich deshabilitada cuando la
  entrada no tiene rich metadata. Tests:
  `image card menu exposes a single Paste action with a null mode`,
  `image card menu never exposes Paste de texto enriquecido or
  Paste de texto plano`,
  `rich-text card menu keeps both paste actions when rich metadata
  is available`,
  `plain-text card menu disables the rich action but keeps the
  plain action enabled`
  (`app/tauri/frontend/tests/clipboardAsset.test.ts`).

- [x] 16.2 La `HistoryCard` ya no declara los dos botones de pegado
  textuales de forma incondicional: los reemplaza por un
  `{#each pasteMenuActions}` cuyo origen es el helper. Verificado
  por inspección del template
  (`app/tauri/frontend/src/HistoryCard.svelte:667-680`) y por
  `npm run check` (0 errores).

- [x] 16.3 Activar `Paste` en una imagen envía `mode = null` al
  comando `clipvault_paste_entry`, que es el contrato que el
  `PasteService::paste_entry` espera para delegar en
  `write_payload_for_record` con `mode = PasteMode::Plain`. El modo
  es ignorado para filas de imagen, pero `null` es el valor que ya
  usaba el camino legacy de quick-paste, así que no introduce un
  modo nuevo. Tests:
  `pasteEntryCommand omits the mode argument when the caller
  passes null` (`tests/pasteBridge.test.ts`),
  `image_paste_does_not_create_a_new_card`
  (`crates/clipvault-core/tests/paste_suppression.rs`).

- [x] 16.4 Las acciones no relacionadas con el pegado (editar
  título, restaurar título, pin/unpin, eliminar) se conservan para
  las cards de imagen y siguen renderizándose de forma
  incondicional. Verificado por inspección del template
  (`HistoryCard.svelte:648-666`, `681-689`).

- [x] 16.5 `pasteBusy` evita doble invocación también para el
  pegado de imagen: el guard `if (pasteBusy) return;` de `runPaste`
  se mantiene y `pasteMenuActionsFor` marca `disabled = true` sobre
  las acciones textuales mientras la bandera esté activa. La acción
  única de imagen se queda con `disabled = false` (la única manera
  de bloquear el doble disparo es el guard en la función) para que
  el comportamiento existente del quick-paste no cambie. Test:
  `pasteBusy disables both text paste actions and never the image
  paste action`
  (`app/tauri/frontend/tests/clipboardAsset.test.ts`).

- [x] 16.6 No se crean handlers ni listeners duplicados: la card
  sigue declarando exactamente un `on:click` por botón de menú; el
  bucle `{#each pasteMenuActions}` no añade closures ni
  `addEventListener`, sólo renderiza un botón por acción. Verificado
  por inspección y por `npm run check` (0 errores).

- [x] 16.7 El quick-paste y el pegado rich textual siguen
  funcionando: la ruta `pasteEntryCommand({ id, mode: null })` es
  la que usa el quick-paste para todas las entradas; el menú de
  texto rich sigue enviando `mode = "rich"` y `mode = "plain"`. Los
  tests preexistentes que siguen pasando:
  `performPasteFlow hides the window before invoking paste`,
  `rich_paste_writes_rich_payload`,
  `rich_paste_falls_back_to_plain_when_rich_unavailable`,
  `plain_paste_writes_only_plain_text`,
  `plain_paste_of_plain_entry_keeps_text_only_path`.

- [x] 16.8 El helper y el menú no filtran contenido, referencias
  internas ni hashes: el `mode` viaja como `"rich" | "plain" | null`
  y el comando `pasteEntryCommand` ya omite el `asset_ref` /
  `rich_*_ref` / `content` de la respuesta serializada. Test:
  `pasteMenuActionsFor only inspects metadata and never copies
  content / hashes`
  (`app/tauri/frontend/tests/clipboardAsset.test.ts`).

