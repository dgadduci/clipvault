# Proposal: collection-subcollection-visibility

## Por qué

Las colecciones principales pueden contener capturas que también están
asignadas a una o más subcolecciones. Al seleccionar la colección principal,
el usuario no tiene un control directo para ocultar temporalmente esas
capturas compartidas y ver sólo las asignadas de forma directa.

## Qué cambia

- Colocar el botón de ojo inmediatamente después del nombre de la colección y
  antes de sus badges en las filas principales que tienen subcolecciones.
- Ampliar un 10% el ancho del panel lateral que contiene las colecciones y los
  equipos vinculados.
- Con el ojo activo, mostrar las capturas asociadas directamente a la
  principal, incluidas las que también pertenecen a una subcolección.
- Con el ojo desactivado, ocultar de esa vista las capturas de la principal
  que también pertenezcan a cualquiera de sus subcolecciones directas.
- Mantener el estado por colección durante la sesión; inicialmente las
  capturas compartidas permanecen visibles.
- Localizar el nombre accesible del control en `en`, `es`, `pt`, `de` y `fr`.

## Capacidades

### Capacidades modificadas

- `tags-and-collections`: filtro temporal de la vista de una colección
  principal por membresías en sus subcolecciones.

## Impacto

- Afecta `OrganizationSidebar.svelte`, `App.svelte`, un helper de proyección
  frontend y los catálogos de traducción.
- No requiere cambios en Rust, Tauri, SQLite, dependencias ni persistencia.
- Las membresías independientes existentes se conservan; las capturas
  asignadas sólo a una subcolección no aparecen por ello en la principal.
- La búsqueda y los filtros existentes se aplican junto con esta proyección.
