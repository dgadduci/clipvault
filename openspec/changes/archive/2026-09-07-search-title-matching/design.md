# Diseño: search-title-matching

## Flujo único

Las dos barras deben conservar este flujo:

    Desktop / Quick Paste
        → runSearch (debounce + cancelación)
        → searchEntriesCommand
        → clipvault_search_entries
        → SearchService::search_with_filter
        → EntryRepository (scope actual)
        → SearchDocument { content, title, updated_at }
        → LocalSearchEngine
        → SearchResponse / SearchEntryHit

No se debe agregar una lista de títulos al frontend ni una segunda consulta
para imágenes. El `SearchResponse` ya devuelve el `EntryRecord` necesario para
renderizar la card o el item de Quick Paste.

## Candidatos y campos

`SearchService` debe consultar todos los `EntryRecord` elegibles del scope
actual, usando el camino que ya conserva imágenes, tags, colecciones y filtro
de aplicación de origen (`entries_filtered` o un equivalente local para el
caso sin filtros).

Al construir cada `SearchDocument`:

- `title` recibe `record.title.as_deref()`; `None` y whitespace-only deben
  comportarse como título ausente.
- `content` conserva el contenido real sólo para `ContentType::is_textual()`.
- Para una imagen o cualquier futuro tipo binario, `content` debe ser vacío o
  el sentinel no buscable definido por el modelo. Nunca se deben leer bytes
  del asset para construir el documento.
- `updated_at` e `entry_id` permanecen disponibles para los desempates
  deterministas.

La consulta vacía debe mantener el comportamiento actual y no recorrer ni
materializar documentos innecesariamente si el servicio ya puede evitarlo.

## Ranking

Se conservan los niveles ya definidos por `LocalSearchEngine`:

    contenido: frase exacta > todos los tokens/subcadena > fuzzy
    título:    frase exacta > todos los tokens/subcadena > fuzzy

Todo nivel de contenido debe permanecer por encima de cualquier nivel de
título. Dentro del mismo nivel se conserva el desempate actual por
`updated_at` descendente y luego `entry_id` descendente. La misma consulta y
los mismos registros deben producir el mismo orden en Desktop y Quick Paste.

Una coincidencia sólo en título debe devolver el `SearchEntryHit` completo con
su `record`. El snippet sigue siendo una representación del contenido
textual; no se debe fabricar un snippet con el título ni mezclar contenido
sensible en logs. Para una imagen el resultado puede tener snippet vacío y
debe seguir siendo renderizable mediante su metadata/asset existente.

## Scopes y ciclo de vida

- Desktop mantiene la colección activa, tags y filtro de aplicación que ya
  entrega al comando. Buscar en Historial debe consultar Historial; buscar en
  una colección debe limitarse a esa colección.
- Quick Paste conserva su alcance actual de historial y su límite.
- El debounce, cancelación, token contra respuestas obsoletas, estados vacío,
  carga, error y sin coincidencias permanecen intactos.
- Al borrar la consulta se restaura el conjunto reciente del scope actual, no
  un listado global accidental.
- La edición de un título, un refresh, pin/unpin, cambio de colección o
  llegada de una captura nueva debe permitir repetir la búsqueda y obtener el
  título actualizado sin clobber de estado local.

## Privacidad y no regresiones

La búsqueda sigue siendo local y no debe registrar ni transportar en eventos
de UI contenido completo, snippets completos, hashes, referencias de assets,
rutas, bytes de imágenes ni identificadores de aplicaciones fuera de los
campos que ya forman parte del `SearchResponse` interno. No se debe añadir una
traza con query, título o contenido.

El cambio no puede reconstruir ni reemplazar `EntryRecord`. Deben permanecer
intactos `asset_ref`, `mime_type`, `payload_width`, `payload_height`, tags,
colecciones, `is_pinned`, timestamps y metadata de aplicación. En especial,
la ruta de imágenes debe continuar siendo:

    SQLite → SearchResponse.record → HistoryCard/QuickPaste → asset bridge →
    Blob URL/thumbnail

El controlador singleton de drag-and-drop, sus fallbacks WebKit/Tauri, el
hit-testing sobre colecciones scrolleables y los controles de las cards no
deben tocarse.

## Tests y prueba manual

Los tests de core deben demostrar que el motor/servicio no depende de una
interfaz gráfica y que una entrada de imagen puede aparecer por título sin
leer su asset. Los tests de frontend deben demostrar que las dos superficies
usan el resultado del comando común y conservan selección, carga de imágenes y
cancelación de respuestas obsoletas.

La prueba manual final debe ejecutarse con una build actual y sin instancias
anteriores abiertas:

1. Crear una captura de texto con título personalizado `Proyecto Alfa` cuyo
   contenido no contenga ese texto.
2. Buscar `Proyecto Alfa` en Desktop y confirmar que aparece.
3. Abrir Quick Paste, buscar el mismo título y confirmar que aparece y puede
   seleccionarse.
4. Crear una captura de imagen con título personalizado que no figure en su
   contenido y repetir la prueba en Historial.
5. Asociar entradas a una colección y confirmar que el mismo título sólo
   aparece cuando esa colección es la activa.
6. Confirmar que una búsqueda por contenido continúa funcionando y que
   limpiar la consulta restaura el scope correcto.
7. Reiniciar la aplicación y comprobar que las imágenes, tags, favoritos y
   títulos persisten y siguen visibles.
