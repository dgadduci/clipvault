# Propuesta: Quick Paste y Desktop polish

## Problema

Se detectaron seis oportunidades de corrección y mejora:

1. Escape dentro del preview de Quick Paste cierra toda la ventana en lugar de
   volver a la lista.
2. Las filas de Quick Paste pueden tener alturas o densidades visuales
   inconsistentes; se necesita una geometría fija de dos líneas.
3. Quick Paste no muestra los tags asociados a cada captura.
4. Las cards Desktop muestran texto plano aunque el preview ya puede mostrar
   resaltado de código, tabs y saltos de línea.
5. El icono de tipo de captura no comunica su significado al pasar el puntero.
6. Desktop no permite filtrar directamente por tag.

## Objetivos

- Definir una máquina de estados explícita para lista y preview en Quick Paste.
- Mantener la ventana abierta al salir del preview con Escape.
- Mantener todas las filas con tamaño fijo, aunque el contenido sea corto.
- Mostrar tags sin aumentar la altura de las filas.
- Reutilizar el renderer/presentación de preview compartido en las cards.
- Proporcionar tooltip y nombre accesible para cada tipo de captura.
- Agregar un combobox de tags junto al filtro de aplicación fuente.

## Decisiones de producto

### Escape en Quick Paste

Quick Paste tendrá dos estados visibles: `list` y `preview`. Escape en
`preview` vuelve a `list`, conserva búsqueda, selección, scroll y foco de la
captura seleccionada, y no oculta la ventana. Escape en `list` mantiene el
comportamiento actual de cerrar/ocultar Quick Paste. Un Escape dentro de un
control editable o menú respeta primero el control activo.

### Filas de Quick Paste

Se conserva la geometría fija actual de la ventana y se adopta una fila fija de
dos líneas. La primera línea contiene tipo, título, tags y metadatos compactos;
la segunda contiene preview o miniatura y tiempo. Aunque el título o preview
ocupen una sola línea, la segunda línea sigue reservada. El contenido se limita
visualmente sin cambiar la altura de la fila.

### Tags en Quick Paste

Los tags se muestran en la línea del título, alineados hacia la derecha, como
chips compactos. Se reutiliza la hidratación/consulta de organización existente;
la ausencia o carga pendiente de tags no debe cambiar la altura de la fila ni
mostrar errores como contenido del usuario. Muchos tags se recortan de forma
estable con un indicador accesible de cantidad restante.

### Preview en cards Desktop

Las cards reutilizan la misma proyección segura que usa el preview compartido:
resaltado de código cuando corresponde, preservación de tabs y saltos de línea,
y sanitización existente. La card mantiene límites fijos y sólo muestra una
versión acotada; no se incrusta HTML original sin sanitizar ni se crea otro
renderer.

### Filtro por tags

El nuevo filtro se coloca entre el filtro de aplicación fuente y el menú de
overflow. Incluye `Todas` como primera opción y se limita al historial o a la
colección activa, igual que el filtro por aplicación. Los filtros de aplicación
y tag se combinan con AND cuando ambos están activos. El filtrado es inmediato,
sin botón, y se reinicia de forma segura al cambiar de colección o scope.

## Alcance

Incluye:

- estado list/preview y Escape de Quick Paste;
- layout fijo de filas y tags en Quick Paste;
- presentación segura de código/whitespace en HistoryCard;
- tooltip y accesibilidad del icono de tipo;
- consulta, bridge y UI del filtro por tags.

No incluye:

- nuevas capacidades de clipboard o formatos;
- cambios de persistencia de entries, tags o colecciones;
- renombrado, borrado, favoritos, retención o drag-and-drop;
- cambio del ranking de búsqueda;
- cambios de pegado o copia;
- red, telemetría, embeddings, LLM o dependencias innecesarias.

## Criterios de aceptación

- Escape en preview vuelve a la lista y Escape en lista cierra Quick Paste.
- Todas las filas de Quick Paste tienen idéntica altura y dos líneas reservadas.
- Los tags aparecen en la línea de título sin alterar la geometría.
- Código, tabs y saltos se visualizan en cards con la misma lógica segura del
  preview.
- El icono de tipo tiene tooltip visible y nombre accesible.
- El filtro por tags funciona en Historial y en la colección activa, combinando
  correctamente con el filtro de aplicación.
