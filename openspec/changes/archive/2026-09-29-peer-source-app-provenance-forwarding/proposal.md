# Propuesta: reenviar la atribución de aplicación de capturas importadas

## Why

Cuando un equipo importa una captura desde un par, el nombre y el icono de la
aplicación origen se guardan correctamente en la procedencia de esa
importación. La fila local de la captura conserva sin cambios sus campos de
origen propios. Al compartir la misma captura desde ese equipo hacia otro par,
las rutas de historial e importación consultan sólo los campos propios de la
fila; por eso el nuevo receptor pierde el nombre y el icono y muestra el origen
como desconocido.

El resultado rompe la atribución al pasar por más de un equipo aunque cada
importación individual haya recibido metadata válida.

## What Changes

- Al compartir una fila, preferir su metadata de aplicación propia cuando
  exista y sea válida.
- Si la fila no tiene metadata propia, resolver la procedencia de importación
  más reciente que contenga metadata de aplicación para esa misma fila.
- Incluir el nombre validado en las filas de historial remoto y el nombre más
  el PNG validado sólo en la respuesta de una importación explícita.
- Mantener la metadata reenviada como procedencia local del nuevo par; no
  sobrescribir `clipboard_entries.source_app*` ni enviar el peer_id de origen.
- Cubrir el flujo de reenvío para texto e imágenes, con fallback desconocido
  cuando falte metadata válida.

## Fuera de alcance

- Cambiar el protocolo de confianza, discovery, capacidades o pairing.
- Incluir iconos o identificadores de aplicación en las filas de exploración.
- Cambiar cómo se deduplica contenido, se guardan iconos o se muestran las
  colecciones vinculadas a cada par.
- Agregar migraciones, dependencias o almacenamiento de datos de aplicación
  fuera de las procedencias que ya existen.
