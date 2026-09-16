# Propuesta: recapturar texto eliminado sin recrear capturas persistidas

## Problema

La deduplicación de capturas de texto tiene dos capas que deben permanecer
coherentes:

1. `EntryRepository::insert_or_touch` busca el hash únicamente entre las
   filas vivas. Cuando la fila correspondiente ya fue eliminada la
   persistencia acepta el hash y crea un id nuevo.
2. `CaptureWatcher` conserva en memoria un fingerprint del último payload
   observado y devuelve `Unchanged` cuando la siguiente lectura devuelve
   exactamente el mismo fingerprint. Esa capa es la única protección contra
   reprocesar el mismo payload poll tras poll.

El contrato actual del trait `ClipboardBackend` solo expone el payload
vigente. Con ese único dato no se puede distinguir entre:

- el portapapeles manteniendo exactamente el mismo texto entre polls;
- el usuario copiando nuevamente el mismo texto (mismo contenido, pero
  nueva escritura del portapapeles).

La implementación anterior aprovechaba esa ambigüedad para "invalidar" el
watcher limpiando `last_hash = None` después de una eliminación efectiva.
Eso hace que el siguiente poll devuelva `changed = true` aunque el
portapapeles no haya cambiado, y SQLite acaba recreando una fila con el
mismo contenido aunque nadie haya copiado nada nuevo. El comportamiento
resultante contradice el contrato deseado: el texto que ya estaba en el
portapapeles se vuelve a capturar automáticamente.

## Objetivo

Reemplazar la "invalidación" basada en hash por una señal de cambio del
portapapeles que la frontera de plataforma expone de forma metadata-only.
El watcher compara la revisión actual con la anterior y, solo cuando la
revisión cambió, enruta el payload hacia la pipeline de persistencia.
Cuando la revisión no cambió, el watcher devuelve `Unchanged` aunque el
texto siga ahí.

Cuando una operación destructiva elimina filas, el watcher establece un
baseline no persistido con la revisión actual del portapapeles, de modo
que el siguiente poll con la misma revisión devuelva `Unchanged` sin
necesidad de inventar una captura. Si después el usuario copia exactamente
el mismo texto (revisión nueva) o un texto distinto, el watcher detecta el
cambio y deja que SQLite decida si crea una fila nueva (no hay fila viva)
o devuelve `Duplicate` (la fila original sigue viva).

## Alcance

- Añadir un marcador metadata-only de cambio del portapapeles a la frontera
  de plataforma: `revision` en `ClipboardBackend` (equivalente a
  `NSPasteboard.changeCount` en macOS y a las notificaciones de ownership
  `XFixesSetSelectionOwnerNotifyMask` en X11/XWayland).
- Garantizar que la lectura del payload y la lectura de la revisión
  correspondan a la misma observación (sin carreras entre dos llamadas
  separadas).
- Reescribir el `CaptureWatcher` para comparar revisiones en lugar de
  fingerprints derivados del contenido. El watcher conserva la última
  revisión y el último fingerprint de payload.
- Actualizar las operaciones destructivas (`delete_entry`,
  `clear_non_favorites`, `clear_unorganized_history`, `apply_retention`)
  para invocar `baseline_dedupe_state` cuando se eliminaron filas. Esa
  operación lee la revisión actual sin persistirla, fija el baseline y
  deja que el siguiente poll decida.
- Cubrir macOS y Linux con adaptadores diferenciados. En Linux, el adaptador
  de texto actual usa el stream de selección de X11; por ello el monitor de
  XFixes cubre X11 y Wayland cuando existe XWayland, sin inspeccionar ni
  transportar contenido. Los tests usan un backend falso con revisión
  configurable.

## Fuera de alcance

- Cambiar el contrato del frontend (no debe seguir enviando texto, hashes
  ni referencias de assets al borrar).
- Implementar transportación de contenido, hashes, snippets o rutas en
  logs o respuestas. La señal de revisión es metadata-only.
- Recuperar el identificador de una captura eliminada, crear historial de
  versiones o reescribir la deduplicación de filas vivas.
- Modificar migraciones SQLite, el formato persistido de hashes o el
  contrato de `insert_or_touch`.
- Modificar la política de expiración, favoritos, búsqueda o limpieza de
  assets, salvo para conservar sus contratos actuales.

## Criterios de aceptación

- Capturar `A`, eliminar la fila y mantener `A` en el portapapeles con la
  misma revisión: el siguiente tick devuelve `Unchanged` y no crea una fila.
- Capturar `A`, eliminar la fila y copiar `A` nuevamente (revisión nueva
  del portapapeles): el siguiente tick devuelve `Stored` con un id
  distinto al eliminado.
- Copiar `A` mientras la fila original sigue viva: el siguiente tick con
  revisión nueva devuelve `Duplicate` sobre el id existente, nunca crea
  una fila nueva.
- El borrado sin confirmación, el borrado de un id inexistente y un
  `clear_non_favorites` que no eliminó filas no alteran el baseline del
  watcher (el siguiente tick con la misma revisión sigue devolviendo
  `Unchanged`).
- La supresión existente para los pegados generados por ClipVault
  continúa funcionando sobre el fingerprint del payload, no sobre la
  revisión.
- El loop de fondo y el tick manual siguen compartiendo la misma
  instancia/clones del `CaptureWatcher`; las invalidaciones son visibles
  para ambos.
- En X11 y en sesiones Wayland servidas por XWayland, una segunda copia de
  `A` emite una notificación de ownership nueva aun cuando el contenido sea
  idéntico, y por eso se recaptura tras eliminarla.
- Si no existe un stream de ownership utilizable, el backend devuelve
  `ClipboardRevision::UNKNOWN`; no simula una revisión a partir del texto y
  conserva el fallback seguro documentado.
