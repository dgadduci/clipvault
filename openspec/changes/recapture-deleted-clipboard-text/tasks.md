# Tareas de implementación

## Artefactos OpenSpec

- [x] 1.1 Reescribir la propuesta para reflejar el contrato correcto:
  la recaptura no es automática; depende de una nueva revisión del
  portapapeles. Eliminar la expectativa de "invalidar" el hash como
  gatillo del reingreso.
- [x] 1.2 Reescribir el diseño para introducir la señal metadata-only de
  cambio en la frontera de plataforma, el baseline no persistido y la
  lectura atómica (payload, revision).
- [x] 1.3 Actualizar los delta specs (`clipboard-management`,
  `clipboard-text-history`) para reflejar el nuevo contrato basado en
  revisiones.

## Plataforma

- [x] 2.1 Añadir `ClipboardRevision` y un método metadata-only
  `revision` al trait `ClipboardBackend`. La lectura del payload y la
  lectura de la revisión se exponen de forma atómica para evitar
  carreras.
- [x] 2.2 macOS: implementar `revision` consultando
  `NSPasteboard::changeCount` en la misma main-thread hop que ya usa
  el resto del adaptador nativo.
- [x] 2.3 Linux X11: añadir `X11ClipboardRevisionMonitor`, conectado al
  `$DISPLAY` de la sesión y suscrito por XFixes a los eventos metadata-only
  `SetSelectionOwner` de `CLIPBOARD`. `ArboardClipboard` usa ese contador
  para que una nueva copia de texto idéntico tenga revisión nueva.
- [x] 2.4 Linux Wayland: documentar y cubrir el trayecto actual a través de
  XWayland. El adaptador de texto y el monitor usan el mismo `$DISPLAY`, por
  lo que un evento bridgeado de ownership vuelve observable la segunda copia
  idéntica. Sin XWayland/XFixes se retorna `UNKNOWN`, sin simular la señal.
- [x] 2.5 Adaptar `ArboardClipboard`, `NoopClipboardBackend`,
  `MacOsPasteboardClipboard`, `CompositeClipboard`, `FakeClipboardBackend`
  y los demás backends ya implementados para exponer la revisión.
  `NoopClipboardBackend` y el `UnknownBackend` de los tests
  regresan `ClipboardRevision::UNKNOWN` para preservar el estado
  cuando la plataforma no puede reportar una revisión.

## Core

- [x] 3.1 Reescribir `CaptureWatcher::tick` para comparar revisiones
  primero; solo enrutar el payload a la pipeline cuando la revisión
  cambió. Mantener `last_revision` y `last_hash` sincronizados.
- [x] 3.2 Sustituir `invalidate` por `baseline_dedupe_state`: lee la
  revisión actual y fija el baseline sin persistir nada. Si la
  plataforma no ofrece revisión, conserva el estado anterior y
  registra la indisponibilidad en OpenSpec.
- [x] 3.3 Actualizar `HistoryManagementService::notify_destructive_change`
  para invocar `baseline_dedupe_state` cuando se eliminaron filas
  (delete, clear non-favorites, clear unorganized, retention).
- [x] 3.4 Conservar la supresión de pegados: la supresión sigue
  decidiendo por fingerprint del payload, no por revisión.

## Shell y contratos Tauri

- [x] 4.1 Mantener `clipvault_delete_entry`, `clipvault_clear_history`,
  `clipvault_clear_unorganized_history` y la ruta de retención como
  adaptadores delgados; no introducir hashes ni contenido en sus
  argumentos o respuestas.
- [x] 4.2 Confirmar que el wiring `attach_capture_watcher` y los clones
  del watcher siguen siendo la única instancia compartida entre el loop
  de fondo y el tick manual.

## Pruebas

- [x] 5.1 Backend falso con revisión configurable. Cubrir:
  - mismo payload + misma revisión → `Unchanged`;
  - mismo payload + revisión nueva → `Stored` (id nuevo) o `Duplicate`
    (fila viva);
  - payload diferente + revisión nueva → `Stored` (id nuevo).
- [x] 5.2 Capturar `A`, eliminar fila, mantener `A` con misma revisión:
  el siguiente tick devuelve `Unchanged` y no crea fila.
- [x] 5.3 Capturar `A`, eliminar fila, copiar `A` con revisión nueva:
  el siguiente tick devuelve `Stored` con id distinto al eliminado.
- [x] 5.4 Copiar `A` con la fila original viva: el siguiente tick con
  revisión nueva devuelve `Duplicate` sobre el id existente.
- [x] 5.5 Borrado sin confirmación, id inexistente, borrado masivo sin
  filas afectadas, retención que no purga: el watcher sigue
  devolviendo `Unchanged` con la misma revisión.
- [x] 5.6 Borrar otra entrada (distinta del payload actual) no duplica
  la fila viva: tras el baseline el watcher ve la nueva revisión y
  SQLite resuelve como `Duplicate`.
- [x] 5.7 Verificar que los clones del watcher comparten el mismo
  baseline: invalidación por una vía es visible para la otra.
- [x] 5.8 Mantener las pruebas existentes de imágenes, rich text,
  limpieza de assets, favoritos, diagnósticos metadata-only y
  supresión de pegados.
- [x] 5.9 Verificar que el fallback de `ArboardClipboard` conserva dedupe de
  payload para polls ordinarios, pero no fabrica una revisión utilizable para
  baseline sin XFixes.
- [x] 5.10 Cubrir que `CompositeClipboard` reexpone primero la revisión del
  adaptador rico nativo y usa la pata plain solo como fallback, para que el
  baseline de macOS reciba `NSPasteboard.changeCount`.
- [x] 5.11 Cubrir en SQLite que una fila textual viva conserva su id cuando
  el mismo contenido alterna entre representación rich y plain.
- [x] 5.12 Cubrir end-to-end ambos órdenes rich→plain y plain→rich; el
  segundo no debe crear assets rich sin una fila que los referencie.

## Verificación y cierre

- [x] 6.1 Ejecutar `cargo build --workspace`, los tests relevantes de
  `clipvault-core`, `clipvault-db` y Tauri, `cargo fmt --check`,
  `cargo clippy -p clipvault-core -p clipvault-app --no-deps` y
  `git diff --check`.
- [x] 6.2 Ejecutar `openspec validate recapture-deleted-clipboard-text
  --strict --type change` y revisar el diff completo.
- [x] 6.3 Repetir la validación automatizada y manual de recaptura en X11 y
  Wayland/XWayland después de sustituir el contador por diff de payload por
  el monitor de ownership XFixes.
- [ ] 6.4 Repetir manualmente en macOS el flujo: capturar `A`, eliminarla y
  mantener `A` sin nueva copia; debe devolver `Unchanged` y no recrear la
  fila. Después, copiar `A` de nuevo y verificar que se recaptura.
- [ ] 6.5 No sincronizar specs canónicas ni archivar el cambio todavía.
