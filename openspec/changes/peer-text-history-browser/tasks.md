# Tareas: navegación de historial textual de un par

## Notas de implementación

> El cambio implementa el camino completo de extremo a extremo.
> El dial mTLS productivo **no** queda diferido a `peer-text-import`:
> el cliente resuelve y diala al peer trusted y activo mediante mTLS,
> el listener autentica y autoriza, y proyecta únicamente el historial
> local del host. La superficie `PairingMessage::ListRecentText{,Ack,
> Invalid,Unavailable}` se enchufa al runtime `PeerTransport` con un
> trait method + envelope, pero el dial real (`list_recent_text` en
> `tls.rs`) y la ruta del listener (`run_pairing_session` /
> `handle_list_recent_text`) se completan en este cambio. El comando
> Tauri invoca el transporte autenticado a través de `PeerTextHistoryService`
> y deja de leer la SQLite local como fuente de los previews remotos.

> Pendiente:
>
> - 4.2 — Prueba manual en Wayland, X11 y macOS.

## 1. Contexto

- [x] 1.1 Leer paraguas y los tres cambios predecesores; confirmar pairing,
  trusted health y runtime TLS funcionales antes de añadir API.
- [x] 1.2 Revisar EntryRecord, ContentType, cursor/búsqueda existente y
  baselines de cards/lista de colecciones sin alterar cambios previos.

## 2. Core y transporte

- [x] 2.1 Definir RemoteTextPreview, cursor opaco y eligibility shared para
  texto sin image/rich payload; validar tamaño/Unicode sin body completo.
- [x] 2.2 Implementar proyección newest-first, tie-breaker, límite 50, preview
  escapado de dos líneas/300 caracteres e invalid_cursor inicial.
  > Reapertura: la corrección clampa el `limit` solicitado a `1..=50` y lo
  > propaga del `HistoryHostHandler` al `PeerTextHistoryService::serve` /
  > `HostHistorySource::page_after` para que el host nunca devuelva más
  > filas que el límite aceptado. El `next_cursor` se emite sólo cuando
  > quedan más resultados (petición de `limit + 1` o equivalente acotado)
  > y no sólo porque la página llegó al límite.
- [x] 2.3 Dial mTLS productivo: el cliente diala al peer trusted y activo,
  presenta el cert client-side y verifica el pin SHA-256; el listener
  acepta `ListRecentText`, autentica al peer contra el pin y delega en un
  trait de core (`HostHistorySource`) inyectado por el bootstrap.
  platform no depende de SQLite ni de Tauri. La corrección añade un
  `HistoryHostHandler` productivo instalado por el bootstrap antes de
  aceptar la primera sesión entrante y un `EntryRepositoryHostHistorySource`
  sobre la DB local; todo `ListRecentText` real deja de llegar con
  `history_handler = None`.
  > Reapertura: el camino conserva extremo a extremo la señal
  > `invalid_cursor`. `dial_list_recent_text_async` ya no la convierte
  > en `TransportError::Malformed`; el adaptador core expone un outcome
  > tipado específico (`PeerHistoryTransportError::InvalidCursor` /
  > `PeerHistoryOutcome::InvalidCursor`) y el comando Tauri / frontend
  > la renderiza con su copy tipado. La corrección no convierte un
  > cursor manipulado, de otro peer o firmado antes de una rotación en
  > un error de red genérico.
  >
  > Reapertura: el handler de historial debe persistir dentro de
  > `PairingRuntime` y atravesar cada reinicio del listener. La ruta
  > productiva `install_pairing_transport_with_resolver` invoca
  > `start_with_material_resolver_and_history` pasando el handler
  > registrado en lugar de `None`; `install_history_handler_inner`
  > registra el handler en el runtime para que un nuevo bind lo herede.
  > Como defensa adicional, las rutas heredadas
  > `install_with_material` / `install_with_material_and_resolver`
  > preservan un handler existente si reciben `None`. Se añade un test
  > `PairingRuntime` con transporte grabador (instalar handler, iniciar
  > con resolver, afirmar `start_with_material_resolver_and_history`
  > con `Some(handler)` y `stop`/`start` re-pasando el handler) y un
  > test TLS productivo que arranca el listener por la ruta del
  > toggle (no por el helper de test que recibe manualmente el
  > handler) y verifica que un `ListRecentText` autenticado llega al
  > handler sin devolver `not_available`.
- [x] 2.4 Reemplazar el cursor percent-encoded por HMAC-SHA256 sobre
  `(peer_id, created_at, id)` con secreto de 32 bytes por peer; rotar el
  secreto en `Revoked`; persistir el secreto ligado al peer; devolver
  `invalid_cursor` ante firma inválida, timestamp manipulado o secreto
  rotado. La corrección completa `PeerCursorSecret::generate()` /
  `set_cursor_secret()` con provisionamiento, persistencia, rotación y
  limpieza productiva; el cliente trata el cursor como opaco y sólo
  el host que posee su secreto privado lo valida.
  > Reapertura: añadir un backfill seguro durante el bootstrap para
  > pares `trusted` heredados que carecen de `cursor_secret`. El
  > recorrido por `known_peers` valida la columna como hex de 64
  > caracteres, persiste un secreto CSPRNG nuevo cuando está vacía o
  > malformada, y sólo instala el valor en la caché del servicio
  > después de persistirlo. Filas no `trusted` no se tocan. La
  > corrección queda cubierta por un test representativo de bootstrap
  > con DB temporal (trusted sin secreto → secreto persistido válido e
  > instalado; trusted con secreto válido → preservado; no trusted sin
  > secreto → permanece vacío). Los logs usan mensajes fijos sin
  > `peer_id`, secreto, hostname, IP, puerto, certificado ni
  > contenido.
- [x] 2.5 Reemplazar `snapshot_id` por fingerprint SHA-256 no reversible
  sobre el header de la página transferible; excluir `Html` del set
  elegible (aunque sea textual) y cubrirlo con test.
- [x] 2.6 Tests: orden, cursor firmado (válido, inválido, secreto rotado,
  timestamp manipulado), boundaries, exclusiones (image / rich-text /
  Html), escape, snapshot id no reversible, que DTOs / eventos /
  diagnósticos no exponen contenido completo e integración real con dos
  runtimes TLS productivos (primera página, página con cursor firmado,
  rechazo por cursor manipulado / peer equivocado / secreto rotado /
  handler ausente / persistencia caída).
  > Reapertura: el test de integración TLS real ya no se queda en el
  > `HistoryHostHandler` unitario. Cubre primera página desde el host,
  > segunda página mediante cursor firmado, límite menor que 50,
  > cursor manipulado / de otro peer / invalidado por rotación →
  > `invalid_cursor` en el cliente, handler ausente y fallo de
  > persistencia con outcomes tipados, y verificación mTLS / pin
  > activa. No usa multicast, IP manual ni LAN real: dos `TlsPeerTransport`
  > productivo sobre `127.0.0.1`.
  >
  > Reapertura: el test con `FixedHandler` se conserva únicamente como
  > prueba de routing TLS; la composición productiva de core
  > (`PeerTextHistoryHostHandlerAdapter` + `PeerTextHistoryService::serve`
  > + `RemoteHistoryCursor` HMAC real + `PeerCursorSecret` por peer +
  > rotación del secreto + host source que falla + cursor firmado para
  > otro peer) necesita una integración real con dos `TlsPeerTransport`
  > vivos y el adaptador productivo. El test nuevo vive en
  > `clipvault-core` (donde conviven el adaptador core y
  > `TlsPeerTransport`); el de `FixedHandler` se renombra para que el
  > nombre describa lo que acredita (routing TLS, no cursor firmado
  > productivo) y deja de citarse como evidencia de HMAC, rotación o
  > persistencia.

## 3. Shell y UI

- [x] 3.1 Comando Tauri invoca el transporte autenticado
  (`PeerTransport::list_recent_text`); la fachada `PeerTextHistoryService`
  deja de usar la SQLite local como fuente de previews remotos y
  enruta la solicitud al runtime TLS. No escribe SQLite ni emite
  history-updated.
- [x] 3.2 Restaurar la estructura de dos columnas del desktop. Insertar
  `Equipos vinculados` **dentro** de `OrganizationSidebar` debajo de la
  lista de colecciones; ambos bloques son scrollers verticales
  independientes. `activePeerId` se limpia al seleccionar colección o
  `Historial`. `peerSnapshotCommand` se ejecuta explícitamente al
  cerrar `PeerPairingModal`. Punto verde sólo para
  `trusted && is_present`; gris para trusted no disponible. Sin
  llamada de red al seleccionar peer gris. Sin botón `Volver`.

> Reapertura: la implementación original cubrió el snapshot inicial
    > y el refresh post-pairing, pero NO la actualización reactiva
    > cuando llega o desaparece un anuncio remoto (`Observed` /
    > `Removed`). El desktop (`App.svelte`) es el único propietario del
    > snapshot compartido por `LinkedPeers` y `RemoteHistoryRail`, así
    > que la corrección es local al shell:
    >
    > - Centralizar `refreshPeerSnapshot()` con un guard single-flight
    >   (módulo / closure) y reusarlo desde `refresh()` y
    >   `onPairingClosed()`. Eliminar las llamadas directas a
    >   `peerSnapshotCommand()` que quedan fuera del helper. Una falla
    >   conserva el snapshot anterior y nunca rompe el desktop.
    > - Añadir un polling de 2 s (constante local, consistente con
    >   `PeerSharingModal`) en `App.svelte`, arrancado en `onMount` y
    >   detenido en `onDestroy`. Ejecutar una actualización inicial al
    >   montar. `LinkedPeers` y `RemoteHistoryRail` siguen recibiendo el
    >   snapshot reactivo y NO abren `peerSnapshotCommand()` por su
    >   cuenta.
    > - Alcance: NO se modifica `MdnsPeerDiscoveryAdapter`,
    >   `PeerDiscoveryRuntime`, presencia, pairing ni mTLS. La transición
    >   tras un cierre abrupto se apoya en el cambio archivado
    >   `local-peer-presence-liveness`: el adaptador publica
    >   `Removed` (goodbye, expiración o verificación DNS-SD acotada)
    >   como evento autoritativo, no en `PRESENCE_TTL = 120 s`. El TTL
    >   deja de ser la razón por la que un par pasa a No disponible.
- [x] 3.3 Selección de peer dispara el dial real con outcomes tipados;
  rechazo para revoked / blocked / not trusted / pin inválido / peer
  ausente / cursor fabricado; respuesta tardía no puede sobrescribir
  el panel del peer activo.
  > Reapertura: el outcome tipado propagado por el adaptador ya no
  > convierte `invalid_cursor` en `transport_unavailable` —
  > `PeerHistoryBrowseResponse::InvalidCursor` permanece como
  > variante estable y conserva el `snapshot_id` que el host emitió
  > sin reconstruir un fingerprint remoto parcial.
  >
  > Reapertura: el motivo tipado se conserva de extremo a extremo —
  > un peer local inactivo sigue devolviendo `not_active` y un host
  > que rechaza por falta de trust o de secreto devuelve `not_trusted`
  > (no `not_active`). El cliente (`browse`) traduce `Revoked` /
  > `Blocked` a `not_trusted`, y un `state.active == false` se mantiene
  > `not_active`. Se añaden tests específicos para la distinción
  > `not_active` vs `not_trusted`.
- [x] 3.5 El cierre normal del shell (`⌘Q`, *tray Salir*, `Ctrl-C`)
  llama a `stop_network_subsystems` antes de la pasada de
  retention: primero `stop_pairing_transport()` y después
  `PeerDiscoveryRuntime::stop()`. El peer remoto observa el
  `ServiceRemoved` en ≤ 5 s y el desktop refleja `No disponible`
  gracias al evento autoritativo `Removed` del adaptador mDNS
  (goodbye / expiración / verificación DNS-SD acotada). La
  disponibilidad deja de depender de un TTL fijo: la transición
  está anclada en el cambio archivado `local-peer-presence-liveness`.
  Ambas paradas son best-effort: un fallo en una de las dos sólo
  registra un `warn!` sin IP, puerto, `peer_id` ni contenido;
  nunca impide la salida. La regresión vive en
  `app/tauri/src-tauri/src/bootstrap.rs::tests` y combina un
  adapter de discovery y un transporte de pairing instrumentados
  para asegurar el orden y la idempotencia. El protocolo mDNS,
  el pairing y mTLS no se tocan; una caída abrupta, una
  suspensión o Wi-Fi apagado siguen dependiendo del goodbye /
  expiración que el adaptador traduce a `Removed`.
- [x] 3.4 `RemotePreviewCard` con esqueleto visual de card local pero
  sin reutilizar `HistoryCard` ni habilitar drag/drop, pin, editar,
  copy/paste o acciones locales. El único menú muestra `Importar
  (próximamente)` deshabilitado. La fecha se formatea igual que la
  card local. Cuando el panel principal muestra historial remoto, los
  filtros locales (búsqueda / source-app / tag) no se aplican al rail
  remoto.

## 4. Verificación

- [x] 4.1 Ejecutar fmt, tests Rust relevantes, npm check/build/test,
  OpenSpec strict validation y git diff --check. Regresión
  `pointerDragAndDrop` y test de layout frontend (dos columnas,
  scrollers, toolbar horizontal, rail restaurada). La corrección
  revalida este apartado porque el flujo productivo real (dial
  mTLS + `HistoryHostHandler` + `EntryRepositoryHostHistorySource` +
  secreto HMAC persistido) no estaba cubierto por la verificación
  previa. La nueva revisión vuelve a ejecutar esta batería porque
  se reabre el `invalid_cursor` extremo a extremo, el clamp del
  límite, la propagación del `snapshot_id` y el test de
  integración TLS real.
  > Reapertura: 4.1 vuelve a reabrirse porque la integración
  > productiva (`PeerTextHistoryHostHandlerAdapter` + `TlsPeerTransport`
  > reales + HMAC real + `PeerCursorSecret` por peer + rotación del
  > secreto + cursor firmado para otro peer + source que falla +
  > handler ausente + verificación de no exposición de secreto /
  > cuerpo / hash / IP / puerto / fingerprint en el payload
  > serializado) aún no estaba cubierta por la verificación previa.
  > Se ejecuta y pasa antes de marcar 4.1 / 2.6 como completas.
  >
  > Reapertura: 4.1 vuelve a reabrirse para revalidar la batería
  > completa porque las correcciones de backfill de secretos, de
  > ciclo de vida del handler y de distinción tipada
  > `not_active` / `not_trusted` introducen tests nuevos que no se
  > habían ejecutado antes. La nueva revisión ejecuta los tests
  > focales repetidos (≥3 veces para el test TLS productivo) y
  > vuelve a pasar `cargo check --workspace`, `cargo fmt`,
  > `git diff --check` y `npx openspec validate` antes de cerrar el
  > cambio.
- [ ] 4.2 Prueba manual en Wayland, X11 y macOS: lista reactiva de
  pares, puntos activo/no disponible, reemplazo del panel principal,
  previews horizontales, páginas, falla de red, menú Importar
  deshabilitado, ausencia de mutación local y dial mTLS real entre
  dos máquinas.
