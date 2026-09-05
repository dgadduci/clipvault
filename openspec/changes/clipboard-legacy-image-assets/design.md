# Diseño: clipboard-legacy-image-assets

## Forma de los assets en disco

El layout en disco no cambia:

```text
<data_dir>/assets/clipboard/<lowercase-sha256>.png
```

`asset_ref` sigue siendo el valor relativo `clipboard/<sha>.png` que
SQLite almacena y que el bridge consume. Ni el decoder ampliado ni
el nuevo colector seguro modifican ni mueven ningún archivo
existente.

## Protección arquitectónica del bootstrap

`AppBootstrap` deja de detectar el host por defecto. Antes del
cambio, `finish()` ejecutaba `DefaultPlatform::detect()` cuando no
se había llamado a `with_platform_adapters`, resolviendo
`data_dir` a `~/.clipvault`. Eso bastó para que
`crates/clipvault-core/tests/organization.rs` (y todas las demás
suites que llamaban a `bootstrap_at(tempdir.db)`) apuntaran el
colector al directorio real del desarrollador: la auditoría de
filesystem a las 11:20:44 observó cuatro `unlink` contra
`~/.clipvault/assets/clipboard/*.png` desde el binario de tests.

La nueva implementación:

1. Añade la variante `BootstrapError::MissingPlatformAdapters`,
   con un mensaje que indica explícitamente que el caller debe
   inyectar `PlatformAdapters` o llamar a
   `with_default_platform_adapters()`.
2. Refactoriza `finish()` para exigir `PlatformAdapters`
   explícitos. Si no se inyectaron, devuelve el nuevo error.
3. Expone `AppBootstrap::with_default_platform_adapters(self) ->
   Result<Self, BootstrapError>` para que el shell de Tauri siga
   pudiendo construir el bundle a partir de la detección del host,
   pero con una intención explícita.

Las dos rutas seguras quedan así:

| Caller                          | Método                                                                 | Resultado                                                                          |
| ------------------------------- | ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Tests                           | `with_platform_adapters(IsolatedTestHarness::build_isolated_adapters)` | `data_dir` y `home_dir` viven dentro de un `tempfile::TempDir`. Colector aislado. |
| Producción (Tauri shell)        | `with_default_platform_adapters()`                                     | `data_dir` y `home_dir` coinciden con la ruta resuelta por `DefaultPlatform::detect`. |

El shell de Tauri (`app/tauri/src-tauri/src/bootstrap.rs`) sigue
funcionando sin cambios porque ya llamaba a `with_platform_adapters`.

## Helper de aislamiento de tests

`crates/clipvault-core/src/test_support.rs` expone el helper
compartido que toda la suite usa a partir de este cambio:

```rust
pub struct IsolatedTestHarness {
    pub dir: tempfile::TempDir,
    pub home_dir: PathBuf,
    pub data_dir: PathBuf,
    pub context: AppContext,
    pub adapters: PlatformAdapters,
    pub asset_store: ClipboardAssetStore,
    pub rich_asset_store: RichTextAssetStore,
}

pub fn isolated_harness() -> (tempfile::TempDir, AppContext);
pub fn isolated_harness_with_clock(clock: Arc<dyn Clock>) -> (tempfile::TempDir, AppContext);
pub fn isolated_harness_at(when: time::OffsetDateTime) -> (tempfile::TempDir, AppContext);
pub fn build_isolated_adapters(home_dir: &Path, data_dir: &Path) -> PlatformAdapters;
pub struct FixedClock { /* ... */ }
impl Clock for FixedClock { /* ... */ }
```

El helper garantiza, por construcción, cuatro invariantes:

1. `home_dir` y `data_dir` viven dentro del `tempfile::TempDir`
   que el harness posee. El `PlatformInfo` reporta esas dos
   rutas, así que `ClipboardAssetStore::root()` y
   `RichTextAssetStore::root()` resuelven al tempdir.
2. Las capacidades se fijan a `Capabilities::ALL_AVAILABLE`, así
   la destructividad de los tests no depende del capability gate.
3. El bundle es 100% fake: `FakeClipboardBackend`,
   `FakeHotkeyManager`, `FakeActiveApplication`,
   `FakePasteController`, `FakeTrayController`,
   `FakeSettingsNavigator`, `NoopApplicationMetadataProvider`.
4. El `TempDir` se mantiene vivo en el harness y se libera al
   drop, así que ningún archivo sobrevive entre tests.

Toda llamada existente a `AppBootstrap::bootstrap_at(tempdir.db)`
se actualiza para pasar por este helper (o por
`build_isolated_adapters` cuando el test construye el bundle a
mano). La auditoría de la suite
(`audit_no_test_root_points_at_real_clipvault_home`) confirma
estáticamente que `data_dir` nunca resuelve al `~/.clipvault`
del host.

## Decodificador ampliado (fix previo)

`decode_png` se reorganiza en dos pasos:

1. Validación de cabecera, tamaño y dimensiones (igual que antes).
2. Decodificación del frame y conversión a un buffer 8-bit RGBA
   a través de un helper `expand_png_frame_to_rgba8` que acepta
   las siguientes combinaciones legales de `(ColorType, BitDepth)`:

   | ColorType       | BitDepth                | Conversión                                       |
   | --------------- | ----------------------- | ------------------------------------------------ |
   | `Rgba`          | `Eight`, `Sixteen`      | canal a canal (16-bit usa el byte alto)          |
   | `Rgb`           | `Eight`, `Sixteen`      | añade alpha `0xFF`                                |
   | `Grayscale`     | `Eight`, `Sixteen`      | replica R=G=B, alpha `0xFF`                       |
   | `Grayscale`     | `One`, `Two`, `Four`    | expande bits empaquetados y escala a 8 bits       |
   | `GrayscaleAlpha`| `Eight`, `Sixteen`      | replica el canal gris en RGB, alpha intacto        |
   | `Indexed`       | `One`, `Two`, `Four`, `Eight` | lookup en `PLTE`, respeta `tRNS` por entrada |

   Cualquier combinación ilegal bajo la especificación PNG — por
   ejemplo RGB de sub-byte depth — se rechaza con `AssetError::NotPng`,
   manteniendo la superficie de error del bridge.

El helper `pixel_count_for_frame` se introduce para calcular el
número de píxeles que cubre el buffer crudo sin asignar memoria
dos veces y para fallar a `NotPng` si el tamaño no encaja con la
geometría anunciada.

Este decoder ampliado se conserva. Aunque ahora sabemos que los
archivos antiguos ya no están físicamente disponibles, el decoder
sigue siendo necesario para que las imágenes futuras escritas en
cualquier formato legal se sirvan correctamente.

## Diagnóstico metadata-only (fix previo)

`ClipboardAssetStore::diagnose` corre la misma pipeline de
validación que `read_bytes` pero devuelve un `AssetDiagnostic` que
no carga bytes al frontend:

```rust
pub enum AssetDiagnosticKind {
    Loaded,
    InvalidReference,
    NotFound,
    WrongDataDir,
    WrongNamespace,
    TooLarge { size: usize },
    InvalidPng { color_type: &'static str, bit_depth: &'static str },
    InvalidDimensions { width: u32, height: u32 },
    Io { reason: String },
}
```

Cada variante expone un `kind_str()` estable (snake_case) que el
frontend puede usar para elegir el copy de fallback sin parsear el
mensaje. La función nunca devuelve:

- la ruta absoluta del archivo;
- el `asset_ref` (la referencia es metadata pero el helper la trata
  como input, no como output);
- los bytes del PNG;
- el hash del contenido.

`WrongDataDir` se distingue de `NotFound` comparando la existencia
del directorio `<data_dir>/assets/clipboard/` antes de devolver
el resultado.

## Colector seguro (fix nuevo)

`HistoryManagementService::collect_unreferenced_assets` deja de
tratar `Ok(empty_set)` y `Err(database_error)` como el mismo caso.
La nueva implementación distingue tres caminos explícitos:

1. `Ok(set)` y `set` no vacío: el colector corre y elimina sólo
   los archivos cuyo nombre no está en `set`. Los assets
   compartidos por varias filas permanecen vivos.
2. `Ok(set)` y `set` vacío: la base confirma que no hay
   referencias. El colector corre y elimina sólo los huérfanos
   legítimos (archivos `.tmp` abandonados por una captura
   interrumpida, archivos sin fila asociada).
3. `Err(...)`: SQLite no pudo producir el conjunto vivo (lock,
   WAL ocupado, migración incompleta, conexión ocupada, etc.). El
   colector **no se ejecuta**. Ningún archivo se borra y se
   reporta `assets_removed_count = 0` con los flags
   `image_reference_query_failed = true`,
   `image_collection_skipped = true` (o sus equivalentes para
   rich text).

La separación entre los casos 2 y 3 es el fix: un `Err` no
degrada al colector a "todo está sin referenciar".

### Diagnóstico metadata-only del colector

La función devuelve un nuevo struct público:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct AssetCollectionOutcome {
    pub image_reference_query_succeeded: bool,
    pub image_reference_query_failed: bool,
    pub image_collection_skipped: bool,
    pub rich_reference_query_succeeded: bool,
    pub rich_reference_query_failed: bool,
    pub rich_collection_skipped: bool,
    pub assets_removed_count: usize,
}
```

El struct es metadata-only. No contiene:

- el `asset_ref` (ni como input ni como output);
- una ruta absoluta, ni siquiera la ruta del `data_dir`;
- bytes, hashes o snippets del portapapeles;
- el contenido del error SQLite (sólo se loguea una cadena
  estable como `image_reference_query_failed`).

Los nombres serializados son snake_case estables y sirven tanto
para `tracing` como para consumidores externos del struct.

### Compatibilidad con el resto de la pipeline

- `delete_entry`, `clear_non_favorites`,
  `clear_unorganized_history` y `apply_retention` siguen
  invocando el colector. Internamente ignoran el retorno
  `AssetCollectionOutcome` para preservar la firma pública de
  esos métodos; el outcome sigue disponible para tests y para
  cualquier caller que quiera observarlo a través de
  `collect_unreferenced_assets` directamente.
- `run_retention` (startup y shutdown) sigue siendo invocado por
  el shell de Tauri sin cambios. Ahora es seguro ante un fallo de
  SQLite: nunca borra nada si la consulta falla.
- La pipeline de captura (`TextHistoryService::record_clipboard_payload`)
  no se modifica; sigue escribiendo los assets antes de tocar la
  base de datos, así que un fallo posterior de SQLite no afecta a
  los bytes ya escritos.

## Compatibilidad hacia atrás

- `asset_ref` no se reescribe nunca. Las imágenes escritas por
  builds anteriores siguen resueltas a través del mismo path
  canónico.
- El `paste` pipeline (`paste.rs::write_image_payload`) consume el
  resultado de `decode_png`, así que aceptar nuevos `(ColorType,
  BitDepth)` significa que las imágenes legacy pueden pegarse
  también — no solo visualizarse.
- La nueva superficie `diagnose` y `AssetCollectionOutcome` son
  aditivas: los callers existentes
  (`clipvault_clipboard_asset`,
  `clipvault_clipboard_asset_for_test`,
  `clipvault_apply_retention`) no cambian. Un cambio posterior
  podrá exponer el outcome a través de un comando Tauri específico
  si el frontend necesita observarlo.

## Tests requeridos

### Rust / core — decoder y diagnóstico (fix previo, conservados)

- `decode_png_reads_legacy_rgba8_assets`
- `decode_png_reads_legacy_rgb8_assets`
- `decode_png_reads_indexed_palette_assets`
- `decode_png_reads_grayscale_8bit_assets`
- `decode_png_reads_grayscale_alpha_8bit_assets`
- `decode_png_reads_grayscale_4bit_assets`
- `decode_png_reads_grayscale_2bit_assets`
- `decode_png_reads_grayscale_1bit_assets`
- `decode_png_rejects_oversized_assets`
- `decode_png_rejects_a_corrupt_png_payload`
- `store_serves_a_legacy_rgba_asset_after_a_restart`
- `store_serves_a_legacy_grayscale_asset_after_a_restart`
- `store_serves_a_legacy_palette_asset_after_a_restart`
- `collector_keeps_a_legacy_grayscale_asset_after_a_delete`
- `diagnostic_returns_loaded_for_a_persisted_rgba_asset`
- `diagnostic_returns_loaded_for_a_legacy_palette_asset`
- `diagnostic_returns_invalid_reference_for_a_foreign_reference`
- `diagnostic_returns_not_found_when_the_file_is_missing`
- `diagnostic_returns_wrong_data_dir_when_the_namespace_is_absent`
- `diagnostic_returns_wrong_namespace_for_a_symlink_inside_the_namespace`
- `diagnostic_returns_too_large_for_an_oversized_asset`
- `diagnostic_returns_invalid_png_for_a_corrupt_payload`
- `diagnostic_never_carries_absolute_paths_or_payload_bytes`

### Rust / core — colector seguro (fix nuevo)

- `failing_image_reference_query_keeps_every_image_asset_on_disk`
- `failing_rich_text_reference_query_keeps_every_rich_asset_on_disk`
- `failing_reference_query_reports_metadata_only_diagnostics`
- `successful_query_with_referenced_assets_keeps_every_file`
- `successful_query_with_no_references_removes_only_orphans`
- `shared_asset_survives_every_pass_including_failing_query`
- `apply_retention_with_a_failing_reference_query_keeps_every_asset`
- `startup_retention_pass_with_a_failing_query_keeps_every_asset`
- `shutdown_retention_pass_with_a_failing_query_keeps_every_asset`
- `fresh_capture_survives_a_previous_failed_collection_pass`
- `asset_collection_outcome_serialises_with_snake_case_fields`
- `asset_collection_outcome_default_reports_no_diagnostics`

### Frontend (fix previo, conservados)

- `legacy RGBA8 image bytes are accepted and rendered as an image/png blob`
- `legacy palette (indexed) PNG bytes are accepted through the resolver`
- `legacy grayscale (8-bit) PNG bytes are accepted through the resolver`
- `legacy grayscale + alpha (8-bit) PNG bytes are accepted through the resolver`
- `legacy 16-bit RGBA PNG bytes are accepted through the resolver`
- `a stale resolution never overwrites a freshly committed legacy card state`
- `backend rejection collapses to the documented fallback state`
- `backend returning null collapses to the documented fallback state`
- `resolver reuses the cached legacy blob URL across remounts after a restart`
- `hasRenderableImage accepts a coherent legacy image row`
- `hasRenderableImage rejects every incoherent image row`
- `clipboardAssetCommand returns the legacy PNG bytes through the bridge`

## Verificación manual

1. **No ejecutar clear history, delete ni retention hasta corregir
   el colector.** Mientras el bug esté presente, cada operación
   destructiva o el paso automático de `run_retention` puede
   borrar los PNG físicos que SQLite sigue referenciando.
2. Capturar una imagen nueva.
3. Confirmar que aparece un PNG en
   `~/.clipvault/assets/clipboard/`.
4. Cerrar la aplicación.
5. Volver a abrirla.
6. Recompilar con:
   ```text
   cd /Users/diegoadducilagreca/Documents/ClipVault/app/tauri
   cargo clean -p clipvault-app
   cargo tauri dev
   ```
7. Repetir el ciclo dos o tres veces.
8. Confirmar que el PNG no desaparece.
9. Confirmar que la nueva imagen sigue visible después de
   reiniciar.
10. Simular una falla de consulta (renombrando
    `clipboard_entries` a `clipboard_entries_damaged` desde una
    sesión `sqlite3` mientras la app está cerrada, o
    deshabilitando temporalmente el directorio de la base) y
    confirmar que ningún PNG es eliminado al volver a arrancar.

La verificación manual debe completarse antes de declarar el
cambio como resuelto. Si los assets ya están perdidos en el
entorno del usuario, documentar explícitamente cuáles faltan y
cuáles siguen presentes; no afirmar que el fix recuperó imágenes
cuyos archivos físicos ya no existen.
