# Tareas de implementación

## 1. Diagnóstico de la causa raíz

- [x] 1.1 Auditar la causa raíz real sobre macOS: el flujo
  `NSPasteboard → read_png_main_thread → CompositeClipboard →
  arboard::get_image` falla porque **la metadata (resolución +
  perfil) vive en el leg `public.tiff`, no en el leg
  `public.png`**. Documentar la causa raíz real en `proposal.md`
  y la limitación documentada del verbatim path.
- [x] 1.2 Confirmar que el decoder PNG ampliado del cambio
  `clipboard-legacy-image-assets` no recupera esa metadata:
  sólo acepta cualquier `(ColorType, BitDepth)` legal al leer.
- [x] 1.3 Documentar la decisión de inspeccionar ambas
  representaciones (`public.png` y `public.tiff`) en el bridge
  y reconstruir el PNG cuando `public.png` no trae la
  metadata que el leg `public.tiff` sí expone.
- [x] 1.4 Documentar la limitación intencional: cuando
  `public.png` no trae `pHYs`/`iCCP` y `public.tiff` los
  aporta, ClipVault **no puede persistir el PNG original
  byte-por-byte** porque los bytes del PNG no contienen la
  resolución. El rebuild produce un PNG con los mismos
  píxeles + la metadata inyectada.

## 2. Scanner de chunks PNG (`png_metadata_summary`)

- [x] 2.1 Añadir `PngMetadataSummary` con campos
  `phys: Option<[u8; 9]>`, `icc_profile_chunk: Option<Vec<u8>>`,
  `has_srgb`, `has_gama`, `has_chrm`, `has_text`.
- [x] 2.2 Añadir `png_metadata_summary(bytes: &[u8]) ->
  PngMetadataSummary` que camina chunks ancillares pre-`IDAT`.
- [x] 2.3 Añadir `parse_ppu_triple(payload: &[u8; 9]) ->
  Option<(u32, u32, u8)>` y `phys_to_dpi(...)`.
- [x] 2.4 Exportar el módulo desde `clipvault_platform::lib`.
- [x] 2.5 Tests: `png_metadata_summary_detects_phys_chunk`,
  `png_metadata_summary_reports_text_chunk_presence`,
  `png_metadata_summary_rejects_invalid_signatures`,
  `phys_to_dpi_rounds_at_the_standard_inch`,
  `phys_to_dpi_returns_none_for_unspecified_unit`,
  `png_metadata_summary_helpers_classify_fidelity_paths`.

## 3. Parser de TIFF (`tiff_metadata`)

- [x] 3.1 Crear `crates/clipvault-platform/src/tiff_metadata.rs`.
- [x] 3.2 Implementar `TiffMetadata { x_resolution, y_resolution,
  resolution_unit, icc_profile }` + `TiffResolutionUnit`.
- [x] 3.3 Implementar `parse_tiff_metadata(bytes: &[u8]) ->
  TiffMetadata` que lee tags 282, 283, 296, 34675.
- [x] 3.4 Añadir helpers `dpi_x() / dpi_y() / has_resolution()`.
- [x] 3.5 Hand-rolled (sin dep nueva). Documentar la
  justificación en el rustdoc.
- [x] 3.6 Tests: `parses_144_dpi_x_resolution_in_inches`,
  `parses_centimeter_resolution_into_dpi`,
  `parses_inline_icc_profile_payload`, `rejects_short_buffers`,
  `rejects_invalid_magic_number`, `empty_dpi_when_unit_is_none`.

## 4. Eliminar el fallback silencioso del bridge

- [x] 4.1 Reemplazar `validate_png` por `validate_png ->
  PngValidationOutcome` con `Valid(ValidatedPng)`, `NotPng`,
  `Invalid(PngValidationError)`. (Sin cambios respecto a la
  primera versión.)
- [x] 4.2 Reemplazar `read_png_main_thread -> BridgeResult<Option<ClipboardImage>>`
  por `read_png_main_thread -> BridgeResult<NativePngRead>`
  con cuatro variantes tipadas: `Native { image, metadata }`,
  `TiffOnly { image, metadata }`, `NoPng { metadata }`,
  `InvalidPng(error)`.
- [x] 4.3 Inspeccionar `public.png` Y `public.tiff` en cada
  snapshot. Usar las constantes `NSPasteboardTypePNG` /
  `NSPasteboardTypeTIFF`. Si alguna falla, probar el UTI
  string como fallback.
- [x] 4.4 Construir `PasteboardImageMetadata { png_chunks,
  tiff, resolution_detected, profile_detected }`.
- [x] 4.5 Documentar el enum expandido en el rustdoc.
- [x] 4.6 Exportar `NativePngRead` y `PasteboardImageMetadata`
  desde `lib.rs`.

## 5. Eliminar el fallback silencioso del normalizador

- [x] 5.1 `normalize_image_with_original` deja de caer al
  encoder legacy cuando `original_png` está presente y la
  validación falla. Devuelve
  `Err(AssetError::OriginalPngValidation(error))`. (Sin cambios
  respecto a la primera versión.)
- [x] 5.2 Añadir variante `AssetError::OriginalPngValidation(_)`
  con `kind_str()` estable. (Sin cambios.)
- [x] 5.3 `normalize_image` legacy queda intacto y reservado
  al camino `original_png = None`.

## 6. Cuatro rutas de fidelidad en el persistence layer

- [x] 6.1 Añadir `NormalizedSource` con cuatro variantes:
  `LegacyEncoded`, `NativeVerbatim`, `NativeRebuiltWithMetadata`,
  `TiffMetadataOnly`.
- [x] 6.2 Añadir `NormalisedImage::source()` y
  `was_rebuilt_with_metadata()`.
- [x] 6.3 Implementar `rebuild_for_pasteboard_metadata(bytes,
  width, height, rgba, pasteboard_metadata) ->
  Result<Option<Vec<u8>>, AssetError>` con las reglas
  documentadas en design.md.
- [x] 6.4 Implementar `rebuild_png_with_metadata(width, height,
  rgba, phys_payload, icc_chunk, srgb_intent) ->
  Result<Vec<u8>, AssetError>`.
- [x] 6.5 Implementar `deflate_icc_profile(&[u8]) ->
  Result<Vec<u8>, AssetError>`.
- [x] 6.6 Añadir `IccChunk` struct público.
- [x] 6.7 Tests: `rebuild_helper_emits_pHYs_and_iCCP`,
  `rebuild_helper_without_metadata_emits_canonical_png`.

## 7. Modelo `ClipboardImage` extendido

- [x] 7.1 Añadir campo `original_png: Option<Vec<u8>>` (ya
  existía).
- [x] 7.2 Añadir campo `pasteboard_metadata:
  PasteboardImageMetadata`.
- [x] 7.3 Implementar `ClipboardImage::with_pasteboard_metadata(
  rgba, width, height, original_png, pasteboard_metadata)`.
- [x] 7.4 Exponer accesores `original_png() -> Option<&[u8]>`,
  `into_original_png() -> Option<Vec<u8>>`,
  `has_original_png() -> bool`, `pasteboard_metadata() ->
  &PasteboardImageMetadata`.
- [x] 7.5 Mantener `Debug` metadata-only: nunca imprime pixels,
  ni hash, ni el contenido del PNG original. Reporta
  `has_original_png: bool` y `pasteboard_metadata_kind_str()`.

## 8. Trait `ClipboardBackend`

- [x] 8.1 Mantener `read_image_png()` con default
  `Err(UnsupportedFormat)`.
- [x] 8.2 Mantener capacidad `supports_image_png_read()` con
  default `false`.
- [x] 8.3 Documentar que el composite decide entre
  `read_image_png` y `read_image` según la capacidad y el
  resultado, **pero** no cae al fallback cuando el resultado
  es `Err(InvalidImage(_))`.

## 9. Adapter `ArboardClipboard`

- [x] 9.1 `supports_image_png_read() = false` (sin cambios).
- [x] 9.2 `to_clipboard_image` envuelve el bitmap en
  `ClipboardImage::new` (`original_png = None`).
- [x] 9.3 `read_image` mantiene el comportamiento actual.

## 10. Adapter `MacOsPasteboardClipboard`

- [x] 10.1 `supports_image_png_read() = true`.
- [x] 10.2 `read_image` cambia:
  - delega al nuevo bridge `read_png_main_thread()`;
  - `Ok(Native { image, metadata })` → reconstruye via
    `attach_pasteboard_metadata`;
  - `Ok(NoPng { metadata: _ })` → `Err(UnsupportedFormat)`;
  - `Ok(InvalidPng(error))` →
    `Err(InvalidImage(ImageValidationError::InvalidPng { kind }))`;
  - `Err(DispatchUnavailable)` →
    `Err(Unavailable { capability: ClipboardReadImage })`.
- [x] 10.3 `read_image_png` cambia con el mismo contrato pero
  propagando `Ok(Native)` y `Ok(NoPng)`.
- [x] 10.4 Añadir `attach_pasteboard_metadata` helper que
  reconstruye `ClipboardImage` con metadata adjunta.

## 11. Tipado del error de PNG inválido

- [x] 11.1 `ImageValidationError::InvalidPng { kind:
  &'static str }` ya existente.
- [x] 11.2 `error_kind_strings_are_stable` y tests
  relacionados, sin cambios.

## 12. Composite clipboard

- [x] 12.1 `CompositeClipboard::read_image` cambia:
  - `read_image_png()` → `Ok(Some)` → usar;
  - `Ok(None)` / `Err(Unsupported)` / `Err(Unavailable)` →
    fallback al plain;
  - `Err(InvalidImage)` → **NO** fallback;
  - otros errores → propagar.
- [x] 12.2 `CompositeClipboard::read_payload` con el mismo
  contrato en las dos ramas `read_image_png()`.
- [x] 12.3 Tests del composite con cobertura de `InvalidImage`:
  `read_image_does_not_fall_back_when_native_png_is_invalid` y
  `read_payload_does_not_fall_back_when_native_png_is_invalid`.

## 13. Validación centralizada en el core

- [x] 13.1 `OriginalPngValidationError` (privado) y
  `ValidatedPng` (privado), sin cambios.
- [x] 13.2 `pub fn validate_original_png(bytes, rgba,
  expected_width, expected_height) -> Result<(), ...>` sin
  cambios.
- [x] 13.3 Re-exportar desde `lib.rs`.

## 14. Persistencia en disco

- [x] 14.1 `ClipboardAssetStore::store_image` usa
  `image.png()` (que ahora puede ser el PNG original o el
  rebuildado). El resto se conserva.
- [x] 14.2 `content_size` es la longitud de los bytes
  persistidos (`NormalizedImage::byte_len()`).

## 15. Captura en el core

- [x] 15.1 `TextHistoryService::persist_image` usa
  `normalize_image_with_original`. Sin cambios.
- [x] 15.2 `payload_width` / `payload_height` usan
  `normalized.width()` / `normalized.height()`. Sin cambios.
- [x] 15.3 Tras un `Stored`, emitir
  `ImageCaptureDiagnostic::from_normalized(...)` con la
  fuente mapeada desde `NormalizedSource` a `ImageSource`.

## 16. Diagnóstico metadata-only extendido

- [x] 16.1 Reemplazar `ImageSource` con cuatro variantes:
  `NativePng`, `NativePngPlusMetadata`, `TiffMetadata`,
  `ArboardFallback`.
- [x] 16.2 Añadir `RepresentationSource` (PngChunks / TiffIfd
  / None) y `ColorProfileKind` (Iccp / Srgb / TiffIccProfile
  / None).
- [x] 16.3 Ampliar `ImageCaptureDiagnostic` con cinco
  campos adicionales.
- [x] 16.4 Re-exportar el módulo desde `lib.rs`.
- [x] 16.5 Tests del módulo:
  `image_source_as_str_is_stable`,
  `image_source_display_matches_as_str`,
  `representation_source_as_str_is_stable`,
  `color_profile_kind_as_str_is_stable`,
  `diagnostic_display_contains_only_metadata_fields`,
  `diagnostic_display_marks_none_dpi_when_missing`,
  `diagnostic_from_normalized_reads_metadata_only`,
  `capture_diagnostic_never_leaks_payload_metadata`.

## 17. Pegado

- [x] 17.1 `paste::write_image_payload` mantiene el contrato
  (read_bytes → decode_png → write_image_png).
- [x] 17.2 Confirmar que no se redimensiona ni se re-encoda
  (los bytes persistidos son el PNG original o el rebuildado).
- [x] 17.3 Emitir `ImageCaptureDiagnostic` vía
  `log_image_paste_diagnostic` tras una copia exitosa. Gated
  por `CLIPVAULT_DEBUG_IMAGE_PASTE=1`.

## 18. Privacidad

- [x] 18.1 Ningún log con bytes, hashes, dimensiones exactas
  o rutas absolutas. Los `warn!` que ya existían se conservan
  y siguen emitiendo sólo el `kind_str`.
- [x] 18.2 Los tests usan un PNG sintético con chunks `pHYs`
  (144 ppi = 5669 ppm) y `tEXt` (con un marker fixture-only)
  construido en un `tempfile::TempDir` local. Ningún test
  escribe fixtures con datos sensibles ni reproduce
  contenido real del clipboard.
- [x] 18.3 Confirmar que `Debug` de `ClipboardImage`,
  `NormalizedImage`, `NativePngRead`, `TiffMetadata`,
  `PasteboardImageMetadata`, `ImageCaptureDiagnostic` y
  `PngMetadataSummary` siguen metadata-only.

## 19. Tests

### 19.1 Rust / platform — validador PNG

- [x] 19.1.1 `validate_png_accepts_a_well_formed_png`.
- [x] 19.1.2 `validate_png_returns_not_png_for_non_png_payloads`.
- [x] 19.1.3 `validate_png_returns_not_png_for_empty_payload`.
- [x] 19.1.4 `validate_png_returns_not_png_for_truncated_signatures`.
- [x] 19.1.5 `validate_png_returns_invalid_for_zero_dimensions`.
- [x] 19.1.6 `validate_png_returns_invalid_for_oversized_payloads`.
- [x] 19.1.7 `validate_png_never_leaks_bytes_in_kind_str`.
- [x] 19.1.8 `validate_png_passes_a_reported_size_through_unchanged`.

### 19.2 Rust / platform — scanner de chunks PNG (nuevo)

- [x] 19.2.1 `png_metadata_summary_detects_phys_chunk`.
- [x] 19.2.2 `png_metadata_summary_reports_text_chunk_presence`.
- [x] 19.2.3 `png_metadata_summary_rejects_invalid_signatures`.
- [x] 19.2.4 `phys_to_dpi_rounds_at_the_standard_inch`.
- [x] 19.2.5 `phys_to_dpi_returns_none_for_unspecified_unit`.
- [x] 19.2.6 `png_metadata_summary_helpers_classify_fidelity_paths`.

### 19.3 Rust / platform — parser de TIFF (nuevo)

- [x] 19.3.1 `parses_144_dpi_x_resolution_in_inches`.
- [x] 19.3.2 `parses_centimeter_resolution_into_dpi`.
- [x] 19.3.3 `parses_inline_icc_profile_payload`.
- [x] 19.3.4 `rejects_short_buffers`.
- [x] 19.3.5 `rejects_invalid_magic_number`.
- [x] 19.3.6 `empty_dpi_when_unit_is_none`.

### 19.4 Rust / platform — bridge `read_png_main_thread`

- [x] 19.4.1 `native_png_read_kind_str_is_stable` (extendido a
  cuatro variantes).
- [x] 19.4.2 `native_png_read_kind_str_never_leaks_payload_metadata`.

### 19.5 Rust / platform — composite clipboard

- [x] 19.5.1 `read_image_prefers_native_png_when_available`.
- [x] 19.5.2 `read_image_falls_back_to_plain_when_native_returns_none`.
- [x] 19.5.3 `read_image_falls_back_to_plain_when_native_returns_unsupported`.
- [x] 19.5.4 `read_image_does_not_fall_back_when_native_png_is_invalid`.
- [x] 19.5.5 `read_payload_does_not_fall_back_when_native_png_is_invalid`.

### 19.6 Rust / core — modelo y validación

- [x] 19.6.1 `clipboard_image_with_original_png_keeps_invariants`.
- [x] 19.6.2 `clipboard_image_with_original_png_rejects_stride_mismatch`.
- [x] 19.6.3 `clipboard_image_with_original_png_rejects_empty_bytes`.
- [x] 19.6.4 `clipboard_image_default_has_no_original_png`.
- [x] 19.6.5 `clipboard_image_debug_never_leaks_pixels_or_original_bytes`.
- [x] 19.6.6 `validate_original_png_accepts_a_well_formed_png`.
- [x] 19.6.7 `validate_original_png_rejects_wrong_signature`.
- [x] 19.6.8 `validate_original_png_rejects_oversized_payload`.
- [x] 19.6.9 `validate_original_png_rejects_wrong_dimensions`.
- [x] 19.6.10 `validate_original_png_rejects_rgba_mismatch`.
- [x] 19.6.11 `normalize_image_with_original_preserves_original_bytes`.
- [x] 19.6.12 `normalize_image_with_original_rejects_on_validation_failure`.
- [x] 19.6.13 `normalize_image_with_original_uses_legacy_path_when_no_original`.
- [x] 19.6.14 `decode_png_round_trip_preserves_a_png_with_phys_chunk_dimensions`.

### 19.7 Rust / core — captura y persistencia

- [x] 19.7.1 `capture_persists_original_png_bytes_verbatim`.
- [x] 19.7.2 `capture_uses_legacy_fallback_when_no_original_png`.
- [x] 19.7.3 `capture_rejects_when_original_png_fails_validation`.
- [x] 19.7.4 `original_png_dedupe_reuses_existing_asset`.
- [x] 19.7.5 `rgba_mismatch_rejects_without_persisting_a_degraded_png`.

### 19.8 Rust / core — cuatro rutas de fidelidad (nuevo)

- [x] 19.8.1 `png_with_native_metadata_persists_verbatim`.
- [x] 19.8.2 `png_without_metadata_rebuilds_with_tiff_metadata`.
- [x] 19.8.3 `png_with_phys_but_no_profile_keeps_phys_and_injects_icc`.
- [x] 19.8.4 `png_without_sibling_metadata_persists_verbatim`.
- [x] 19.8.5 `rebuild_helper_emits_pHYs_and_iCCP`.
- [x] 19.8.6 `rebuild_helper_without_metadata_emits_canonical_png`.
- [x] 19.8.7 `capture_persists_rebuilt_png_with_tiff_metadata_end_to_end`.
- [x] 19.8.8 `preview_and_copy_share_the_rebuilt_asset`.
- [x] 19.8.9 `restart_loads_rebuilt_png_asset`.
- [x] 19.8.10 `capture_diagnostic_never_leaks_payload_metadata`.
- [x] 19.8.11 `png_metadata_summary_kind_strings_are_stable`.
- [x] 19.8.12 `tiff_metadata_dpi_helpers_match_expected`.
- [x] 19.8.13 `max_image_dim_constant_matches_public_surface`.

### 19.9 Rust / core — regresión sintética con pHYs + tEXt

- [x] 19.9.1 `asset_store_preserves_phys_chunk_and_text_chunk_verbatim`.

### 19.10 Rust / core — restart

- [x] 19.10.1 `restart_loads_original_png_image_after_reopen`.
- [x] 19.10.2 `restart_loads_legacy_normalized_image_after_reopen`.
- [x] 19.10.3 `original_png_asset_preserves_metadata_after_restart`.
- [x] 19.10.4 `restart_loads_rebuilt_png_asset`.

### 19.11 Rust / core — pegado

- [x] 19.11.1 `paste_publishes_persisted_bytes_verbatim`.

### 19.12 Rust / core — diagnóstico metadata-only extendido

- [x] 19.12.1 `image_source_as_str_is_stable` (4 variantes).
- [x] 19.12.2 `image_source_display_matches_as_str`.
- [x] 19.12.3 `representation_source_as_str_is_stable`.
- [x] 19.12.4 `color_profile_kind_as_str_is_stable`.
- [x] 19.12.5 `diagnostic_display_contains_only_metadata_fields`.
- [x] 19.12.6 `diagnostic_display_marks_none_dpi_when_missing`.
- [x] 19.12.7 `diagnostic_from_normalized_reads_metadata_only`.

### 19.13 Frontend

- Sin cambios necesarios. El blob URL lifecycle, el bridge y
  el resolver ya consumen bytes PNG; el cambio de fuente no
  requiere tocar el frontend.

## 20. Verificación

- [x] 20.1 Ejecutar `cargo fmt --all -- --check`.
- [x] 20.2 Ejecutar
  `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 20.3 Ejecutar `cargo test --workspace`.
- [x] 20.4 Ejecutar `npm run check` en `app/tauri/frontend`.
- [x] 20.5 Ejecutar `npm run build` en `app/tauri/frontend`.
- [x] 20.6 Ejecutar `npm test` en `app/tauri/frontend`.
- [x] 20.7 Ejecutar
  `openspec validate clipboard-original-png-bytes --strict
  --type change`.
- [x] 20.8 Confirmar que no se agregaron secretos, fixtures
  con contenido real, ni archivos generados innecesarios.

## 21. Verificación manual (NO completar)

> **No marcar como completada la verificación manual hasta
> confirmar con `sips` que una captura nueva conserva 144 ppi
> y el perfil original.**

- [ ] 21.1 Cerrar completamente cualquier instancia
  anterior de ClipVault.
- [ ] 21.2 Compilar una build limpia.
- [ ] 21.3 Copiar una imagen que indique 144 ppi y Display P3.
- [ ] 21.4 Capturarla con ClipVault.
- [ ] 21.5 Comparar la imagen original y la almacenada con
  `sips -g pixelWidth -g pixelHeight -g dpiWidth -g
  dpiHeight -g profile <archivo>`.
- [ ] 21.6 Confirmar que ambas informen las mismas
  dimensiones, 144 ppp y el mismo perfil.
- [ ] 21.7 Verificar también preview y copy desde Quick Paste
  usen los mismos bytes.
- [ ] 21.8 Confirmar con `CLIPVAULT_DEBUG_IMAGE_CAPTURE=1` y
  `CLIPVAULT_DEBUG_IMAGE_PASTE=1` que el log incluye
  `image_source=native_png_plus_metadata`,
  `tiff_resolution_present=true`, `dpi_x=144`, `dpi_y=144`
  cuando `public.png` no trae la resolución correcta o contiene un
  `pHYs` predeterminado de 72 ppi.

## 22. Corrección de la resolución real del pasteboard macOS

- [x] 22.1 Auditar el caso manual en el que `public.png` ya
  contiene un `pHYs` predeterminado de 72 ppi y `public.tiff`
  contiene la resolución real de 144 ppi; documentar que la
  presencia de `pHYs` en PNG no lo convierte automáticamente en
  la fuente autoritativa.
- [x] 22.2 Hacer que `rebuild_for_pasteboard_metadata` prefiera
  la pareja X/Y del TIFF/AppKit sobre `png_chunks.phys` cuando
  existe y difiere, generando el `pHYs` correcto sin alterar los
  píxeles ni el formato de `asset_ref`.
- [x] 22.3 Añadir al bridge macOS un fallback de resolución
  basado en `NSBitmapImageRep::pixelsWide/pixelsHigh/size` para
  TIFFs cuyo IFD no expone de forma suficiente la resolución.
  Validar finitud, positividad y límites; no propagar bytes,
  perfiles ni rutas al diagnóstico.
- [x] 22.4 Corregir el parser TIFF para reconocer la etiqueta
  ICCProfile 34675 (`0x8773`) y mantener el recorrido de IFDs
  acotado y seguro ante entradas malformadas, incluyendo
  `SubIFDs`, `ExifIFD` e `InteroperabilityIFD`.
- [x] 22.5 Añadir regresiones deterministas para PNG 72 ppi +
  TIFF 144 ppi, TIFF con unidad omitida, resolución en IFD
  secundario, conversión exacta a píxeles por metro, perfil ICC y
  preservación byte-a-byte de los píxeles. Ejecutar la suite nativa
  con `macos-native` para comprobar que el fallback AppKit compila y
  queda cableado.
- [x] 22.6 Actualizar `proposal.md`, `design.md` y este archivo
  con la causa raíz, la precedencia TIFF/AppKit y la limitación
  no destructiva para assets históricos.
- [ ] 22.7 Repetir la verificación manual en un host macOS:
  cerrar la app anterior, capturar una imagen nueva que informe
  144 ppi, inspeccionar el asset recién creado con `sips` y
  confirmar 144 ppi en ambos ejes, el perfil esperado, preview
  completo y copy/paste con los mismos bytes persistidos.

## 23. Fallback de resolución del host macOS

- [x] 23.1 Confirmar que existen capturas nativas recientes cuyo
  `public.png` no contiene `pHYs` (o sólo declara el valor genérico
  de 72 ppi) y cuyo pasteboard no expone una metadata TIFF utilizable;
  el PNG persistido queda entonces en 72 ppi aunque la captura
  provenga de una pantalla Retina.
- [x] 23.2 Añadir al snapshot nativo la escala de la pantalla macOS
  (`NSScreen.backingScaleFactor`) y convertirla de forma acotada a
  ppi (`escala × 72`), sin leer ni registrar pixels, rutas o payloads.
- [x] 23.3 Propagar el valor como metadata explícita
  `inferred_display_dpi`, separada de `public.tiff`, para que el
  diagnóstico no confunda una inferencia del host con un tag TIFF.
- [x] 23.4 Hacer que el normalizador use este fallback sólo cuando
  PNG y TIFF no ofrecen resolución, reconstruyendo `pHYs` sin tocar
  dimensiones ni pixels; cualquier resolución nativa explícita
  continúa teniendo precedencia.
- [x] 23.5 Añadir tests deterministas para `2× → 144 ppi`, límites,
  reconstrucción sin cambio de pixels y procedencia de diagnóstico.
- [ ] 23.6 Repetir la prueba manual de macOS con una captura nueva y
  confirmar mediante `sips` `dpiWidth=144` y `dpiHeight=144`; esta
  tarea sigue pendiente hasta que el usuario la verifique.

## 24. Regresión del transporte del fallback al core

- [x] 24.1 Reproducir el caso en que el bridge calcula
  `inferred_display_dpi`, pero `public.png` no tiene chunks de
  metadata y `public.tiff` no aporta una IFD utilizable.
- [x] 24.2 Corregir `attach_pasteboard_metadata` para que no devuelva
  prematuramente la imagen sin metadata cuando existe la inferencia
  de escala del display; el valor debe llegar al normalizador y
  permitir la reconstrucción de `pHYs`.
- [x] 24.3 Añadir una regresión de adapter que comprueba que
  `2× → 144 ppi` cruza el límite platform → core sin perderse y
  que los bytes originales siguen presentes.
- [ ] 24.4 Repetir la verificación manual de macOS y confirmar con
  `sips` que el asset recién capturado informa 144 ppi en ambos
  ejes. No marcar por inferencia automatizada.

## 25. Robustez de la resolución de pantalla en el arranque

- [x] 25.1 Analizar el caso en que `NSScreen.mainScreen` no entrega
  una escala utilizable durante el arranque, dejando el PNG nativo
  sin `pHYs` y exponiendo el valor implícito de 72 ppi.
- [x] 25.2 Añadir fallback a `NSScreen.screens` en el mismo dispatch
  del pasteboard, conservando sólo escalas finitas entre 1× y 4× y
  sin transportar bytes ni rutas.
- [x] 25.3 Verificar que el adapter conserva el fallback de escala y
  que la suite platform/core/frontend permanece verde.
- [ ] 25.4 Repetir la captura real en macOS con la aplicación
  completamente cerrada y confirmar el resultado con `sips`. Esta
  verificación no se completa automáticamente.

## 26. Regresión observada: captura degradada a arboard

> El diagnóstico manual del 2026-09-07 informó
> `image_source=arboard_fallback`,
> `representation_source=none`,
> `tiff_resolution_present=false` y
> `original_png_preserved=false`. Eso prueba que el flujo observado
> no llegó a persistir una representación nativa del pasteboard; no
> es evidencia de que el archivo nativo haya sido leído y luego haya
> perdido sus metadatos en el normalizador.

- [x] 26.1 Auditar el transporte macOS y confirmar que el salto
  off-main no use un helper thread seguido de un segundo scheduler
  que pueda agotar el timeout mientras la cola principal está
  activa.
- [x] 26.2 Encolar directamente la operación en
  `DispatchQueue::main()` mediante `exec_async` y un canal acotado;
  conservar un timeout para que un event loop ausente se traduzca
  en `Unavailable`, nunca en un bloqueo indefinido.
- [x] 26.3 Evitar que `CompositeClipboard::read_image` y
  `read_payload` degraden un `Unavailable` nativo a
  `arboard::get_image` en el mismo tick. El watcher debe tratarlo
  como soft miss y reintentar, preservando la oportunidad de leer
  `public.png` + `public.tiff` en el siguiente tick.
- [x] 26.4 Mantener el fallback a `arboard` sólo para
  `UnsupportedFormat` o ausencia real de `public.png`, donde no
  existe una representación nativa que pueda perder metadatos.
- [x] 26.5 Añadir una traza de diagnóstico opt-in
  (`CLIPVAULT_DEBUG_IMAGE_CAPTURE=1`) con el resultado de la lectura
  nativa (`native_png`, `no_png_with_tiff_metadata`,
  `invalid_png` o `main_queue_unavailable`), sin bytes, contenido,
  hashes, identificadores ni rutas.
- [x] 26.6 Actualizar el contrato y el diseño de este cambio para
  que la ausencia temporal de la main queue no se describa como un
  fallback silencioso.
- [ ] 26.7 Repetir la prueba manual en macOS después de cerrar la
  instancia anterior y compilar la versión actual. Con
  `CLIPVAULT_DEBUG_IMAGE_CAPTURE=1`, confirmar primero una lectura
  nativa; luego verificar con `sips` que el asset nuevo informa
  144 ppi y el perfil original cuando el pasteboard los publica.

## 27. Regresión real: `⌘⇧4` publica TIFF sin PNG utilizable

> La prueba comparativa confirmó que
> `screencapture -i -c` conserva la resolución, mientras que
> `⌘⇧4` seguido de Copy terminaba en
> `image_source=arboard_fallback`. El caso específico es una
> captura de pantalla cuyo pasteboard expone un TIFF decodificable
> con la metadata de 144 ppi, pero no entrega un PNG utilizable al
> lector nativo. Antes de esta sección, `NoPng` descartaba el TIFF y
> permitía que `arboard` re-encodificara el bitmap a 72 ppi.

- [x] 27.1 Confirmar que el bridge lee los legs PNG y TIFF en un
  único snapshot de `NSPasteboard` sobre la main queue, sin registrar
  bytes, rutas ni contenido.
- [x] 27.2 Agregar el resultado tipado `TiffOnly` para distinguir
  un TIFF nativo decodificable de la ausencia real de una imagen.
- [x] 27.3 Decodificar `public.tiff` con AppKit a un buffer RGBA
  completo, manteniendo `pixelsWide`/`pixelsHigh` y sin aplicar
  escalado, recorte o inversión vertical.
- [x] 27.4 Transportar una imagen TIFF-only sin `original_png` a
  través de `ClipboardImage` mediante un constructor explícito de
  metadata sin PNG original.
- [x] 27.5 Hacer que el normalizador reconstruya el PNG canónico
  con la resolución/perfil TIFF y marque `TiffMetadataOnly`, sin
  volver a `arboard` ni perder la metadata.
- [x] 27.6 Mapear `TiffMetadataOnly` al diagnóstico
  `image_source=tiff_metadata` y conservar el contrato de privacidad.
- [x] 27.7 Añadir pruebas de dimensiones/píxeles TIFF-only y de
  reconstrucción a 144 ppi; ejecutar las regresiones nativas y core.
- [x] 27.8 Repetir en macOS real: cerrar instancias anteriores,
  iniciar la build actual con
  `CLIPVAULT_DEBUG_IMAGE_CAPTURE=1`, ejecutar `⌘⇧4` → Copy,
  comprobar una lectura nativa TIFF y verificar con `sips` que el
  asset nuevo informa 144 ppi en ambos ejes y conserva las
  dimensiones completas. Verificado manualmente por el usuario.
