## Por qué

La investigación previa concluyó que las imágenes antiguas dejaban
de verse por el decoder PNG, pero la verificación directa del
entorno en producción reveló dos regresiones distintas:

1. **Las imágenes nuevas no podían persistir** porque el decoder
   PNG del bridge rechazaba cualquier `(ColorType, BitDepth)` que
   no fuese `Rgb`/`Eight` o `Rgba`/`Eight`. Una captura tomada por
   una build externa o por una build previa de ClipVault quedaba
   sin bytes servibles.
2. **`~/.clipvault/assets/clipboard/` está vacío** a pesar de que
   SQLite conserva cuatro filas con `content_type = "image"`,
   `asset_ref`, `content_size`, `mime_type`, `payload_width` y
   `payload_height`. Las nuevas capturas funcionan porque el PNG se
   escribe después del arranque; las antiguas no pueden mostrarse
   porque sus PNG ya no están físicamente disponibles.

El tamaño de la card sólo prueba que la metadata permanece en
SQLite; no prueba que el archivo exista.

### Causa raíz del barrido de assets

La regresión más grave fue capturada por una auditoría de
filesystem a las 11:20:44: el binario de tests
`crates/clipvault-core/tests/organization.rs` ejecutó cuatro
`unlink` contra `~/.clipvault/assets/clipboard/*.png` mientras
operaba sobre una base SQLite temporal. El binario estaba usando
un `AppBootstrap::bootstrap_at(tempdir/clipvault.db)` y el
constructor no inyectaba `PlatformAdapters`, por lo que
`AppBootstrap::finish()` recurría a `DefaultPlatform::detect()` y
resolvía `data_dir` a `~/.clipvault`.

El colector de assets, llamado desde `delete_entry`,
`clear_non_favorites` y `apply_retention`, enumeró ese directorio
real. Con cero referencias vivas en la base temporal, el conjunto
vivo era legítimo y estaba vacío, así que el colector borró todos
los PNG que encontró. La causa del barrido no fue el bug del
`Err` que ya está cubierto por este cambio; fue el diseño del
`AppBootstrap`, que permitía construir un contexto cuyo `data_dir`
no tenía ninguna relación con la ruta del SQLite.

### Causa raíz de la pérdida de assets por el bug del collector

El bug está en
`crates/clipvault-core/src/management.rs::collect_unreferenced_assets`.
Cuando `referenced_asset_refs()` falla, el código original
transformaba el `Err` en un `BTreeSet::new()` "vacío por error":

El bug está en
`crates/clipvault-core/src/management.rs::collect_unreferenced_assets`.
Cuando `referenced_asset_refs()` falla, el código original
transformaba el `Err` en un `BTreeSet::new()` "vacío por error":

```rust
match repo.referenced_asset_refs() {
    Ok(refs) => refs,
    Err(error) => {
        warn!(...);
        BTreeSet::new()
    }
};
```

Después de ese match se ejecutaba igualmente
`store.collect_unreferenced(&referenced)`. Eso es inseguro: el
recolector interpreta el conjunto vacío como "nada está
referenciado" y borra todos los archivos que no estén en el
conjunto. Con `Err` no se conoce el conjunto real, así que esa
interpretación es incorrecta: borra archivos que una fila de
SQLite sigue referenciando.

El mismo patrón se replicaba para `referenced_rich_asset_refs()`.

### Causas probables por las que `referenced_asset_refs()` puede fallar

- Bloqueo de SQLite (lock del WAL en una transacción previa).
- Lectura del WAL desde una conexión ocupada por otra operación
  del bootstrap, del startup o del shutdown.
- Migración incompleta entre dos versiones del esquema.
- Conexión ocupada por el `parking_lot::Mutex` que protege la
  `Database` y que retiene otra operación larga.
- Secuencia entre bootstrap, startup y shutdown: el `setup` de
  Tauri llama a `run_retention`, el `RunEvent::ExitRequested`
  también lo llama, y ambos pasan por la misma ruta de borrado.
- Consulta contra una tabla que otra rama todavía no terminó de
  preparar.

### Hipótesis descartadas

- Las imágenes nuevas sí se muestran, lo que demuestra que la
  pipeline de captura, el `ClipboardAssetStore::store_image`, la
  creación del `Blob` URL y el comando Tauri siguen funcionando.
- La fila conserva `content_type`, `asset_ref`, `mime_type`,
  `payload_width` y `payload_height` (verificado manualmente por
  el usuario).
- El decoder PNG ya fue ampliado en el commit anterior para
  aceptar todas las combinaciones legales de `(ColorType,
  BitDepth)`. Aunque ese fix sigue siendo válido y los tests
  legacy siguen pasando, **no recupera los archivos que ya fueron
  borrados del disco**: ningún decoder puede resucitar bytes
  físicos que no existen.
- El bug del decoder era el responsable de las cards en estado
  `thumbnailState = "error"`, pero NO explica que
  `~/.clipvault/assets/clipboard/` esté completamente vacío.

## Qué cambia

- **Protección arquitectónica del bootstrap**: `AppBootstrap`
  deja de detectar el host por defecto. El nuevo error
  `BootstrapError::MissingPlatformAdapters` rechaza cualquier
  llamada a `bootstrap_at`, `bootstrap_default` o
  `bootstrap_with_database` que no haya inyectado
  `PlatformAdapters` (o no haya llamado explícitamente a
  `with_default_platform_adapters` para optar por el
  `DefaultPlatform::detect`). El shell de Tauri sigue siendo el
  único consumidor que puede cruzar ese límite, y lo hace con
  intención explícita. Las dos rutas seguras son:

  1. **Tests**: inyectan `PlatformAdapters` con un `PlatformInfo`
     cuyo `data_dir` vive dentro de un `tempfile::TempDir`. El
     helper compartido
     `clipvault_core::test_support::IsolatedTestHarness`
     construye el bundle y deja `ClipboardAssetStore` /
     `RichTextAssetStore` apuntando exclusivamente a ese tempdir.
  2. **Producción**: el shell llama a
     `with_default_platform_adapters()`, que sigue resolviendo el
     host a través de `DefaultPlatform::detect()` pero ahora
     requiere la llamada explícita. El colector sigue activo.

- Reescribir `HistoryManagementService::collect_unreferenced_assets`
  para separar explícitamente los dos casos:

  1. `Ok(set)` (incluso vacío): la base confirma el conjunto real;
     el colector corre y elimina sólo lo que no esté en `set`.
  2. `Err(error)`: no se conoce el conjunto real; el colector NO
     se ejecuta. No se borra ningún archivo y se reporta
     `assets_removed_count = 0` con `*_reference_query_failed =
     true` y `*_collection_skipped = true`.

- Devolver un nuevo struct `AssetCollectionOutcome` con campos
  metadata-only: `image_reference_query_succeeded`,
  `image_reference_query_failed`, `image_collection_skipped`,
  `rich_reference_query_succeeded`,
  `rich_reference_query_failed`, `rich_collection_skipped`,
  `assets_removed_count`. El struct nunca incluye rutas
  absolutas, `asset_ref`, hashes, snippets ni el contenido del
  portapapeles.
- Mantener la corrección previa del decoder PNG
  (`decode_png` con todas las combinaciones legales de
  `(ColorType, BitDepth)`) y el helper metadata-only
  `ClipboardAssetStore::diagnose`. Los tests de decoder legacy se
  conservan, pero **no declaran resuelto el problema sólo porque
  el decoder acepta formatos antiguos**: ahora sabemos que los
  archivos antiguos ya no están físicamente disponibles y
  requieren una copia de seguridad para recuperarse.
- Mantener `asset_ref` invariante. No se reescriben filas SQLite,
  no se renombran archivos, no se borra PNG al arrancar ni al
  cerrar la aplicación si no existe un conjunto confiable de
  referencias.
- `run_retention` (startup y shutdown) sigue ejecutando el
  colector, pero ahora el colector es seguro ante cualquier fallo
  de SQLite: nunca borra nada si no puede consultar el conjunto
  vivo.

## No objetivos

- **No recuperar los assets ya borrados.** Las imágenes
  antiguas cuyos PNG ya no están en disco sólo pueden volver
  desde una copia de seguridad. El fix impide nuevas pérdidas;
  no restaura bytes que el colector ya eliminó.
- No cambiar la captura de imágenes nuevas: la pipeline de
  normalización sigue produciendo PNG 8-bit RGBA.
- No cambiar la ruta de paste: el pegado sigue requiriendo la
  decodificación del PNG al layout RGBA8 que produce el decoder.
- No relajar los límites de tamaño (16 MiB) ni de dimensiones
  (8192×8192).
- No añadir dependencias nuevas; el cambio sólo reorganiza el
  colector, el guard del bootstrap y el helper de aislamiento.
- No exponer rutas absolutas, hashes ni bytes a través del canal
  de diagnóstico.
- No modificar el comportamiento de favoritos, tags, colecciones,
  búsqueda, drag-and-drop, pegado, ni la captura de texto / HTML /
  rich text.

## Capacidades afectadas

### Capacidades modificadas

- `clipboard-rich-content`: el colector de assets distingue
  `Ok(set)` y `Err(...)` para evitar pérdida de datos; el
  decoder sigue aceptando todas las combinaciones legales de
  `(ColorType, BitDepth)`; el bootstrap exige `PlatformAdapters`
  explícitos y el helper de tests garantiza aislamiento.

## Impacto esperado

- Rust:
  - `crates/clipvault-core/src/bootstrap.rs`: nueva variante
    `BootstrapError::MissingPlatformAdapters`, nuevo método
    `AppBootstrap::with_default_platform_adapters`, rechazo en
    `finish()` cuando no hay adapters inyectados.
  - `crates/clipvault-core/src/test_support.rs`: nuevo módulo
    con `IsolatedTestHarness` y helpers (`isolated_harness`,
    `isolated_harness_with_clock`, `isolated_harness_at`,
    `build_isolated_adapters`, `FixedClock`).
  - `crates/clipvault-core/src/management.rs`: separación de
    `Ok` / `Err`, struct `AssetCollectionOutcome`, serialización
    snake_case.
  - `crates/clipvault-core/tests/asset_isolation.rs`: nueva suite
    de regresión (10 tests) que demuestran el aislamiento.
  - `crates/clipvault-core/tests/organization.rs`,
    `management.rs`, `bootstrap.rs`, `history.rs`,
    `source_app_metadata.rs`, `type_detection.rs`,
    `blacklist_app_picker.rs`, `clipboard_rich_text.rs`,
    `paste_regression.rs`, `paste_suppression.rs`,
    `platform_integration.rs`, `desktop_dnd_card_visual_corrections.rs`,
    `desktop_header_card_dnd.rs`, `privacy_settings.rs`:
    actualizados para inyectar `PlatformAdapters` aislados en
    cada `bootstrap_at` / `bootstrap_with_database`.
- Tauri: `app/tauri/src-tauri/src/bootstrap.rs` mantiene su
  inyección de adapters; ningún cambio funcional. `run_retention`
  se sigue invocando en `setup` y en `RunEvent::ExitRequested`;
  ahora es seguro ante un fallo de SQLite y contra el borrado de
  assets en hosts donde el `data_dir` no coincide con la base.
- DB: sin cambios. Las migraciones existentes ya añadieron las
  columnas `asset_ref`, `mime_type`, `payload_width`,
  `payload_height`, `rich_text_hash`, `rich_html_ref`,
  `rich_rtf_ref` y `rich_preview_ref` en los cambios anteriores.
- Frontend: sin cambios. El contrato `hasRenderableImage`, el
  lifecycle del resolver y el fallback "Imagen no disponible"
  permanecen iguales.
