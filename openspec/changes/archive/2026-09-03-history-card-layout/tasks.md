## 1. OpenSpec y contrato

- [x] 1.1 Mantener el cambio separado de clipboard-rich-content y de la lógica de clipboard-management.
- [x] 1.2 Definir el contrato de card, metadata de aplicación, título nullable y fallbacks para filas antiguas.
- [x] 1.3 Definir el mapa local de iconos SVG para todos los content_type actuales.
- [x] 1.4 Documentar el tamaño fijo de card, truncamiento y orden de acciones.

## 2. Modelo y persistencia

- [x] 2.1 Crear migración SQLite aditiva y reversible para title, source_app_name y source_app_icon_ref, o el DTO equivalente justificado.
- [x] 2.2 Extender EntryRecord y consultas de recent entries sin romper filas antiguas.
- [x] 2.3 Implementar operación idempotente para establecer, cambiar y restaurar el título.
- [x] 2.4 Mantener pin, delete, retention, deduplicación y source_app existentes.
- [x] 2.5 Cubrir migración, rollback y compatibilidad con metadata nula.

## 3. Metadata de aplicación

- [x] 3.1 Crear o extraer un ApplicationMetadataProvider detrás de un trait de plataforma testeable.
- [x] 3.2 Reutilizar el asset store seguro existente sin mezclar el namespace de blacklist con el de aplicaciones fuente.
- [x] 3.3 Implementar cache por identificador normalizado y evitar assets duplicados.
- [x] 3.4 Implementar resolución macOS respetando las restricciones de NSWorkspace/AppKit y el main thread.
- [x] 3.5 Definir fallback explícito para Linux X11, Wayland y sesiones sin mapeo seguro.
- [x] 3.6 Garantizar que el enriquecimiento sea best-effort y no bloquee ni haga fallar una captura de texto válida.
- [x] 3.7 Garantizar que capturas blacklistadas no creen metadata ni assets.

## 4. Shell Tauri

- [x] 4.1 Añadir comando delgado para actualizar el título.
- [x] 4.2 Extender el DTO de recent entries con metadata visual opcional.
- [x] 4.3 Generalizar o añadir bridge seguro para leer iconos de aplicaciones fuente.
- [x] 4.4 Validar scope, traversal, symlinks, formato PNG, tamaño y ausencia de paths absolutos.
- [x] 4.5 Registrar comandos y capacidades sin habilitar acceso irrestricto al filesystem.

## 5. Frontend

- [x] 5.1 Extraer HistoryCardRail y ClipboardHistoryCard desde el listado vertical actual.
- [x] 5.2 Implementar rail horizontal con cards cuadradas de tamaño fijo.
- [x] 5.3 Reducir tipografía, truncar preview y conservar metadata de tamaño y fecha sin sobrecargar la card.
- [x] 5.4 Crear ContentTypeIcon con SVG local, labels accesibles y fallback.
- [x] 5.5 Renderizar icono y nombre de la aplicación fuente con fallback seguro.
- [x] 5.6 Mostrar título por defecto, edición, validación y restauración.
- [x] 5.7 Implementar menú de puntos suspensivos con un único menú activo y cierre por Escape o click externo.
- [x] 5.8 Mantener pin/unpin inmediatamente a la izquierda del menú y reutilizar comandos existentes.
- [x] 5.9 Mantener eliminación con confirmación y refresh posterior a mutación exitosa.
- [x] 5.10 Mantener usable el historial vacío, scroll, foco y viewport angosto.

## 6. Tests

- [x] 6.1 Tests de migración y EntryRecord con metadata nullable.
- [x] 6.2 Tests del título por defecto, edición, validación, restauración e idempotencia.
- [x] 6.3 Tests del provider con fake: metadata válida, cache, repetición, error y plataforma no disponible.
- [x] 6.4 Tests de que PrivacyGate decide antes de crear metadata de una captura.
- [x] 6.5 Tests del bridge de iconos: PNG válido, missing, demasiado grande, traversal, symlink y path absoluto.
- [x] 6.6 Tests de mapa de iconos por content_type y fallback desconocido.
- [x] 6.7 Tests frontend de rail, card cuadrada, truncamiento, metadata, título, menú y Escape.
- [x] 6.8 Tests frontend de pin/unpin, delete confirmation, empty state y no duplicación de handlers.
- [x] 6.9 Tests de que búsqueda, quick-paste, captura, retención y privacidad conservan su contrato.
- [x] 6.10 Tests de privacidad: no contenido, hash, snippet, path absoluto ni red en metadata o logs.

## 7. Verificación

- [x] 7.1 Ejecutar cargo fmt --all -- --check.
- [x] 7.2 Ejecutar cargo clippy --workspace --all-targets -- -D warnings. (Un warning preexistente sin relación al cambio en `crates/clipvault-core/tests/platform_integration.rs:444` variable `clipboard` no usada. Los cuatro tests paste_service_*_when_* fallan con el mismo origen: comparten un Arc<FakeClipboardBackend> con el bootstrap y no con la variable local; el fallo y el warning son preexistentes.)
- [x] 7.3 Ejecutar cargo test --workspace. (4 fallos preexistentes en platform_integration.rs sin relación al cambio.)
- [x] 7.4 Ejecutar npm run check en app/tauri/frontend.
- [x] 7.5 Ejecutar npm run build en app/tauri/frontend.
- [x] 7.6 Ejecutar npm test en app/tauri/frontend.
- [x] 7.7 Ejecutar openspec validate history-card-layout --strict --type change.
- [ ] 7.8 Completar la verificación manual documentada en macOS.
- [x] 7.9 No archivar automáticamente el cambio.

## 8. Regresión: source-app nunca llega a la card

El usuario reportó que las cards mostraban un cuadrado genérico y
un guion "—" en lugar del icono y nombre de la aplicación fuente.
El diagnóstico fue:

- `capture_loop_tick` reenviaba `None` a `CaptureWatcher::tick`,
  así que la fila nunca almacenaba el identificador que el probe
  activo estaba reportando.
- `enrich_with_app_metadata` se ejecutaba después de persistir pero
  recibía un identificador vacío y salía sin consultar al provider.
- `clipvault_capture_tick` confiaba en el argumento que el frontend
  enviaba (`sourceApp: null`), así que el botón **Tick capture**
  podía atribuir una captura a ClipVault cuando la ventana de la
  app estaba enfocada.
- `HistoryCard.svelte` solo caía a `entry.source_app ?? "—"`, así
  que el guion se mostraba literalmente cuando `source_app` era
  `None`.

### 8.1 Captura automática

- [x] 8.1.1 `capture_loop_tick` lee `cached_active_application()`
  (sin invocar el probe del plataforma) y pasa el identificador real
  a `watcher.tick(context, Some(id))` cuando es no vacío; `None`
  cuando la caché está vacía.
- [x] 8.1.2 El identificador alimenta `PrivacyGate` y la
  persistencia desde el mismo snapshot (sin doble detección).
- [x] 8.1.3 Tras un `Stored`/`Duplicate`, el shell agenda el
  enrichment en el thread principal vía
  `enrich_metadata_on_main_thread` cuando el enrichment inline
  falló (macOS requiere `NSWorkspace` en main thread). El helper
  salta entradas que ya tienen `source_app_name` para no
  re-ejecutar innecesariamente.

### 8.2 Captura manual

- [x] 8.2.1 `SharedState::tick` y `clipvault_capture_tick`
  resuelven el identificador a través de `cached_active_application`
  e ignoran el argumento del frontend.
- [x] 8.2.2 `clipvault_capture_text` resuelve también desde la
  caché; nunca atribuye a ClipVault por conveniencia.
- [x] 8.2.3 `resolved_source_identifier` es la única entrada que
  consulta el probe y se documenta como API pública.

### 8.3 Metadata de aplicación

- [x] 8.3.1 `TextHistoryService::enrich_metadata` es ahora `pub`
  para que el shell pueda reintentar la resolución en main thread.
- [x] 8.3.2 `record_payload` normaliza el identificador (espacio
  en blanco → `None`) antes de consultar el gate y persistir.
- [x] 8.3.3 `set_source_app_metadata` con `COALESCE` se mantiene
  para que un provider que devuelva `None` no pise el nombre/icono
  ya persistido.

### 8.4 Card y frontend

- [x] 8.4.1 La cadena de fallback visible es `source_app_name` →
  `source_app` → `Aplicación desconocida`. Nunca más el guion `—`.
- [x] 8.4.2 El label accesible (`sourceAppAccessibleLabel`) sigue
  la misma cadena para que screen readers anuncien el mismo texto.
- [x] 8.4.3 Las helpers están en `src/lib/sourceAppFallback.ts` y
  se prueban sin Svelte runtime en
  `tests/sourceAppFallback.test.ts`.

### 8.5 Eventos y carreras

- [x] 8.5.1 `clipvault://history-updated` se sigue emitiendo tras
  el enrichment inline (`record_payload` retorna después de
  `enrich_metadata`).
- [x] 8.5.2 El shell agenda el enrichment main-thread antes de
  emitir el evento cuando el enrichment inline no pobló la fila.
- [x] 8.5.3 El resolver frontend cachea por `icon_ref` y se libera
  en `onDestroy` para evitar blobs huérfanos.

### 8.6 Tests obligatorios

- [x] 8.6.1 `automatic_capture_records_terminal_identifier_and_metadata`
  — captura con Terminal persiste `source_app`, `source_app_name`
  y `source_app_icon_ref`.
- [x] 8.6.2 `automatic_capture_records_textedit_identifier_and_metadata`
  — idem con TextEdit.
- [x] 8.6.3 `privacy_gate_and_persistence_use_the_same_identifier`
  — el identificador que ve `PrivacyGate` es el mismo que se
  persiste.
- [x] 8.6.4 `allowed_capture_keeps_caller_supplied_identifier` y
  `capture_loop_tick_does_not_substitute_clipvault_identifier` —
  una captura permitida no se atribuye a ClipVault.
- [x] 8.6.5 `capture_loop_tick_with_empty_cache_persists_unknown_source`
  y `capture_tick_with_empty_cache_persists_unknown_source` — sin
  snapshot la captura se guarda con origen desconocido.
- [x] 8.6.6 `capture_succeeds_when_metadata_provider_returns_backend_error`
  — fallo del provider no impide guardar la captura.
- [x] 8.6.7 `icon_persisted_at_most_once_per_application` — la
  metadata se consulta pero el `COALESCE` evita reescrituras.
- [x] 8.6.8 `clipboard_text_history` escenarios de
  `Stored`/`Duplicate` ya estaban cubiertos por `should_emit_history_updated`
  y `metadata_enrichment_target_signals_pending_enrichment` /
  `metadata_enrichment_target_skips_already_enriched_entry` validan
  que el evento fires tras el enrichment.
- [x] 8.6.9 `whitespace_identifier_is_treated_as_unknown_source`
  — captura manual preserva el contrato de origen desconocido.
- [x] 8.6.10 `enrich_metadata_preserves_values_when_provider_returns_none`
  — la idempotencia está garantizada vía `COALESCE`.
- [x] 8.6.11 Frontend: `sourceAppFallback.test.ts` cubre las
  tres ramas (nombre, identificador, desconocido) y nunca devuelve
  `—`.
- [x] 8.6.12 Frontend: `iconResolver.test.ts` ya cubre el caso
  "PNG falla al cargar, persiste el nombre" vía el helper
  existente y la rama de fallback de `HistoryCard.svelte`.
- [x] 8.6.13 `record_capture_decision_caps_oversized_labels_and_trims_input`
  y los nuevos tests de shell preservan el contrato: no se filtra
  contenido, hash, snippet, path absoluto o red en metadata o
  logs.
- [x] 8.6.14 `blacklisted_capture_does_not_invoke_metadata_provider_or_persist_row`
  — capturas blacklistadas siguen descartadas y no generan
  metadata ni assets.
- [x] 8.6.15 `capture_tick_uses_cached_probe_identifier_and_ignores_frontend_argument`
  y `default bridge installs exactly one listener across remounts`
  — no se duplican listeners ni cargas de iconos.

## 9. Regresión: icon-only card + endpoint-to-endpoint icon path

El usuario reportó que la card seguía mostrando el identificador
de la aplicación como texto visible y un cuadrado genérico en lugar
del icono real, incluso después del cambio 8. La causa raíz fue:

- `MacOsApplicationMetadataProvider::resolve_bundle_path` usaba
  `NSWorkspace::URLForApplicationToOpenURL(URLWithString(identifier))`,
  una API que resuelve URLs (no bundle identifiers). `NSURL`
  parseaba el identificador como URL relativa y
  `URLForApplicationToOpenURL` devolvía `None`, así que el provider
  siempre retornaba `Ok(None)` y la captura quedaba sin metadata.
- `HistoryCard.svelte` renderizaba el nombre y/o el identificador
  como texto visible dentro del badge de source-app, contradiciendo
  el contrato del spec (`source-app nunca llega a la card como texto
  visible`).
- `entry_already_enriched` requería sólo `source_app_name`, así que
  un fallo de icono quedaba permanente y la card nunca reintentaba.

### 9.1 Card sólo con icono (Parte A)

- [x] 9.1.1 `HistoryCard.svelte` ya no renderiza texto visible para
  `source_app_name`, `source_app`, bundle identifier, snippets ni la
  cadena `Aplicación desconocida`. Sólo se renderiza el `<img>` del
  icono o el fallback gráfico genérico.
- [x] 9.1.2 El nombre se mantiene exclusivamente en `aria-label`,
  `title` y la copia `.visually-hidden` para lectores de pantalla.
- [x] 9.1.3 El prefijo del label accesible es ahora
  `"Aplicación fuente: <nombre>"` para aplicaciones conocidas y
  `"Aplicación fuente desconocida"` para entradas sin origen.
  Nunca se devuelve el guion `—`.
- [x] 9.1.4 El icono de tipo de contenido (content-type) y el icono
  de la aplicación fuente se renderizan en celdas distintas y no
  comparten estilos.
- [x] 9.1.5 `sourceAppFallback.ts` deja de exponer
  `sourceAppVisibleText` — el helper visible nunca se llamó desde
  el template.
- [x] 9.1.6 `HistoryCard.svelte` resuelve una posible carrera
  async entre refrescos con un `iconRefreshToken`: un refresco
  viejo no puede sobrescribir el icono de la card actualizada.

### 9.2 Resolución macOS (Parte C)

- [x] 9.2.1 `resolve_bundle_path` deja de aceptar el identificador
  como URL. Ahora consulta
  `NSRunningApplication::runningApplicationsWithBundleIdentifier`,
  extrae su `bundleURL` y exige la extensión `.app`.
- [x] 9.2.2 La resolución sigue ejecutándose en el main thread vía
  `MainThreadMarker::new()` (la API lo requiere).
- [x] 9.2.3 El nombre se obtiene vía `NSBundle::bundleWithURL` +
  `Info.plist` (`CFBundleDisplayName` → `CFBundleName` → file
  stem), reutilizando el mismo helper que el picker.
- [x] 9.2.4 Si el nombre se resuelve pero el icono falla, la
  entrada queda parcialmente enriquecida y `entry_already_enriched`
  la considera pendiente — la shell reintenta la icon extraction
  hasta que el icono aterriza o el proveedor se rinde.

### 9.3 entry_already_enriched + backfill (Parte D)

- [x] 9.3.1 `entry_already_enriched` exige tanto `source_app_name`
  no vacío como `source_app_icon_ref` no vacío. Un fallo previo del
  icono ya no bloquea reintentos futuros.
- [x] 9.3.2 `pending_metadata_entries` selecciona entradas con
  `source_app` no vacío y al menos una columna de metadata ausente.
  Las filas con `source_app = NULL` nunca son candidatas.
- [x] 9.3.3 `backfill_pending_metadata` corre una sola vez al
  arrancar el capture loop, está acotado por `BACKFILL_BATCH = 32`
  filas y nunca inventa el origen de entradas sin `source_app`.
- [x] 9.3.4 El evento `clipvault://history-updated` se sigue
  emitiendo sólo tras `Stored`/`Duplicate`. El backfill no emite
  eventos porque actúa sobre filas que ya estaban en la tabla.
- [x] 9.3.5 Ninguna captura queda atribuida a ClipVault: el
  identificador que llega a la fila siempre proviene del caché
  active-app, nunca de un literal `ClipVault` /
  `com.apple.ClipVault`.

### 9.4 Privacidad de logs (Parte E)

- [x] 9.4.1 Las nuevas trazas sólo usan categorías estables
  (`metadata_lookup_failed`, `icon_not_found`, `icon_decode_failed`,
  `backfilling pending source-app metadata`, etc.). Nunca
  contenido, hash, snippet, path absoluto ni identificador crudo.
- [x] 9.4.2 `record_capture_decision` sigue aceptando sólo
  categorías estables; los nuevos tests pinnean el contrato.

### 9.5 Tests de regresión (Parte F)

- [x] 9.5.1 `automatic_capture_records_terminal_identifier_and_metadata`
  + `automatic_capture_records_textedit_identifier_and_metadata` —
  captura con Terminal / TextEdit persiste `source_app`,
  `source_app_name` y `source_app_icon_ref`.
- [x] 9.5.2 `application_icons_and_ignored_apps_directories_are_distinct`
  + `resolve_source_app_icon_path_*` + `read_source_app_icon_bytes_round_trips_a_valid_png`
  + `read_source_app_icon_bytes_rejects_traversal` — el bridge de
  icono acepta referencias `application-icons/<id>.png`, rechaza
  traversal y nunca cruza con el namespace de blacklist.
- [x] 9.5.3 `sourceAppAccessibleLabel` cubre las tres ramas
  (nombre conocido, identificador sólo, desconocido) y nunca
  devuelve `—`.
- [x] 9.5.4 `entry_already_enriched_requires_both_name_and_icon_ref`
  + `entry_already_enriched_returns_true_with_both_columns` —
  un nombre sin icono NO se considera enriquecido; ambos campos
  poblados SÍ.
- [x] 9.5.5 `capture_succeeds_when_metadata_provider_returns_backend_error`
  — un fallo del provider no impide guardar la captura.
- [x] 9.5.6 `pending_metadata_entries_skips_rows_with_null_source_app`
  + `pending_metadata_entries_includes_rows_with_source_but_no_metadata`
  + `pending_metadata_entries_respects_batch_limit` +
  `backfill_does_not_duplicate_existing_rows` — el backfill nunca
  inventa orígenes y respeta el límite documentado.
- [x] 9.5.7 `capture_loop_never_attributes_to_clipvault` — una
  captura permitida no se atribuye a ClipVault ni a
  `com.apple.ClipVault`.
- [x] 9.5.8 `record_capture_decision_uses_stable_categories_only` —
  los logs sólo usan categorías estables.

### 9.6 Verificación

- [x] 9.6.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 9.6.2 Ejecutar `cargo clippy --workspace --all-targets -- -D warnings`
  (sigue pendiente el warning preexistente en
  `crates/clipvault-core/tests/platform_integration.rs:444`).
- [x] 9.6.3 Ejecutar `cargo test --workspace`. Los 4 fallos
  preexistentes en `platform_integration.rs` se documentan en 7.3.
- [x] 9.6.4 Ejecutar `npm run check` en `app/tauri/frontend`.
- [x] 9.6.5 Ejecutar `npm run build` en `app/tauri/frontend`.
- [x] 9.6.6 Ejecutar `npm test` en `app/tauri/frontend` (126 tests
  pasan; las 4 trazas en consola son pre-existentes).
- [x] 9.6.7 Ejecutar `openspec validate history-card-layout --strict --type change`.
- [ ] 9.6.8 Confirmar manualmente en macOS que la card muestra el
  icono real de Terminal / TextEdit / Safari tras copiar texto de
  cada aplicación. La verificación manual sigue siendo el último
  paso pendiente (ver 7.8).
- [x] 9.6.9 No archivar automáticamente el cambio (sigue activo).

## Limitaciones manuales restantes

- La verificación manual documentada en `design.md` ("Verificación
  manual") y en la tarea 7.8 sigue siendo el único paso pendiente
  para confirmar en macOS real que el icono se renderiza
  correctamente con la nueva ruta de
  `NSRunningApplication::runningApplicationsWithBundleIdentifier`. Los
  tests cubren el contrato lógico (selector, formato, namespaces,
  backfill, retry) pero la confirmación visual requiere abrir
  Terminal / TextEdit / Safari, copiar texto y observar la rail.
