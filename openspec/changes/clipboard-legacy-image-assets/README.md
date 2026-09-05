# clipboard-legacy-image-assets

Cambio OpenSpec para corregir tres regresiones relacionadas con
los PNG persistidos por ClipVault:

1. **El collector borraba PNGs durante el arranque o el cierre**
   porque `collect_unreferenced_assets` colapsaba `Err` y
   `Ok(empty_set)`. La corrección separa ambos casos y devuelve
   un struct metadata-only (`AssetCollectionOutcome`) con los
   flags `reference_query_succeeded` / `reference_query_failed`
   / `*_collection_skipped` / `assets_removed_count`.
2. **El decoder PNG rechazaba toda combinación `(ColorType,
   BitDepth)` distinta de `Rgb`/`Eight` o `Rgba`/`Eight`.** La
   corrección amplió el decoder para aceptar todas las
   combinaciones legales del estándar y mantiene el helper
   metadata-only `ClipboardAssetStore::diagnose`.
3. **`AppBootstrap::finish()` recurría a
   `DefaultPlatform::detect()`** cuando no se inyectaban
   `PlatformAdapters`, así que cualquier test que llamara a
   `bootstrap_at(tempdir.db)` resolvía `data_dir` a
   `~/.clipvault`. La auditoría de filesystem a las 11:20:44
   observó cuatro `unlink` contra
   `~/.clipvault/assets/clipboard/*.png` desde el binario de
   tests `crates/clipvault-core/tests/organization.rs`. La
   corrección:
   - añade la variante `BootstrapError::MissingPlatformAdapters`
     que rechaza la construcción del contexto;
   - expone `AppBootstrap::with_default_platform_adapters()`
     para que el shell de Tauri pueda cruzar el límite con
     intención explícita;
   - introduce `clipvault_core::test_support::IsolatedTestHarness`,
     el helper compartido que toda la suite usa para aislar las
     pruebas dentro de un `tempfile::TempDir` (y nunca
     `~/.clipvault`).

Estado: implementación Rust completa. La suite
`crates/clipvault-core/tests/asset_isolation.rs` documenta el
contrato end-to-end con 10 tests de regresión que demuestran el
aislamiento.

La sección 12 del `tasks.md` registra además una corrección
visual aislada del icono de favoritos de la `HistoryCard`:
la chincheta vertical se reemplaza por una chincheta inclinada
~45° con `viewBox="0 0 24 24"`, fondo de botón oscuro, outline
gris/lavanda cuando no está fijado y relleno amarillo `#F5C542`
cuando está fijado. La corrección preserva `aria-pressed`,
`aria-label`, `title`, `focus-visible`, `data-testid`, el
callback `onTogglePin` y el resto del contrato visual, y no
toca el bridge, `EntryRecord`, SQLite, los comandos Tauri, la
lógica de favoritos, tags, colecciones, imágenes, búsqueda ni
el drag-and-drop.
