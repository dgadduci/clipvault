# Design: capture-into-active-collection

## Selección activa

- El frontend informa al backend cada cambio de selección local mediante un
  comando Tauri tipado que recibe el ID de colección o `null`.
- El estado vive en el `CaptureWatcher` compartido por el loop automático y el
  tick manual; no se persiste y por eso arranca vacío en cada sesión.
- Al montar la interfaz, el frontend envía `null` para restablecer `Historial`,
  incluso si se recargó la vista mientras el proceso Tauri seguía activo.
- Seleccionar una colección de usuario, incluida una subcolección, establece
  ese ID. Seleccionar `Historial` o abrir la vista de un equipo vinculado lo
  limpia. Cerrar la vista del equipo restaura el ID de la colección local que
  seguía seleccionada.
- El frontend sólo establece destinos de colecciones de usuario sin vínculo
  remoto; seleccionar una colección vinculada a un par deja el destino vacío.
- La base de datos vuelve a validar que el destino todavía exista y sea una
  colección local de usuario sin vínculo remoto. Un ID obsoleto o inválido no
  cambia el resultado de la captura ni su pertenencia a `Historial`.

## Persistencia

- El flujo existente conserva la transacción que inserta o deduplica la
  entrada y la asocia a `Historial`.
- Cuando el resultado es `Stored` o `Duplicate`, el core añade la membresía
  activa de forma no destructiva con `INSERT OR IGNORE`. No elimina otras
  asociaciones ni crea una fila adicional para duplicados.
- La asociación es best-effort después de la escritura de historial. Si el
  destino se borró o ya no cumple las reglas mientras se procesa la captura,
  la entrada permanece guardada en `Historial`.
- Las capturas de texto e imágenes del watcher comparten la misma regla. El
  comando directo de captura de texto también consulta el estado activo y usa
  la misma operación de asociación.
- La captura de texto escrita manualmente mantiene su flujo y reglas actuales.

## Actualización de la interfaz y privacidad

- El evento `history-updated` existente ya solicita una actualización de la
  organización y de las entradas; se reutiliza para refrescar conteos y
  membresías visibles.
- No se incorporan IDs, contenido, hashes ni rutas a eventos o logs de
  captura.
- No se modifica el payload ni el controlador de drag and drop.

## Verificación

- Cubrir en DB que la nueva asociación sea aditiva, deduplicable y sólo acepte
  destinos locales válidos.
- Cubrir en core que una captura nueva y una duplicada se asocien al destino
  activo y que sin destino continúen en `Historial`.
- Cubrir la sincronización del destino frontend para selección local,
  `Historial`, equipos vinculados y restauración al cerrar la vista remota.
- Ejecutar las regresiones frontend de drag and drop, pruebas Rust afectadas,
  checks/build de Tauri/frontend, validación OpenSpec estricta y
  `git diff --check`.
