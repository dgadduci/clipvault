# Diseño: importación explícita de imágenes desde un par local

## Dependencia y compatibilidad del protocolo

El cambio depende del runtime de peers, el listado de historial remoto y la
importación de texto existentes. La autorización seguirá siendo la combinación
de peer emparejado/trusted, presencia activa y certificado TLS fijado.

### Compatibilidad con clientes legacy y nuevo campo TXT aditivo

La decisión inicial del cambio era anunciar `image_import` como token
adicional dentro del campo `capability` ya existente
(`pairing,image_import`). Tras la revisión del parser de clientes antiguos
se confirmó que varios clientes sólo aceptan el valor exacto `pairing` y
rechazan cualquier otro token por desconocido, lo que rompe el descubrimiento
cuando un host legacy o un cliente de terceros estricto evalúa el campo.

Para preservar esa compatibilidad sin ocultar la incompatibilidad detrás de
un bump de `PROTOCOL_MAJOR` (que exigiría cortar todos los peers emparejados
existentes), el campo `capability` queda limitado al valor canónico
`pairing` (o `discovery_only`). La capacidad `image_import` se anuncia
mediante un nuevo campo TXT aditivo, opcional y compatible hacia adelante:
`caps_extra` (lista separada por comas con tokens adicionales que el host
declara soportar). La capa de descubrimiento acepta ambos formatos y la
resolución de capacidades combina el campo `capability` legacy con el nuevo
`caps_extra`.

> **Migración SQLite:** la columna `known_peers.caps_extra` se añadió con
> la migración aditiva `0018` (definida en
> `crates/clipvault-db/src/registry.rs`). El `up` es
> `ALTER TABLE known_peers ADD COLUMN caps_extra TEXT NOT NULL DEFAULT ''`,
> por lo que todas las filas existentes reciben el valor vacío sin un
> `UPDATE` explícito. El `down` reconstruye `known_peers` con la columna
> omitida y copia explícitamente cada columna preservada
> (`peer_id`, `public_key_fingerprint`, `display_name`, `protocol_major`,
> `capability`, `first_seen_at`, `last_discovered_at`, `updated_at`,
> `trust_state`, `tls_cert_fingerprint`, `paired_at`,
> `paired_protocol_major`, `full_public_key_fingerprint`, `cursor_secret`)
> y recrea los índices `idx_known_peers_last_discovered_at`,
> `idx_known_peers_first_seen_at` y `idx_known_peers_trust_state`. El
> test `known_peers_migration_rolls_back_cleanly` del registro cubre
> exactamente este contrato: aplicar `builtin_migrations()` sobre una
> base temporal, ejecutar `rollback_migration` sobre `0018`, verificar
> que el resto de las columnas sigue presente y que
> `caps_extra` desaparece, y luego re-aplicar la migración.

| Campo legacy (`capability`) | Campo aditivo (`caps_extra`) | Capacidades efectivas |
|-----------------------------|------------------------------|------------------------|
| `pairing`                   | (ausente)                    | `pairing`              |
| `pairing`                   | `image_import`               | `pairing`, `image_import` |
| `discovery_only`            | (ausente)                    | `discovery_only`       |
| `discovery_only`            | `image_import`               | (sin texto; el host no expone endpoints pairing) |

Un cliente antiguo que sólo lea `capability` seguirá viendo `pairing` exacto
y podrá seguir negociando el flujo de emparejamiento; el campo `caps_extra`
es ignorado por su parser. Un cliente nuevo lee ambos campos, combina los
tokens y aplica la verificación de capability con la lista combinada. La
adición de capacidades futuras (p. ej. `rich_text_share`) reutilizará el mismo
mecanismo: añadir el token a `caps_extra` sin modificar `capability`.

Cada observación compatible de mDNS actualiza `known_peers.caps_extra`, además
de los timestamps y cualquier transición de `capability`. Así una fila creada
antes de que `image_import` existiera recibe el token en su siguiente
observación y los resolvers SQLite dejan de tratarla como peer legacy. El
snapshot metadata-only también proyecta `caps_extra` para que el renderer
pueda habilitar Importar sin reinterpretar el campo legacy.

Se añadirá la capacidad `image_import` al contrato de capacidades. El cambio
usará endpoints DTO separados para imágenes, manteniendo `list_recent_text` y
`fetch_text` sin cambios para peers anteriores. La implementación puede
compartir helpers internos de proyección y autorización, pero no expondrá un
payload genérico de bytes al core o al bridge.

Si el peer remoto no anuncia `image_import` (ni en `capability` legacy ni en
`caps_extra`), la UI no ofrecerá una acción de importación de imagen. Los
peers que sólo soportan texto conservarán el flujo actual. Si el endpoint de
imágenes responde `not_available` mientras el endpoint de texto responde
correctamente, el merge consume la respuesta de texto y considera agotado sólo
el stream de imágenes; la ausencia opcional de imágenes no invalida el
historial de texto ni produce un error global de carga.

## Listado remoto y preview

El host proyectará únicamente entradas de imagen que cumplan todas estas
condiciones:

1. `content_type` es imagen.
2. `asset_ref`, MIME, dimensiones, tamaño y hash de la fila son coherentes.
3. El asset existe dentro del namespace `clipboard/` y pasa la validación de
   `ClipboardAssetStore`.
4. El tamaño y las dimensiones están dentro de los límites locales.

La fila remota contendrá sólo metadata segura: `remote_entry_id`, título
validado, fecha, tamaño, ancho, alto y tipo imagen. No contendrá bytes,
`asset_ref`, ruta absoluta, hash, tags, favoritos, colección ni aplicación de
origen.

La tarjeta remota usará una imagen placeholder estática embebida en el cliente,
idéntica para todas las capturas de imagen. Podrá mostrar dimensiones y tamaño
como metadata, pero no intentará descargar el asset durante la navegación.
Los thumbnails generados o transferidos son responsabilidad de un cambio
posterior.

La paginación conservará el cursor autenticado y los límites existentes. La
proyección de texto continuará mostrando previews acotados, mientras que una
fila de imagen no contendrá una vista previa derivada de sus píxeles.

### Semántica de paginación y consumo de buffers

La rail es **paginada con ventana de 50 filas combinadas**: cada página
visible contiene como máximo `MAX_COMBINED_PAGE_ROWS = 50` filas (la
unión newest-first de texto e imagen). Las filas que la respuesta
devuelve pero no entran en la ventana viven en buffers por stream
(`textBuffer`, `imageBuffer`) hasta que el usuario avanza.

`Siguiente` consume los buffers antes de pedir una nueva página al
host:

1. Si hay filas en `textBuffer` o `imageBuffer`, se promueven a la
   ventana visible (mezcladas newest-first, capeadas en 50) y el
   resto vuelve a quedar en buffers. No se hace ninguna llamada al
   bridge.
2. Cuando ambos buffers quedan vacíos **y** al menos un cursor es no
   vacío, se pide la siguiente página al endpoint correspondiente con
   `pickNextCursors` (los streams agotados se omiten vía `null`).
3. Cuando ambos buffers están vacíos y los dos cursores están
   vacíos, la rail marca `exhausted = true` y desactiva `Siguiente`.

El helper `applyResponses` (la única ruta que combina buffers y
respuestas) **no** vuelve a incluir las filas visibles anteriores: la
ventana de la nueva página se construye exclusivamente con las filas
que llegaron del host en la última petición más los buffers que
todavía no se habían consumido (los buffers pendientes al momento de
disparar la petición). Las filas anteriores visibles quedan fuera del
cómputo, garantizando que las filas que la primera respuesta dejó
ocultas en los buffers lleguen a la ventana cuando el usuario avanza,
sin duplicar `remote_entry_id` ni rebasar el límite combinado.

El botón `Siguiente` se mantiene habilitado siempre que queden buffers
por consumir, incluso cuando ambos cursores estén vacíos; sólo se
desactiva cuando la rail está completamente agotada.

## Fetch en el host

El fetch se ejecutará sólo después de la acción explícita `Importar` y sobre la
conexión mTLS autenticada. El host volverá a comprobar:

- identidad y estado autorizado del caller;
- existencia de `remote_entry_id`;
- que la entrada sigue siendo una imagen transferible;
- que el `asset_ref` sigue apuntando a un asset local válido;
- firma, dimensiones y tamaño del PNG.

El host leerá los bytes mediante `ClipboardAssetStore`. La respuesta llevará
el PNG persistido y metadata mínima de transporte necesaria para validarlo;
no llevará la referencia de asset del host, rutas ni metadata organizativa.
Los límites de tamaño y dimensiones reutilizarán `MAX_CLIPBOARD_ASSET_BYTES`,
`MAX_CLIPBOARD_IMAGE_DIM` y los límites de frame/respuesta del transporte. No
se introducirá un límite paralelo sin documentar su relación con esos valores.

## Validación y persistencia local

El receptor tratará los bytes como entrada no confiable. Antes de tocar
SQLite deberá validar firma PNG, decodificación, dimensiones, tamaño y
coherencia del frame. Luego usará la pipeline de imagen existente para obtener
el PNG canónico que se persiste localmente.

El asset se escribirá con `ClipboardAssetStore` y una referencia local de la
forma `clipboard/<sha256>.png`. Nunca se copiará el `asset_ref` remoto ni se
intentará abrir una ruta del otro equipo. La escritura será atómica y usará un
directorio temporal aislado para tests.

No se necesita una tabla nueva si las migraciones actuales de imágenes,
`peer_collection_bindings` y `remote_imports` cubren los campos existentes.
`imported_content_hash` será el hash del PNG canónico que el receptor guarda,
no un hash suministrado por el peer. Si la implementación descubre que el
esquema actual no permite distinguir correctamente snapshots de imagen, debe
pausar y actualizar este spec antes de cambiar la migración.

## Transacción de importación

La lógica vivirá en core Rust, no en un comando Tauri. El flujo será:

1. Resolver y validar el peer autorizado y el `remote_entry_id`.
2. Obtener y validar el PNG sin escribir al clipboard.
3. Normalizarlo y escribirlo de forma atómica en el asset store local.
4. Buscar una entrada local por el hash canónico persistido.
5. Crear una entrada de imagen si no existe, con hora local de importación y
   título remoto sólo si pasa la validación de títulos.
6. Reutilizar una entrada idéntica sin modificar título, timestamp, favoritos,
   tags, colecciones, metadata local o asset ya existente.
7. Resolver el binding de colección por `peer_id`, reutilizando el servicio de
   importación de texto y sus reglas de colisión.
8. Insertar membership y provenance de forma idempotente.
9. Confirmar la transacción y emitir los eventos metadata-only existentes.

Si falla la transacción, el asset temporal se limpia y no quedan entrada,
membership o provenance parciales. Un asset existente compartido no se borra
como parte del rollback.

### Coordinación de staging entre commits concurrentes

La protección contra rollback destructivo durante una importación
concurrente usa dos mecanismos coordinados:

- `SqliteImageImportPersistence` mantiene el conteo de leases y un
  marcador `deleting` por `asset_ref`. `stage_image_asset` serializa
  store + registro de lease; si el asset está reservado para cleanup,
  espera hasta que termine antes de escribir o reutilizarlo.
- El último `release_staged_asset` retira su lease y establece la
  reserva `deleting` bajo el mutex. Mantiene la reserva mientras
  comprueba referencias SQLite y decide si elimina el archivo; todos
  los caminos de salida retiran la reserva y notifican a los stages
  que esperan.
- `ClipboardAssetStore` comparte un mutex de mutación entre sus clones.
  El rollback mantiene ese guard desde el chequeo de referencias hasta
  el unlink. La captura local mantiene el mismo guard desde
  `store_image` hasta confirmar `insert_or_touch`, evitando que un
  rollback elimine un asset que la captura reutilizó pero aún no había
  referenciado en SQLite.
- `commit_import_transaction` decrementa el lease después del commit
  SQLite exitoso. Un rollback sólo elimina cuando no quedan leases ni
  filas `clipboard_entries` que referencien el asset.

La invariante concurrente cubre tanto una segunda importación que ya
tenía una lease como una que empieza después de que el primer rollback
reservó el cleanup: la segunda operación espera el unlink, vuelve a
escribir el PNG y puede confirmar una fila cuyo asset sigue siendo
legible. Si una captura local gana el guard primero, el rollback observa
su fila y preserva el archivo; si gana el rollback, la captura escribe
el asset nuevamente antes de insertar su fila.

### Agotamiento real vs. límite de trabajo en `fill_page_after`

El helper de paginación del host (`fill_page_after`) rellena hasta
`limit + 1` filas elegibles usando `PAGE_AFTER_MAX_BATCHES` lotes de
`page_after`. El helper debe distinguir dos terminaciones:

- `Exhausted`: la capa de persistencia devolvió un lote vacío, lo
  cual significa que no hay más candidatos. El caller puede cerrar
  la página sin emitir un `next_cursor`.
- `WorkLimitReached`: el helper alcanzó `PAGE_AFTER_MAX_BATCHES`
  pero todavía hay candidatos por revisar (filtro de asset /
  dimensión / tamaño devolvió menos filas elegibles que las
  solicitadas). El caller debe reanudar el escaneo desde la
  posición del último candidato examinado, **manteniendo el
  cursor HMAC opaco**: el host firma un cursor cuyo payload es
  `(peer_id, created_at, id)` del último candidato escaneado; el
  cliente lo envía tal cual al pedir la siguiente página. La firma
  garantiza que la continuación no se puede forjar ni apuntar a
  una posición arbitraria.

La fuente SQLite devuelve lotes candidatos sin filtrar los assets;
`fill_page_after` es la única capa que aplica `is_eligible` y conserva
el límite de trabajo y la posición escaneada. `serve` pide a cada
relleno sólo los slots que quedan en la página y no acumula más de
`limit` filas devueltas. Si un relleno agota su presupuesto antes de
llenar la página, `serve` continúa internamente desde el último
candidato escaneado. Si la página se completa, el cursor externo se
firma desde la última fila realmente devuelta, nunca desde filas
elegibles que se escanearon pero se descartaron. Al agotarse el límite
de iteraciones de `serve`, firma la posición escaneada únicamente si
todas las filas elegibles de ese tramo están incluidas en la página.
Una respuesta vacía con cursor firmado sigue siendo una continuación,
no agotamiento; el cliente decide agotamiento por la ausencia de cursor,
no por el número de filas de esa respuesta.

## Dedupe, snapshots y revocación

`remote_imports` se reutilizará con la clave compuesta por `peer_id`,
`remote_entry_id` e `imported_content_hash`. Reimportar el mismo snapshot no
crea duplicados. Si el peer modifica la imagen, el siguiente fetch puede
producir otro hash y se trata como snapshot nuevo, sujeto al dedupe local por
bytes.

La revocación, bloqueo, desconexión o eliminación de la colección vinculada no
borra las entradas, assets ni provenance ya importados. Sólo impide futuros
fetches mientras el peer no vuelva a cumplir las condiciones de acceso.

## Tauri y frontend

El comando Tauri será un adaptador delgado que delega en core/platform y
devuelve un outcome discriminado: importado, deduplicado, no autorizado,
peer no disponible, entrada no transferible, PNG inválido, límite excedido o
fallo de persistencia.

La respuesta y los eventos no incluirán bytes, contenido, rutas, hashes,
`asset_ref` remoto ni datos sensibles. Tras commit se reutilizarán
`history-updated` y `organization-updated`; la UI refrescará Historial y la
colección sin descargar de nuevo la imagen.

La tarjeta remota mantendrá sus controles aislados de `HistoryCard`: no tendrá
drag-and-drop, pin, edición, copia ni pegado. Para filas elegibles mostrará
`Importar`; mientras dure la request mostrará estado busy y permitirá reintento
tras un error seguro. La imagen real será visible en la card local sólo una
vez que el asset haya sido importado.

## Privacidad y no regresión

La navegación no descargará contenido completo. La importación será siempre
iniciada por el usuario y no escribirá al clipboard ni invocará el flujo de
pegado. Ningún log, toast, evento o payload de drag contendrá bytes, contenido,
rutas o identificadores sensibles.

El cambio no modificará el controlador singleton de drag-and-drop, el fallback
de mouse, pointer capture, cancelación, `data-testid="history-card"`,
`draggable="false"` ni el payload opaco de las cards locales.

## Verificación manual propuesta

La matriz debe cubrir Linux X11, Linux Wayland y macOS cuando estén
disponibles: peer activo y emparejado, importación nueva, dedupe, snapshot
editado, restart, revoke/block, peer no disponible, asset inválido y fallo de
persistencia. Debe confirmar que el placeholder remoto no transfiere bytes,
que la imagen real aparece después de importar, que el clipboard no cambia y
que ningún asset local existente se borra o renombra.
