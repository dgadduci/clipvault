# Diseño: quick-paste-desktop-polish

## 1. Máquina de estados de Quick Paste

El controlador existente debe distinguir explícitamente:

```text
window hidden → list → preview
                 ↑       │
                 └─ Escape┘
list ─ Escape → window hidden
```

El listener de Escape debe consultar el estado actual antes de ocultar la
ventana. En `preview`, sólo cambia a `list`, restablece el foco al contenedor o
al item seleccionado y conserva `selectedIndex`, `selectedEntryId`, query y
scroll. En `list`, delega en el cierre existente de Quick Paste. Debe existir
un único listener activo y cleanup idempotente.

## 2. Geometría fija de filas

La fila debe tener una constante de altura única, compatible con la ventana
720×520 y el scroll actual. La implementación debe definir explícitamente:

- `min-height` y `height` iguales para cada fila;
- grid/flex con dos tracks de línea estables;
- overflow oculto o ellipsis por contenido, nunca crecimiento vertical;
- miniaturas y placeholders con la misma huella;
- tags limitados en una sola línea.

No se debe usar la cantidad de texto, tags, estado de thumbnail o carga de
metadata para cambiar la altura.

## 3. Tags en Quick Paste

Quick Paste debe recibir una proyección de organización por `entry.id`, igual
que Desktop. La proyección puede tener estados `loading`, `loaded` y `error`,
pero todos deben ocupar la misma línea y altura. Los chips deben ser seguros,
escapados y no incluir ids internos, rutas, hashes, bytes ni contenido del
clipboard. Si hay más tags de los que entran, mostrar algunos chips y un
indicador `+N` accesible.

La hidratación no debe reemplazar la EntryRecord ni afectar thumbnails,
selección, orden o búsqueda. Las respuestas obsoletas no deben sobrescribir
el scope actual.

## 4. Preview seguro en HistoryCard

HistoryCard debe reutilizar los helpers existentes de `ClipboardPreview`,
`shouldRenderHighlightedPreview`, sanitización y `renderHighlightedCode`.
Preferentemente se extraerá una proyección pura compartida si la card no puede
consumir directamente el componente de overlay.

Para código:

- mostrar label/lenguaje ya detectado cuando exista;
- conservar colores de syntax highlighting permitidos;
- conservar tabs, indentación y saltos de línea;
- escapar el fallback de texto;
- mantener clipping y límites de la card.

Para contenido no code, conservar la presentación actual y no forzar colores o
gramáticas por heurística duplicada. Imágenes y rich preview deben conservar
sus rutas/resolvers y estados actuales.

## 5. Tooltip del tipo de captura

El icono del tipo debe tener un nombre canónico existente, por ejemplo `Texto`,
`Imagen`, `Código · Python`, `HTML`, `JSON`, etc. Debe exponerse mediante:

- tooltip al hover/focus, sin depender sólo de color;
- `title` o componente tooltip existente;
- `aria-label` descriptivo.

No introducir un tooltip global duplicado si ya existe un primitive reutilizable.
El tooltip no debe cambiar el layout de la card.

## 6. Filtro por tags en Desktop

Agregar un `TagFilter` junto a `SourceAppFilter`, antes del overflow. Debe
reutilizar la semántica accesible de combobox/listbox y el patrón de bridge del
filtro de aplicación.

Contrato:

- primera opción `Todas`;
- opciones derivadas del scope activo;
- chips/opciones con nombre visible, sin ids internos en la UI;
- selección inmediata;
- sin botón de aplicar;
- filtro tag y source-app combinados con AND;
- reset al cambiar de colección si la selección ya no pertenece al scope;
- refresh tras `history-updated` sin listeners duplicados;
- query vacía conserva todas las entradas del scope.

La consulta puede reutilizar el endpoint de organización existente. Si hace
falta ampliar el core/bridge para obtener tags por scope, debe hacerse de forma
aditiva, tipada y metadata-only; no agregar migraciones ni duplicar reglas de
filtrado en frontend y backend.

## 7. No regresiones y privacidad

- No modificar SQLite, EntryRecord, asset_ref, hashes, timestamps ni bytes.
- No poner contenido, snippets, rutas o referencias de assets en DOM, logs,
  eventos o bridge payloads innecesarios.
- Preservar imágenes antiguas, source-app icons, tags, colecciones, favoritos,
  títulos, búsqueda y drag-and-drop.
- Preservar la selección y navegación de Quick Paste y Desktop.
- Preservar `platform-permission-guidance` sin modificar su validación manual.

## 8. Tests requeridos

### Quick Paste

- Escape desde preview vuelve a la lista sin ocultar la ventana.
- Escape desde lista oculta Quick Paste.
- Escape en controles no rompe el control activo.
- query, selección, scroll y foco se conservan al volver de preview.
- todas las filas tienen altura fija y dos líneas.
- contenido corto sigue reservando ambas líneas.
- tags cargados, vacíos, pendientes y con error no alteran altura.
- tags aparecen después de hidratar y no contaminan otras entradas.

### Desktop preview y tooltip

- cards de código reutilizan la proyección de preview;
- tabs, indentación, saltos y colores se conservan;
- contenido inseguro sigue sanitizado;
- imágenes y rich text no regresan;
- icono de tipo tiene tooltip y aria-label correctos.

### Filtro por tags

- opciones derivadas de Historial;
- opciones derivadas de colección activa;
- `Todas` como primera opción;
- filtrado inmediato;
- combinación AND con source-app;
- reset/refresh/hydration sin respuestas obsoletas;
- no se filtra por tags de otro scope.

### Regresiones protegidas

- imágenes persistidas y preview compartido;
- orden de Quick Paste y navegación por teclado;
- selección Desktop, menú, Escape y shortcuts;
- títulos, tags, colecciones, favoritos y copy/paste;
- drag-and-drop pointer/mouse y privacidad metadata-only.
