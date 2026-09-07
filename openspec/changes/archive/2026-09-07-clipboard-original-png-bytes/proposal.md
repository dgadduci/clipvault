# Proposal: clipboard-original-png-bytes

## Why

Captura de imágenes en macOS degrada silenciosamente la imagen
original antes de persistirla. El clipboard publica una imagen de
**1104×396 px, 144 ppi y perfil Display P3**; ClipVault termina
almacenando un PNG con dimensiones distintas, **72 ppi y sin
perfil de color**. El pegado reproduce fielmente ese PNG alterado,
porque el paste publica exactamente los bytes persistidos.

### Causa raíz real (auditada)

Una imagen de captura de pantalla en macOS publica **dos**
representaciones en el pasteboard simultáneamente:
`public.png` y `public.tiff`. Las dos transportan exactamente los
mismos píxeles, pero **la metadata que el operador observa (144
ppi, perfil Display P3) no siempre está completa en `public.png`;
la metadata está en `public.tiff` o en un IFD secundario del TIFF:

- `public.png` puede construirse sin chunks `pHYs` ni `iCCP`, o
  incluir un `pHYs` predeterminado de 72 ppi. macOS deja el PNG en
  una forma "visual" mínima porque el spec no obliga a publicar la
  resolución física real dentro de ese leg.
- `public.tiff` lleva los tags XResolution (282) /
  YResolution (283) / ResolutionUnit (296) que codifican 144 ppi,
  y el tag ICCProfile (34675) con el perfil Display P3.

El primer intento de implementación sólo leyó `public.png` y
persistió los bytes verbatim. Como esos bytes no contienen
`pHYs` ni `iCCP`, el archivo persistido **carecía de la
resolución** — `sips` lo reportaba como 72 ppi (el default PNG
cuando ningún `pHYs` está presente).

Adicionalmente, dos fallbacks silenciosos destruían fidelidad
incluso cuando `public.png` tenía chunks:

1. **El bridge `read_png_main_thread()` colapsaba cualquier
   fallo de validación de `public.png` a `Ok(None)`** —
   incluido el caso «`public.png` existe pero los bytes son
   inválidos». El composite interpretaba `Ok(None)` como
   «no hay PNG nativo» y caía al `arboard::get_image()`
   clásico, que produce el bitmap RGBA sin `original_png`.
2. **`normalize_image_with_original()` colapsaba cualquier
   fallo de `validate_original_png` al encoder legacy**
   (`normalize_image`) en lugar de propagar el error. Esto
   significaba que, incluso si el bridge entregaba bytes
   válidos pero el bitmap RGBA adjunto no coincidía con la
   decodificación de esos bytes, el core re-encodaba
   silenciosamente.

### Hipótesis descartadas

- **El decoder PNG (commit anterior) acepta todos los
  `(ColorType, BitDepth)` legales**: arregla la lectura de PNG
  antiguos en disco, pero no recupera la metadata perdida
  durante la captura.
- **Cambiar `normalize_image()` para preservar `pHYs`/`iCCP`**:
  no funciona, la información ya se perdió en
  `arboard::get_image()` antes de que `normalize_image` viera los
  pixels.
- **Usar `arboard::set_image` con opciones especiales**:
  `arboard` no expone una API para preservar metadata del PNG
  original; su `set_image` re-encoda desde un bitmap RGBA.
- **Convertir el flujo a un único encoder NSBitmapImageRep**:
  perderíamos la fidelidad del RGBA y mantendríamos el problema
  de `NSBitmapImageRep` alterando la imagen cuando el cache
  descarta el original.
- **Persistir sólo los bytes de `public.png`**: no resuelve la
  pérdida de resolución porque la resolución no está en esos
  bytes.

### Decisión

Eliminar los dos fallbacks silenciosos identificados y preservar
los bytes PNG originales leídos directamente desde `NSPasteboard`
mediante el bridge existente al main thread, distinguiendo
tipadamente las cinco formas que puede tener la respuesta del
pasteboard:

| Estado | Outcome tipado | ¿Fallback a `arboard`? |
|---|---|---|
| `public.png` válido | `Native { image, metadata }` | No — persistir verbatim o rebuild + metadata |
| `public.png` ausente, TIFF válido | `TiffOnly { image, metadata }` | No — decodificar TIFF y reconstruir con metadata |
| Sin PNG utilizable ni TIFF decodificable | `NoPng { metadata }` | Sí — bitmap clásico como último fallback |
| `public.png` presente pero falla validación (tamaño, dim, decoder) | `InvalidPng(error)` | **No** — error tipado |
| Main thread no disponible | `Err(Unavailable)` | No en ese tick — reintentar sin degradar |

`normalize_image_with_original()` deja de caer silenciosamente
al encoder legacy: cuando `original_png` está presente y la
validación falla, devuelve un `AssetError::OriginalPngValidation`
tipado. El encoder legacy queda reservado para el caso
`original_png = None` (Linux/arboard).

## What changes

### Modelo de imagen extendido

`ClipboardImage` se amplía de forma compatible para transportar
los bytes PNG originales y la metadata del pasteboard cuando el
clipboard los ofrece:

- `rgba`: buffer RGBA validado, mismo de hoy. Se mantiene como
  superficie canónica para dedupe y para la supresión de paste.
- `width` / `height`: dimensiones reales de la imagen. Cuando
  viene del PNG original, coinciden con las dimensiones del IHDR
  del PNG (no las del bitmap convertido por AppKit).
- `original_png`: opcional. `Some(bytes)` cuando el clipboard
  ofreció `public.png` y los bytes validaron; `None` para el
  fallback `arboard` y para la nueva ruta nativa TIFF-only.
- `pasteboard_metadata`: opcional. Contiene el resumen
  `png_chunks` (pHYs, iCCP, sRGB, ...) que el scanner del PNG
  reportó, y el `tiff` que el parser de TIFF reportó para
  `public.tiff`. Permite al persistence layer reconstruir un
  PNG con los chunks de resolución + perfil cuando `public.png`
  no los trae pero `public.tiff` sí.

`original_png` y `pasteboard_metadata` nunca se imprimen,
loguean, ni serializan vía `Debug`/`Display`. El `Debug` se
mantiene metadata-only (dimensiones + longitud del buffer +
booleano de presencia + tipo de metadata).

### Bridge nativo `read_png_main_thread`

`macos_clipboard_main_queue` expone:

```rust
pub enum NativePngRead {
    /// `public.png` válido y decodificado; el inner
    /// `ClipboardImage` carga los bytes originales y la
    /// metadata del pasteboard.
    Native {
        image: ClipboardImage,
        metadata: PasteboardImageMetadata,
    },
    /// `public.png` ausente, pero `public.tiff` decodificable.
    /// El image contiene el frame RGBA completo y la metadata
    /// permite reconstruir el PNG antes de persistirlo.
    TiffOnly {
        image: ClipboardImage,
        metadata: PasteboardImageMetadata,
    },
    /// Sin raster nativo utilizable. El composite puede caer al
    /// último fallback `arboard`.
    NoPng {
        metadata: PasteboardImageMetadata,
    },
    /// `public.png` presente pero los bytes fallaron validación.
    InvalidPng(PngValidationError),
}
```

El flujo dentro del dispatch al main thread:

1. `NSPasteboard::generalPasteboard()`.
2. `pasteboard.dataForType(NSPasteboardTypePNG)` y
   `pasteboard.dataForType(NSPasteboardTypeTIFF)` (constantes
   canónicas de AppKit). Si alguna falla, probar el UTI string
   `"public.png"` / `"public.tiff"` como fallback para hosts
   que no declaran el tipo canónico.
3. Construir `PasteboardImageMetadata { png_metadata,
   tiff_metadata, resolution_detected, profile_detected }`.
4. Si no hay bytes de `public.png`, decodificar los bytes de
   `public.tiff` en la misma closure de main thread. Si es válido,
   devolver `Ok(NativePngRead::TiffOnly { image, metadata })`; sólo
   cuando no haya TIFF decodificable devolver `NoPng` y permitir el
   último fallback a `arboard`. Si el salto a la main queue falla,
   devolver `Err(Unavailable)` y reintentar sin degradar.
5. Validar firma, tamaño (`<= MAX_CLIPBOARD_PNG_BYTES`) y
   dimensiones (`<= MAX_CLIPBOARD_IMAGE_DIM`) con
   `clipboard_image_png::validate_png`.
6. El validador devuelve `PngValidationOutcome::Valid | NotPng |
   Invalid(_)`. Mapear:
   - `Valid` → construir `ClipboardImage::with_original_png(...)`
     y devolver `Ok(NativePngRead::Native { image, metadata })`;
   - `NotPng` → `Ok(NativePngRead::NoPng { metadata })`;
   - `Invalid` → `Ok(NativePngRead::InvalidPng(error))`.

El bridge conserva una API síncrona para el caller, pero el salto
off-main se encola directamente en `DispatchQueue::main()` mediante
`exec_async` y un canal acotado. El timeout se traduce en
`Unavailable`; el watcher reintenta y no vuelve silenciosamente a
`arboard` en ese mismo tick.

### Scanner de chunks PNG (`png_metadata_summary`)

`clipboard_image_png.rs` añade un walker de chunks que detecta
metadata preservada sin inspeccionar los píxeles:

- Identifica los chunk types: `IHDR`, `pHYs`, `iCCP`, `sRGB`,
  `gAMA`, `cHRM`, `tEXt`, `iTXt`, `zTXt`, `IDAT`, `IEND`.
- Para `pHYs`, captura el payload (9 bytes: ppuX, ppuY,
  unit specifier) para que el bridge lo reinyecte si hace
  falta.
- Para `iCCP`, captura los bytes del chunk (incluyendo el
  perfil comprimirigido) para reutilizarlo verbatim cuando el
  rebuild preserva el perfil original.
- Para `sRGB`, simplemente declara su presencia (no requiere
  payload explícito).
- El walker respeta la frontera `IDAT`: chunks después de
  `IDAT` se ignoran (pertenecen al frame siguiente en PNGs
  animados, no a la metadata layer del primer frame).

### Parser de TIFF (`parse_tiff_metadata`)

`tiff_metadata.rs` es un walker de IFDs estricto pero
deliberadamente estrecho: lee los cuatro tags que el bridge
consume:

| Tag  | Tipo      | Significado                        |
|------|-----------|------------------------------------|
| 282  | RATIONAL  | XResolution                        |
| 283  | RATIONAL  | YResolution                        |
| 296  | SHORT     | ResolutionUnit (1=None, 2=inch, 3=cm) |
| 34675| UNDEFINED | ICCProfile                         |

El parser nunca decodifica píxeles: sólo metadata. No hay
dependencias nuevas: el parser está hand-rolled sobre el
stream binario y la rationale está documentada en el rustdoc
del módulo.

`TiffMetadata` expone `dpi_x() / dpi_y() / has_resolution() /
icc_profile` con la conversión de unidades correcta (centímetros
→ inches: × 2.54; redondeo para evitar drift bidireccional).

### Persistencia con cuatro rutas de fidelidad

`clipboard_assets::normalize_image_with_original` ahora decide
entre cuatro rutas de persistencia:

| Ruta | Disparador | Bytes persistidos |
|---|---|---|
| `NativeVerbatim` | `original_png` válido + `public.png` ya trae `pHYs` + iCCP/`sRGB` | PNG original byte-por-byte |
| `NativeRebuiltWithMetadata` | `original_png` válido, `public.png` sin `pHYs`/`iCCP`, `public.tiff` tiene la metadata | PNG nuevo con mismos píxeles + chunk `pHYs` reinsertado + chunk `iCCP` con perfil comrimido (o `sRGB` si el `public.tiff` no tenía perfil pero la imagen es sRGB) |
| `TiffMetadataOnly` | `original_png = None`, TIFF decodificable y con metadata | PNG canónico reconstruido con los mismos píxeles + `pHYs`/`iCCP` o `sRGB` del TIFF |
| `LegacyEncoded` | `original_png = None` y nada en `public.tiff` | 8-bit RGBA PNG canónico (sin metadata) |

Cada ruta produce bytes diferentes. El `NormalizedSource`
tracking se persiste en memoria para que el diagnóstico
`ImageCaptureDiagnostic::image_source` reporte la ruta
correcta: `native_png`, `native_png_plus_metadata`,
`tiff_metadata`, `arboard_fallback`.

El rebuild usa la nueva API pública:

```rust
pub fn rebuild_png_with_metadata(
    width: u32, height: u32, rgba: &[u8],
    phys_payload: Option<[u8; 9]>,
    icc_chunk: Option<IccChunk>,
    srgb_intent: Option<u8>,
) -> Result<Vec<u8>, AssetError>
```

Más `deflate_icc_profile(&[u8]) -> Result<Vec<u8>, AssetError>`
para comprimir el payload ICC en el formato zlib que `iCCP`
espera.

### Adapter `MacOsPasteboardClipboard`

`MacOsPasteboardClipboard::read_image_png` mapea `NativePngRead`
a cinco formas tipadas:

- `Native { image, metadata }` → reconstruye
  `ClipboardImage::with_pasteboard_metadata(rgba, w, h, png,
  metadata)` para que la metadata viaje adjunta al `image`.
- `TiffOnly { image, metadata }` → `Some(image)` después de
  adjuntar la metadata. La imagen no tiene `original_png`; el core
  la normaliza e inyecta la resolución/perfil TIFF.
- `NoPng { metadata: _ }` → `Ok(None)` sólo cuando no existe un
  raster TIFF decodificable; ahí sí se permite el último fallback a
  `arboard`.
- `InvalidPng(error)` → `Err(InvalidImage(...))` (sin
  fallback).
- `Err(MainQueueBridgeError::DispatchUnavailable)` →
  `Err(Unavailable { capability: ClipboardReadImage })`.
  Es un resultado soft y reintentable; no se degrada a `arboard`
  en ese tick porque el camino RGBA elimina la metadata nativa.

`MacOsPasteboardClipboard::read_image` usa el mismo bridge pero
devuelve `Err(UnsupportedFormat)` en lugar de `Ok(None)` para
ser simétrico con `arboard::get_image` cuando no hay
representación.

### Composite `CompositeClipboard`

`CompositeClipboard::read_image` y `read_payload` deciden el
fallback según el resultado del rich adapter:

- `Ok(Some(image))` → usar la imagen nativa;
- `Ok(None)` → fallback al plain adapter (sin `public.png`);
- `Err(UnsupportedFormat)` → fallback al plain adapter
  (backend sin transporte de PNG nativo);
- `Err(Unavailable { .. })` → propagar el resultado soft y
  reintentar en el siguiente tick; no invocar `arboard` en ese
  intento porque la degradación sería silenciosa;
- `Err(InvalidImage(_))` → **NO** fallback al plain adapter;
  propagar el error tipado para que la pipeline no guarde
  una imagen degradada;
- otros errores → propagar.

### Pegado: bytes persistidos

`paste::write_image_payload` mantiene el contrato:

1. Lee los bytes persistidos por `asset_ref` (vía
   `ClipboardAssetStore::read_bytes`).
2. Decodifica para validar y armar el fingerprint de
   supresión.
3. Si la sesión soporta `write_image_png`, publica los bytes
   persistidos verbatim.
4. Si no, cae a `write_image` con el bitmap decodificado.

El paso de decodificación es solo para validación y
fingerprinting; los bytes que la sesión recibe son exactamente
los bytes persistidos. Como el rebuild produce una copia
fiel (mismos píxeles, mismos chunks `pHYs`/`iCCP`), el
receptor observa la imagen con su resolución y perfil
originales.

### Diagnóstico metadata-only extendido

`ImageCaptureDiagnostic` se amplía con cinco campos
adicionales:

| Campo | Tipo | Significado |
|---|---|---|
| `representation_source` | `RepresentationSource` | `PngChunks` (la metadata vino del PNG) / `TiffIfd` (del TIFF) / `None` |
| `png_chunks_kind` | `&'static str` | kind_str del scanner (`png_with_phys_and_iccp`, ...) |
| `tiff_resolution_present` | `bool` | el TIFF leg publicó X/YResolution |
| `tiff_icc_profile_present` | `bool` | el TIFF leg publicó ICCProfile |
| `resolution_dpi_x` / `resolution_dpi_y` | `Option<u32>` | DPI resuelto por la metadata |
| `profile_kind` | `ColorProfileKind` | `Iccp` / `Srgb` / `TiffIccProfile` / `None` |

El `ImageSource` también se amplía:

| Valor | Significado |
|---|---|
| `NativePng` | bytes verbatim, ya traían `pHYs` + perfil |
| `NativePngPlusMetadata` | bytes rebuildados con metadata del TIFF leg |
| `TiffMetadata` | sólo TIFF leg, sin PNG |
| `ArboardFallback` | bitmap RGBA clásico |

`log_image_capture_diagnostic` y `log_image_paste_diagnostic`
emiten los nuevos campos gated por las variables de entorno
`CLIPVAULT_DEBUG_IMAGE_CAPTURE=1` y `CLIPVAULT_DEBUG_IMAGE_PASTE=1`.
El diagnóstico nunca lleva bytes, byte length, content hash,
identificador de app fuente, snippet ni rutas absolutas: solo
los campos enumerados.

### Compatibilidad

- **PNG antiguos** (`normalize_image` legacy): siguen siendo
  legibles porque `decode_png` ya acepta todo `(ColorType,
  BitDepth)` legal. Las dimensiones y metadata que faltan en
  esos PNG no se restauran: **no se pueden resucitar bytes
  que ya están en disco**. Esta es una limitación intencional
  del cambio; los assets legacy siguen funcionando pero no
  recuperan metadata perdida.
- **Asset references existentes**: no se renombran ni se
  reescriben. El colector ya es seguro (cambio
  `clipboard-legacy-image-assets`).
- **`asset_ref` invariante**: el formato `clipboard/<sha>.png`
  se conserva.
- **Frontend**: sin cambios. El blob URL lifecycle, el bridge y
  el resolver ya consumen bytes PNG; el cambio de fuente no
  requiere tocar el frontend.
- **Texto / rich text / HTML / RTF**: cero cambios.
- **Favoritos / tags / colecciones / búsqueda / drag-and-drop**:
  cero cambios.
- **Tauri commands**: cero cambios (el bridge devuelve bytes
  PNG como hoy).

### Privacidad

- No se loguean bytes del PNG.
- No se loguean hashes, perfiles, ni rutas absolutas.
- No se guarda contenido del portapapeles en fixtures,
  documentación o tests más allá de un PNG sintético con
  `pHYs` + `tEXt` + dimensiones explícitas, construido en
  un `tempfile::TempDir`.
- El `Debug` y `Display` de `ClipboardImage`,
  `NormalizedImage`, `NativePngRead`, `TiffMetadata`,
  `PasteboardImageMetadata`, `ImageCaptureDiagnostic` y
  `PngMetadataSummary` siguen metadata-only.
- La dependencia nueva `flate2` se justifica: PNG `iCCP`
  requiere zlib-deflate para el payload del perfil, y
  `flate2` ya estaba transitivamente en el grafo vía `png`.

## No-goals

- No se introduce soporte para Windows.
- No se reescribe la pipeline de captura para imágenes que ya
  están en disco (no se migran destructivamente).
- No se cambia el tamaño visual de las cards ni la geometría
  del preview; sólo cambia la fuente de bytes.
- No se agrega persistencia de metadata PNG en SQLite (la
  metadata viaja con los bytes del PNG).
- No se elimina `arboard` como adaptador de imagen; sigue
  siendo el fallback y el camino por defecto en Linux X11.
- No se cambia la ruta de captura de rich text, texto plano,
  HTML ni RTF.
- No se relajan los límites de tamaño ni de dimensiones.
- No se construye un decoder PNG de píxeles en el bridge; se
  sigue usando el `png` crate ya en el grafo.
- No se reemplaza la decisión sobre el path de captura desde
  `read_image()`; la ruta canónica sigue siendo
  `read_image_png()` cuando el backend lo soporta.

## Capacidades afectadas

### Capacidades modificadas

- `clipboard-rich-content`: la captura de imágenes en macOS
  preserva los bytes PNG originales cuando `public.png` está
  disponible; cuando `public.png` no trae resolución ni
  perfil, los píxeles se persisten junto con un PNG
  **rebuildado** que incluye `pHYs`/`iCCP` recuperados del
  leg `public.tiff`; el decoder sigue aceptando cualquier PNG
  legal; el colector seguro y el bootstrap aislado se
  conservan intactos. El cambio introduce además la tipología
  `native_png` vs `native_png_plus_metadata` vs `tiff_metadata`
  vs `arboard_fallback` que se diagnostica a través de
  `ImageCaptureDiagnostic`.

## Impacto esperado

- `crates/clipvault-platform/src/clipboard_image_png.rs`:
  refactor de `validate_png` para devolver el nuevo
  `PngValidationOutcome::{Valid, NotPng, Invalid}` tipado;
  nuevo walker `png_metadata_summary` que detecta pHYs, iCCP,
  sRGB; nuevos helpers `parse_ppu_triple` y `phys_to_dpi`.
- `crates/clipvault-platform/src/tiff_metadata.rs` (nuevo):
  walker de IFD que devuelve `TiffMetadata { x_resolution,
  y_resolution, resolution_unit, icc_profile }`; helpers
  `dpi_x`, `dpi_y`, `has_resolution`.
- `crates/clipvault-platform/src/runtime/macos_clipboard_main_queue.rs`:
  nueva función `read_png_main_thread()` que devuelve
  `BridgeResult<NativePngRead>` con el enum expandido (Native
  con metadata, TiffOnly con raster decodificado, NoPng con
  metadata, InvalidPng); usa `NSPasteboardTypePNG` y
  `NSPasteboardTypeTIFF` constantes.
- `crates/clipvault-platform/src/runtime/macos_clipboard.rs`:
  `read_image` y `read_image_png` propagan el tipado
  `NativePngRead` → `Ok/Err` tipado; helper
  `attach_pasteboard_metadata` que reconstruye
  `ClipboardImage` con metadata adjunta.
- `crates/clipvault-platform/src/runtime/composite_clipboard.rs`:
  `read_image` y `read_payload` propagan `Err(InvalidImage)` sin
  caer al fallback `arboard::get_image()`.
- `crates/clipvault-platform/src/clipboard.rs`:
  nuevo variant `PasteboardImageMetadata` con campos
  `png_chunks` y `tiff`; nuevos accesores en `ClipboardImage`
  para adjuntar metadata, incluyendo imágenes sin PNG original;
  nuevo variant
  `ImageValidationError::InvalidPng { kind }`.
- `crates/clipvault-core/src/clipboard_assets.rs`:
  nuevos tipos `NormalizedSource` y `IccChunk`; nuevas funciones
  `rebuild_png_with_metadata` y `deflate_icc_profile`;
  `normalize_image_with_original` decide entre cuatro rutas de
  fidelidad; `NormalizedImage::was_rebuilt_with_metadata()`
  expone la fuente.
- `crates/clipvault-core/src/image_capture_diagnostic.rs`:
  enum extendido `ImageSource { NativePng, NativePngPlusMetadata,
  TiffMetadata, ArboardFallback }`; nuevos enums
  `RepresentationSource` y `ColorProfileKind`; campos
  adicionales en `ImageCaptureDiagnostic`; rustdoc describe
  cada fuente y cada carrier.
- `crates/clipvault-core/src/history.rs::persist_image`:
  consume el normalizador tipado y emite el diagnóstico de
  captura con los nuevos campos.
- `crates/clipvault-core/src/paste.rs::write_image_payload`:
  emite el diagnóstico de paste con los nuevos campos y
  mantiene el contrato de bytes verbatim.
- Tauri shell y frontend: sin cambios.
- Tests: ampliación de `clipboard_image_png.rs`
  (validador, scanner, helpers `pHYs`/`iCCP`/`sRGB`); nuevo
  `tiff_metadata.rs` con casos de regresión; nuevo
  `pasteboard_png_metadata.rs` (core) con casos para las
  cuatro rutas de fidelidad; ampliación de
  `original_png_bytes.rs` con `png_chunks_kind` estable;
  ampliación del composite con cobertura de `InvalidImage` sin
  fallback.

## Limitación documentada

Cuando `public.png` no trae `pHYs` y `public.tiff` aporta la
resolución, **ClipVault no puede persistir el PNG original
byte-por-byte** porque los bytes del PNG no contienen la
resolución. La persistencia produce un PNG rebuildado que
comparte:

- los mismos píxeles (verificado byte-por-byte a través del
  RGBA decode del PNG original);
- el mismo chunk `pHYs` con la resolución detectada (144 ppi
  cuando aplica);
- el mismo chunk `iCCP` con el perfil ICC detectado (Display
  P3 cuando aplica).

El receiver (otro `sips`, Preview, Safari, ...) observa el
mismo contenido visual con la misma resolución y perfil; el
asset diffiere del `public.png` original sólo en sus chunks
de metadata. Esta es la única ruta donde ClipVault no puede
garantizar identidad byte-por-byte con `public.png`, y está
documentada en el rustdoc del módulo y en este proposal.

## Corrección posterior a la verificación manual

La primera implementación del camino de cuatro fidelidades seguía
conservando 72 ppi en el caso real de macOS. La auditoría confirmó
dos variantes que el diseño inicial no distinguía:

1. `public.png` puede incluir un `pHYs` válido pero predeterminado
   de 72 ppi. Su presencia no significa que sea la resolución física
   de la imagen fuente.
2. Algunos TIFF publicados por Apple/ImageIO llevan la resolución
   en una forma que el parser IFD estrecho no puede resolver por sí
   solo, aunque `NSBitmapImageRep` la expone correctamente mediante
   `pixelsWide`, `pixelsHigh` y `size`.

La corrección establece que la pareja X/Y obtenida del TIFF/AppKit es
autoritativa cuando existe. El bridge decodifica `public.tiff` en el
main thread y calcula `pixels / logical_points * 72`, con validación
de finitud, positividad y límites. Esos dos enteros DPI se incorporan
al resumen metadata-only del TIFF. El core convierte esa resolución
directamente a `pHYs` y la antepone al `pHYs` del PNG; por lo tanto,
PNG 72 ppi + TIFF/AppKit 144 ppi produce un asset nuevo con 144 ppi,
los mismos píxeles y, cuando está disponible, el perfil ICC.

La decisión es deliberadamente no destructiva: no se cambian
`asset_ref`, hashes de entradas existentes ni archivos ya guardados.
Sólo las capturas nuevas utilizan la resolución corregida. Las
capturas antiguas que ya quedaron en 72 ppi no pueden recuperar una
metadata que no se conservó en sus bytes.
