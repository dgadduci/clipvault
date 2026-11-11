# Diseño: filtro de tipo de captura y creación rápida de texto

## Filtro de tipo

El código actual no monta un control de tipo en `DesktopToolbar`, aunque el
backend ya admite el filtro. Se agrega un solo `ContentTypeFilter.svelte`; no
se conserva ni se duplica un selector inexistente. El control presenta
«Todas» y los tipos presentes en el alcance activo, con los mismos iconos y
claves de traducción que las cards.

Las opciones salen de una consulta metadata-only `clipvault_content_types`,
que delega en `TextHistoryService` y `EntryRepository` para leer tipos
distintos de la colección. No se derivan de las capturas cargadas en el rail:
su límite de paginación podría omitir tipos que sí existen más atrás en una
colección grande. `collection_id = null` representa Historial. El frontend
ordena las opciones usando el orden canónico de `ContentTypeValue` y siempre
antepone «Todas».

`App.svelte` es dueño de la selección. El nuevo control notifica cambios a
`App.svelte`, y cada solicitud de recents y búsqueda envía el tipo como
metadato opcional. El comando Tauri ya lo decodifica como
`ContentType`, y el core lo combina con colección, etiquetas y aplicación en
la consulta parametrizada de SQLite. La ausencia del filtro conserva la
consulta actual. Al cambiar de colección, la selección vuelve a «Todas», como
los filtros de aplicación y etiqueta. También se reinicia al cambiar el filtro
de colección adicional de Historial, y si una actualización deja de ofrecer el
tipo seleccionado.

El filtro se aplica antes del límite del rail y antes del ranking de búsqueda,
de modo que una página llena de otros tipos no oculte resultados que sí
coinciden. Los bytes y el contenido de las capturas no participan en el
filtro. La solicitud de opciones usa un token monotónico para descartar una
respuesta atrasada después de cambiar el alcance.

## Acción de captura de texto

El botón queda después de los controles de filtro existentes y antes del
grupo del menú de tres puntos. Presenta un icono local de documento con signo más y la etiqueta
compacta «Texto», con nombre accesible y tooltip «Nueva captura de texto».
Conserva `canCreateManualText`, el callback y el modal existentes. El foco
vuelve al botón al cerrar el modal.

## Compatibilidad

Los métodos existentes de historial conservan su firma y delegan a una
variante interna con filtro de tipo opcional. La búsqueda suma el campo
opcional a `SearchFilter`; su valor por defecto mantiene la búsqueda actual.
SQLite enlaza el valor snake_case del enum como parámetro, nunca interpolado
en el SQL.

No se modifican cards, colección, rail ni listeners de drag-and-drop. El
botón de creación no participa del arrastre.
