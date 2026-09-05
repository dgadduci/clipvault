# desktop-toolbar-layout

Cambio OpenSpec para reorganizar la barra superior del desktop principal:
la búsqueda pasa a la columna de cards, las acciones globales se agrupan en
un menú de puntos suspensivos y el basurero queda como acción independiente.
El panel de colecciones y la columna de búsqueda/cards comparten la misma
altura visible.

Estado: propuesto, pendiente de implementación.

Este cambio no modifica Rust, SQLite ni la semántica de captura, búsqueda,
imágenes, favoritos, tags, colecciones, pegado o drag-and-drop. No archivar
sin una instrucción explícita.
