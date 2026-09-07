# Diseño: desktop-card-preview

## Fuente única de verdad

La preview actual de Quick Paste debe convertirse en una pieza reutilizable.
La forma preferida es separar:

```text
lib/clipboardPreview.ts
  - entryFullPreviewText
  - escapeForPreview
  - predicates/estado común si corresponde
  - matcher Cmd/Ctrl+Enter compartido

ClipboardPreview.svelte
  - header tipo + título + cierre
  - body de texto o imagen
  - estados loading/loaded/error
  - footer de aplicación fuente + tiempo
  - foco, Escape y click exterior

QuickPaste.svelte ───────┐
                         ├── ClipboardPreview.svelte
App/HistoryCard ─────────┘
```

El nombre exacto de los archivos puede variar, pero debe existir una única
implementación ejecutable de la superficie. No es suficiente copiar el markup
de Quick Paste a `HistoryCard.svelte` ni mantener dos funciones equivalentes.
Los tests deben demostrar que ambas superficies importan/usan el helper o
componente compartido.

La extracción debe preservar el comportamiento que ya fue verificado en Quick
Paste. Si para extraerlo aparece una diferencia de contrato, MiniMax debe
pausar y actualizar este diseño antes de improvisar una segunda variante.

## Datos y renderizado

El componente compartido recibe un `EntryRecord` ya cargado y callbacks de
cierre/foco. No consulta SQLite ni accede directamente a APIs del sistema.

Para texto y tipos textuales:

- usa el contenido canónico completo;
- no usa el preview truncado de la fila/card;
- escapa el texto antes de insertarlo en HTML/DOM;
- conserva el fallback y la representación segura que ya usa Quick Paste;
- un payload largo tiene scroll interno y no cambia el tamaño de la ventana.

Para rich text:

- conserva exactamente la política actual de Quick Paste;
- sólo usa la preview sanitizada existente cuando el contrato actual la
  contempla;
- nunca renderiza HTML/RTF original sin sanitizar, scripts, handlers,
  navegación ni contenido activo.

Para imágenes:

- solicita los bytes mediante el mismo `createClipboardAssetResolver` y
  `clipboardAssetCommand` que ya usa la aplicación;
- conserva loading/loaded/error y el guard contra respuestas obsoletas;
- usa `object-fit: contain` para que se vea la imagen completa dentro del área,
  sin modificar los bytes, dimensiones intrínsecas, perfil o ppi del asset;
- libera el Blob URL con `releaseFor`/`release` al cambiar de entrada o
  desmontar.

El componente nunca recibe ni muestra `asset_ref` como texto. La referencia
relativa sólo se usa internamente para el bridge validado, igual que en la
implementación actual.

## Coordinación en Desktop

Debe existir una sola preview Desktop abierta a la vez. El estado puede vivir
en `App.svelte`/el rail y recibir solicitudes desde `HistoryCard`, o en un
coordinador equivalente; no debe montarse una overlay independiente por cada
card.

El flujo recomendado es:

```text
HistoryCard menu / Cmd+Enter
  → dispatch/request preview(entry.id)
  → App/rail resuelve la entrada actualmente visible
  → ClipboardPreview(entry)
```

Si el resultado de búsqueda, colección, eliminación o refresh hace que la
entrada ya no exista, la preview debe cerrarse limpiamente sin mostrar otra
entrada ni producir una excepción.

El menú de la card conserva exactamente sus acciones actuales y agrega una
acción común `Previsualizar`. El evento de la acción no debe alcanzar el flujo
de drag, selección, pin, borrado, pegado o edición de título.

## Atajo compartido

El matcher de plataforma debe vivir en un helper compartido y aceptar sólo:

- macOS: `metaKey` + `Enter`, sin `Shift` ni `Alt`;
- Linux: `ctrlKey` + `Enter`, sin `Shift` ni `Alt`.

La card del Desktop debe ser enfocável mediante teclado sin romper el
controlador singleton de drag-and-drop. El handler debe ignorar inputs,
textareas, editores de título, botones, menú abierto y otros controles
interactivos. No se debe instalar un listener global por cada card.

Quick Paste debe usar el mismo matcher extraído, conservando su listener único,
su selección actual y su regla de que el atajo abre la preview del item
seleccionado.

## Foco, cierre y geometría

- Preview Desktop se muestra como overlay/modal dentro de la ventana principal;
  no abre una ventana Tauri adicional.
- Debe quedar por encima del rail y no modificar el tamaño del desktop, la
  barra de búsqueda, el scroll horizontal ni el scroll del panel de
  colecciones.
- La card/overlay tiene el mismo lenguaje visual y la misma jerarquía que la
  preview de Quick Paste.
- Al abrir, el foco pasa al diálogo o a su botón de cierre según el contrato
  actual; al cerrar, vuelve al botón/elemento que la abrió cuando sea posible.
- `Escape` cierra el overlay antes que cualquier cierre global del Desktop.
- Click exterior cierra; click dentro no debe cerrar accidentalmente.
- Cleanup elimina listeners y revoca URLs. No puede haber listeners o
  resolvers acumulados después de abrir/cerrar repetidamente.

## Privacidad y no regresiones

La preview es read-only y local. No debe generar logs ni eventos con contenido
completo, snippets, hashes, bytes, rutas absolutas, referencias de assets o
identificadores sensibles. Las referencias opacas sólo pueden viajar al bridge
validado existente.

Antes de entregar, se deben verificar explícitamente estos baselines:

- imágenes antiguas y nuevas tras reinicio, incluyendo captura y preview;
- títulos, tags, colecciones, favoritos y búsqueda por título/contenido;
- drag-and-drop con puntero/fallback WebKit y colecciones scrolleables;
- menú de card, edición de título y acciones de paste;
- Quick Paste: iconos, selección, scroll, copy-only, menú, preview y cierre
  por pérdida de foco;
- listeners idempotentes y ausencia de crecimiento del layout.

## Tests y verificación manual

Los tests deben cubrir helpers puros, componente común, integración Desktop y
Quick Paste, focus/keyboard, texto, rich text, imagen, estados de error,
cleanup y privacidad. Los tests de imágenes deben usar temporales y nunca
`~/.clipvault`.

La verificación manual se realiza con una build actual y sin instancias
anteriores abiertas:

1. En Desktop, abrir el menú `...` de una card de texto y de una imagen y
   activar `Previsualizar`.
2. Confirmar que la preview muestra el texto completo o la imagen completa,
   sin crear una card ni alterar el clipboard.
3. Enfocar una card y probar `Cmd+Enter` en macOS o `Ctrl+Enter` en Linux.
4. Probar `Escape`, click exterior, apertura repetida y cambio de colección o
   búsqueda mientras la preview está abierta.
5. Confirmar que Quick Paste conserva exactamente su preview y shortcut.
6. Reiniciar y comprobar imágenes, títulos, tags, favoritos, colecciones,
   búsqueda y drag-and-drop.
