# Proposal: peer-image-preview-thumbnails

## Why

El cambio `peer-image-import` permite navegar capturas de imagen remotas, pero
por decisión explícita muestra el mismo placeholder estático para todas ellas.
La imagen original sólo se transfiere cuando el usuario pulsa `Importar`.
Ahora que la importación y su matriz manual están verificadas, el siguiente
paso útil es reconocer visualmente cada captura antes de importarla sin
convertir la navegación en una descarga de imágenes completas.

## What Changes

- Mostrar una miniatura PNG derivada de la imagen remota cuando su card entra
  en el viewport del rail.
- Mantener las páginas de historial metadata-only y transportar miniaturas por
  una solicitud autenticada, independiente y acotada.
- Mantener el placeholder estático durante la carga y como fallback ante peers
  antiguos, falta de capability, error, respuesta obsoleta o miniatura inválida.
- Conservar `Importar` como única acción que obtiene y persiste el PNG original.
- No escribir miniaturas en disco/SQLite ni mutar clipboard, historial local,
  pegado, drag payload o assets locales.

## Fuera de alcance

- Vista previa a resolución completa, zoom, edición o descarga automática del
  PNG original.
- Cambios al formato de las páginas de historial, al pipeline de importación,
  a sus deduplicaciones o a las colecciones vinculadas.
- Cache persistente, migraciones SQLite, sincronización fuera de la LAN,
  llamadas cloud, telemetría o nuevas preferencias de usuario.
- Miniaturas para capturas locales o thumbnails de otros tipos de contenido.

## Resultado esperado

Un peer actualizado anuncia la capability aditiva `image_preview_thumbnail`.
El cliente solicita una miniatura sólo para una imagen visible; el host vuelve
a autorizar y validar la fila y genera en memoria un PNG reducido con límites
de dimensiones y bytes. Los clientes/hosts anteriores conservan el
placeholder, y `Importar` continúa transfiriendo la imagen canónica completa.
