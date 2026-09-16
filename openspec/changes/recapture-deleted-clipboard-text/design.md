# Diseño: revisión de portapapeles + baseline destructivo

## Limitación del contrato actual

`ClipboardBackend::read_payload` expone el contenido actual del
portapapeles pero no permite distinguir dos escenarios que el watcher
debe tratar de forma opuesta:

- el portapapeles sigue conteniendo el mismo texto entre dos polls;
- el usuario copió exactamente el mismo texto (nueva escritura).

Limpiar el fingerprint del watcher después de una eliminación era una
solución incompleta: con la primera lectura, el fingerprint volvía a
estar vacío y el siguiente poll interpretaba la permanencia del texto
como un cambio, recreando la fila recién eliminada.

## Decisión

Añadir un marcador metadata-only de cambio en la frontera de
`clipboard-platform`. El trait `ClipboardBackend` expone un método
`revision` que devuelve un contador entero monótono:

- En macOS: `NSPasteboard::changeCount` (entero `NSInteger` que
  incrementa cada vez que el pasteboard declara un nuevo dueño o
  recibe contenido nuevo). Es el análogo oficial de
  `NSPasteboard.changeCount` que la documentación de Apple recomienda
  para evitar re-capturas.
- En Linux X11: `ArboardClipboard` mantiene una conexión de observación
  separada y se suscribe a `XFixesSetSelectionOwnerNotifyMask` para la
  selección `CLIPBOARD`. Cada evento `SetSelectionOwner` incrementa un
  contador in-memory metadata-only, incluso si el owner vuelve a publicar
  exactamente el mismo texto.
- En Linux Wayland: el build actual de `arboard` no activa el protocolo
  privilegiado `wayland-data-control`; sus lecturas de texto se sirven por
  XWayland. Cuando `$DISPLAY` ofrece XWayland, el mismo monitor XFixes
  observa ese stream de selección y proporciona la revisión para la sesión
  Wayland. No se finge una revisión mediante un diff del texto.
- Cuando no existe X11/XWayland o XFixes no está disponible, `arboard`
  conserva el diff de payload sólo para colapsar polls ordinarios, pero
  `revision()` devuelve `ClipboardRevision::UNKNOWN`. Así el baseline
  destructivo conserva el estado en vez de fabricar una señal de copia.

La lectura del payload y la lectura de la revisión se hacen **dentro
del mismo lock de backend** (o en una operación `read_payload_with_revision`
atómica) para que la revisión y el contenido correspondan siempre a la
misma observación. La trait actualiza la firma para ofrecer ambos datos
acoplados.

Cuando macOS se construye como `CompositeClipboard`, la señal no puede
quedar encerrada en el adaptador rico nativo: el watcher usa
`ClipboardBackend::revision` directamente al establecer el baseline tras un
borrado. Por ello el adaptador nativo implementa ese accessor con
`NSPasteboard.changeCount` y el composite lo reexpone con prioridad sobre la
pata `arboard`. La misma prioridad se usa para `read_observation`. Linux no
cambia de ruta: allí ambas patas son el backend `arboard` y su contador
XFixes sigue siendo la fuente de revisión.

## Modelo del watcher

`WatcherState` deja de almacenar solo `last_hash` y pasa a guardar:

```rust
struct WatcherState {
    last_revision: Option<ClipboardRevision>,
    last_hash: Option<String>,
    interval: Duration,
}
```

Flujo del tick:

1. Lee `(payload, revision)` en una sola llamada atómica.
2. Si `last_revision == Some(revision)` → `Unchanged` sin tocar
   persistencia.
3. Si `last_revision.is_none()` o `last_revision != Some(revision)`:
   - calcula `payload_fingerprint` para enrutar la supresión y la
     deduplicación;
   - actualiza `last_revision` y `last_hash`;
   - ejecuta el camino de captura habitual (suppression check →
     `record_clipboard_payload_with_correlation`).

`ClipboardRevision` es un `struct` metadata-only que envuelve un `u64`
opaco; su `Debug` solo imprime `revision=<u64>` y nunca expone el
payload.

## Baseline después de eliminar

Cuando una operación destructiva elimina al menos una fila, invoca un
nuevo método del watcher:

```rust
pub fn baseline_dedupe_state(&self) {
    // 1. Lee (None, revision) en una sola llamada atómica.
    // 2. Fija last_revision = Some(revision) y last_hash = None.
    // 3. NO persiste nada: el siguiente tick con la misma
    //    revisión verá last_revision == Some(revision) y volverá
    //    Unchanged.
}
```

`baseline_dedupe_state` no recibe ni expone contenido, hashes o rutas:
solo lee la revisión actual a través de la frontera de plataforma. Si
la plataforma no puede ofrecer una revisión utilizable, el método
devuelve un resultado `BaselineUnavailable` y el watcher conserva su
estado anterior — un fallo de baseline no resetea el fingerprint.

## Coordinación con la deduplicación de SQLite

- `EntryRepository::insert_or_touch` sigue deduplicando solo contra
  filas vivas. Cuando el watcher detecta una nueva revisión y la fila
  viva correspondiente existe, SQLite devuelve `Duplicate` con el id
  existente (no se crea una segunda fila).
- Para texto, `content_hash` es la identidad canónica incluso si la
  representación cambia entre plain y rich. Una captura plain refresca la
  fila textual viva más reciente con ese hash; una captura rich prefiere su
  `rich_text_hash` exacto y, si no existe, refresca una fila plain con el
  mismo texto. Así una copia/pegado que cambie de representación no crea una
  segunda card. Dos variantes rich independientes conservan filas separadas
  cuando no existe una fila plain que las unifique. Antes de escribir assets
  rich, el core consulta esa misma identidad y actualiza la fila existente si
  corresponde, de modo que un `Duplicate` no deja archivos sin referencia.
- Cuando el watcher detecta una nueva revisión y la fila viva
  correspondiente no existe (por una eliminación previa), SQLite crea
  una fila nueva con un id distinto.

## Secuencia esperada

```text
capturar A (rev=R1)        -> Stored(id_1), watcher: (rev=R1, hash=A)
capturar A (rev=R1)        -> Unchanged (misma revisión)
eliminar id_1              -> baseline_dedupe_state: watcher: (rev=R1, hash=None)
capturar A (rev=R1)        -> Unchanged (misma revisión, no se recrea)
capturar A (rev=R2)        -> Stored(id_2), id_2 != id_1
capturar A (rev=R2)        -> Unchanged
eliminar id_2              -> baseline_dedupe_state: watcher: (rev=R2, hash=None)
capturar B (rev=R3)        -> Stored(id_3), id_3 != id_1, id_3 != id_2
capturar A (rev=R4)        -> Stored(id_4) (no hay fila viva para A)
capturar A (rev=R4)        -> Unchanged
```

## Límites entre capas

- El core sigue siendo dueño de la semántica de captura, eliminación y
  deduplicación. El shell solo conecta el watcher al management
  service a través del accessor delgado existente
  (`attach_capture_watcher`); no recibe contenido, hashes ni rutas.
- El loop de fondo y `SharedState::tick` siguen recibiendo el mismo
  `Arc<CaptureWatcher>`; las invalidaciones se aplican sobre ese mismo
  handle.
- La implementación de `revision` por plataforma vive en
  `clipvault-platform`. El trait expone solo el contrato metadata-only;
  el core nunca lee contenido del backend directamente.

## Concurrencia y consistencia

- La lectura atómica `(payload, revision)` se serializa a través del
  `Mutex` interno del watcher para que dos ticks concurrentes no
  intercalen su baseline.
- Si dos operaciones destructivas llaman a `baseline_dedupe_state`
  simultáneamente, la última lectura gana: ambas fijan el mismo
  `last_revision` y el resultado es determinista.

## Limitaciones por plataforma

- **macOS (probado)**: `NSPasteboard.changeCount` está documentado y es
  estable. El adaptador nativo (`MacOsPasteboardClipboard`) lo consulta
  en la misma main-thread hop que ya usa para `read_payload`, a través
  del helper `read_change_count_main_thread`. Sin limitación
  conocida. El método retorna [`ClipboardRevision::UNKNOWN`] cuando la
  bridge no puede alcanzar la main thread; el watcher conserva el
  estado anterior en ese caso.

- **Linux X11 (probado manualmente)**: el monitor
  `X11ClipboardRevisionMonitor` se conecta al `$DISPLAY` de la sesión y se
  suscribe a `XFixesSetSelectionOwnerNotifyMask` para `CLIPBOARD`. El contador
  no contiene ni deriva contenido; sólo avanza con eventos de ownership. Esto
  permite distinguir una nueva copia de texto idéntico de un clipboard que
  permanece sin cambios.

- **Linux Wayland vía XWayland (probado manualmente)**: como el adaptador de
  texto actual usa la selección de XWayland, el monitor se conecta a ese mismo
  `$DISPLAY` y recibe los eventos bridgeados de ownership. Una nueva copia del
  mismo texto provoca una revisión nueva y vuelve a pasar por persistencia.
  No se afirma soporte para una sesión Wayland puramente nativa sin XWayland:
  en ese caso `revision()` devuelve `UNKNOWN` y se conserva el fallback seguro.

- **Plataformas sin backend (Windows, `Other`, sin display server)**
  caen al `NoopClipboardBackend`, que retorna
  [`ClipboardRevision::UNKNOWN`]. El watcher conserva el estado
  anterior y no fabrica observaciones. Esta rama cumple el contrato
  "no simular comportamiento" del OpenSpec change.

- **Fallback común**: sin un stream de ownership utilizable, `revision()` es
  `UNKNOWN`; nunca se deriva una supuesta nueva copia desde el payload. El
  watcher conserva su estado al intentar baseline y el diff de payload sólo
  evita reprocesar polls idénticos. Esto conserva la propiedad "el texto que
  ya estaba en el portapapeles no se recrea automáticamente" sin prometer una
  recaptura que la plataforma no puede observar.

## Verificación

- Test del watcher: backend falso con revisión configurable;
  - mismo payload + misma revisión → `Unchanged`;
  - mismo payload + revisión nueva → `Stored` (id nuevo) o
    `Duplicate` (fila viva);
  - payload diferente + revisión nueva → `Stored`.
- Test de persistencia: `insert_or_touch` solo deduplica contra filas
  vivas; eliminado el row, el mismo hash se vuelve a insertar con un
  id distinto.
- Test de gestión: confirmación, idempotencia, favoritos, borrado
  masivo, retención y eliminación de id inexistente siguen
  devolviendo `Unchanged` cuando la revisión no cambia.
- Test del shell: el wiring `attach_capture_watcher` y los comandos
  Tauri siguen siendo adaptadores delgados; los comandos nunca reciben
  ni calculan hashes de contenido.
- Test del adaptador Linux: el fallback sin XFixes informa `UNKNOWN` para
  baselines y conserva su dedupe de payload; la prueba manual X11/XWayland
  confirma que una segunda copia de texto idéntico produce un nuevo evento de
  ownership.
- Ejecución de validación estricta de OpenSpec, tests relevantes,
  `cargo fmt --check`, `cargo clippy` para los crates afectados y
  `git diff --check`.

## Verificación

- Test del watcher: backend falso con revisión configurable;
  - mismo payload + misma revisión → `Unchanged`;
  - mismo payload + revisión nueva → `Stored` (nuevo id) o
    `Duplicate` (fila viva);
  - payload diferente + revisión nueva → `Stored`.
- Test de persistencia: `insert_or_touch` solo deduplica contra filas
  vivas; eliminado el row, el mismo hash se vuelve a insertar con un
  id distinto.
- Test de gestión: confirmación, idempotencia, favoritos, borrado
  masivo, retención y eliminación de id inexistente siguen
  devolviendo `Unchanged` cuando la revisión no cambia.
- Test del shell: el wiring `attach_capture_watcher` y los comandos
  Tauri siguen siendo adaptadores delgados; los comandos nunca reciben
  ni calculan hashes de contenido.
- Ejecución de validación estricta de OpenSpec, tests relevantes,
  `cargo fmt --check`, `cargo clippy` para los crates afectados y
  `git diff --check`.
