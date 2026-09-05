# Diseño: desktop-header-card-dnd

## Eliminación del encabezado

El desktop no debe renderizar la línea que contiene el nombre de la aplicación
ni el subtítulo Historial local, búsqueda y pegado rápido. El espacio vertical
liberado debe quedar disponible para toolbar, panel de colecciones y rail.

Los estados de conexión, error, búsqueda y accesibilidad no forman parte de
esa línea y deben conservarse donde sigan siendo útiles. El cambio no debe
eliminar el nombre accesible del documento o de la ventana Tauri.

## Drag and drop de cards a colecciones

El drag-and-drop se implementa entre el frontend y usa únicamente un
identificador numérico de entry. El payload de DataTransfer debe usar un MIME
propio de ClipVault y nunca el contenido, snippet, hash, asset_ref, ruta o
bytes de la captura.

Una card textual o de imagen puede iniciar un drag. Una colección de usuario
es un drop target. Al arrastrar sobre ella se muestra feedback visual y al
soltar:

1. se valida el entry id y la colección destino;
2. se obtienen las asociaciones actuales si el estado local no está hidratado;
3. se agrega la colección destino al conjunto existente;
4. se delega en entry_collections_set, conservando todas las demás
   colecciones y Historial;
5. se refrescan la organización y los chips de la card después del éxito.

El drop es una operación de adición, no de reemplazo ni de movimiento. Repetir
el drop es idempotente. Soltar sobre Historial no debe modificar la asociación,
porque toda entrada ya pertenece a esa colección protegida; puede mostrar un
feedback de no-op accesible. Soltar en una colección inexistente o sobre una
card no válida no ejecuta ninguna mutación.

Si las asociaciones actuales no pueden hidratarse, la operación falla de forma
segura y no envía una lista vacía que pueda eliminar asociaciones existentes.
Los errores se muestran sin contenido sensible y el estado de drag se limpia
siempre.

El drag-and-drop no reordena el rail. La card permanece en Historial y en su
colección actual. La alternativa accesible debe conservar el selector de
colecciones existente desde el menú de la card; además, el drop target debe
tener nombre y estado accesibles para teclado y tecnologías asistivas.

## Altura mínima y posición inicial

El desktop debe ocupar sólo la altura indispensable para mostrar toolbar,
panel/rail, estados mínimos y márgenes. La altura mínima de tauri.conf.json y
la altura CSS no deben contradecirse. Se debe evitar una banda vacía debajo del
contenido y mantener min-width: 0 en los contenedores que alojan rails.

Durante setup, la ventana main se dimensiona dentro del área de trabajo del
monitor primario y se posiciona una sola vez:

- x = work_area.x + (work_area.width - window.width) / 2;
- y = work_area.y.

La posición y tamaño deben considerar el scale factor del monitor. Si la
consulta del monitor o el posicionamiento falla, se conservan los valores de
configuración y la aplicación sigue iniciando. No se toca quick-paste.

La operación inicial no debe ejecutarse en un loop ni sobrescribir posteriores
redimensionamientos o movimientos hechos por el usuario.

## Icono de pin

El control de favorito de la card debe mostrar un SVG local de pin, vacío o
relleno según is_pinned. Debe conservar el comando setFavorite, aria-pressed,
labels Anclar/Desanclar, foco visible, estado busy y el cache de organización.
No debe usarse una estrella como glyph, emoji o recurso externo.

## Protección de imágenes y privacidad

El cambio de layout, drag, hidratación y pin no puede reconstruir entries con
campos parciales. Deben conservarse asset_ref, mime_type, payload_width,
payload_height, content_size y el ciclo loading/loaded/error de thumbnail.

Después de reiniciar, las imágenes previamente almacenadas deben seguir siendo
devueltas por recent_entries y clipvault_clipboard_asset y mostrarse en sus
cards. El drag sólo transporta un id opaco, y ningún log, evento o error debe
contener contenido, snippets, hashes, rutas absolutas o bytes.

## Tests

- Helpers puros para construir/leer el payload de drag y combinar asociaciones
  sin duplicados.
- Frontend para dragstart, dragover, drop, cancelación, feedback, errores,
  idempotencia y limpieza.
- Tests de teclado que demuestren que el selector existente permite la misma
  asignación sin mouse.
- Integración de organización que pruebe adición, conservación de Historial,
  conservación de otras colecciones y drop repetido.
- Tests de layout que verifiquen ausencia del encabezado y límites de altura.
- Tests de Tauri/setup para posición centrada arriba, fallback sin monitor y
  no interferencia con quick-paste.
- Tests de pin que verifiquen icono, estado accesible y no-pruning del cache.
- Regresiones de imágenes tras reinicio, hidratación, drag y pin/unpin.
