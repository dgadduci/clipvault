# Proposal: search-title-matching

## Por qué

Las barras de búsqueda de Desktop y Quick Paste usan el mismo camino local de
`clipvault_search_entries`, pero la experiencia observada sólo encuentra el
contenido textual de las capturas de texto. Una captura cuyo título
personalizado se puede ver en la card no aparece cuando se consulta ese
título.

El repositorio ya contiene parte del modelo necesario: `EntryRecord` expone el
campo `title` y `LocalSearchEngine` tiene soporte de scoring para coincidencias
de título. Sin embargo, el camino de servicio/consulta sigue construyendo su
conjunto de candidatos desde lecturas textuales, por lo que una coincidencia de
título no es consistente entre tipos de captura ni entre las dos superficies.

## Qué cambia

- Hacer que `SearchService` construya documentos de búsqueda con todas las
  entradas actualmente soportadas, no sólo con las que tienen contenido
  textual.
- Mantener el contenido como campo buscable sólo para tipos textuales. Una
  imagen nunca se convierte en texto ni se inspeccionan sus bytes; su título
  personalizado sí puede ser buscado.
- Incluir el título personalizado persistido (`EntryRecord.title`) como campo
  buscable para texto, rich text, imágenes y cualquier otro tipo actual.
- Reutilizar el scoring existente: coincidencias de contenido conservan
  prioridad sobre coincidencias de título, y las coincidencias de título
  mantienen niveles deterministas por frase, tokens y fuzzy.
- Mantener un único contrato para Desktop y Quick Paste a través de
  `searchEntriesCommand` y `clipvault_search_entries`, respetando la colección,
  tags, aplicación de origen, límites, debounce y cancelación actuales.
- Agregar cobertura de regresión para título solamente, contenido solamente,
  entradas de imagen, scopes de colección y ambas interfaces.

## Semántica del título

En este cambio “título” significa el título personalizado almacenado en
`EntryRecord.title`, después de aplicar las reglas existentes de trim y
validación. Cuando el campo es `None` o sólo contiene espacios, no existe un
título personalizado buscable.

La etiqueta visual derivada de `content_type` (`Texto`, `Imagen`, etc.) no se
debe duplicar como texto localizado dentro del core. Buscar esas etiquetas
predeterminadas queda fuera de este cambio; el objetivo es corregir la
búsqueda del título que el usuario asignó a la card.

## No objetivos

- No agregar migraciones SQLite, FTS, índices externos, embeddings, red,
  telemetría ni dependencias nuevas.
- No crear una segunda búsqueda específica para Quick Paste ni filtrar títulos
  exclusivamente en el frontend.
- No cambiar la persistencia, edición, restauración o validación de títulos.
- No cambiar el ranking vigente de coincidencias de contenido, salvo la
  extensión necesaria para que una coincidencia de título pueda aparecer.
- No buscar dentro de bytes de imágenes, assets, HTML/RTF originales, hashes,
  rutas ni metadatos sensibles.
- No alterar selección, favoritos, tags, colecciones, drag-and-drop, captura,
  pegado ni carga de imágenes.

## Capacidades afectadas

### Capacidades modificadas

- `clipboard-search`: amplía el conjunto de campos y tipos que puede consultar
  la búsqueda local.
- `desktop-shell-layout`: la barra existente mostrará correctamente las
  cards cuyo título personalizado coincide.
- `quick-paste`: la paleta existente mostrará y seleccionará coincidencias de
  título mediante el mismo comando local.

## Impacto esperado

- Core/search: ajuste de la construcción de documentos y del conjunto de
  candidatos del `SearchService`; el motor determinista se reutiliza.
- DB: reutilización de `entries`/`entries_filtered` o equivalente; no se cambia
  el esquema.
- Tauri: el comando existente conserva su firma y sólo devuelve los mismos
  `SearchResponse` con hits adicionales cuando corresponde.
- Frontend: cambios mínimos o nulos; se deben verificar `App.svelte`,
  `QuickPaste.svelte`, `search.ts` y `tauri.ts` para confirmar que ambos usan
  el comando común y no descartan títulos.
