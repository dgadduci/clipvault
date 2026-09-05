# Design: tags-and-collections

## 1. Límites de arquitectura

```text
HistoryCard / sidebar / filters
             ↓ thin Tauri commands
      core organization service
             ↓ transactions
 SQLite entries + collections + tags + join tables
```

La lógica de membresía, reglas de `Historial`, validación de nombres y filtros
pertenece al core. SQLite sólo implementa persistencia y transacciones. Tauri
traduce comandos y resultados. Svelte renderiza el estado y no accede a la
base directamente.

## 2. Modelo de datos

La migración debe ser aditiva y reversible, posterior a la versión actual, y
debe habilitar foreign keys. El modelo lógico es:

```text
collections(
  id INTEGER PRIMARY KEY,
  stable_key TEXT UNIQUE NOT NULL,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,                 -- system | user
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
)

tags(
  id INTEGER PRIMARY KEY,
  normalized_name TEXT UNIQUE NOT NULL,
  display_name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
)

entry_collections(
  entry_id INTEGER NOT NULL,
  collection_id INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY(entry_id, collection_id),
  FOREIGN KEY(entry_id) REFERENCES clipboard_entries(id) ON DELETE CASCADE,
  FOREIGN KEY(collection_id) REFERENCES collections(id) ON DELETE CASCADE
)

entry_tags(
  entry_id INTEGER NOT NULL,
  tag_id INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY(entry_id, tag_id),
  FOREIGN KEY(entry_id) REFERENCES clipboard_entries(id) ON DELETE CASCADE,
  FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
)
```

`Historial` se identifica por `stable_key = "history"` y `kind = "system"`,
no por el texto visible. La migración crea esa fila una sola vez y asocia a
ella todas las entradas existentes. No se debe depender de un ID fijo entre
bases.

Toda inserción de una entrada nueva debe insertar también su relación con
`Historial` dentro de la misma transacción. Una entrada no puede existir sin
esa relación válida.

## 3. Nombres y validación

Los nombres de colecciones y tags se recortan, rechazan si están vacíos y se
limitan a una longitud documentada. Los tags se normalizan para identidad con
trim, espacios internos colapsados y comparación Unicode sin distinguir
mayúsculas/minúsculas; se conserva un `display_name` legible. Las colecciones
también deben impedir nombres vacíos y duplicados de forma case-insensitive.

No hay colores, iconos configurables, jerarquías ni tags automáticos en este
cambio. El tipo detectado de una captura sigue siendo `content_type`, no un
tag generado en segundo plano.

## 4. Servicio de organización

Crear un servicio de core que centralice:

- listar colecciones y tags;
- crear, renombrar y eliminar colecciones de usuario;
- crear, renombrar y eliminar tags;
- obtener y reemplazar las asociaciones de una entrada;
- quitar una entrada de la colección actualmente seleccionada;
- consultar si la operación implica eliminación global.

Las operaciones de asociación deben aceptar conjuntos completos de IDs y
reemplazar de forma atómica sólo las asociaciones solicitadas. `Historial` se
agrega siempre y no puede ser quitada por el selector normal.

## 5. Semántica de eliminación

- En una colección secundaria, `Quitar de esta colección` borra sólo la fila
  de `entry_collections` correspondiente.
- En `Historial`, no se ofrece quitar sólo la asociación. La acción existente
  `Delete` elimina la entrada, sus relaciones y cualquier asset que quede sin
  referencias, con confirmación.
- Eliminar una colección secundaria borra su fila y asociaciones, pero no las
  entradas ni `Historial`.
- Eliminar un tag borra su fila y asociaciones, pero no las entradas.
- Delete, clear y retention deben dejar las relaciones sin huérfanos dentro de
  la misma transacción. Las definiciones de tags y colecciones permanecen,
  incluso si quedan temporalmente vacías.

## 6. Búsqueda y filtros

Extender `SearchService` con filtros opcionales locales:

- `collection_id`: una colección seleccionada por el sidebar;
- `tag_ids`: cero o más tags combinados con AND.

La consulta de texto mantiene exactamente el ranking actual. Los filtros sólo
reducen el conjunto elegible antes de rankear; no cambian exact phrase,
all-token substring, fuzzy, recencia ni desempates. Sin filtro de colección la
vista equivale a `Historial` y devuelve todas las entradas.

Quick-paste no recibe selector de colección en este cambio: continúa leyendo el
conjunto global de historial para preservar la interacción rápida existente.

## 7. UI

El sidebar muestra `Historial` primero y las colecciones de usuario debajo.
Debe permitir crear, renombrar y eliminar colecciones de usuario. `Historial`
no muestra controles de renombrar/eliminar.

La card muestra hasta dos tags compactos; si tiene más, muestra `+N`. Si no
tiene tags, no reserva una fila vacía. El menú incluye `Agregar tag` y
`Agregar a colección`, cada uno con búsqueda, checkboxes, selección múltiple,
guardar y cancelar. Al abrir una card dentro de una colección secundaria debe
existir `Quitar de esta colección`.

Si una operación modifica asociaciones, el rail, los contadores y los filtros
se refrescan una sola vez después del éxito. No se agregan listeners globales
por card; se usan callbacks/eventos ya existentes con registradores
idempotentes.

## 8. Tauri y privacidad

Los comandos deben aceptar IDs y nombres validados, nunca contenido del
clipboard. Las respuestas pueden incluir nombres de tags/colecciones y conteos
necesarios para la UI, pero no contenido adicional, hashes, snippets, paths ni
bytes. Los eventos de actualización siguen siendo metadata-only.

La operación de membership no cambia `content`, timestamps de captura,
favorite state, source app ni assets. El contenido nunca aparece en logs.

## 9. Compatibilidad y estados vacíos

Una base anterior debe recibir `Historial` y asociaciones de sus entradas sin
perder datos. Si no existen colecciones secundarias o tags, el sidebar y los
selectores muestran estados vacíos accionables. Si una entrada desaparece
entre la apertura del selector y el guardado, la operación responde de forma
tipada y el resto de la vista continúa usable.
