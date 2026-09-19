# Tareas: descubrimiento persistente de pares locales

## 1. Contexto

- [x] 1.1 Leer local-peer-text-transfer y local-peer-identity-foundation,
  project.md, AGENTS.md, status/diff y el registro de migraciones.
- [x] 1.2 Confirmar que la identidad/profiling foundation está implementada y
  compilable antes de abrir runtime de discovery.

## 2. Datos y settings

- [x] 2.1 Agregar la migración known_peers, repositorio y tests de rollback,
  FK/índices si correspondieran, merge idempotente y ausencia de endpoints.
- [x] 2.2 Agregar local_peer_sharing_enabled a Settings/servicio/bridge, con
  default false y rechazo si no hay identidad segura.
- [x] 2.3 Definir modelos de observación, presencia TTL, validación de identity
  record, self-filter y conflicto de peer_id.

## 3. Runtime y plataforma

- [x] 3.1 Añadir `mdns-sd` detrás de PeerDiscoveryAdapter y fakes; registrar y
  browsear realmente `_clipvault._tcp.local.` con capability
  discovery_only/puerto 0. El `NoopPeerDiscoveryAdapter` actual sólo es un
  doble de tests o fallback explícito para plataformas no soportadas: no puede
  ser el backend productivo de macOS/Linux.
- [x] 3.2 Integrar start/stop idempotente y un worker de eventos al ciclo de
  vida Tauri sin TCP de aplicación ni acceso a contenido. El worker debe
  drenar y persistir observaciones continuas, retirar anuncio/browse al apagar
  y conservar `known_peers`.
- [x] 3.3 Declarar Local Network/Bonjour reales para macOS en el bundle final,
  y mantener un único camino Rust para Linux Wayland y X11. Comentarios o
  documentación de una obligación futura no satisfacen esta tarea.
- [x] 3.4 Propagar `local-peer-discovery-mdns` desde el feature productivo de
  `clipvault-app` hacia `clipvault-core` y `clipvault-platform` en Linux y
  macOS. La prueba manual en Wayland detectó que Linux sólo habilitaba el
  feature de plataforma: el core seleccionaba `NoopPeerDiscoveryAdapter` y
  devolvía `Sin red local` sin llegar a abrir mDNS.
- [x] 3.5 En el apagado ordenado, desregistrar el `fullname` mediante
  `mdns_sd::ServiceDaemon::unregister` antes de `shutdown`, para que los
  browsers remotos reciban `ServiceRemoved` y la presencia cambie a `No
  disponible` sin esperar el TTL. Agregar una regresión de dos adapters que
  compruebe `Observed` seguido de `Removed` al detener uno; puede omitir el
  intercambio sólo cuando el host no permita multicast.

## 4. UI y comandos

- [x] 4.1 Exponer comandos/bridge metadata-only para sharing y snapshot de
  pares; no incluir IP, puerto, texto ni secreto.
- [x] 4.2 Agregar toggle en Settings y vista Equipos de sólo lectura con N
  pares y estados Detectado/No verificado/No disponible.
- [x] 4.3 Añadir tests frontend de toggle, snapshot, estados y ausencia de
  acciones de pairing/historial.
- [x] 4.4 Refrescar el snapshot metadata-only mientras `PeerSharingModal` esté
  abierto y el runtime esté activo, liberando el temporizador al destruir el
  modal. La prueba manual detectó que el worker recibía anuncios pero la UI
  sólo consultaba al montar o cambiar el toggle, dejando una presencia
  histórica como `No disponible` hasta reabrir el modal. Todas las rutas de
  lectura del snapshot deben compartir el mismo guardia single-flight; el
  guardia sólo en el tick no evita el solapamiento con la recarga inmediata.

## 5. Verificación

- [x] 5.1 Probar core/runtime fake: anuncios válidos, malformados, conflicto,
  self-filter, TTL, merge, start/stop y sin contenido en diagnósticos.
- [ ] 5.2 Ejecutar fmt, tests relevantes (incluido el adaptador mDNS), npm
  check/build/test, OpenSpec strict validation y git diff --check. El runner
  `npm test` tiene un bloqueo ESM preexistente: documentarlo con evidencia,
  pero no marcar esta tarea completa mientras siga sin ejecutarse. El fix del
  preflight de loopback deja los tests Rust relevantes en verde, pero no
  desbloquea el runner frontend. Detalle en "Notas de entrega → 5.2".
- [ ] 5.3 Probar manualmente descubrimiento y desaparición entre equipos en
  Wayland, X11 y macOS; registrar firewall/multicast como limitación.

## Notas de entrega

- **3.1 — Adaptador mDNS productivo.** `clipvault-platform` ahora
  declara `local-peer-discovery-mdns` como feature opcional y
  exporta `MdnsPeerDiscoveryAdapter` (en
  `crates/clipvault-platform/src/peer_discovery/mdns.rs`). El
  adaptador usa `mdns-sd` v0.11, registra `_clipvault._tcp.local.`
  con `port = 0` y `capability = discovery_only`, y traduce
  `ServiceEvent::ServiceResolved` en `DiscoveryEvent::Observed`
  sin propagar host / puerto / IP. El `bootstrap` de
  `clipvault-core` instala el adapter por defecto en macOS / Linux
  cuando el feature está habilitado; el fallback sigue siendo
  `NoopPeerDiscoveryAdapter` para Windows builds y cross-compiles.
  - **Bug fix ServiceRemoved → peer_id real.** El adaptador
    publica la instancia con el `display_name` (sanitizado) y la
    helper previa derivaba el `peer_id` desde ese prefijo, así
    que un `ServiceRemoved` con `fullname = "Studio._clipvault._tcp.local."`
    emitía `Removed { peer_id: "Studio" }` en lugar del
    `peer_id` real. La corrección reemplaza esa derivación por
    una asociación privada `service_fullname -> peer_id`
    mantenida dentro del propio adaptador (`AdapterState::peer_registry`,
    instalada en `install`, limpiada en `shutdown`). El
    `ServiceResolved` inserta el mapeo leyendo el `peer_id`
    validable del TXT; el `ServiceRemoved` lo retira y emite
    `DiscoveryEvent::Removed` con el `peer_id` original. No se
    exponen `fullname`, hostname, IP ni puerto fuera del
    adaptador; los `peer_id` distintos para dos pares con el
    mismo `display_name` quedan correctamente separados porque
    el daemon mDNS desambigua las instancias conflictivas en
    `fullname`s distintos que la asociación trata como claves
    independientes. La traducción por evento se extrajo a
    `process_event(&ServiceEvent, &sink, &peer_registry)` para
    poder ejercitarla con eventos sintéticos sin depender de
    la TTL de mDNS, que en mDNS es de ~120 s y volvería el
    bucle de `Removed` impracticable como test.
- **3.2 — Worker y start/stop idempotente.** `PeerDiscoveryRuntime`
  expone `set_persistence(...)` para que el bootstrap inyecte el
  closure que envuelve `KnownPeerRepository::upsert_observation`.
  `start()` arma el `DiscoveryAdvertisement` desde la identidad
  local, instala el `RuntimeSink` que empuja eventos a la cola y
  arranca el worker `clipvault-peer-discovery-worker`. `stop()`
  cancela el worker, une el thread, retira el browse / anuncio y
  conserva `known_peers`. La UI sólo reporta "Activado" cuando
  `kind === "active"`; un fallo de multicast devuelve
  `runtime_stopped` y la UI muestra el estado tipado seguro.
- **3.4 — Propagación del feature mDNS al core.** El feature
  productivo `local-peer-discovery-mdns` en
  `app/tauri/src-tauri/Cargo.toml` ahora habilita, además de
  `clipvault-platform/local-peer-discovery-mdns`, el forwarding
  `clipvault-core/local-peer-discovery-mdns`. La causa raíz era
  exactamente la del bug manual: el `default_peer_discovery_adapter`
  en `crates/clipvault-core/src/bootstrap.rs:992` discrimina con
  `#[cfg(all(feature = "local-peer-discovery-mdns", any(target_os
  = "macos", target_os = "linux")))]`, y ese `cfg` se evalúa contra
  las features del *propio* `clipvault-core`. Hasta ahora el shell
  sólo forwardaba la feature hacia `clipvault-platform`, así que en
  Linux el `cfg` del core quedaba en `false` aunque el de plataforma
  estuviera activo: el bootstrap resolvía `NoopPeerDiscoveryAdapter`
  y el toggle quedaba persistido pero el runtime en `Sin red local`.
  Centralizar el forwarding en la declaración del feature evita
  duplicar el wiring target-specific (los bloques
  `[target.'cfg(target_os = "macos")'.dependencies]` y
  `[target.'cfg(all(target_os = "linux", not(target_os = "macos")))'.dependencies]`
  ya no necesitan repetir
  `local-peer-discovery-mdns` para `clipvault-core`; siguen
  habilitándolo sobre `clipvault-platform` para conservar el
  contrato existente). Verificación:
  `cargo check -p clipvault-app` (compila, recompila
  `clipvault-core` con el feature activo),
  `cargo tree -e features -p clipvault-app -i clipvault-core`
  (muestra `clipvault-core feature "local-peer-discovery-mdns"`
  activada por `clipvault-app feature "local-peer-discovery-mdns"`),
  `cargo tree -e features -p clipvault-app` (saca `mdns-sd v0.11.5`
  en el grafo), 16/16 en `cargo test --package clipvault-core --lib
  peer_discovery`, 16/16 en `cargo test -p clipvault-platform --lib
  --features local-peer-identity-keychain,local-peer-discovery-mdns
  peer_discovery`, 149/149 en `cargo test --package clipvault-db
  --lib`, `cargo fmt --check`, `openspec validate
  local-peer-discovery --strict` y `git diff --check` limpios. No se
  tocaron adaptador mDNS, TXT metadata, puertos, UI ni contratos de
  discovery. Pendiente la prueba manual Linux↔macOS de 5.3.
- **3.3 — Bundle macOS.** El plist real vive en
  `app/tauri/src-tauri/macos/Info.plist` y `tauri.conf.json` lo
  enlaza vía `bundle.macOS.infoPlist`. Tauri mergea
  `NSLocalNetworkUsageDescription` y `NSBonjourServices` (con
  `_clipvault._tcp`) en el `Info.plist` del `.app` durante
  `tauri build`; build.rs ya no documenta la obligación, la
  declara en código verificable.
- **5.2 — Pendiente por infraestructura ESM preexistente.** El
  runner `npm test` sigue cayendo con
  `ERR_MODULE_NOT_FOUND: .../src/types` importado desde
  `.../src/lib/tauri.js`. El problema es la falta de extensión
  `.ts` en los imports relativos (los artefactos `tsc --noCheck`
  resuelven sin extensión pero Node `--test` no). No es
  regresión de este cambio; lo arrastra `local-peer-identity-foundation`
  y la fundación de identidad ya lo documenta en sus notas de
  entrega. Adicionalmente, el test de loopback
  `two_daemons_discover_each_other_on_loopback_when_multicast_is_available`
  fue afinado para *no* probar `mdns_sd::ServiceDaemon::new()`
  por adelantado: ese preflight con `.expect(...)` podía abortar
  con `Operation not permitted` en sandboxes donde el runtime
  todavía consigue tipificar la indisponibilidad vía
  `AdapterError::MulticastUnavailable`. La fix sólo elimina la
  línea del preflight y deja que las dos llamadas a
  `MdnsPeerDiscoveryAdapter::start()` ya existentes dicten el
  camino: si alguna falla, se cierran los adapters iniciados y
  el test sale como omitido (`eprintln!("loopback skipped: ...")`
  y `return`); si ambas arrancan, se conserva la aserción real
  que verifica que el segundo observa el anuncio metadata-only
  del primero. El adaptador productivo y la asociación
  `service_fullname -> peer_id` (con su test
  `resolved_then_removed_emit_same_peer_id`) quedan intactos.
  Resultado en este host tras la fix: 16/16 con código 0 en
  `cargo test -p clipvault-platform --lib --features
  local-peer-identity-keychain,local-peer-discovery-mdns
  peer_discovery` (los 13 tests previos del módulo +
  `resolved_then_removed_emit_same_peer_id`,
  `two_peers_with_same_display_name_dont_interfere` y
  `removed_without_prior_resolved_emits_no_event` ejercitando
  `process_event` con eventos sintéticos sin depender de la TTL
  real de mDNS, y el caso loopback saliendo como
  `loopback skipped` cuando el sandbox bloquea multicast).
  Chequeos adicionales que también salieron con código 0:
  `cargo fmt --check`, `cargo check --workspace --all-targets`,
  `cargo test --package clipvault-core --lib peer_discovery`
  (16/16, incluido `removed_event_flips_presence_to_not_available_immediately`),
  `cargo test --package clipvault-db --lib` (149/149),
  `npm run check` (0 errores), `npm run build`,
  `openspec validate local-peer-discovery --strict` (valid) y
  `git diff --check`. El runtime sigue cubierto además por el
  test loopback con dos daemons `mdns-sd` en la misma máquina
  (cuando el sandbox lo permite).
- **5.3 — Pendiente.** Queda para la prueba manual entre equipos
  reales en Wayland, X11 y macOS; en este cambio no se pudo
  verificar entre equipos. La asociación `service_fullname ->
  peer_id` se valida con eventos sintéticos y el caso loopback
  de la pasada anterior; la cobertura entre hosts queda
  pendiente hasta la prueba manual.
- **3.5 — Goodbye packet y regresión de dos adapters.** El bug
  observado en la prueba manual Linux ↔ macOS — el peer
  remoto seguía marcado como `Detectado` hasta el TTL mDNS
  (~120 s) tras desactivar sharing — venía de que el adaptador
  llamaba `ServiceDaemon::shutdown()` sin ejecutar antes
  `ServiceDaemon::unregister(&fullname)`. En `mdns-sd` v0.11
  `unregister` es el apagado *graceful* del servicio: envía el
  goodbye RFC 6762 §6.7 que provoca el `ServiceRemoved` en los
  browsers remotos, mientras que `shutdown` sólo apaga el
  daemon local sin enviar nada por multicast. La corrección
  vive en `crates/clipvault-platform/src/peer_discovery/mdns.rs`:
  - `MdnsHandle` ahora expone `shutdown(&mut self)` con un
    flag `Cell<bool>` (`shutdown_started`) que ejecuta el
    orden exacto que el diseño exige
    (`local-peer-discovery/design.md` §"Descubrimiento,
    presencia y compatibilidad") y queda idempotente para que
    `Drop` y el camino explícito `stop()` converjan en una
    sola pasada. `MdnsPeerDiscoveryAdapter::shutdown` toma el
    `MdnsHandle` del estado (`Option::take`) y delega en
    `handle.shutdown()`; al salir del `if let`, `Drop` ve la
    guarda activada y corta en seco.
  - El orden es `cancel.store(true) → daemon.unregister(&fullname) →
    daemon.shutdown() → thread.take().join()`. La llamada a
    `unregister` se cubre con `UNREGISTER_WAIT = 750 ms`
    (`recv_timeout` sobre el `Receiver<UnregisterStatus>`
    que `mdns-sd` expone para la confirmación); si vence el
    plazo o `unregister` mismo falla, se registra una sola
    frase técnica segura (`"mdns-sd unregister timed out;
    continuing shutdown"` /
    `"mdns-sd unregister failed; continuing shutdown"` /
    `"mdns-sd daemon shutdown failed"`) — sin `fullname`,
    hostname, IP, puerto ni detalle del error de red.
  - El `fullname` sólo se usa dentro del adaptador para
    pasárselo al daemon; no se loguea ni se expone al core,
    SQLite, frontend, comandos Tauri ni a los sinks.
  - El test `two_daemons_discover_each_other_on_loopback_when_multicast_is_available`
    se extiende para, después del `Observed`, llamar
    `first.stop()` y esperar (≤ 5 s) un
    `DiscoveryEvent::Removed { peer_id }` con el `peer_id`
    del primer adapter. Si alguna de las dos `start` falla
    con `MulticastUnavailable`, el caso sale con
    `loopback skipped` y `eprintln!` igual que antes, sin
    relajarse: una vez que ambos adapters publicaron, tanto
    `Observed` como `Removed` son aserciones obligatorias
    (`assert!(observed, ...)` + `assert!(removed, ...)`).
    Los tests sintéticos previos
    (`resolved_then_removed_emit_same_peer_id`,
    `two_peers_with_same_display_name_dont_interfere`,
    `removed_without_prior_resolved_emits_no_event`,
    `sanitise_instance_name_*`, `translate_resolved_*`,
    `build_txt_properties_round_trips_the_advertisement`,
    `adapter_is_idempotent_around_start_stop`,
    `adapter_rejects_empty_peer_id_with_typed_error`,
    `adapter_rejects_empty_capability_with_typed_error`) y los
    tests neutrales de `peer_discovery.rs` siguen pasando
    intactos.
  - Verificación: 16/16 en
    `cargo test -p clipvault-platform --lib --features
    local-peer-identity-keychain,local-peer-discovery-mdns
    peer_discovery` (los 13 tests previos del módulo +
    el caso loopback ahora ampliado con la aserción
    `Removed`, que pasa dentro del límite de 5 s con
    multicast disponible), 16/16 en
    `cargo test --package clipvault-core --lib
    peer_discovery`, 149/149 en
    `cargo test --package clipvault-db --lib`,
    `cargo fmt --check` (limpio),
    `cargo check --workspace --all-targets`,
    `cargo clippy -p clipvault-platform --features
    local-peer-identity-keychain,local-peer-discovery-mdns
    --tests --no-deps -- -D warnings`,
    `npm run check` (0 errores),
    `npm run build`,
    `openspec validate local-peer-discovery --strict`
    (valid) y `git diff --check` limpios. La verificación
    entre equipos reales sigue pendiente de la prueba
    manual 5.3; tras el fix, en LAN con multicast el peer
    remoto debería pasar a `No disponible` en
    aproximadamente 2–4 s (goodbye + retransmisión a
    120 ms + polling del receiver cada 500 ms). Una
    desconexión abrupta del proceso o de la red puede
    seguir dependiendo del TTL.
- Los tests de `bootstrap::tests::capture_loop_*` y
  `runtime::linux_app_metadata::tests::*` que fallan en el host son
  preexistentes en `HEAD` antes de este cambio y no están
  relacionados con él.
- **4.4 — Refresco periódico del snapshot metadata-only.**
  `app/tauri/frontend/src/PeerSharingModal.svelte` arma un
  `setInterval` de `2_000 ms` (constante
  `PEER_SNAPSHOT_REFRESH_MS`) que delega en el helper compartido
  `refreshSnapshot` (siempre vía `peerSnapshotCommand`, sin
  SQLite / sockets / IPs / puertos / mDNS en el frontend) y se
  libera en `onDestroy` con `clearInterval`. El temporizador
  sólo se inicia cuando `toggle.kind === "active"` (el runtime
  está navegando la LAN); en `identity_unavailable` /
  `runtime_stopped` el modal deja de hacer polling porque el
  worker no está drenando eventos nuevos. El guardia
  `snapshotRefreshPromise` vive dentro de `refreshSnapshot`, no
  en el callback del temporizador: cuando hay un round-trip en
  curso, el helper devuelve esa misma promesa en lugar de
  abrir un segundo `peerSnapshotCommand`. Por eso el tick
  periódico, la recarga inmediata al montar, la recarga
  inmediata posterior al toggle y el reintento de identidad
  comparten el mismo single-flight: el `peerSnapshotCommand()`
  directo sólo existe dentro de `refreshSnapshot`. La IIFE
  limpia la promesa en `finally` para que un refresh rechazado
  no deje el guardia bloqueado. La recarga inmediata al montar
  y al cambiar el toggle se conserva intacta. Verificación:
  5/5 en `node --test
  node_modules/.cache/clipvault-test-build/tests/peerSharingModal.test.js`
  (cadencia acotada, gate sobre `toggle.kind === "active"`,
  un único `peerSnapshotCommand()` directo dentro de
  `refreshSnapshot`, guardia `snapshotRefreshPromise`
  reutilizando la promesa en curso, ausencia de
  `peerSnapshotCommand()` directo en `refresh` /
  `toggleSharing` / `refreshIdentity` / callback del
  `setInterval`, `clearInterval` dentro de `onDestroy`,
  refresco inmediato al montar y al toggle), `npm run check`
  (0 errores), `npm run build`, `openspec validate
  local-peer-discovery --strict` (valid) y `git diff --check`
  limpios. La nota rápida del manual — cerrar y reabrir el
  modal tras unos segundos debe seguir siendo la prueba
  visual de que el worker ya estaba emitiendo y el defecto era
  sólo del refresco en UI.
