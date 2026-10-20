# Proposal: rotate image captures from preview

## Why

El preview compartido de Desktop y QuickPaste permite inspeccionar una captura
de imagen, pero no corregir su orientación. Una rotación que solo se aplique
visualmente en el modal se perdería al cerrarlo o reiniciar ClipVault.

## What Changes

- Agregar dos botones accesibles al modal de preview de imágenes: giro de 90°
  horario y giro de 90° antihorario.
- Persistir cada giro sobre la captura existente, conservando su ID y sus
  asociaciones, notas, título, favorito y metadata de origen.
- Mantener los assets de imagen inmutables y compartibles: guardar el PNG
  transformado como un asset nuevo y actualizar únicamente la referencia y
  metadata de la captura rotada, conservando sus metadatos visuales de
  resolución y color cuando estén presentes.
- Aplicar la funcionalidad al preview compartido de Desktop y QuickPaste.
- Mostrar un spinner accesible durante el giro y hasta que termine la recarga
  del PNG resultante.
- Localizar etiquetas accesibles y errores en `en`, `es`, `pt`, `de` y `fr`.

## No Goals

- No cambiar la orientación del portapapeles ni reescribir la imagen en el
  sistema operativo.
- No rotar capturas de texto o rich text.
- No agregar dependencias ni migraciones de base de datos.

## Impact

- Modifica el preview compartido, el servicio de historial del core, el
  repositorio de capturas, el comando Tauri y los catálogos de traducción.
- Reutiliza `image` y el almacenamiento PNG content-addressed existente.
- La rotación actualiza el contenido de imagen y su hash; el resto de los
  metadatos de la captura permanece igual.
