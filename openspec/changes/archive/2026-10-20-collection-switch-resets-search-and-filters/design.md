# Design: collection-switch-resets-search-and-filters

## Cambio de ámbito

- `selectCollectionById` conserva la colección destino y reinicia los criterios
  del toolbar antes de volver a cargar las capturas: consulta de texto vacía,
  aplicación en `Todas`, tag en `Todas` y filtro de colección en `Todas`.
- La limpieza de búsqueda invalida el token de la solicitud en curso, cancela
  su controlador y restablece su estado visual. Una respuesta tardía no puede
  volver a pintar resultados del ámbito anterior.
- Se recargan las opciones de aplicación para el nuevo ámbito y después se
  cargan las capturas sin texto de búsqueda ni filtros seleccionados.
- El filtro de colección del toolbar es independiente del ámbito seleccionado
  en el sidebar; se reinicia a `Todas`, pero la colección activa se conserva.

## Verificación

- Añadir una regresión que compruebe la limpieza del texto, la invalidación de
  búsqueda pendiente y el reinicio de los tres filtros al cambiar de colección.
- Ejecutar pruebas frontend de colecciones y drag and drop, checks, build,
  validación estricta de OpenSpec y `git diff --check`.
