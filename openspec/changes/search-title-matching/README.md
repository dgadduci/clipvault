# search-title-matching

Corrección de la búsqueda local para que Desktop y Quick Paste encuentren
entradas por el título personalizado de la card además de por su contenido.

El cambio debe reutilizar `SearchService`, `LocalSearchEngine` y
`clipvault_search_entries`. No crea un índice paralelo ni implementa un filtro
de títulos en Svelte.

Estado: propuesta preparada para implementación por MiniMax. No implementado,
no sincronizado y no archivado.
