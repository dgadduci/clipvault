# Propuesta: filtro por aplicación fuente

## Problema

Las cards ya conservan la aplicación desde la que se obtuvo cada captura y
pueden mostrar su icono, pero el usuario no tiene una forma directa de
reducir el rail a las capturas producidas por una aplicación concreta. La
búsqueda actual trabaja sobre texto y la colección activa, pero no ofrece este
criterio de organización.

## Solución

Agregar un combobox accesible entre la barra de búsqueda y el menú de
configuración. Su primera opción será `Todas`, seguida por las aplicaciones
que tienen capturas en el alcance de la colección activa. Cada opción tendrá:

- icono local de la aplicación cuando exista;
- nombre visible de la aplicación;
- identificador estable interno usado sólo para el filtro.

La selección dispara inmediatamente una nueva consulta. No se agrega botón de
aplicación ni se altera el contenido persistido.

## Recomendaciones adoptadas

1. El filtro se implementa en core/SQLite y no como filtro parcial del array de
   cards. Así el listado de opciones no queda limitado por el rail visible y
   funciona con el límite de resultados actual.
2. Las opciones se calculan por colección activa, independientemente de la
   consulta de texto actual. Esto evita que el combobox cambie sus opciones
   mientras el usuario escribe.
3. El filtrado usa el identificador estable `source_app`, no el nombre visible,
   porque dos aplicaciones pueden presentar nombres parecidos y el nombre
   puede cambiar entre versiones del sistema.
4. Se incluye `Aplicación desconocida` sólo si el alcance contiene capturas sin
   identificador. Nunca se muestra un bundle identifier crudo como sustituto
   de nombre.
5. El filtro vuelve a `Todas` al cambiar de colección. El usuario entra así a
   cada colección sin arrastrar silenciosamente un criterio que puede no tener
   sentido en el nuevo alcance.

## Fuera de alcance

- No modificar la captura ni el almacenamiento de `source_app`.
- No modificar el enriquecimiento ni la extracción de iconos de las cards.
- No agregar sincronización, red, telemetría, embeddings ni dependencias
  innecesarias.
- No cambiar el ranking determinista, fuzzy matching, límites ni privacidad de
  la búsqueda, salvo reducir el conjunto candidato con el nuevo filtro.
- No cambiar tags, favoritos, colecciones, retención, borrado, pegado,
  quick-paste, imágenes ni drag-and-drop.
