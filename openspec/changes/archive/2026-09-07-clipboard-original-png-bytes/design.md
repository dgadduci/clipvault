# Design: clipboard-original-png-bytes

## Causa raíz (auditada sobre macOS)

Cuando un usuario copia una imagen (especialmente una captura
de pantalla nativa) con `⌘⇧4` o desde Preview / Safari, el
`NSPasteboard` publica **simultáneamente** dos
representaciones: `public.png` y `public.tiff`. Los píxeles
son idénticos entre las dos, pero la metadata crítica del
operador (resolución y perfil) **no siempre está completa en
`public.png`**:

- `public.png` puede construirse con sólo IHDR/IDAT/IEND o
  puede incluir un `pHYs` predeterminado de **72 ppi**. En
  ambos casos no es necesariamente la resolución física que
  Preview muestra para la imagen fuente.
- `public.tiff` lleva los tags IFD 282/283/296 (XResolution,
  YResolution, ResolutionUnit) que codifican 144 ppi, y el tag
  34675 (ICCProfile) con el perfil Display P3. El parser recorre
  también los IFD secundarios referenciados por `SubIFDs`,
  `ExifIFD` e `InteroperabilityIFD`, porque la ubicación no es
  uniforme entre productores de TIFF.

El primer intento de implementación sólo leía `public.png`,
persistía verbatim los bytes sin chunks de resolución, y
`sips -g dpiWidth` reportaba 72 ppi para el archivo
persistido. La "captura estaba mal" era el síntoma; la
causa real era que **la metadata viaja en el leg `public.tiff`,
no en el leg `public.png`**.

## Forma del asset en disco

El layout en disco no cambia:

```text
<data_dir>/assets/clipboard/<lowercase-sha256>.png
```

`asset_ref` sigue siendo el valor relativo
`clipboard/<sha>.png` que SQLite almacena y que el bridge
consume. Lo que cambia es **el contenido del archivo**:

- cuando `public.png` ya contiene `pHYs` + `iCCP`/`sRGB`,
  ClipVault persiste verbatim esos bytes;
- cuando `public.png` no trae `pHYs`/`iCCP` pero `public.tiff`
  sí los declara, ClipVault **rebuilda** un PNG con los
  mismos píxeles + un chunk `pHYs` con la resolución
  detectada + un chunk `iCCP` con el perfil detectado;
- cuando `public.png` trae un `pHYs` predeterminado (por
  ejemplo 72 ppi) y `public.tiff` declara otra resolución
  (por ejemplo 144 ppi), el TIFF/AppKit es la fuente
  autoritativa y el PNG se rebuilda con esa resolución real;
- cuando ninguno de los dos legs expone metadata, ClipVault
  persiste el PNG canónico de 8-bit RGBA (ruta legacy).

## Modelo de imagen extendido

`ClipboardImage` en
`crates/clipvault-platform/src/clipboard.rs` se amplía con dos
campos opcionales:

```rust
pub struct ClipboardImage {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    original_png: Option<Vec<u8>>,
    pasteboard_metadata: PasteboardImageMetadata,
}
```

Constructores:

- `ClipboardImage::new(rgba, width, height)` — el constructor
  legacy; `original_png = None`, `pasteboard_metadata = default()`.
  Es el único constructor usado por `arboard` y por los tests
  no-macOS; nada cambia para ellos.
- `ClipboardImage::with_original_png(rgba, width, height,
  original_png)` — valida que `rgba.len()` coincide con
  `width * height * 4` y que `original_png` no esté vacía.
  `pasteboard_metadata` queda vacío.
- `ClipboardImage::with_pasteboard_metadata(rgba, width,
  height, original_png, pasteboard_metadata)` — nueva
  variante: invoca el constructor anterior y adjunta el bloque
  de metadata del pasteboard. El bridge del macOS usa este
  constructor para que el persistence layer pueda consultar la
  metadata reconstruida sin inspectar bytes.
- `ClipboardImage::with_pasteboard_metadata_without_original(rgba,
  width, height, pasteboard_metadata)` — variante explícita para
  un raster nativo TIFF-only. Conserva la metadata sin fingir que
  existe un PNG original vacío.

`PasteboardImageMetadata` contiene dos campos opcionales y dos
banderas:

```rust
pub struct PasteboardImageMetadata {
    pub png_chunks: PngMetadataSummary,
    pub tiff: Option<TiffMetadata>,
    pub resolution_detected: bool,
    pub profile_detected: bool,
}
```

`Debug` se mantiene metadata-only: dimensiones + `rgba_len`
+ `has_original_png: bool` + `pasteboard_metadata_kind_str()`.
Ningún byte buffer se imprime jamás.

## Trait `ClipboardBackend`

`crates/clipvault-platform/src/clipboard.rs` ya expone
`read_image_png` con default `Err(UnsupportedFormat)`. El método
y la capacidad `supports_image_png_read` no cambian.

`MacOsPasteboardClipboard::read_image_png` mapea `NativePngRead`
cinco formas:

- `Native { image, metadata }` → reconstruye
  `ClipboardImage::with_pasteboard_metadata(rgba, w, h, png,
  metadata)` para que la metadata viaje adjunta al `image`.
- `TiffOnly { image, metadata }` → `Some(image)` después de
  adjuntar la metadata. La imagen contiene el RGBA completo
  decodificado por AppKit, sin `original_png`; el core la normaliza
  e inyecta la resolución/perfil del TIFF antes de persistirla.
- `NoPng { metadata: _ }` → `Ok(None)` sólo cuando no existe un
  raster TIFF decodificable. En ese caso el composite puede usar
  `arboard` como último fallback.
- `InvalidPng(error)` → `Err(InvalidImage(ImageValidationError::InvalidPng { kind }))`
  (sin fallback).
- `Err(MainQueueBridgeError::DispatchUnavailable)` →
  `Err(Unavailable { capability: ClipboardReadImage })`.
  Es un resultado reintentable: no se permite degradar a
  `arboard` en el mismo tick porque el pasteboard nativo puede
  contener resolución/perfil que el camino RGBA perdería.

`CompositeClipboard::read_image` y `read_payload` deciden el
fallback según el resultado del rich adapter:

- `Ok(Some(image))` → usar la imagen nativa;
- `Ok(None)` → fallback al plain adapter (sin `public.png`);
- `Err(UnsupportedFormat)` → fallback al plain adapter
  (backend sin transporte de PNG nativo);
- `Err(Unavailable { .. })` → propagar el resultado soft y
  reintentar en el siguiente tick; no invocar `arboard` en ese
  intento, porque la degradación sería silenciosa;
- `Err(InvalidImage(_))` → **NO** fallback al plain adapter;
  propagar el error tipado para que la pipeline no guarde
  una imagen degradada;
- otros errores → propagar.

## Bridge nativo `read_png_main_thread`

`crates/clipvault-platform/src/runtime/macos_clipboard_main_queue.rs`
expone un enum tipado con cuatro variantes:

```rust
pub enum NativePngRead {
    Native {
        image: ClipboardImage,
        metadata: PasteboardImageMetadata,
    },
    TiffOnly {
        image: ClipboardImage,
        metadata: PasteboardImageMetadata,
    },
    NoPng {
        metadata: PasteboardImageMetadata,
    },
    InvalidPng(PngValidationError),
}

pub fn read_png_main_thread() -> BridgeResult<NativePngRead>
```

`PasteboardImageMetadata` se construye siempre, incluso
cuando `public.png` está ausente: el composite puede seguir
usando la metadata en la ruta actual de
`TiffMetadataOnly`.

El flujo dentro del dispatch al main thread:

1. `NSPasteboard::generalPasteboard()`.
2. `pasteboard.dataForType(NSPasteboardTypePNG)` y
   `pasteboard.dataForType(NSPasteboardTypeTIFF)` —
   usan las constantes canónicas de AppKit. Si alguna falla,
   se intenta el UTI string `"public.png"` / `"public.tiff"`
   como fallback para hosts que no declaran el tipo canónico.
3. Construir `PasteboardImageMetadata` combinando:
   - `png_chunks = png_metadata_summary(png_bytes)` (o
     `default()` si `png_bytes = None`);
   - `tiff = Some(parse_tiff_metadata(tiff_bytes))` cuando los
     bytes reportan al menos un tag (el `default()` se descarta);
   - si ambos legs omiten resolución, consultar la escala acotada
     de `NSScreen.mainScreen.backingScaleFactor` y guardarla como
     `inferred_display_dpi` (sin presentarla como metadata TIFF);
   - `resolution_detected = png_chunks.preserves_resolution()
     || tiff.has_resolution() || inferred_display_dpi.is_some()`;
   - `profile_detected = png_chunks.icc_profile_chunk.is_some()
     || png_chunks.has_srgb || tiff.icc_profile.is_some()`.
4. Si no hay bytes de `public.png`, intentar decodificar los bytes
   de `public.tiff` con `NSBitmapImageRep` dentro de esta misma
   closure. Si la decodificación entrega un raster válido, devolver
   `Ok(NativePngRead::TiffOnly { image, metadata })`; si no, devolver
   `Ok(NativePngRead::NoPng { metadata })` y permitir el último
   fallback a `arboard`. Si el puente no logra ejecutar la closure,
   devolver `Err(DispatchUnavailable)`; esta rama no se convierte
   en una lectura de `arboard` dentro del mismo tick.
5. Validar firma, tamaño (`<= MAX_CLIPBOARD_PNG_BYTES`) y
   dimensiones (`<= MAX_CLIPBOARD_IMAGE_DIM`) con
   `clipboard_image_png::validate_png`.
6. Mapear el `PngValidationOutcome`:
   - `Valid` → construir `ClipboardImage::with_original_png(...)`
     y devolver `Ok(NativePngRead::Native { image, metadata })`;
   - el resultado TIFF-only ya fue resuelto en el paso 4 y llega
     como `NativePngRead::TiffOnly` con el frame RGBA completo;
   - `NotPng` → `Ok(NativePngRead::NoPng { metadata })`;
   - `Invalid` → `Ok(NativePngRead::InvalidPng(error))`.

El bridge conserva una API síncrona para el caller, pero el salto
off-main se encola directamente en `DispatchQueue::main()` mediante
`exec_async` y un canal acotado. Así evita el helper-thread + segundo
hop que podía agotar el timeout aun cuando la cola principal estaba
activa. El timeout se traduce en `Unavailable` y el watcher reintenta;
no se permite volver silenciosamente a `arboard`.

### Último fallback para un PNG nativo sin metadata

Se observó en una captura real que el PNG persistido podía contener
únicamente `IHDR`/`IDAT`/`IEND` (o un `pHYs` genérico de 72 ppi): no
había una metadata TIFF utilizable disponible para el bridge.
En ese caso el payload no contiene una resolución recuperable. Para
que una captura de pantalla Retina no se degrade silenciosamente al
valor implícito de 72 ppi, el bridge toma la escala del display
principal en el hilo principal. Si `mainScreen` todavía no está
disponible durante el arranque, inspecciona las pantallas que AppKit
ya expone y toma la mayor escala válida. La conversión es `escala ×
72`, con escalas finitas entre 1× y 4×; por ejemplo, 2× produce 144
ppi.

El valor viaja en `PasteboardImageMetadata.inferred_display_dpi`,
separado de `tiff`. La resolución TIFF explícita siempre tiene
precedencia; un `pHYs` PNG se conserva salvo cuando es el valor
genérico de 72 ppi y el host ofrece una escala superior.
La metadata se conserva también cuando el PNG y el TIFF no tienen
chunks útiles: el adapter no puede descartar esta inferencia al
reconstruir `ClipboardImage`.
El diagnóstico informa `representation_source=display_scale_fallback`
para dejar claro que se trata de una inferencia del host. No se
modifican píxeles, dimensiones, perfil, hash lógico ni la referencia
relativa del asset.

## Scanner de chunks PNG

`crates/clipvault-platform/src/clipboard_image_png.rs`
añade un walker que enumera los chunks ancillares pre-`IDAT`:

```rust
pub struct PngMetadataSummary {
    pub phys: Option<[u8; 9]>,
    pub icc_profile_chunk: Option<Vec<u8>>,
    pub has_srgb: bool,
    pub has_gama: bool,
    pub has_chrm: bool,
    pub has_text: bool,
}

pub fn png_metadata_summary(bytes: &[u8]) -> PngMetadataSummary
pub fn parse_ppu_triple(payload: &[u8; 9]) -> Option<(u32, u32, u8)>
pub fn phys_to_dpi(ppu_x: u32, ppu_y: u32, unit: u8) -> Option<(u32, u32)>
```

El walker:

1. Verifica la firma PNG; falla con `default()` si no
   coincide.
2. Salta `IHDR` (no es metadata).
3. Enumera los chunks hasta `IDAT`; chunks después de
   `IDAT` se ignoran (pertenecen al frame siguiente en APNG).
4. Para `pHYs` (9 bytes): captura el payload completo.
5. Para `iCCP` (variable): captura el payload completo,
   incluida la carga zlib-deflate.
6. Para `sRGB`, `gAMA`, `cHRM`, `tEXt`/`iTXt`/`zTXt`: sólo
   reporta presencia booleana.
7. En macOS, decodifica el leg TIFF con `NSBitmapImageRep`
   dentro del mismo dispatch cuando no hay PNG, preservando
   exactamente `pixelsWide`/`pixelsHigh` y el buffer RGBA completo.
   También usa `pixelsWide / size.width * 72` (y el equivalente
   vertical) como fallback cuando la resolución del IFD es
   incompleta o contradice la resolución que AppKit expone. Sólo se
   transportan los píxeles validados y los dos enteros DPI.
8. Si `NSScreen.mainScreen` no aporta una escala utilizable, consulta
   `NSScreen.screens` en el mismo dispatch y usa la mayor escala
   finita dentro del límite 1×–4×.

`phys_to_dpi` convierte `ppuX / ppuY` a DPI con redondeo:
ppm × 0.0254, redondeo al entero más cercano.

## Parser de TIFF

`crates/clipvault-platform/src/tiff_metadata.rs` es un walker
de IFD estricto:

```rust
pub enum TiffResolutionUnit { None, Inch, Centimeter }
pub struct TiffMetadata {
    pub x_resolution: Option<(u32, u32)>,
    pub y_resolution: Option<(u32, u32)>,
    pub resolution_unit: TiffResolutionUnit,
    pub icc_profile: Option<Vec<u8>>,
}

pub fn parse_tiff_metadata(bytes: &[u8]) -> TiffMetadata
```

Lee los cuatro tags IFD que el bridge consume:

| Tag  | Tipo      | Significado                          |
|------|-----------|--------------------------------------|
| 282  | RATIONAL  | XResolution                          |
| 283  | RATIONAL  | YResolution                          |
| 296  | SHORT     | ResolutionUnit                       |
| 34675| UNDEFINED | ICCProfile                           |

`TiffMetadata::dpi_x() / dpi_y()` aplican la conversión de
unidades: centímetros → inches con factor × 2.54,
redondeo al entero más cercano. El bridge conserva además
los valores que AppKit obtiene de la representación decodificada
cuando el IFD no es suficiente; esto cubre TIFFs de Apple que
Preview muestra como 144 ppi aunque el parser de tags no pueda
resolver el valor de forma completa.

Justificación del parser hand-rolled: el grafo de
dependencias no admite un segundo codec TIFF. Los cuatro
tags son estables y bien documentados; la implementación
son ~250 líneas. La alternativa `image` (Rust crate)
añade dependencias nuevas (image crate + codecs) que el
AGENTS desaconseja.

## Validación centralizada

`validate_png` mantiene su contrato original
(`PngValidationOutcome::{Valid, NotPng, Invalid}`).

`validate_original_png` en core mantiene su contrato y
devuelve `OriginalPngValidationError` typed.

## Persistencia con cuatro rutas de fidelidad

`clipboard_assets::normalize_image_with_original` decide
entre cuatro rutas:

| Ruta | Disparador | Bytes persistidos |
|---|---|---|
| `NativeVerbatim` | `original_png` válido + `public.png` ya trae `pHYs` + iCCP/`sRGB` | PNG original byte-por-byte |
| `NativeRebuiltWithMetadata` | `original_png` válido, o bien `public.png` no trae resolución, o bien trae un `pHYs` predeterminado distinto al TIFF/AppKit; `public.tiff` tiene la metadata | PNG nuevo con mismos píxeles + chunk `pHYs` de la resolución autoritativa + chunk `iCCP` con perfil comprimido (o `sRGB`) |
| `TiffMetadataOnly` | `original_png = None`, `public.tiff` decodificable y declara metadata | PNG canónico reconstruido con los mismos píxeles + `pHYs`/`iCCP` o `sRGB` de TIFF |
| `LegacyEncoded` | `original_png = None` y nada en `public.tiff` | 8-bit RGBA PNG canónico (sin metadata) |

`NormalizedImage` se amplía con un campo `source:
NormalizedSource` y un accesor `was_rebuilt_with_metadata()`.
`NormalizedImage::png()` sigue apuntando a los bytes que el
asset store persistirá (verbatim o rebuildados).

`rebuild_for_pasteboard_metadata(bytes, width, height, rgba,
pasteboard_metadata) -> Result<Option<Vec<u8>>, AssetError>` es
el helper que decide qué reconstruir:

1. Si el TIFF/AppKit aporta una pareja X/Y válida, esa pareja
   es la resolución autoritativa aunque `png_chunks.phys` exista.
   Esto evita conservar un `pHYs` predeterminado de 72 ppi.
2. Si no existe una resolución TIFF/AppKit utilizable y el bridge
   entregó `inferred_display_dpi`, usar esa inferencia del host.
3. Si tampoco existe esa inferencia, usar el `pHYs` del PNG tal como
   está.
4. Si `png_chunks.phys` coincide con la resolución elegida y el
   PNG ya tiene `iCCP`/`sRGB`, no reconstruir: el PNG original ya
   trae la metadata y el camino verbatim es byte-por-byte.
5. Si la resolución elegida difiere del `pHYs` del PNG, o el TIFF
   trae un ICCProfile que el PNG no tiene, reconstruir preservando
   los mismos píxeles.
6. Si el TIFF trae ICCProfile → comprimir el perfil con
   `flate2::ZlibEncoder` y construir el chunk `iCCP`.
7. Si nada aplica → `Ok(None)` (el caller persiste verbatim,
   sin rebuild).

`rebuild_png_with_metadata(width, height, rgba, phys_payload,
icc_chunk, srgb_intent) -> Result<Vec<u8>, AssetError>` es el
helper de bajo nivel. Invoca el `png` crate con `write_chunk`
para emitir `pHYs` / `iCCP` / `sRGB` antes de los IDAT.

`deflate_icc_profile(&[u8]) -> Result<Vec<u8>, AssetError>`
comprime el payload ICC en zlib (lo que `iCCP` espera). La
dependencia nueva es `flate2`, ya en el grafo transitivo vía
`png`; el AGENTS permite añadir dependencias pequeñas,
maduras y justificadas.

`AssetError::OriginalPngValidation(_)` se mantiene: la
validación del PNG original sigue siendo dura cuando
`original_png` está presente pero los bytes fallan
verificación. El rebuild NO la evita: si el `original_png` no
valida, no hay rebuild posible.

`ClipboardAssetStore::store_image(image)` no cambia: lee
`image.png()`, calcula hash, escribe atómicamente. La lógica
atómica (temp + rename), el `asset_ref`, el `content_size`
son idénticos desde la perspectiva del store.

## Captura: `TextHistoryService::persist_image`

`crates/clipvault-core/src/history.rs::persist_image` llama
`normalize_image_with_original` con `image.original_png() !=
None` o el camino legacy.

Tras un `Stored`, el método emite el diagnóstico metadata-only
`ImageCaptureDiagnostic` vía `log_image_capture_diagnostic`.

El diagnóstico mapea `NormalizedSource` a `ImageSource`:

- `NativeVerbatim` → `ImageSource::NativePng`.
- `NativeRebuiltWithMetadata` → `ImageSource::NativePngPlusMetadata`.
- `TiffMetadataOnly` → `ImageSource::TiffMetadata`.
- `LegacyEncoded` → `ImageSource::ArboardFallback`.

## Pegado: bytes persistidos

`crates/clipvault-core/src/paste.rs::write_image_payload` se
mantiene intacto en su contrato:

1. `store.read_bytes(asset_ref)` → bytes del PNG persistido.
2. `decode_png(&bytes)` para validar y obtener el bitmap
   decodificado (necesario para `arm_image_suppression`).
3. `clipboard.write_image_png(&bytes)` cuando la sesión lo
   soporta.
4. Si no, `clipboard.write_image(&image)` con el bitmap
   decodificado.

Tras una copia exitosa, el método emite el diagnóstico
metadata-only `ImageCaptureDiagnostic` vía
`log_image_paste_diagnostic`.

Como `read_bytes` devuelve los bytes del PNG que el store
tiene en disco, y el store ahora puede contener el PNG
original o el rebuildado (con `pHYs`/`iCCP`/`sRGB`), el pegado
publica la imagen fiel al receptor.

El decoder del core (`decode_png`) ya acepta cualquier
`(ColorType, BitDepth)` legal. El rebuild siempre produce un
PNG 8-bit RGBA con perfiles incrustados en `iCCP` o
marcadores `sRGB`.

## Diagnóstico metadata-only extendido

`ImageCaptureDiagnostic` añade cinco campos sobre la versión
anterior:

```rust
pub enum ImageSource {
    NativePng,
    NativePngPlusMetadata,
    TiffMetadata,
    ArboardFallback,
}

pub enum RepresentationSource {
    PngChunks,
    TiffIfd,
    None,
}

pub enum ColorProfileKind {
    Iccp,
    Srgb,
    TiffIccProfile,
    None,
}

pub struct ImageCaptureDiagnostic {
    pub image_source: ImageSource,
    pub representation_source: RepresentationSource,
    pub png_chunks_kind: &'static str,
    pub tiff_resolution_present: bool,
    pub tiff_icc_profile_present: bool,
    pub resolution_dpi_x: Option<u32>,
    pub resolution_dpi_y: Option<u32>,
    pub profile_kind: ColorProfileKind,
    pub original_png_preserved: bool,
    pub width: u32,
    pub height: u32,
}

pub fn log_image_capture_diagnostic(ImageCaptureDiagnostic);
pub fn log_image_paste_diagnostic(ImageCaptureDiagnostic);
```

Los helpers se emiten gated por las variables de entorno:

- `CLIPVAULT_DEBUG_IMAGE_CAPTURE=1` para la captura;
- `CLIPVAULT_DEBUG_IMAGE_PASTE=1` para el pegado.

Cualquier valor distinto de `1` mantiene el diagnóstico
silencioso. Ningún campo lleva bytes, byte length, content hash,
identificador de app fuente, snippet ni rutas absolutas.

## Compatibilidad hacia atrás

- **PNG legacy en disco**: se siguen leyendo porque el decoder
  acepta cualquier `(ColorType, BitDepth)` legal. Las
  dimensiones y metadata que faltan en esos PNG no se
  restauran: **no se pueden resucitar bytes que ya están en
  disco**. Esta es una limitación intencional del cambio; los
  assets legacy siguen funcionando pero no recuperan la
  metadata perdida.
- **Asset references existentes**: no se renombran ni se
  reescriben.
- **`asset_ref` invariante**: el formato `clipboard/<sha>.png`
  se conserva.
- **Frontend**: sin cambios. El blob URL lifecycle, el
  bridge y el resolver ya consumen bytes PNG; el cambio de
  fuente no requiere tocar el frontend.
- **Texto / rich text / HTML / RTF**: cero cambios.
- **Favoritos / tags / colecciones / búsqueda / drag-and-drop**:
  cero cambios.
- **Tauri commands**: cero cambios.

## Pruebas requeridas

### Rust / platform — validador PNG

- `validate_png_accepts_a_well_formed_png`
- `validate_png_returns_not_png_for_non_png_payloads`
- `validate_png_returns_not_png_for_empty_payload`
- `validate_png_returns_not_png_for_truncated_signatures`
- `validate_png_returns_invalid_for_zero_dimensions`
- `validate_png_returns_invalid_for_oversized_payloads`
- `validate_png_never_leaks_bytes_in_kind_str`
- `validate_png_passes_a_reported_size_through_unchanged`

### Rust / platform — scanner de chunks PNG

- `png_metadata_summary_detects_phys_chunk`
- `png_metadata_summary_reports_text_chunk_presence`
- `png_metadata_summary_rejects_invalid_signatures`
- `phys_to_dpi_rounds_at_the_standard_inch`
- `phys_to_dpi_returns_none_for_unspecified_unit`
- `png_metadata_summary_helpers_classify_fidelity_paths`

### Rust / platform — parser de TIFF

- `parses_144_dpi_x_resolution_in_inches`
- `parses_centimeter_resolution_into_dpi`
- `parses_inline_icc_profile_payload`
- `rejects_short_buffers`
- `rejects_invalid_magic_number`
- `empty_dpi_when_unit_is_none`

### Rust / platform — bridge `read_png_main_thread`

- `native_png_read_kind_str_is_stable` (extendido)
- `native_png_read_kind_str_never_leaks_payload_metadata`

### Rust / platform — composite clipboard

- `read_image_prefers_native_png_when_available`
- `read_image_falls_back_to_plain_when_native_returns_none`
- `read_image_falls_back_to_plain_when_native_returns_unsupported`
- `read_image_does_not_fall_back_when_native_png_is_invalid`
- `read_payload_does_not_fall_back_when_native_png_is_invalid`

### Rust / core — modelo y validación

- `clipboard_image_with_original_png_keeps_invariants`
- `clipboard_image_with_original_png_rejects_stride_mismatch`
- `clipboard_image_with_original_png_rejects_empty_bytes`
- `clipboard_image_default_has_no_original_png`
- `clipboard_image_debug_never_leaks_pixels_or_original_bytes`
- `validate_original_png_accepts_a_well_formed_png`
- `validate_original_png_rejects_wrong_signature`
- `validate_original_png_rejects_oversized_payload`
- `validate_original_png_rejects_wrong_dimensions`
- `validate_original_png_rejects_rgba_mismatch`
- `normalize_image_with_original_preserves_original_bytes`
- `normalize_image_with_original_rejects_on_validation_failure`
- `normalize_image_with_original_uses_legacy_path_when_no_original`
- `decode_png_round_trip_preserves_a_png_with_phys_chunk_dimensions`

### Rust / core — captura y persistencia (con metadata del pasteboard)

- `capture_persists_original_png_bytes_verbatim`
- `capture_uses_legacy_fallback_when_no_original_png`
- `capture_rejects_when_original_png_fails_validation`
- `original_png_dedupe_reuses_existing_asset`
- `rgba_mismatch_rejects_without_persisting_a_degraded_png`

### Rust / core — cuatro rutas de fidelidad (nuevo test file)

- `png_with_native_metadata_persists_verbatim`
- `png_without_metadata_rebuilds_with_tiff_metadata`
- `png_with_phys_but_no_profile_keeps_phys_and_injects_icc`
- `png_without_sibling_metadata_persists_verbatim`
- `rebuild_helper_emits_pHYs_and_iCCP`
- `rebuild_helper_without_metadata_emits_canonical_png`
- `capture_persists_rebuilt_png_with_tiff_metadata_end_to_end`
- `preview_and_copy_share_the_rebuilt_asset`
- `restart_loads_rebuilt_png_asset`
- `capture_diagnostic_never_leaks_payload_metadata`
- `png_metadata_summary_kind_strings_are_stable`
- `tiff_metadata_dpi_helpers_match_expected`
- `max_image_dim_constant_matches_public_surface`

### Rust / core — regresión sintética con pHYs + tEXt (legado)

- `asset_store_preserves_phys_chunk_and_text_chunk_verbatim`
  Construye un PNG con `pHYs` (144 ppi = 5669 ppm) y un chunk
  `tEXt` con un marker fixture-only. Verifica que el asset
  en disco sea **byte-for-byte igual** al PNG original, que
  el `asset_ref` apunte al SHA-256 de los bytes originales,
  y que Quick Paste publique exactamente esos bytes.

### Rust / core — restart

- `restart_loads_original_png_image_after_reopen`
- `restart_loads_legacy_normalized_image_after_reopen`
- `original_png_asset_preserves_metadata_after_restart`
- `restart_loads_rebuilt_png_asset`

### Rust / core — pegado

- `paste_publishes_persisted_bytes_verbatim`

### Rust / core — diagnóstico metadata-only extendido

- `image_source_as_str_is_stable` (4 variantes)
- `image_source_display_matches_as_str`
- `representation_source_as_str_is_stable`
- `color_profile_kind_as_str_is_stable`
- `diagnostic_display_contains_only_metadata_fields` (11 campos)
- `diagnostic_display_marks_none_dpi_when_missing`
- `diagnostic_from_normalized_reads_metadata_only`

### Rust / core — regresiones (no deben cambiar)

- Texto, rich text, favoritos, tags, colecciones, búsqueda,
  drag-and-drop, paste, retention.

## Verificación manual

1. Capturar una imagen en una app que publique PNG con
   `pHYs` y perfil de color (Capturas de pantalla nativas,
   Preview, Safari, etc.).
2. Confirmar con `sips -g pixelWidth -g pixelHeight -g
   dpiWidth -g dpiHeight -g profile <archivo>` que el PNG
   persistido conserva `pHYs` con 144 ppi y el perfil de
   color (Display P3 / `iCCP` / `sRGB`), y que las dimensiones
   del IHDR son 1104×396.
3. Pegar la imagen en otra app y confirmar que la imagen
   pegada tiene las mismas dimensiones y perfil.
4. Reiniciar ClipVault y confirmar que la imagen histórica
   sigue visible con las mismas dimensiones y metadata.
5. Probar una captura sin metadata (sólo IHDR/IDAT/IEND)
   y confirmar que sigue funcionando.
6. Confirmar que una imagen legacy persistida por una build
   anterior sigue siendo legible (limitación: no puede
   recuperar metadata perdida).
7. Confirmar que copiar desde Quick Paste publica los bytes
   persistidos (mismo hash del PNG en el clipboard del
   receptor que en `~/.clipvault/assets/clipboard/<sha>.png`).
8. Confirmar con `CLIPVAULT_DEBUG_IMAGE_CAPTURE=1` y
   `CLIPVAULT_DEBUG_IMAGE_PASTE=1` que el log incluye
   `image_source=native_png_plus_metadata` y
   `tiff_resolution_present=true` cuando se capturó un PNG
   sin pHYs/iCCP.
