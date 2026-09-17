# Propuesta: corregir el atajo de edición de capturas textuales

## Problema

`editable-text-captures` agregó el indicador del atajo y un matcher local en
`HistoryCard.svelte`, pero la prueba manual confirmó que `Ctrl+E` no abre el
editor en Linux y `Cmd+E` no lo abre en macOS. El matcher actual depende de que
el evento llegue al `article` de la card y sólo acepta `ctrlKey`; no comparte
la tabla de modificadores usada por los atajos desktop existentes.

## Objetivo

Hacer que el atajo de edición funcione de forma determinística en el desktop
principal: Linux Wayland y X11 con `Ctrl+E`, y macOS con `Cmd+E`.

El atajo debe abrir el mismo `EntryTextEditorModal` que la acción `Editar
captura`, usando el mismo entry elegible y sin alterar el backend, la
persistencia ni el drag-and-drop. Si el entry seleccionado no es editable, el
atajo debe consumirse y mostrar un aviso claro, sin reabrir un editor anterior.

## Alcance

- Agregar o corregir un helper puro de matching y etiquetas de atajo que use
  la plataforma ya detectada por los diagnósticos existentes.
- Registrar un único camino de teclado a nivel desktop/rail, o reutilizar el
  listener compartido existente, para que el atajo no dependa exclusivamente
  del foco directo en el `article`.
- Resolver la card objetivo desde el entry seleccionado o desde la card que
  actualmente tiene el foco, según el contrato vigente del rail.
- Delegar la apertura al estado/flujo existente del modal y conservar el
  retorno de foco al disparador.
- Mostrar una indicación coherente con la plataforma: `Ctrl+E` en Linux y
  `⌘E` en macOS, con `aria-keyshortcuts` equivalente.
- Agregar regresiones automáticas y validar manualmente en Wayland, X11 y
  macOS.

## Fuera de alcance

- Cambios en `TextHistoryService`, SQLite, comandos Tauri o persistencia.
- Nuevos listeners por cada card, listeners globales duplicados o una nueva
  dependencia de plataforma.
- Cambios en selección, tamaño, orden, menú, contenido del modal o
  `pointerDragAndDrop.ts`, salvo el cableado mínimo imprescindible.
- Hacer que el atajo intercepte inputs, textareas, `contenteditable`, menús,
  diálogos u otros controles interactivos.
- Cambiar el atajo de Quick Paste, búsqueda o previsualización.

## Criterio de aceptación

Con una captura textual elegible seleccionada o enfocada, `Ctrl+E` abre el
editor en Linux tanto bajo Wayland como X11 y `Cmd+E` lo abre en macOS. El
menú muestra el modificador correspondiente, el flujo no se duplica y los
eventos originados en controles interactivos siguen su comportamiento normal.
Las cards de imágenes/rich text no abren el editor; muestran un aviso
informativo sin mutar datos; y todos los baselines de drag-and-drop permanecen
intactos.

## Corrección posterior a la prueba manual

La prueba manual confirmó que el atajo ya abre el modal, pero detectó que la
primera captura abierta queda reutilizada: después de seleccionar otra card,
el atajo vuelve a mostrar o editar la entrada anterior. Se agrega una
corrección de identidad de target para que cada solicitud transporte y valide
el ID opaco correcto y el modal siempre se inicialice con la entrada
solicitada.

## Corrección posterior: aviso para capturas no editables

La prueba manual posterior detectó que, al invocar el atajo sobre una imagen o
una captura de texto enriquecido, el listener global retornaba sin consumir el
evento. El handler de la card que conservaba el foco podía entonces reabrir el
último editor textual. La corrección consume el atajo, muestra un aviso
informativo reutilizando `Modal.svelte` y evita cualquier solicitud de edición
o mutación de persistencia.

## Corrección posterior: una sola card resaltada

La prueba manual confirmó el retorno de foco a la card editada, pero detectó
que, al cambiar después la selección con las flechas, el foco residual dejaba
la card anterior resaltada junto con la nueva. El estilo de foco azul debe
aplicarse únicamente cuando la card enfocada sigue siendo la selección actual.

La verificación posterior mostró que el WebView podía pintar además un anillo
nativo rojo en la card con foco residual. Ese anillo debe suprimirse para que
la interfaz tenga una sola marca visual: la card actualmente enfocada y
seleccionada, recuadrada en azul.
