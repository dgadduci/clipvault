## Why

ClipVault ya captura y clasifica texto, pero el clipboard también puede
contener imágenes. Actualmente el watcher las ignora y las cards sólo pueden
mostrar previews textuales. El siguiente incremento del MVP debe permitir
guardar una imagen localmente, reconocerla en el historial y pegarla de vuelta
sin introducir nube, OCR o una segunda interfaz visual.

La referencia de producto es la misma galería horizontal de cards que ya se
implementó para el historial textual. Una imagen debe ser otra entrada de esa
galería, no un flujo paralelo.

## What Changes

- Extender el contrato de clipboard del core/plataforma con payload textual o
  imagen raster.
- Capturar imágenes cuando no exista un texto utilizable que deba conservar el
  comportamiento textual actual.
- Normalizar la imagen a PNG y guardarla como asset local bajo el directorio de
  datos de ClipVault.
- Persistir en SQLite metadata de imagen y una referencia relativa segura al
  asset, manteniendo compatibles las filas textuales existentes.
- Reutilizar el hash para deduplicar imágenes y reutilizar el asset cuando ya
  exista.
- Mostrar thumbnails en `HistoryCard` y en la lista de quick-paste usando el
  mismo rail, selección y acciones actuales.
- Permitir que `paste_entry` escriba imágenes en el clipboard antes de
  ejecutar el pegado sintético.
- Informar capacidades y errores de lectura/escritura de imagen por sesión,
  diferenciando macOS, Linux X11 y Linux Wayland sin prometer soporte que el
  adapter no pueda verificar.
- Limpiar assets no referenciados al borrar o retener entradas.

## Capabilities

### New Capabilities

- `clipboard-rich-content`: captura local de imágenes raster, assets seguros,
  thumbnails y pegado.

### Modified Capabilities

- `clipboard-text-history`: una imagen soportada deja de tratarse como un
  evento textual ignorado; el flujo de texto y su deduplicación permanecen
  compatibles.
- `desktop-platform-integration`: el watcher, la matriz de capacidades y el
  paste pipeline reconocen lectura/escritura de imágenes.

## Non-Goals

- No capturar HTML binario, RTF, archivos, audio, vídeo ni formatos arbitrarios.
- No convertir una imagen a texto, ejecutar OCR ni hacer reconocimiento visual.
- No incluir imágenes en la búsqueda textual ni añadir búsqueda semántica.
- No implementar edición, compresión configurable, anotaciones o previews
  remotos.
- No enviar bytes, hashes, contenido ni metadata a la red.
- No modificar la lógica de blacklist, permisos, hotkeys o el contrato de
  quick-paste salvo la extensión necesaria para transportar una imagen.
- No agregar un framework de UI ni una librería de iconos.
