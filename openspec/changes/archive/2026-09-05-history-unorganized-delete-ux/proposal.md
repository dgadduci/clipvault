# Propuesta: borrado general de Historial claro y contextual

## Problema

El basurero general está en la barra del desktop y puede interpretarse como
una acción sobre la colección que el usuario está viendo. Sin embargo, su
operación real es más específica: elimina sólo entradas no favoritas y sin
ninguna colección de usuario. Cuando el usuario está dentro de una colección,
la acción no corresponde al contexto visible y puede generar temor a borrar
contenido organizado.

## Solución

- Mostrar el basurero general únicamente con `Historial` como colección activa.
- Ocultarlo por completo, incluido del árbol accesible y del orden de tabulación,
  al seleccionar una colección de usuario.
- Usar el tooltip y nombre accesible `Eliminar capturas no organizadas`.
- Mantener la confirmación existente, pero hacer que su título, resumen y
  botón describan de forma consistente que sólo se eliminan entradas no
  favoritas y sin colecciones de usuario.
- Reutilizar `unorganizedClearableCountCommand`,
  `clearUnorganizedHistoryCommand` y el flujo de confirmación existente, sin
  crear otra operación de borrado.

## Decisiones confirmadas

1. Una entrada favorita nunca entra en este borrado general.
2. Una entrada asociada a una o más colecciones de usuario nunca entra en este
   borrado general.
3. `Historial` es la vista predeterminada y la única vista desde la que se
   ofrece esta acción global.
4. `Quitar de esta colección` permanece disponible en las cards cuando el
   contexto lo permite y no se reemplaza por el basurero general.
5. El borrado individual del menú de una card permanece disponible y conserva
   su semántica de eliminación global de esa entrada.

## Fuera de alcance

- No cambiar consultas, transacciones, migraciones ni contratos de Rust/core.
- No cambiar qué filas elimina `clearUnorganizedHistoryCommand`.
- No cambiar el borrado individual de una card.
- No cambiar la operación `Quitar de esta colección`.
- No cambiar favoritos, tags, colecciones, retención, búsqueda, captura,
  pegado, imágenes ni drag-and-drop.
- No agregar dependencias, red, telemetría ni listeners globales.

## Criterios de aceptación

- En Historial el basurero es visible, tiene tooltip claro y abre la
  confirmación sin mutar datos antes de confirmar.
- En cualquier colección de usuario el basurero no existe en el DOM y no puede
  ejecutarse mediante teclado ni lector de pantalla.
- Cancelar no ejecuta ningún comando.
- Confirmar invoca una sola vez el comando existente y actualiza la lista y el
  contador.
- Favoritos, entradas organizadas, imágenes y membresías no sufren regresiones.
