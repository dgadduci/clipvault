# Prompt para MiniMax: implementación de colores de colecciones

Eres la LLM implementadora de ClipVault. Implementa únicamente el cambio
OpenSpec `collection-colors-and-card-collection-labels` en el checkout actual.
Codex ya dejó preparados `proposal.md`, `design.md`, las dos delta specs y
`tasks.md`; OpenSpec es la fuente de verdad.

## Contexto obligatorio

Antes de editar:

1. Lee `project.md` y `AGENTS.md`.
2. Lee todos los artefactos de
   `openspec/changes/collection-colors-and-card-collection-labels/`.
3. Lee los specs base de `tags-and-collections` y
   `clipboard-history-cards`.
4. Revisa `git status --short`, `git diff --check` y el diff existente.
   Hay cambios de sincronización/archive de otro cambio en el checkout; no
   los reviertas ni los mezcles.
5. Revisa la implementación actual de `Collection`,
   `OrganizationRepository`, la migración de organización, los comandos Tauri,
   `OrganizationSidebar`, `HistoryCard`, `HistoryCardRail` y sus tests.

## Resultado requerido

Implementa los requisitos completos del cambio:

- `Collection.color_hex` persistente como `#rrggbb` opaco.
- Migración aditiva, idempotente y reversible; color azul para `Historial` y
  colecciones existentes sin color.
- Color aleatorio backend de la paleta roja, amarilla, verde y azul al crear
  nuevas colecciones.
- Actualización tipada de color sin alterar memberships, entries, tags,
  favoritos, source-app metadata ni assets.
- Cuadrado de color junto a cada colección en la sidebar.
- Doble click y teclado sobre el cuadrado abren un modal accesible con
  `svelte-awesome-color-picker` v4, sin alpha, swatches, HEX, Guardar,
  Cancelar, Escape y foco de retorno.
- Todas las colecciones de usuario asignadas debajo de las tags en cada card,
  excluyendo `Historial` de los chips inline; si hay overflow, un chip `+N`
  abre un modal con todas las memberships, incluida `Historial`.
- Evento existente `clipvault://organization-updated` con payload vacío y
  refreshes idempotentes.

## Librería aprobada para el picker

Usa `svelte-awesome-color-picker` v4, compatible con el Svelte 5 actual del
frontend. Configúralo sin alpha y persiste el valor HEX normalizado. No uses
APIs nativas de Wayland, X11 o macOS: el componente debe ejecutarse dentro del
WebView de Tauri. Si la API exacta de la versión instalada contradice el
diseño, pausa y solicita actualizar OpenSpec; no cambies la librería por
iniciativa propia.

## Restricciones críticas

- La lógica de negocio permanece en Rust/core/persistencia; Tauri es thin y
  Svelte no accede a SQLite.
- No envíes color, nombres, IDs u otros datos de colección dentro del payload
  de drag de cards; ese payload sigue transportando sólo el ID opaco de entry.
- Conserva `data-testid="history-card"`, `data-entry-id`,
  `draggable="false"`, acciones de pin/menú/título y el controlador singleton
  `pointerDragAndDrop.ts`.
- No elimines, renombres ni limpies `~/.clipvault` o sus assets.
- No agregues red, telemetría, cuentas, cloud, LLMs ni una base nueva.
- No modifiques ni archives otros cambios activos.
- Si encuentras una contradicción arquitectónica, detente y pide actualizar
  OpenSpec antes de continuar.

## Orden de trabajo

1. Completa persistencia, migración, DTOs, validación y servicio.
2. Conecta comando Tauri, evento y bridge TypeScript.
3. Integra picker, modal y sidebar.
4. Integra etiquetas de colecciones en cards sin alterar drag/drop.
5. Agrega sólo tests necesarios para los comportamientos nuevos y regresiones
   reales.
6. Marca cada tarea como `[x]` inmediatamente después de verificarla y agrega
   evidencia concreta cuando corresponda.

## Verificación obligatoria

Ejecuta los checks relevantes y registra sus resultados en `tasks.md`:

```bash
cargo fmt --all -- --check
cargo test -p clipvault-db -p clipvault-core
cargo clippy -p clipvault-db -p clipvault-core --all-targets -- -D warnings
cd app/tauri/frontend && npm run check && npm run build && npm test
cd ../../.. && openspec validate collection-colors-and-card-collection-labels --strict --type change
git diff --check
```

Como el cambio toca cards, ejecuta además las regresiones frontend específicas
de drag and drop indicadas en `AGENTS.md`. Si una falla, no cierres el cambio:
corrige la regresión y vuelve a verificar.

La prueba manual final debe cubrir Ubuntu GNOME Wayland, Ubuntu X11 y macOS.
No marques esas tareas como completas por inferencia entre plataformas.
No ejecutes `/opsx-sync` ni `/opsx-archive`; esas acciones quedan para la
entrega posterior, después de la revisión del diff y de la orden explícita.

## Corrección solicitada después de la prueba manual

La implementación inicial de colores y membresías funciona, pero la revisión
manual detectó dos problemas y una mejora requerida. Implementa sólo esta
corrección adicional; no reescribas el backend de colores ni el modal de
selección de color que ya fueron verificados.

### 1. Chips de colecciones con el estilo de tags

En `HistoryCard.svelte`, conserva la fila `.collection-chips` y sus chips ya
estilizados como tags. El ajuste debe mantener fondo, borde, radio redondeado,
padding, tipografía compacta, truncamiento seguro y color de texto individual.
Mantén la posición debajo de la fila de tags. No cambies el tamaño fijo de la
card ni el orden de sus acciones.

### 2. Overflow responsive con chip `+N`

La fila no debe depender de scroll horizontal para resolver el overflow. Cuando
no entren todos los chips en el ancho real disponible, muestra los chips que
entren y un único chip interactivo `+N`. Cuando todos entren, no muestres el
chip.

- Calcula la condición con `ResizeObserver` o una estrategia equivalente
  basada en `scrollWidth`/`clientWidth`; no uses `assignedCollections.length > N`.
- Calcula `N` como la cantidad exacta de colecciones de usuario ocultas por
  falta de ancho. `Historial` no se muestra inline ni se cuenta en `+N`.
- El chip debe conservar `type="button"`, el estilo de
  `.tag-chip.more`, foco visible, `aria-haspopup="dialog"`, `aria-label`,
  `title`, `data-testid="history-card-collections-overflow"` y el id de la
  entrada.
- No debe cambiar la selección de la card, abrir el menú de la card ni iniciar
  drag. Agrégalo al guard `isInteractiveTarget` y respeta el singleton de
  `pointerDragAndDrop.ts`.
- El ancho del chip `+N` debe participar en el cálculo del subconjunto visible;
  no uses límites fijos de cantidad. Actualízalo ante resize.

### 3. Modal con todas las colecciones de la captura

Reutiliza el componente existente
`CollectionMembershipModal.svelte` y `Modal.svelte`. Al activar el chip `+N`,
abre el modal informativo que:

- lista todas las colecciones asignadas a esa entrada, incluida `Historial`;
- muestra nombre completo y `color_hex` en chips con el mismo estilo de tags;
- no permite editar ni quitar memberships;
- tiene título, `aria-labelledby`, foco inicial, foco de retorno al icono,
  botón cerrar, Escape, backdrop y scroll interno si la lista es larga;
- no incluye clipboard, hashes, paths, assets ni datos de drag en eventos o
  payloads.

Si sólo hay una card abierta, puede mantener el estado del modal en
`HistoryCard`; no agregues listeners globales por card ni una segunda fuente de
verdad para las asociaciones. Usa el snapshot ya hidratado de
`assignedCollections`. El foco debe regresar al chip `+N` al cerrar.

### Tests y verificación de esta corrección

Agrega o actualiza tests para:

- chips de colección con borde y fondo;
- todos los chips visibles cuando entran;
- chip ausente cuando no existe overflow y valores `+1`/`+2` cuando sí existe;
- actualización del conteo tras resize y medición del ancho del chip `+N`;
- click del chip, modal con el listado completo, cierre por
  Escape/backdrop/botón y retorno de foco;
- que el chip no selecciona la card ni inicia drag;
- que `data-testid="history-card"`, `data-entry-id`, `draggable="false"`,
  pin, menú, título, tags y drag-and-drop permanecen intactos.

Ejecuta al menos:

```bash
cd app/tauri/frontend
npm run check
npm run build
npm test
cd ../../..
openspec validate collection-colors-and-card-collection-labels --strict --type change
git diff --check
```

Marca las tareas 9.1–9.5 como completadas sólo después de verificarlas y
agrega evidencia concreta. No sincronices ni archives el cambio.

## Corrección bloqueante reportada por la prueba manual

La automatización informa `1275/1275` tests pass, pero la prueba manual falló:
en una card cuyo conjunto de colecciones supera visualmente el ancho disponible
no aparece el chip `+N`. No des por cerrado el cambio por los tests source-level
existentes; reproduce y corrige el comportamiento en la ejecución real de
Tauri.

Hay una sospecha concreta que debes verificar antes de editar: en la versión
actual, la tira de medición (`data-testid="history-card-collection-chips-measure"`)
se renderiza como hermana de `.collection-chips`, mientras que
`measureCollectionChipWidths()` la busca con `collectionChipsEl.querySelector(...)`.
Eso puede dejar los anchos en cero. Además, medir `scrollWidth` sobre una fila
que ya fue derivada como `visibleUserCollections` puede devolver que todo cabe,
aunque el conjunto completo no entre. Confirma el diagnóstico con la geometría
real del DOM y corrige la causa, sin reescribir el backend de colores ni el
modal de memberships.

### Resultado funcional obligatorio

- La medición debe alcanzar realmente la tira completa de todos los chips de
  usuario y el chip `+N`, aunque la tira sea un nodo hermano del row visible.
- El overflow debe decidirse comparando el ancho intrínseco total de todos los
  chips, incluidos gaps, contra el ancho disponible de la card; no puede
  depender del `scrollWidth` de un subconjunto ya recortado.
- El primer render hidratado debe medir después de que los nodos tengan
  geometría. Recalcula ante cambios de memberships, carga de fuentes y resize;
  conserva `ResizeObserver` o una alternativa equivalente y un fallback seguro
  para WebKit/Tauri.
- La secuencia debe converger: medir todo → determinar overflow → reservar el
  ancho medido del `+N` → derivar los chips visibles. Cuando todos caben no hay
  `+N`; cuando no caben aparece exactamente `+1`, `+2`, etc. `Historial` no es
  inline ni cuenta en `N`.
- Conserva `data-testid="history-card-collections-overflow"`, el modal
  existente, accesibilidad, retorno de foco y todos los baselines de card y
  drag-and-drop.

### Tests y aceptación de esta corrección

Agrega una regresión al nivel más cercano posible a `HistoryCard` (DOM/layout
simulado o integración), no únicamente otro test del helper puro. Debe fallar
con la implementación actual y pasar con la corrección, cubriendo:

1. suficientes colecciones para exceder el ancho real de una card y mostrar
   `+N` en runtime;
2. conteos exactos `+1` y `+2`, excluyendo `Historial`;
3. medición de la tira hermana y primer render después de hidratación;
4. resize que oculta o muestra el chip según corresponda;
5. ausencia del chip cuando todos los chips caben;
6. no regresión de selección, modal, focus, pointer/mouse drag, payload opaco
   y atributos protegidos de la card.

Ejecuta nuevamente:

```bash
cd app/tauri/frontend
npm run check
npm run build
npm test
cd ../../..
openspec validate collection-colors-and-card-collection-labels --strict --type change
git diff --check
```

Después de los checks, cierra cualquier instancia previa de ClipVault,
arranca la build actual y repite la prueba manual. Marca las tareas 10.1–10.5
de `tasks.md` sólo con evidencia concreta de la reproducción, la corrección,
los tests y la observación manual del `+N`. No ejecutes `opsx-sync` ni
`opsx-archive`.
