# Proposal: tags-and-collections

## Problema

El historial de ClipVault crece como una única colección visual. Favoritos,
búsqueda y retención resuelven sólo parte del problema: el usuario no puede
agrupar capturas por proyecto, cliente o contexto ni volver a encontrarlas por
una clasificación propia.

## Resultado esperado

ClipVault tendrá una colección de sistema llamada `Historial` y colecciones
secundarias planas creadas por el usuario. Todas las capturas entrarán en
`Historial`; podrán asociarse a varias colecciones adicionales sin dejar de
pertenecer al historial. Los tags serán etiquetas independientes y combinables.

La ventana principal mostrará las colecciones en el sidebar, tags como filtros y
las capturas en la rail horizontal actual. Desde el menú de cada card el usuario
podrá asignar varios tags o colecciones mediante un selector multi-selección.

## Alcance

1. Modelo de datos SQLite para colecciones, tags y relaciones many-to-many.
2. Creación y backfill transaccional de la colección `Historial`.
3. Integración de cada nueva captura con `Historial`.
4. CRUD local de colecciones y tags, respetando las reglas del sistema.
5. Gestión de asociaciones desde las cards y el sidebar.
6. Filtros locales de colección y tags AND sobre historial y búsqueda.
7. Integración con delete, clear, retention y favoritos.
8. Respuestas Tauri delgadas y estados accesibles del frontend.

## Reglas de ciclo de vida

- Eliminar una colección secundaria sólo elimina asociaciones.
- Eliminar un tag sólo elimina asociaciones.
- Eliminar una entrada elimina todas sus asociaciones y no elimina por
  accidente las definiciones de tags o colecciones.
- Clear y retention eliminan asociaciones de las entradas eliminadas en la
  misma transacción.
- La eliminación desde `Historial` es eliminación global de la entrada y exige
  la confirmación existente.

## Criterios de aceptación

- Una captura nueva aparece en `Historial` automáticamente.
- Una captura puede verse en `Historial`, `Trabajo` y `Clientes` al mismo
  tiempo.
- Quitarla de `Trabajo` no la quita de `Historial`.
- Eliminarla desde `Historial` la elimina de todas las colecciones.
- Tags y colecciones sobreviven al reinicio y no alteran el clipboard.
- Seleccionar una colección filtra correctamente; varios tags se combinan con
  AND.
- Los cambios no introducen red, dependencias innecesarias ni contenido del
  clipboard en logs.
