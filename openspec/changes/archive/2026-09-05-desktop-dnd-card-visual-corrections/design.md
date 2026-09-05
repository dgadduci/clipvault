# Diseño: desktop-dnd-card-visual-corrections

## Diagnóstico del drop

La implementación debe reproducir primero si el fallo está en:

1. la card no genera dragstart;
2. WebKit no expone el MIME privado en dragover;
3. el sidebar no ejecuta preventDefault y por eso no recibe drop;
4. el parser rechaza el valor;
5. App no encuentra la entrada o la colección;
6. la hidratación no tiene asociaciones actuales;
7. entry_collections_set no se invoca, falla o su resultado no se refleja.

No aceptar como evidencia sólo que exista la clase visual drag-over: la prueba
obligatoria es que la asociación se lea después de refrescar, cambiar de
colección y reiniciar.

## Contrato compatible de DataTransfer

El MIME principal sigue siendo el tipo privado de ClipVault
application/x-clipvault-entry-id. Para WebKit/Tauri, la card también debe
escribir un fallback text/plain con un prefijo exacto y una versión estable,
por ejemplo clipvault-entry:v1:<integer-id>.

El target:

- acepta dragover si está presente el MIME privado o el fallback exacto;
- usa preventDefault en cada dragover válido;
- en drop intenta primero el MIME privado y luego el fallback;
- rechaza cualquier texto que no cumpla exactamente el formato;
- no interpreta texto arbitrario de otra aplicación como una entrada;
- nunca incluye ni lee contenido del clipboard.

El helper de parseo debe ser puro, aceptar sólo enteros finitos positivos y
rechazar campos adicionales, espacios ambiguos, rutas, JSON extraño y valores
externos.

## Flujo de persistencia del drop

Una colección de usuario es un destino válido. Historial sigue siendo una
colección protegida y el drop allí es un no-op. Una vez parseados entry id y
collection id:

1. comprobar que la entrada y la colección siguen existiendo;
2. obtener las asociaciones actuales desde el cache sólo si están en estado
   loaded; de lo contrario, consultar el bridge metadata-only existente;
3. fusionar la colección destino de forma aditiva, sin duplicados;
4. invocar el comando existente entry_collections_set;
5. esperar su resolución antes de mostrar éxito;
6. refrescar organization y la hidratación de esa entry;
7. conservar Historial, otras colecciones, tags y todos los campos del
   EntryRecord.

Si falla la hidratación o el comando, no se debe enviar [] ni una lista
parcial. El drop debe mostrar un error seguro, liberar su estado busy y dejar
las asociaciones sin modificar. Drops repetidos durante una operación en vuelo
deben ignorarse o deduplicarse.

## Feedback visual

Mientras el puntero arrastra un payload válido sobre una colección de usuario,
el item debe cambiar suavemente de color mediante transition local de
background, border y/o box-shadow. El estado debe ser perceptible, mantener
contraste y diferenciarse del estado active.

El highlight sólo aparece en destinos válidos. Debe limpiarse en dragleave real,
drop, dragend, cancelación con Escape, cambio de colección, error y destroy.
No debe quedar pegado por cruzar hijos internos del item. No se requieren
animaciones de red ni recursos externos.

## Icono de favorito

La propuesta visual es una chincheta o pushpin minimalista: cuerpo vertical
redondeado, cabeza pequeña y punta orientada hacia abajo, con una variante
outline para no fijado y una variante rellena/destacada para fijado. Es más
directa que una estrella porque representa literalmente fijar una captura.

Debe ser un SVG local simple, centrado, de tamaño legible y sin cruces
innecesarios. Conserva is_pinned, setFavorite, aria-pressed, Anclar/Desanclar,
focus-visible, hover y estado busy. El código no debe contener una estrella o
glyph de estrella como fallback.

## Icono de aplicación fuente

El tamaño visual actual del source-app icon debe aumentar exactamente un 50%.
Si la base actual es 1.1rem, el nuevo tamaño debe ser 1.65rem, salvo que una
variable común equivalente sea la fuente real. El icono debe conservar
object-fit, border-radius, fallback, tooltip y aria-label. El header debe
reacomodarse sin desbordar, ocultar el título o cambiar el tamaño cuadrado de
la card.

El cambio es sólo de presentación: no modifica source_app, source_app_name,
source_app_icon_ref ni el resolver de assets.

## Eliminación de la línea inferior

La línea debajo del rail que muestra el estado técnico del listener y de
pegado sintético debe dejar de renderizarse en el desktop principal. Si algún
dato sigue siendo útil para diagnóstico, debe permanecer accesible dentro de
Development, no desaparecer del sistema. Los errores y avisos accionables del
flujo principal no deben quedar ocultos.

Eliminar esta línea no debe cambiar el alto del rail, la carga de imágenes, el
scroll de colecciones ni el funcionamiento de quick-paste.

## Protección contra regresiones

No reconstruir EntryRecord con spreads parciales para resolver el drop o
actualizar la UI. Deben conservarse asset_ref, mime_type, payload_width,
payload_height, content_size, created_at, tags, collections e is_pinned.

La prueba de imagen debe cubrir:

SQLite → recent_entries → HistoryCardRail → clipboard_asset → Blob URL →
thumbnail

después de reiniciar, hidratar tags, cambiar de colección, hacer drop y hacer
pin/unpin. Ningún resultado obsoleto puede reemplazar un thumbnail válido.

## Tests requeridos

- Helpers puros para payload privado, fallback text/plain y parseo estricto.
- Frontend para dragstart con ambos formatos, dragover con sólo fallback,
  drop, idempotencia, busy, error y cleanup.
- Integración real del callback hasta entry_collections_set y rehidratación.
- Tests de transición visual y limpieza del highlight.
- Tests de pin outline/relleno y ausencia de estrella.
- Test del aumento del icono fuente en 50% y fallback intacto.
- Test de ausencia de listener-status en el desktop y presencia del diagnóstico
  en Development si sigue siendo necesario.
- Tests de imagen tras reinicio y tras operaciones de organización.

## Fallback de puntero para Tauri/WebKit

La implementación mantiene los helpers de `DataTransfer` para hosts que
entregan el ciclo HTML5 correctamente, pero el desktop no depende de ellos.
En WebKit embebido puede verse el gesto de arrastre sin que lleguen
`dragover`/`drop` al DOM; por eso `App.svelte` instala una sola vez un
controlador delegado de `pointerdown`/`pointermove`/`pointerup`.

El controlador reconoce una card por sus atributos de UI, espera un umbral
de movimiento para no convertir clicks en drags, abre la sesión interna y
resuelve el destino con `document.elementFromPoint` en cada movimiento. Esto
incluye filas que están dentro de un `overflow-y: auto`: el hit-test usa el
contenido actualmente visible, no el orden lógico de la lista.

El controlador emite eventos internos metadata-only al elemento bajo el
puntero. La lista de colecciones valida que sea una colección `user`,
mantiene el highlight y delega la mutación al callback existente de
`App.svelte`. El desktop no muestra un recuadro inferior ni un banner verde
después del drop; el único feedback visible de destino es el highlight
transitorio de la colección. Ningún evento recibe contenido del clipboard.

El controlador también crea una previsualización genérica y efímera después
del umbral de activación. La previsualización sólo contiene las etiquetas
`Captura`, `Arrastrando…` y `Suelta en una colección`, tiene
`pointer-events: none` y no contiene texto de la captura, título, aplicación
fuente, referencias de assets ni píxeles de imagen. La card bloquea la
selección nativa de texto durante el gesto (`preventDefault` y
`user-select: none`), mientras que menú, pin, título y demás controles
interactivos siguen excluidos como fuentes de arrastre.

La card lleva `draggable="false"` para evitar que el drag HTML5 competidor
interrumpa el canal de puntero en Tauri. El cleanup es idempotente y cubre
`pointerup`, `pointercancel`, blur y destroy. El drop sólo se considera
válido cuando la sesión interna sigue activa y el destino resuelve una fila
actual, de modo que un arrastre externo no puede generar una asociación.
