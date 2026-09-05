# Diseño: source-app-filter

## Modelo de filtro

El filtro debe distinguir tres estados:

```text
Todas                 → no restringe por aplicación
Aplicación conocida   → source_app == identificador estable
Aplicación desconocida→ source_app IS NULL o vacío
```

Internamente, el core puede representar este estado con un enum
`SourceAppFilter::{All, Known(String), Unknown}`. La representación Tauri debe
ser explícita y no debe depender de que `null` signifique simultáneamente
`Todas` y `desconocida`. El identificador estable es un valor de matching
interno; no se renderiza como texto en la interfaz.

El filtro de aplicación se combina con el filtro de colección mediante AND y
con los tags mediante los AND ya existentes. La búsqueda textual se ejecuta
después de reducir los candidatos, conservando el ranking y los límites
actuales.

## Alcance y opciones

El core debe exponer una consulta local de opciones de aplicación para el
alcance actual, por ejemplo `source_applications(collection_filter)`. La
consulta debe:

- considerar todas las filas elegibles de la colección, no sólo el límite del
  rail;
- incluir aplicaciones conocidas agrupadas por `source_app`;
- incluir una sola opción desconocida si hay filas con `source_app` nulo o
  vacío;
- elegir de forma determinista el nombre/icono no vacío para cada identificador
  (preferir la metadata más reciente y usar fallback si falta);
- ordenar primero `Aplicación desconocida` si existe y luego nombres visibles
  de forma case-insensitive, con el identificador estable como desempate
  interno;
- devolver sólo metadata: identificador, nombre, referencia relativa del
  icono y un estado de fallback. Nunca devolver contenido, snippets, hashes,
  rutas absolutas ni bytes de clipboard.

La consulta debe aceptar el contexto de `Historial` como “todas las entradas de
historial” y una colección de usuario como “sólo entradas asociadas a ella”,
igual que `recent_entries_filtered` y `search_entries` actuales.

## Contrato de comandos

Se recomienda extender los comandos existentes en forma aditiva:

- `clipvault_recent_entries_filtered`: aceptar el filtro de aplicación además
  de colección y tags;
- `clipvault_search_entries`: aceptar el mismo filtro de aplicación;
- agregar un comando delgado, por ejemplo
  `clipvault_source_applications`, para obtener las opciones del alcance
  activo.

Los nombres finales pueden seguir la convención existente, pero el contrato
debe ser único para recents y búsqueda. No duplicar una segunda implementación
de filtrado en el frontend. Los comandos deben delegar al core y permanecer
metadata-only donde corresponda.

El resultado de opciones debe contener, como mínimo:

```text
{
  kind: "all" | "known" | "unknown",
  source_app: string | null,
  display_name: string,
  icon_ref: string | null
}
```

`source_app` es interno y no se muestra visualmente. Para `kind: "all"` debe
ser `null`; para `kind: "unknown"` también debe ser `null`, pero el `kind`
evita confundirlo con Todas.

## Frontend y combobox

`DesktopToolbar` debe renderizar una sola instancia del nuevo control entre el
input de búsqueda y el botón de puntos suspensivos. El padre mantiene el
estado del filtro porque también controla la colección activa y las consultas.

El control debe ser un combobox accesible o equivalente, no un `<select>` que
impida presentar iconos de manera consistente. Debe incluir:

- `role="combobox"` y `aria-expanded`;
- `aria-controls` apuntando a un único `role="listbox"`;
- `aria-activedescendant` o foco equivalente en la opción activa;
- `aria-label="Filtrar por aplicación fuente"`;
- valor visible con icono y nombre;
- `Todas` como primera opción;
- iconos locales: icono de aplicaciones/lista para Todas, icono persistido
  para una aplicación conocida y fallback genérico para desconocida o icono
  no disponible;
- apertura por click/Enter/Espacio, selección inmediata y cierre por Escape,
  click fuera o selección;
- navegación con ArrowUp/ArrowDown, Home/End y Enter;
- foco visible y funcionamiento sin mouse.

El nombre visible debe proceder de `display_name`. Si falta, se usa
`Aplicación desconocida`; nunca se muestra `source_app` como fallback visible.
La carga de iconos debe reutilizar el bridge y resolver/cache existente para
referencias locales, revocando Blob URLs al eliminar opciones o destruir el
componente. Un fallo de icono no debe impedir seleccionar el filtro.

## Composición de consultas

El estado del padre debe mantener por separado:

```text
selectedCollectionId
searchQuery
selectedSourceAppFilter
```

Las consultas deben enviar siempre el mismo `selectedSourceAppFilter`:

- query vacía: `recent_entries_filtered` con colección, tags existentes y
  filtro de aplicación;
- query no vacía: `search_entries` con colección, tags existentes y filtro de
  aplicación;
- `Todas`: sin restricción de aplicación;
- `Unknown`: filas con `source_app IS NULL` o vacío;
- `Known`: igualdad exacta del identificador estable.

Al cambiar de colección, seleccionar `Todas`, recargar opciones y ejecutar una
sola actualización de cards. Al recibir `clipvault://history-updated`,
refrescar opciones y cards sin duplicar listeners; si la nueva captura añade
una aplicación, debe aparecer sin reiniciar la aplicación.

Las respuestas obsoletas de consultas o carga de iconos no pueden sobrescribir
un filtro o colección más recientes. Reutilizar los tokens/cancelación de
`runSearch` y los guards existentes.

## No-regresiones obligatorias

- Las cards siguen usando exactamente el rail horizontal y conservan su
  tamaño, tags, pin, menú, títulos e imágenes.
- Las imágenes guardadas antes de reiniciar siguen resolviendo por
  `clipboard_asset` y Blob URL después de aplicar/quitar el filtro o cambiar
  de colección.
- El drag-and-drop de cards a colecciones conserva pointer capture, fallback de
  mouse, ghost, bloqueo de selección, hit-testing del viewport scrolleable y
  cancelación existente. El combobox no debe iniciar ni bloquear un drag de
  cards.
- El filtro no cambia `source_app`, timestamps, favoritos, tags, membresías ni
  contenido.
- No se registran contenidos, snippets, hashes, bytes, rutas ni identificadores
  crudos en logs, tooltips o textos de error.
- No se agregan listeners globales duplicados; todos los listeners del
  combobox se desmontan al destruirlo.

## Pruebas requeridas

### Core/DB/Tauri

- opción `Todas` y opciones conocidas agrupadas sin duplicados;
- opción desconocida sólo cuando corresponde;
- filtro por aplicación conocido, desconocido y ausencia de filtro;
- combinación con Historial, colección de usuario, tags y búsqueda;
- ranking, límites y orden existentes sin cambios;
- consulta de opciones independiente del límite de cards;
- metadata determinista y sin payload sensible;
- imágenes, tags, favoritos y membresías intactos.

### Frontend

- posición única entre búsqueda y configuración;
- `Todas` primero, icono/nombre por opción y fallback seguro;
- apertura, cierre, Escape, click fuera, navegación y selección por teclado;
- filtrado inmediato sin botón Aplicar;
- combinación con query y colección activa;
- reset a `Todas` al cambiar de colección;
- actualización tras `history-updated` sin listeners duplicados;
- respuestas obsoletas no pisan el estado actual;
- lifecycle de Blob URLs e iconos;
- regresiones de cards, imágenes tras reinicio, tags, favoritos y drag-and-drop.
