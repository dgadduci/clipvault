# Diseño: card-title-editing-regression

## Flujo único de edición

`HistoryCard` debe conservar una única máquina de estado local para:

```text
displayTitle → editingTitle/titleDraft → validating → saving → displayTitle
```

Las tres entradas deben llamar a la misma función:

```text
doble click sobre el título ─┐
Enter/F2 con el título       ├─→ startEditTitle()
Editar título en el menú   ─┘
```

El menú debe contener `Editar título`. `Restaurar título` puede mantenerse
para títulos personalizados y debe seguir usando la misma validación y
`setEntryTitleCommand({ title: null })`. No debe existir una ruta alternativa
que escriba directamente en SQLite o que duplique el estado del editor.

El editor conserva:

- input inline con foco y selección del valor actual;
- confirmación por Enter o icono;
- cancelación por Escape o icono;
- validación de longitud y texto vacío según `validateTitle`;
- estado busy que evita doble escritura;
- error visible y seguro si falla el bridge;
- actualización inmediata sólo después de respuesta exitosa;
- persistencia tras cambiar de colección y reiniciar.

## Arbitraje entre doble click y drag

El título es una superficie de interacción dual. El controlador debe seguir
este contrato:

```text
pointerdown/mousedown sobre título
  ├─ movimiento menor al umbral + pointerup
  │    → conservar click/dblclick nativo; no ghost; no sesión de drag
  └─ movimiento >= umbral
       → reclamar el gesto como drag; pointer capture desde ese momento;
         bloquear selección; crear ghost; emitir drag-over/drop
```

La implementación concreta puede diferir, pero debe evitar capturar o cancelar
prematuramente el gesto del título de forma que suprima `dblclick`. En
particular:

- no llamar `preventDefault()` sobre el título antes de cruzar el umbral;
- no iniciar una sesión de drag para un click que termina sin movimiento;
- no crear ghost para un doble click;
- al cruzar el umbral, activar el controlador singleton existente y hacer
  cleanup completo en pointerup, mouseup, pointercancel, Escape y blur;
- mantener el fallback `mousedown/mousemove/mouseup` de WebKit/Tauri;
- los controles del editor (`input`, confirmar y cancelar) siguen siendo
  interactivos y nunca inician drag;
- el menú, pin, tags, colecciones y otros controles siguen excluidos del drag.

El controlador debe continuar permitiendo arrastrar desde el título después de
cruzar el umbral. La corrección no debe convertir el título en una zona que
funcione para editar pero no para arrastrar.

## Accesibilidad y focus

- El título mantiene su nombre accesible y foco visible.
- `Editar título` tiene `role="menuitem"`, nombre claro y test id estable.
- El menú se cierra exactamente una vez al iniciar la edición.
- El input recibe foco al abrirse.
- Escape dentro del input cancela la edición y no cancela otra acción del
  desktop de forma inesperada.
- La interacción por teclado sigue funcionando sin mouse.

## Persistencia y privacidad

El frontend debe reutilizar `setEntryTitleCommand`; no se envía contenido de la
captura para editar un título. Los logs y errores no deben incluir contenido,
snippets, hashes, referencias de assets, rutas ni source app.

Las cards de imagen deben conservar sus campos y seguir cargando sus thumbnails
antes y después de editar el título, reiniciar y cambiar de colección. El
editor sólo modifica `title`.

## Tests requeridos

### Frontend/editor

- doble click sobre el título abre el editor;
- click simple no abre el editor;
- Enter y F2 sobre el título abren el editor;
- el menú contiene `Editar título` y abre el mismo editor;
- confirmar por Enter y por icono persiste una sola vez;
- cancelar por Escape e icono no persiste;
- título vacío restaura el título por defecto según el contrato existente;
- título demasiado largo conserva el valor anterior y muestra error;
- rechazo del bridge deja el editor recuperable;
- el título se conserva tras remount/cambio de colección/reinicio simulado;
- la acción funciona en cards de texto, rich text e imagen.

### Drag-and-drop

- click/doble click sobre título entrega click/dblclick y no crea sesión;
- pointer drag sobre título después del umbral crea ghost y drop válido;
- mouse fallback sobre título también arrastra y hace drop;
- input y botones del editor no inician drag;
- Escape, blur, pointercancel, pointerup y mouseup limpian captura, ghost,
  selección y sesión;
- se mantienen drop en colección scrolleable y payload sólo con entry id.

### Regresiones

- imágenes persistidas siguen visibles;
- tags, favoritos, colecciones y búsqueda no cambian;
- Delete y Quitar de esta colección siguen funcionando;
- no hay listeners o menús duplicados;
- no hay filtración de contenido sensible.
