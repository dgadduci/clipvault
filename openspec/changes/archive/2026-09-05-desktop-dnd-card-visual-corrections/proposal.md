## Por qué

La primera implementación de drag-and-drop dibuja el estado de arrastre, pero
el drop no siempre llega a ejecutar la asociación. En WebKit/Tauri puede no
estar disponible el MIME personalizado durante dragover o drop, por lo que el
target rechaza el evento antes de invocar la mutación.

También se necesita una señal visual más clara durante el arrastre, un pin
reconocible y mejor integrado que el icono actual, un icono de aplicación
fuente más visible y la eliminación de la línea técnica que queda debajo del
rail.

## Qué cambia

- Hacer que el drop sobre una colección de usuario agregue realmente la card,
  usando el MIME privado y un fallback text/plain que transporte únicamente el
  identificador de la entrada.
- Animar suavemente el color y el estado del item de colección válido durante
  dragover, con cleanup garantizado.
- Rediseñar el favorito como una chincheta/pushpin local minimalista, sin
  estrellas ni emojis.
- Aumentar un 50% el icono de la aplicación fuente dentro de la card.
- Eliminar de la superficie principal la línea de texto técnica inferior;
  conservar información funcional relevante en Development si corresponde.

## No objetivos

- No cambiar la semántica de Historial ni quitar colecciones existentes.
- No reemplazar el selector de colecciones accesible.
- No transportar contenido, snippets, hashes, rutas, asset references ni bytes
  en drag and drop.
- No modificar la persistencia de imágenes ni regenerar assets.
- No cambiar las reglas de tags, favoritos, retención, búsqueda o paste.
- No agregar red, telemetría ni dependencias innecesarias.

## Capacidades

### Nuevas capacidades

- desktop-dnd-card-visual-corrections: drop funcional y feedback visual,
  rediseño del pin, icono de aplicación ampliado y desktop sin línea técnica
  inferior.

### Capacidades modificadas

- desktop-header-card-dnd: compatibilidad real del DataTransfer con WebKit/Tauri.
- clipboard-history-cards: icono de pin e icono de aplicación fuente.
- desktop-shell-layout: elimina la línea técnica inferior del desktop principal.
- tags-and-collections: el drop agrega una asociación sin reemplazar las
  existentes.

## Impacto esperado

- Frontend: dragAndDrop.ts, App.svelte, OrganizationSidebar.svelte,
  HistoryCard.svelte y sus tests.
- Tauri/core: sólo si la investigación demuestra un problema de propagación;
  reutilizar los comandos existentes y mantener la frontera delgada.
- No se espera migración de SQLite.
