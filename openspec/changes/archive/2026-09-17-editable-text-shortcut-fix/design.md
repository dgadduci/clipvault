# Diseño: atajo multiplataforma para editar capturas textuales

## 1. Diagnóstico y decisión

La implementación actual declara `on:keydown` en el `article` de cada card y
usa un matcher que sólo acepta `ctrlKey`. Esto tiene dos problemas:

1. una card seleccionada puede no tener el foco del DOM, por lo que su
   `article` nunca recibe el evento;
2. macOS entrega Command mediante `metaKey`, no `ctrlKey`.

La corrección debe centralizar la decisión del atajo en un helper puro y
conectarla al listener de teclado compartido del desktop o del rail. El helper
no debe depender de Tauri, del navegador real ni de una sesión Wayland/X11.

## 2. Contrato del matcher

Reutilizar `searchShortcutPlatform` / `platformFromDiagnostics` o extraer un
helper pequeño común, evitando una segunda detección de plataforma. El
contrato mínimo es:

- `macos`: aceptar sólo `metaKey + E`;
- `other`: aceptar sólo `ctrlKey + E`;
- rechazar `Alt`, `Shift` y el modificador contrario;
- comparar `event.key` de forma insensible a mayúsculas;
- producir la etiqueta visible y el atributo accesible desde la misma fuente:
  `⌘E` / `Meta+E` en macOS y `Ctrl+E` / `Control+E` en Linux.

El helper debe aceptar objetos mínimos tipables en tests, además de
`KeyboardEvent`, y no debe ejecutar efectos secundarios.

## 3. Routing y ownership

Debe existir un único listener compartido para el atajo en el desktop/rail. Se
puede extender el listener de `App.svelte` o `HistoryCardRail.svelte`, siempre
que se mantenga una sola suscripción con cleanup idempotente.

El routing debe:

1. ignorar el evento si hay un modal/diálogo abierto o si el target está en
   `input`, `textarea`, `select`, `contenteditable`, `button`, menú, item de
   menú, editor de título, selector, chip interactivo u otro control protegido;
2. identificar una card focalizada mediante el ancestro
   `[data-testid="history-card"]` cuando corresponda;
3. cuando exista un `selectedEntryId` vigente, usarlo como identidad
   autoritativa; el ancestro de la card focalizada sólo funciona como fallback
   cuando no hay selección, porque el foco puede permanecer en la última card
   editada después de seleccionar otra;
4. verificar que el entry exista en el conjunto visible;
5. si el entry existe pero `isEditableTextEntry(entry)` es falso, consumir el
   atajo, abrir un diálogo informativo reutilizando el shell `Modal.svelte` y
   no emitir la solicitud de apertura ni ejecutar comandos Tauri. El diálogo
   sólo debe comunicar que las capturas que no son texto plano no pueden
   editarse y no debe renderizar contenido de la captura;
6. ejecutar `preventDefault()` y `stopImmediatePropagation()` cuando el
   atajo fue aceptado, incluyendo la apertura del aviso para una captura no
   editable, para impedir que el handler de una card con foco residual
   reabra el último editor;
7. invocar el mismo evento/callback/request que abre
   `EntryTextEditorModal.svelte`, sin duplicar el estado de apertura.

Si se conserva el handler local de `HistoryCard` por compatibilidad, debe
delegar al mismo contrato sin producir dos aperturas para un solo keydown.
No se permite agregar un listener global por card.

El estado `textEditorOpen` y el ciclo de vida del modal siguen teniendo la
misma única fuente de verdad ya establecida en `HistoryCard`. La corrección
del shortcut sólo debe transportar el ID opaco de la entrada o una solicitud
tipada; nunca el contenido.

## 4. Menú y accesibilidad

El item existente `Editar captura` permanece disponible sólo para entradas
elegibles. Su ayuda visible y `aria-keyshortcuts` deben derivarse de la misma
plataforma que el matcher, para evitar que el menú anuncie `Ctrl+E` cuando el
atajo real es `Cmd+E`.

El cambio conserva foco, Escape, retorno de foco, gating de imágenes/rich text,
geometría de cards y orden del menú.

## 5. Verificación multiplataforma

Los tests puros cubren ambos modificadores y combinaciones inválidas. Los
tests de integración/source-level cubren el listener único, selección/foco,
gating de targets y ausencia de doble apertura. La prueba manual cubre la
misma card en GNOME Wayland, GNOME X11 y macOS, incluyendo reabrir el editor y
escribir dentro de controles sin que el shortcut interfiera.

## 6. Corrección de identidad del target

La prueba manual encontró que el flujo conserva la primera captura abierta
cuando luego se selecciona otra. La corrección debe tratar cada solicitud como
una operación dirigida a un `entryId` explícito:

- el evento compartido debe transportar sólo `{ entryId }`;
- `HistoryCardRail` debe resolver el `article` mediante el registro indexado
  por ese ID y despachar la solicitud únicamente a esa card;
- `HistoryCard` debe validar que el ID recibido coincide con `entry.id` antes
  de llamar a `openTextEditor`;
- el modal debe recibir la entrada y el `displayTitle` de esa card, no un
  snapshot o closure de la primera card que abrió el editor;
- al cambiar de target, `draft`, `baselineEntry`, ids accesibles y foco deben
  corresponder a la nueva entrada. No se puede conservar el draft de la
  captura anterior ni enviar su contenido al backend;
- al cerrar el editor por una acción del usuario, `returnFocusTo` debe apuntar
  al `<article>` de la card que se estaba editando, el rail debe conservar o
  establecer su `selectedEntryId` y el foco debe volver allí; así la card usa
  el recuadro azul de navegación y no el foco/estado visual de otra card;
- al cerrar de forma forzada una card A para abrir una solicitud dirigida a B,
  el destino de retorno de A debe anularse para que A no recupere el foco ni
  la selección durante la transición;
- si una card A está abierta y una solicitud posterior apunta a B, el flujo
  debe cerrar/reemplazar A de manera determinística o impedir dos modales
  activos, pero el único modal visible debe editar B;
- las solicitudes obsoletas, IDs no montados y eventos duplicados deben
  descartarse sin modificar persistencia.

La solución debe ser acotada al routing/ownership del target. No se permite
solucionarlo con una variable global del último entry, un selector visual
ambiguo, contenido en eventos ni listeners adicionales por card.

## 7. Ownership visual de foco y selección

El recuadro azul debe representar una única card actualmente seleccionada por
la rail. El foco DOM puede quedar temporalmente en la card editada cuando el
usuario cierra el editor y luego mueve `selectedEntryId` con las flechas; por
eso el estilo azul de foco debe estar condicionado también por la clase de
selección (`.card.card-selected:focus-visible`). De esta forma la card nueva
mantiene el resaltado de selección y la card anterior pierde el recuadro,
aunque conserve el foco DOM residual. No se debe agregar estado paralelo ni
alterar el controlador de navegación o drag-and-drop.

El `outline` nativo de una card no seleccionada también debe suprimirse: su
color depende del tema del WebView y puede aparecer rojo en GTK/WebKit. La
única indicación de foco de una card es el recuadro azul de
`.card.card-selected:focus-visible`, mientras que la card seleccionada puede
mantener además su estilo azul de selección.
