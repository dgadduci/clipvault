# Proposal: import-local-image-files

## Why

ClipVault ya conserva capturas de imagen en su almacén local PNG, pero el
usuario no puede incorporar imágenes que ya están guardadas en su equipo.

## What Changes

- Añadir un botón accesible con icono de foto a la barra de Desktop, entre el
  botón PDF y el botón de eliminar.
- Abrir el selector nativo para elegir uno o varios archivos de imagen comunes
  y mostrar una revisión que permita fijar el orden final antes de importar.
- Decodificar PNG, JPEG, WebP, GIF, BMP y TIFF en el backend y persistir cada
  imagen mediante el almacén PNG y la deduplicación existentes.
- Añadir cada captura a Historial y, si existe, a la colección local activa.
- Excluir HEIF/HEIC. Los archivos se procesan localmente; sus rutas y bytes no
  se envían al frontend ni se conservan como referencia de la captura.
- Mantener el orden elegido en la revisión de la selección para las capturas
  nuevas que aparecen en el historial. Como el selector nativo no garantiza el
  orden de clics entre plataformas, la revisión permite ordenar explícitamente
  los archivos antes de confirmar.
- Mostrar el botón existente para crear capturas manuales de texto sólo con su
  icono en la barra superior, manteniendo su nombre accesible y su atajo.
- Localizar etiquetas, accesibilidad, selector y resumen de importación en los
  cinco idiomas disponibles.

## Capabilities

### New Capabilities

- `local-image-import`: selección e importación local de imágenes como
  capturas PNG.

### Modified Capabilities

- `desktop-shell-layout`: nuevo botón de importación entre PDF y eliminar.

## Impact

- El core Rust incorpora un decodificador de formatos raster con `image`
  `=0.25.10`, usando solo las características necesarias y límites compatibles
  con el tamaño admitido por el almacén de imágenes.
- Tauri presenta el selector nativo y conserva temporalmente las rutas en
  memoria; el frontend recibe identificadores opacos y nombres base para
  revisar el orden, y solo conteos al terminar la importación.
- No requiere cambios de esquema de base de datos ni altera el formato PNG ya
  persistido.
