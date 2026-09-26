## Why

Las cards del historial remoto no conservan la geometría ni el modelo de
selección del historial local: son más grandes, las flechas desplazan la rail
sin seleccionar otra card y la fecha/menú quedan a distinta altura según el
tipo de captura. Además, las cards remotas no identifican la aplicación que
originó la captura y la importación no conserva su nombre ni icono, por lo que
la colección del peer muestra un origen desconocido.

## What Changes

- Igualar el tamaño fijo de las cards de preview remoto al de las cards locales.
- Añadir selección visible con borde azul y navegación de card en card con
  `ArrowLeft` / `ArrowRight`, sin que esas teclas desplacen la rail cuando la
  navegación está activa.
- Fijar la fila de tiempo transcurrido y menú en el borde inferior de todas
  las cards remotas, tanto de texto como de imagen.
- Mostrar el nombre validado de la aplicación fuente directamente en cada
  fila de browse de texto/imagen. No transferir ni mostrar iconos en previews,
  ni hacer una consulta de red adicional por tarjeta; los thumbnails siguen
  sin incluir metadata de aplicación.
- Incluir el nombre y, cuando exista, el PNG validado del icono de aplicación
  también en la transferencia explícita de importación. Guardar el icono en el
  almacén local de `application-icons/` y asociar su referencia y nombre a la
  procedencia del peer, sin sobrescribir metadata de una captura local
  deduplicada.
- Mantener fallbacks honestos para peers antiguos y nombres ausentes, sin
  bloquear el preview ni la importación del contenido principal.
- Mostrar la atribución de una captura importada también en `Historial`,
  usando la procedencia más reciente y sin revelar el identificador del peer.
- En tarjetas importadas, mantener el nombre de la aplicación como etiqueta
  accesible/tooltip del icono, no como texto visible dentro de la tarjeta.

## Capabilities

### New Capabilities

- `peer-source-app-presentation`: capability advertisement, bounded source
  names in authenticated browse rows, and safe local persistence of imported
  application icons.

### Modified Capabilities

- `peer-text-history-browser`: tamaño, selección, navegación por teclado y
  alineación inferior de metadata de las cards remotas.
- `clipboard-history-cards`: excepción acotada para mostrar la atribución de
  aplicación en una captura importada dentro de su colección vinculada.
- `peer-import-collection-visibility`: proyección local del origen de
  aplicación por importación, con el icono real local y un icono estático de
  importación como fallback.
- `peer-text-import`: nombre e icono de aplicación validados en la ruta de
  importación explícita y persistencia asociados a la procedencia del peer.
- `peer-image-import`: el mismo contrato para la importación explícita de
  imágenes, con transporte acotado del icono.

## Impact

- Frontend Svelte: `RemotePreviewCard`, `RemoteHistoryRail`, el modelo de
  selección de rail y la presentación de `HistoryCard` para procedencia de
  importación.
- Core/transporte: capability aditiva y nombre validado en las respuestas de
  browse de texto e imagen, sin endpoint/icono adicional para previews;
  respuestas de fetch explícito conservan el transporte del icono para
  importación.
- SQLite/proyección de organización: nombre y referencia de icono locales,
  opcionales y ligados a cada fila de procedencia `remote_imports`, sin
  sobrescribir metadata local de entradas deduplicadas.
- Assets de icono: recepción sólo de PNG acotados y decodificados, escritura
  bajo el namespace local `application-icons/` con nombre generado localmente;
  nunca se aceptan rutas ni referencias entregadas por el peer.
- No se esperan dependencias externas nuevas ni cambios en el payload de drag,
  clipboard, pegado o transferencia de thumbnails.
