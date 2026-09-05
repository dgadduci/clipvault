# Propuesta: restaurar edición de títulos sin romper drag-and-drop

## Problema

Las cards deben permitir editar su título mediante doble click, pero ese flujo
dejó de responder. Además, el menú actual no ofrece la entrada `Editar título`
que existía anteriormente; sólo aparece `Restaurar título`.

La card comparte la superficie del título con el gesto de arrastre. El
controlador de pointer/mouse puede capturar o cancelar el gesto antes de que
el navegador entregue `click`/`dblclick`, por lo que una corrección que
simplemente agregue el menu item podría dejar el doble click roto o causar una
regresión del drag-and-drop.

## Solución

- Restaurar `Editar título` como menu item de todas las cards.
- Hacer que el doble click del título abra el editor inline existente.
- Mantener Enter/F2, confirmar, cancelar, Escape, validación y persistencia
  existentes.
- Compartir una única función de entrada al editor para doble click, teclado y
  menú; no duplicar el flujo de persistencia.
- Ajustar la arbitraje del gesto en el controlador de drag para que un click o
  doble click por debajo del umbral conserve sus eventos nativos, mientras que
  un movimiento que supera el umbral active el drag, ghost, bloqueo de
  selección y drop actuales.

## Decisiones

1. No se agrega una tabla, migración ni comando nuevo: se reutiliza
   `clipvault_set_entry_title` y el servicio existente.
2. `Editar título` debe estar disponible para cards de texto, rich text e
   imagen, porque todas usan la misma `HistoryCard`.
3. La edición se realiza en el mismo título inline; no se crea un modal ni una
   segunda representación.
4. El menú cierra al iniciar la edición. Guardar sólo cierra el editor después
   de una respuesta exitosa.
5. El título visible no forma parte del payload de drag; el payload continúa
   transportando únicamente el id opaco de la entrada.

## Fuera de alcance

- No cambiar el contenido de la captura, tipo, timestamps, tags, colecciones,
  favoritos, paste, búsqueda o fuente de aplicación.
- No cambiar la semántica de Delete ni de Quitar de esta colección.
- No cambiar el tamaño/layout de las cards salvo lo imprescindible para que el
  editor inline sea usable.
- No cambiar el backend de persistencia del título ni agregar dependencias.
- No modificar el pipeline de imágenes ni limpiar `~/.clipvault`.
- No modificar la arquitectura ni el contrato de platform-permission-guidance.
