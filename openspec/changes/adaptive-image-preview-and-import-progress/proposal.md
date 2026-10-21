# Proposal: adaptive image preview and import progress

## Why

El preview compartido centra la imagen, pero su tarjeta ocupa todo el ancho
disponible incluso cuando la imagen es vertical. La tarjeta tampoco permite que
el usuario ajuste su tamaño. Además, la revisión del orden de importación solo
muestra progreso al terminar cada archivo; una imagen grande puede parecer
detenida mientras todavía se están leyendo sus bytes.

## What Changes

- Adaptar el tamaño de la tarjeta del preview de imágenes a las dimensiones
  renderizadas de la imagen, manteniendo la proporción, el límite de pantalla y
  10 px de separación entre la imagen y cada borde interior del modal.
- Permitir redimensionar el modal de imagen con un control accesible por mouse,
  pantalla táctil y teclado, manteniéndolo dentro del viewport disponible.
- Cerrar el preview únicamente desde su botón de cierre o con `Escape`; la
  interacción con el fondo del modal no debe cerrarlo.
- Mostrar una barra de progreso con porcentaje en el modal de revisión mientras
  se importan imágenes, ubicada a la izquierda de los botones de acción y
  conservando el texto «Importando…» del botón principal. El porcentaje
  combinará bytes leídos con archivos procesados y llegará a 100 al terminar
  todo el procesamiento.
- Comunicar bytes leídos, bytes totales y archivos procesados con un
  identificador opaco de sesión; no transmitir rutas ni contenido de imágenes.

## No Goals

- No cambiar el tamaño fijo de la ventana QuickPaste.
- No persistir el tamaño elegido para el modal entre previews o sesiones.
- No alterar el preview de texto o texto enriquecido.
- No cambiar el orden, la deduplicación ni el resultado de la importación.
- No enviar rutas, nombres de archivo, hashes ni datos de imagen al evento de
  progreso; los contadores de bytes son metadatos numéricos y no incluyen
  contenido.

## Impact

- Modifica el componente compartido `ClipboardPreview.svelte`, el importador de
  imágenes del core y el adaptador Tauri, el modal de orden de importación, la
  integración frontend y los cinco catálogos de traducción. El seguimiento de
  bytes se implementa en el lector del core sin agregar dependencias.
- No agrega dependencias ni cambia la persistencia de capturas o assets.
