# quick-paste-desktop-polish

Este cambio reúne correcciones de interacción y presentación en Quick Paste y
Desktop:

- Escape en Quick Paste distingue entre lista y preview;
- todas las filas de Quick Paste tienen una altura fija y dos líneas estables;
- los tags asociados aparecen en la fila del título de Quick Paste;
- las cards Desktop reutilizan la presentación de preview para código y
  whitespace significativo;
- el icono de tipo de captura muestra un tooltip accesible;
- Desktop agrega un filtro por tags junto al filtro por aplicación fuente.

La implementación debe reutilizar los helpers de preview, detección de código,
organización y filtros existentes. No se debe duplicar el renderer ni crear una
segunda lógica de tags.

Estado: propuesto, pendiente de implementación.

No sincronizar ni archivar automáticamente. No modificar ni marcar la
verificación pendiente de `platform-permission-guidance`.
