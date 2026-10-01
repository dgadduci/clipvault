# Diseño: filtro de tipo de captura y creación rápida de texto

## Filtro de tipo

Se conserva el control de tipo de captura que ya existe en la interfaz; este
cambio no agrega un segundo select. Hay que reparar la fuente de sus opciones
para que el selector existente muestre los tipos presentes en el alcance
activo y conserve los iconos que comparte con las cards mediante
`contentTypeIcons.ts`. `Todas` sigue siendo la opción sin restricción.

`App.svelte` es dueño de la selección. El control existente notifica cambios a
`App.svelte`, y cada solicitud de recents y búsqueda envía el tipo como
metadato opcional. El comando Tauri lo decodifica como
`ContentType`, y el core lo combina con colección, etiquetas y aplicación en
la consulta parametrizada de SQLite. La ausencia del filtro conserva la
consulta actual. Al cambiar de colección, la selección vuelve a «Todas», como
los filtros de aplicación y etiqueta.

El filtro se aplica antes del límite del rail y antes del ranking de búsqueda,
de modo que una página llena de otros tipos no oculte resultados que sí
coinciden. Los bytes y el contenido de las capturas no participan en el
filtro.

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
