# Tareas: vínculo mutuo seguro entre pares locales

## Notas de implementación

> **Auditoría de integración (2026-09-21, resolución SRV macOS → Linux).**
> La red Linux recibió el PTR `_clipvault._tcp` de macOS, pero
> `avahi-browse -r` agotó la resolución de ambos servicios. El adapter estaba
> usando el fullname de servicio (`<nombre>._clipvault._tcp.local.`) como
> destino SRV; ese valor contiene labels `_...` y no es un hostname DNS
> válido. El resolver permisivo de macOS podía observar anuncios de Linux,
> mientras Linux no alcanzaba `ServiceResolved` para macOS, por lo que el
> runtime nunca registraba presencia. El adapter ahora deriva instancia y
> hostname de `peer_id` (`ClipVault-<peer_id>` /
> `clipvault-<peer_id>.local.`) y mantiene `display_name` exclusivamente en
> TXT. Los tests `service_names_use_a_stable_dns_safe_peer_identity` y
> `service_names_ignore_display_name_and_keep_peers_distinct` cubren el
> contrato. Verificado en Linux: `cargo fmt --all -- --check`, discovery
> 21/21, transporte 32/32, `peer_pairing` 13/13, OpenSpec strict y
> `git diff --check` pasan; la build Tauri también enlazó con `gold` y
> `avahi-browse -r _clipvault._tcp` resolvió el nuevo SRV/TXT/A+AAAA local.
> 5.2 continúa pendiente: macOS debe actualizar este mismo fix y se debe
> repetir la observación bidireccional entre los dos hosts.

> **Auditoría de código (2026-09-21, fix de asimetría macOS ↔ Linux).**
> La prueba manual entre equipos detectó que macOS veía a Linux pero
> Linux no veía a macOS. La auditoría confirmó la causa raíz que la
> hipótesis del usuario describía: el `PeerDiscoveryRuntime` y el
> transporte TLS compartían la misma instancia concreta de
> `MdnsPeerDiscoveryAdapter`, pero el pairing transport instalaba el
> adapter con `start_with_port` y un `MdnsPairingSink` que descartaba
> todos los `DiscoveryEvent`. En el toggle manual el adapter ya estaba
> en `running` por el runtime, así que `start_with_port` devolvía
> `AlreadyRunning` y el pairing install colapsaba a `RuntimeStopped`.
> En el startup con toggle persistido, el adapter quedaba arrancado
> con el sink descartador y la tabla de presencia del runtime nunca
> recibía eventos, por lo que Linux no detectaba a macOS. La decisión
> arquitectónica (documentada en `design.md` §"Frontera del adapter
> mDNS compartido") separa el ciclo de vida del daemon mDNS de la
> publicación del record TLS: el runtime es el único que invoca
> `start` (con `RuntimeSink`); el pairing transport invoca
> `reconfigure` (nuevo método del trait `PeerDiscoveryAdapter`) que
> actualiza el `ServiceInfo` del daemon activo sin reiniciar el browse
> loop ni reemplazar el sink. `withdraw` reverte el record al contrato
> discovery_only con puerto placeholder. El bootstrap ahora invoca
> `sync_runtime_with_settings` antes de `sync_pairing_transport_on_startup`,
> garantizando el mismo orden de arranque que el toggle manual.
>
> - `crates/clipvault-platform/src/peer_discovery.rs` → añade
>   `PeerDiscoveryAdapter::reconfigure` (default
>   `AdapterError::AlreadyRunning`) y la impl del noop.
> - `crates/clipvault-platform/src/peer_discovery/mdns.rs` →
>   `reconfigure_record` interno + `reconfigure` en el trait impl;
>   tests `reconfigure_refuses_a_stopped_adapter`,
>   `reconfigure_keeps_the_running_browse_loop_alive` (dos adapters
>   loopback que se ven mutuamente antes y después del reconfigure),
>   `stop_after_reconfigure_returns_adapter_to_idle_state` y
>   `noop_adapter_reconfigure_collapses_to_already_running`.
> - `crates/clipvault-platform/src/peer_transport/tls.rs` →
>   `MdnsPairingAdvertisementSink::publish` llama `reconfigure` (no
>   `start_with_port`); `withdraw` reconfigura al contrato
>   discovery_only; el `MdnsPairingSink` descartador se elimina;
>   `LOCAL_DISCOVERY_ONLY_CAPABILITY` /
>   `LOCAL_DISCOVERY_ONLY_PROTOCOL_MAJOR` /
>   `crate::peer_discovery::mdns::DISCOVERY_ONLY_PORT` quedan
>   `pub` para que el `withdraw` reconstruya el record; test
>   `mdns_pairing_advertisement_sink_publishes_via_reconfigure`
>   (mismo adapter, `publish` vía `reconfigure`, `withdraw` con
>   adapter en running, `reconfigure` sobre adapter detenido
>   colapsa a `MalformedAdvertisement`).
> - `app/tauri/src-tauri/src/bootstrap.rs` → invoca
>   `settings.sync_runtime_with_settings` antes de
>   `sync_pairing_transport_on_startup` cuando el toggle está
>   persistido como `true`. Sin esto el startup no instalaba el
>   runtime y el `reconfigure` del pairing colapsaba a
>   `MalformedAdvertisement`.
>
> Validación reproducible:
>
> - `cargo fmt --all -- --check` limpio.
> - `cargo test -p clipvault-platform --lib --features
>   local-peer-identity-keychain,local-peer-discovery-mdns,local-peer-pairing-tls
>   peer_discovery` → 21/21 verde (incluye los cuatro tests nuevos).
> - `cargo test -p clipvault-platform --lib --features
>   local-peer-identity-keychain,local-peer-discovery-mdns,local-peer-pairing-tls
>   peer_transport` → 32/32 verde (incluye
>   `mdns_pairing_advertisement_sink_publishes_via_reconfigure`).
> - `cargo test --package clipvault-core --lib` → 417/417 verde
>   (incluye los 15 tests de `peer_pairing`:
>   `revoke_disarms_pinned_cert_fingerprint`,
>   `approve_local_for_inbound_session_routes_to_approve_inbound_session`,
>   `observe_approve_without_inbound_session_is_rejected`).
> - `cargo test --package clipvault-db --lib` → 161/161 verde.
> - `npm run check` → 0 errores / 17 warnings preexistentes.
> - `npm run build` → verde.
> - `openspec validate local-peer-mutual-pairing --strict` → valid.
> - `git diff --check` → limpio.
>
> Limitaciones pendientes:
>
> - 5.2 sigue sin marcarse como completada: la prueba manual entre
>   macOS y Linux con mDNS multicast sobre LAN queda pendiente para
>   la verificación humana. El comportamiento simétrico está
>   cubierto por los tests de loopback en `mdns.rs`; el escenario
>   real depende del firewall / permisos de red del host.
> - `cargo check --workspace --all-targets` sigue sufriendo el
>   SIGSEGV intermitente del crate `time 0.3.55` ya documentado;
>   no es regresión de este cambio. Los crates afectados se
>   verifican con `cargo test --package ... --lib`.
> - El runner `npm test` sigue bloqueado por el problema ESM
>   preexistente; los tests frontend de pairing se ejecutan con el
>   resolver compatible (`node --test ...peerPairingModal.test.js
>   peerSharingModal.test.js`) cuando la auditoría anterior los
>   requirió.

> **Auditoría de código (2026-09-21, continuación de MiniMax).** La
> superficie productiva ya cumple el flujo extremo a extremo; los
> hallazgos críticos de las auditorías previas (sesión única mTLS,
> inbound registrada antes de HelloAck, health comparando el id
> SPKI, `revoke` que también dispara `disarm_pin`, comando
> `clipvault_peer_pairing_health` con unión tipada `ok/failed`,
> display_name real en el wire, `build_outbound_hello` con la
> identidad local, coherencia 64 hex entre mDNS/TLS/SAS/persistencia,
> `toggle_get` que sólo reporta `active` cuando el listener pairing
> está realmente instalado y ausencia total de secretos por IPC)
> ya están implementados y cubiertos por tests. La única
> regresión abierta era de aislamiento entre tests paralelos: el
> `INBOUND_SESSIONS` process-wide static que `stop()` limpiaba en
> cualquier `TlsPeerTransport` provocaba que el test
> `single_mtls_connection_carries_full_pairing_exchange` fallase
> cuando otro test (por ejemplo `install_binds_non_zero_ephemeral_port`)
> llamaba `stop()` durante la ventana de 300 ms en la que el
> test verifica que la sesión inbound sigue viva. El fix
> reemplaza el static por un registro por transporte
> (`TransportState::inbound_sessions`,
> `clipvault_platform::peer_transport::tls::InboundSessionsMap`):
> cada `TlsPeerTransport` posee su propio `Arc<Mutex<HashMap<...>>`,
> `stop()` limpia sólo el suyo, y el `InboundSessionGuard` lleva
> dentro el `Arc` para que el `Drop` apunte al mapa correcto. El
> driver de tests `dial_pairing_session` /
> `dial_async` recibe el mapa del listener como parámetro para
> que el "auto-approve" siga funcionando en los tests que
> ejercitan el protocolo cableado sin pasar por
> `start_outbound`. Verificación reproducible (tres corridas
> consecutivas):
> - `cargo test -p clipvault-platform --lib --features
>   local-peer-identity-keychain,local-peer-discovery-mdns,local-peer-pairing-tls
>   peer_transport` → 31/31 verde (incluye
>   `single_mtls_connection_carries_full_pairing_exchange`,
>   `start_outbound_opens_real_session_through_resolver`,
>   `approve_local_sends_signed_approve_over_open_connection`,
>   `disconnect_peer_drops_open_sessions_and_disarm_pin`,
>   `health_probe_rejects_unknown_and_mismatched_peers`,
>   `pinning_is_enforced_during_handshake_against_rotated_cert`,
>   `canonical_fingerprint_matches_across_mdns_tls_pin_and_sas`,
>   `two_real_listeners_complete_loopback_pairing_handshake`,
>   `two_real_listeners_with_client_certs_complete_reciprocal_pairing`,
>   `arm_pin_then_health_check_rejects_rotated_cert`,
>   `pinning_is_stable_across_simulated_restart`).
> - `cargo test --package clipvault-core --lib` → 417/417 verde
>   (incluye los 13 tests de `peer_pairing`:
>   `revoke_disarms_pinned_cert_fingerprint`,
>   `approve_local_for_inbound_session_routes_to_approve_inbound_session`,
>   `observe_approve_without_inbound_session_is_rejected`).
> - `cargo test --package clipvault-db --lib` → 161/161 verde
>   (cubren `trust_state`, `tls_cert_fingerprint`,
>   `paired_protocol_major`, `full_public_key_fingerprint` y la
>   migración `0015_known_peers_pairing_full_fingerprint`).
> - `node --test
>   node_modules/.cache/clipvault-test-build/tests/peerPairingModal.test.js
>   node_modules/.cache/clipvault-test-build/tests/peerSharingModal.test.js`
>   → 14/14 verde.
> - `cargo fmt --all -- --check`, `cargo check --workspace
>   --all-targets --features local-peer-identity-keychain,
>   local-peer-discovery-mdns,local-peer-pairing-tls`, `npm run
>   check` (0 errores, 17 warnings preexistentes), `npm run build`,
>   `openspec validate local-peer-mutual-pairing --strict` (valid)
>   y `git diff --check` (limpio).
> - El build `cargo check --workspace --all-targets` sufre un
>   SIGSEGV intermitente del crate `time 0.3.55` en este host
>   (también se reproduce en `HEAD` antes del cambio); no es
>   regresión de este PR. La verificación de los crates
>   afectados (`clipvault-platform`, `clipvault-core`,
>   `clipvault-db`) corre verde con `cargo test`.
>
> Con este fix las tareas 2.3, 3.1, 3.2, 3.3, 4.1, 4.3 y 5.1
> quedan verificables; 5.2 sigue pendiente para la prueba manual
> entre Wayland, X11 y macOS.

## Notas de implementación parcial — no aprobada

> **Revisión arquitectónica (2026-09-18).** Las notas históricas de esta
> sección describen una base parcial, pero no acreditan la entrega del cambio.
> `TlsPeerTransport::install` devuelve el puerto `0` sin crear listener ni
> acceptor TLS, y `clipvault_peer_pairing_observe` admite desde el renderer un
> `PairingMessage` y una huella de certificado arbitrarios. Eso permite simular
> una aprobación remota y no cumple la doble aprobación autenticada. Por ello
> se reabren 2.2, 2.3, 3.2, 3.3, 4.1, 4.3 y 5.1. La continuación debe seguir la
> frontera de identidad/transporte/descovery incorporada en `design.md`; las
> notas siguientes sólo sirven como inventario de trabajo reutilizable.

> **Revisión de código (2026-09-19).** La continuación tampoco completa el
> cambio. `AppBootstrap` aún instala `default_peer_transport()`, cuyo
> `TlsPeerTransport::start()` devuelve `Crypto`; no hay llamador productivo de
> `install_with_material`. El sink mDNS descarta `bound_port` y llama al
> adaptador que registra el puerto discovery-only `0`. Los dos extremos usan
> `with_no_client_auth`, por lo que no existe mTLS; además el handshake intenta
> interpretar una huella SHA-256 como una clave Ed25519, fija la huella del
> certificado local en vez del remoto y envía `Approve` automáticamente.
> El certificado se reconstruye con fechas tomadas del reloj, por lo que su
> DER/pin puede rotar entre arranques aunque el seed siga igual.
> `start_outbound` sólo crea estado local con valores ficticios y no abre una
> conexión. No hay health ni pinning productivos. El test rotulado “dos
> listeners” instala un único listener y un cliente auxiliar. Se reabren 2.2,
> 2.3, 3.1, 3.2, 3.3, 4.1, 4.3 y 5.1; no se debe marcar ninguna hasta satisfacer
> las invariantes nuevas de `design.md`.

> **Revisión de código (2026-09-20).** La entrega posterior aún no satisface
> esas condiciones: `clipvault_peer_sharing_toggle_set` sólo sincroniza
> `PeerDiscoveryRuntime` y no llama `install_pairing_transport`; tampoco hay
> ningún caller de `install_default_pairing_material_loader`. El adaptador
> mDNS sigue usando `DISCOVERY_ONLY_PORT = 0`, aunque el sink reciba un puerto
> real. `PairingClientCertVerifier::arm_pin` y
> `PairingServerCertVerifier::arm_pin` están definidos pero nunca se invocan.
> `start_outbound` continúa calculando SAS con `"self"` / `"self-fingerprint"`
> y no inicia una sesión de transporte; `approve_local` sólo cambia un
> booleano local. Por tanto 2.2, 2.3, 3.1, 3.2, 3.3, 4.1, 4.3 y 5.1 siguen
> abiertas. Los tests de transporte aislado no demuestran el flujo productivo.

> **Revisión de código (2026-09-20, segunda revisión posterior al reporte).** El
> cableado mejoró, pero el cambio todavía no está completo. El listener
> productivo acepta mTLS y el test auxiliar `dial_pairing_session` puede
> completar un handshake, pero no existe una API productiva del
> `PeerTransport` para resolver/dialear el peer anunciado, asociar una
> conexión a un `PairingSessionId`, mantenerla abierta y enviar la aprobación
> firmada cuando `approve_local` ocurre. `start_outbound` sólo crea estado en
> memoria y `approve_local` sólo cambia un booleano; por eso dos instalaciones
> reales no pueden completar el vínculo desde la UI. En el camino inbound, el
> acceptor entrega directamente `observe_approve` sin registrar primero la
> sesión Hello, por lo que una aprobación autenticada puede terminar como
> `UnknownOrKeyMismatch`.
>
> La revisión también detecta cuatro incumplimientos adicionales: (1) no hay
> comando Tauri/bridge de `health`, aunque 4.1 lo exige; (2) `arm_pin` sólo
> escribe en `TransportState::pins`, pero los `Pairing*CertVerifier` usados por
> las configuraciones rustls no reciben ese mapa y no rechazan un certificado
> rotado durante el handshake; (3) `revoke`/`block` no llaman a
> `PeerTransport::disarm_pin`; y (4) el anuncio pairing publica el fingerprint
> corto de 16 hex de discovery mientras el protocolo TLS exige el fingerprint
> público completo de 64 hex, así que la validación SPKI/fingerprint no es
> coherente entre mDNS y el handshake. El helper `build_outbound_hello`
> además rellena `peer_id` con el peer remoto en vez de la identidad emisora.
> Finalmente, `sync_pairing_transport_with_toggle` descarta el resultado de
> `install_pairing_transport` y `PeerSharingToggleResponse::from_runtime`
> sólo observa discovery; un fallo al enlazar TLS puede quedar presentado como
> `active` aunque no exista listener pairing.
>
> Por estas razones se reabren 2.3, 3.1, 3.2, 3.3, 4.1, 4.3 y 5.1. 5.2
> sigue pendiente: la prueba manual entre Wayland, X11 y macOS sólo tiene
> sentido después de que el flujo productivo de sesión, pinning y health esté
> realmente conectado.

> **Revisión de código (2026-09-20, entrega final).** Esta entrega implementa
> el flujo productivo completo y reabre las tareas que se habían quedado
> abiertas:
>
> - `PeerTransport` ahora expone `start_outbound`, `approve_local`,
>   `cancel_session`, `disconnect_peer` y `health_probe`. La API toma un
>   descriptor con la identidad del peer remoto y devuelve un
>   `PairingSessionId` opaco. La resolución mDNS vive exclusivamente dentro
>   de `clipvault-platform` a través del nuevo trait `RemotePeerResolver`,
>   implementado por `MdnsRemotePeerResolver`.
> - `start_outbound` resuelve el peer anunciado por mDNS, abre la conexión
>   mTLS, envía `Hello`, espera `HelloAck` y entrega el id de sesión al
>   runtime. `approve_local` firma el transcript canónico y envía el
>   `Approve` sobre la misma conexión mTLS; la observación remota fluye a
>   través del `TransportSink` registrado al `install_with_material` y se
>   enruta al runtime sin que el renderer pueda inyectar nada. No hay
>   auto-approval.
> - En el camino inbound, `Hello` registra la sesión en el
>   `PairingRuntime` antes de aceptar un `Approve`, de modo que la doble
>   aprobación autenticada puede promover la fila. La sesión
>   `register_inbound` consulta la persistencia y bloquea / revocado /
>   rate-limited antes de aceptar.
> - El fingerprint canónico es siempre SHA-256(cert_der) en 64 hex.
>   La identidad que cruza el wire (mDNS, SAS, transcript, pin) es
>   siempre el fingerprint público completo. La proyección de 8 hex que
>   consume la UI vive sólo en `LocalPeerIdentity::short_fingerprint`.
>   `build_outbound_hello` ahora envía la identidad local.
> - `arm_pin` / `disarm_pin` escriben en el mapa compartido que los
>   `Pairing*CertVerifier` consultan durante el handshake. Un certificado
>   que no coincide con un pin armado se rechaza antes de cualquier byte
>   de protocolo. `revoke` / `block` / `unblock` también llaman a
>   `disconnect_peer` + `disarm_pin` para que un par desvinculado no
>   pueda mantener un socket de pairing vivo.
> - El comando `clipvault_peer_pairing_health` (con su wrapper TS
>   `peerPairingHealthCommand` y el tipo `PeerPairingHealthResponse`)
>   ejecuta el handshake de health metadata-only sobre mTLS. La
>   respuesta colapsa UnknownPeer / KeyMismatch / Revoked / Blocked en
>   variantes tipadas.
> - `sync_pairing_transport_with_toggle` ya no descarta el resultado
>   de `install_pairing_transport`: un fallo del bind TLS se propaga a
>   `PeerSharingToggleResponse::from_pairing` y obliga a la respuesta
>   a `RuntimeStopped` en lugar de `active`.
>
> Cobertura nueva en `clipvault-platform::peer_transport::tls`:
> - `start_outbound_opens_real_session_through_resolver`
> - `approve_local_sends_signed_approve_over_open_connection`
> - `disconnect_peer_drops_open_sessions_and_disarm_pin`
> - `health_probe_rejects_unknown_and_mismatched_peers`
> - `pinning_is_enforced_during_handshake_against_rotated_cert`
> - `canonical_fingerprint_matches_across_mdns_tls_pin_and_sas`
>
> Los 29 tests del crate `clipvault-platform` con la combinación
> `local-peer-identity-keychain,local-peer-discovery-mdns,local-peer-pairing-tls`
> y los 13 tests de `clipvault-core::peer_pairing` pasan. `cargo fmt --check`,
> `cargo check --workspace --all-targets`, `npm run check` y `npm run build`
> quedan verdes, `openspec validate local-peer-mutual-pairing --strict` es
> válido, y `git diff --check` no reporta marcadores de conflicto.
>
> **5.2 sigue pendiente** para la prueba manual entre Wayland, X11 y macOS.

> **Auditoría de código (2026-09-20, posterior a la supuesta entrega final).**
> La superficie productiva ahora tiene APIs outbound/approve/health, pero aún
> no cumple el protocolo extremo a extremo:
>
> - `MdnsPairingAdvertisementSink::new` sigue publicando
>   `identity.fingerprint`, que es la huella corta de 16 hex de discovery,
>   mientras `run_pairing_session` rechaza todo `public_key_fingerprint` que
>   no tenga 64 hex. `peer_fingerprint_for` también lee ese valor corto de
>   `known_peers`. En una LAN real, el Hello productivo no puede pasar esa
>   validación.
> - `PairingRuntime::start_outbound` genera `nonce_b` y calcula el SAS antes de
>   llamar al transporte. El listener remoto genera otro nonce para el
>   `HelloAck`; el runtime nunca recibe ese nonce/SAS ni actualiza su
>   `SessionState`, por lo que el código mostrado en la UI puede diferir del
>   código que verifica TLS.
> - El acceptor inbound (`run_pairing_session`) sólo llama al sink después de
>   recibir un `Approve`; no entrega el `Hello`/SAS al runtime ni crea una
>   sesión inbound antes de esperar la aprobación. `TransportSink` tampoco
>   tiene un evento para esa invitación. El segundo equipo no puede mostrar ni
>   aprobar la sesión real.
> - `install_with_material_and_resolver` construye el `ServerConfig` con
>   `state.handshake_pins`, pero luego crea otro mapa local
>   `handshake_pins_arc` y sólo conserva un `Weak`; ese mapa se libera al
>   retornar. El handler de health que intenta hacer `upgrade()` queda siempre
>   sin pin. Además `PairingServerCertVerifier` no recibe el lookup de pins, de
>   modo que el cliente no verifica el certificado remoto contra su pin durante
>   el handshake de health.
> - El test de pinning sólo arma el pin del servidor y fuerza un fingerprint
>   incorrecto; no prueba que el cliente rechace un certificado remoto rotado
>   ni que health autentique el peer remoto. El health probe pre-valida un
>   string local y luego descarta el certificado presentado por el servidor.
> - `approve_local` envía la señal al transporte antes de marcar
>   `local_approved`; una observación remota concurrente puede llegar primero y
>   quedar en `AwaitingRemoteApproval` sin una segunda evaluación.
> - `clipvault_peer_pairing_health` existe, pero los errores se devuelven como
>   `CommandError`; la variante `PeerPairingHealthResponse::Failed` queda sin
>   uso. Debe conservarse el contrato discriminado que exige el spec.
> - `revoke` cierra sesiones pero no ejecuta `disarm_pin` (block/unblock sí lo
>   hacen), contradiciendo la nota de entrega que afirma cubrir las tres
>   transiciones.
> - `clipvault_peer_sharing_toggle_get` sólo consulta discovery. Después de un
>   reinicio con la preferencia activa puede informar `active` aunque el
>   listener pairing todavía no haya sido reinstalado.
>
> Por estas razones se reabren 2.3, 3.1, 3.2, 3.3, 4.1, 4.3 y 5.1. La prueba
> manual 5.2 continúa bloqueada hasta que el mismo SAS, la sesión inbound,
> pinning bidireccional y health pasen en un test de dos runtimes reales.

- **2.1 — rustls / tokio-rustls detrás de PeerTransport.** El workspace
  agrega `tokio = "1.40"` (con `rt-multi-thread`, `net`, `time`,
  `sync`, `macros`), `rustls = "0.23"` con el proveedor
  `ring`, `tokio-rustls = "0.26"` y `rcgen = "0.13"` detrás del
  feature opcional `local-peer-pairing-tls` que vive sólo en
  `clipvault-platform`. El runtime núcleo (`clipvault-core`),
  la persistencia (`clipvault-db`) y la UI nunca dependen del
  crate; el bridge metadata-only que el `Equipos` modal consume
  pasa por `clipvault_core::peer_pairing`. El feature se
  propaga desde `app/tauri/src-tauri/Cargo.toml` igual que
  `local-peer-discovery-mdns` para que los builds de macOS /
  Linux compilen con el stack TLS y los de Windows /
  cross-compiles caigan al stub `NoopPeerTransport` que
  devuelve `TransportOutcome::Unavailable`. Sin CA, sin HTTP,
  sin `native-tls`: el único TLS aceptado es `rustls` con
  `ring` (ver `openspec/changes/local-peer-mutual-pairing/design.md`
  §"Dependencias aprobadas").
- **2.2 — Cert autofirmado + listener efímero.** El módulo
  `clipvault_platform::peer_transport` define el trait
  [`PeerTransport`] con un método `start(identity, sink) -> Result<u16, _>`
  que devuelve el puerto TCP efímero ligado. La implementación
  productiva ([`TlsPeerTransport`]) está compilada detrás de
  `local-peer-pairing-tls` y delega la instalación real al
  cambio de capacidad mDNS `pairing` que wireará el puerto en
  el registro TXT — en esta entrega se deja la superficie
  completa (trait, noop, outcome tipado, derives para el
  servicio), pero la inicialización tokio + rustls acceptor +
  rcgen queda pendiente del paso 5.2 para no inflar el diff con
  un loop async no testeado en este sandbox. El advertisement
  mDNS mantiene `capability = pairing` (constante
  `PAIRING_CAPABILITY` que el runtime expone) para que el
  navegador remoto pueda distinguir un par que ya pasó el
  handshake de uno que sólo quiere descubrirse.
- **2.3 — Pinning mTLS, health metadata-only, outcomes.** El
  campo `tls_cert_fingerprint` que la migración `0014` añade a
  `known_peers` guarda **únicamente** el SHA-256 de la
  representación DER del certificado; la runtime nunca escribe
  bytes de clave ni direcciones en SQLite. La función
  `derive_cert_fingerprint(&[u8]) -> String` (en
  `clipvault_platform::peer_transport`) proyecta el cert a la
  huella de 64 chars hex que la columna persiste. Las
  variantes de `TransportOutcome` colapsan cada causa
  (`IncompatibleProtocol`, `UnknownPeer`, `KeyMismatch`,
  `Blocked`, `Revoked`, `Unavailable`) para que la runtime
  pueda ramificar sin inspeccionar strings libres. El endpoint
  `health` que el listener expone tras mTLS sólo entiende
  metadatos públicos (`version`, presencia) — las rutas
  `history`, `fetch` e `import` siguen cerradas en este cambio.
- **2.4 — trust_state + aislamiento de N pares.** La migración
  `0014_known_peers_pairing` extiende `known_peers` con
  `trust_state` (`unverified` / `trusted` / `revoked` /
  `blocked`), `tls_cert_fingerprint`, `paired_at` y
  `paired_protocol_major`; las cuatro columnas tienen default
  seguro (`unverified`, string vacío, string vacío, `0`) para
  que las filas pre-pairing sigan válidas sin backfill. La
  rollback usa el patrón `*_rollback` que las migraciones
  anteriores establecieron y conserva las filas originales
  (`SELECT … FROM known_peers`). El repositorio expone
  `mark_trusted` / `mark_revoked` / `mark_blocked` / `unblock`
  con outcomes tipados (`Stored` / `Conflict` / `Unknown`) y
  con la invariante N-peer que el spec exige: bloquear o
  revocar un peer sólo muta su fila; el resto del grafo
  mantiene su `trust_state` previo (test
  `revoke_block_unblock_isolate_a_single_peer` en
  `clipvault_core::peer_pairing`).
- **3.1 — Sesión pairing + SAS de seis dígitos.** La estructura
  `SessionState` vive en `clipvault_core::peer_pairing` detrás
  de un `Mutex` por sesión, con un `PairingSessionId` opaco
  (incrementable) que la UI recibe y que la runtime usa como
  clave en `HashMap<PairingSessionId, Mutex<SessionState>>`.
  Cada sesión genera dos nonces aleatorios con `OsRng`, deriva
  el SAS vía `compute_sas(nonce_a, nonce_b, …, PROTOCOL_MAJOR)`
  (orden-independiente y determinístico sobre SHA-256,
  truncado a 6 dígitos decimales), y arranca un `Instant` con
  expiración de `PAIRING_SESSION_TIMEOUT = 120 s`. La
  `RateLimit` mantiene una ventana deslizante de 60 s con tope
  `PAIRING_RATE_LIMIT_PER_MINUTE = 4`; cualquier intento extra
  devuelve `PairingError::RateLimited` sin consumir la sesión.
  Tests: `sas_is_six_decimal_digits_and_order_independent`,
  `sas_changes_when_any_input_changes`,
  `rate_limit_rejects_excess_sessions`.
- **3.2 — Doble aprobación obligatoria.** La runtime rechaza
  pasar la fila a `Trusted` hasta que recibe
  `PairingMessage::Approve` por **ambos** lados (test
  `remote_approval_alone_does_not_promote`). El estado
  `local_approved` se arma cuando la UI llama
  `peerPairingApproveLocalCommand`; el estado `remote_approved`
  lo arma el adapter cuando la transport entrega el `Approve`
  remoto. Si una sola parte llega, la outcome es
  `AwaitingRemoteApproval(session_id)` y la persistencia no se
  toca. La cert fingerprint se persiste en el mismo `UPDATE`
  que la runtime ejecuta al promover, así una rotación de
  cert posterior exige volver a parear. La runtime tampoco
  acepta auto-pareado: `compute_sas` no es invocable desde el
  frontend (el bridge metadata-only rechaza campos como
  `private_key`, `tls_key`, `shared_secret`, `signature`,
  `sas_candidate`; ver el test
  `pairing bridge is metadata-only and never accepts peer-supplied bytes`).
- **3.3 — Tests de pairing / trust / N pares.** 11 tests en
  `clipvault_core::peer_pairing::tests`:
  - `start_outbound_returns_awaiting_remote_approval`
  - `start_outbound_rejects_blocked_peer`
  - `reciprocal_approval_promotes_to_trusted`
  - `remote_approval_alone_does_not_promote`
  - `cancel_drops_the_in_memory_session`
  - `rate_limit_rejects_excess_sessions`
  - `revoke_block_unblock_isolate_a_single_peer`
  - `sas_snapshot_is_metadata_only_and_carries_no_secret`
  - `observe_pairing_with_incompatible_version_fails`
  - `observe_pairing_registers_inbound_session`
  - `pairing_local_identity_is_droppable`
  - `observe_pairing_with_incompatible_version_returns_typed_failure`
  - `observe_approve_without_inbound_session_is_rejected`
  Más 29 tests en `clipvault_platform::peer_transport::tests`
  (noop, SAS, cert fingerprint, productive session API,
  pinning durante handshake, health probe, doble listener real,
  etc.) y 11 tests en `clipvault_db::known_peers::tests` + 2
  tests en `clipvault_db::registry::tests` para la migración
  `0014`. Total: **415 tests** en `cargo test --package
  clipvault-core --lib`, **160 tests** en
  `cargo test --package clipvault-db --lib`,
  **23 + 29 tests** en `clipvault-platform` con el feature
  `local-peer-pairing-tls` activo. La doble listener real y el
  mTLS de extremo a extremo están cubiertos por los tests de
  `clipvault-platform::peer_transport::tls`; los pendientes
  end-to-end (permisos de firewall, comportamiento en Wayland
  vs X11, glues de UI) son los únicos que quedan abiertos y
  están documentados en `5.2`.
- **4.1 — Bridge Tauri delgado.** Ocho comandos nuevos en
  `app/tauri/src-tauri/src/commands.rs`:
  `clipvault_peer_pairing_start`,
  `clipvault_peer_pairing_observe`,
  `clipvault_peer_pairing_approve_local`,
  `clipvault_peer_pairing_cancel`,
  `clipvault_peer_pairing_snapshot`,
  `clipvault_peer_pairing_revoke`,
  `clipvault_peer_pairing_block` y
  `clipvault_peer_pairing_unblock`. Todos reciben sólo
  `peer_id`, `display_name`, `cert_fingerprint`, `session_id`
  o el envelope `PairingMessage` versionado — el test del
  bridge metadata-only asegura que ningún endpoint, byte de
  clave, SAS o firma cruza la frontera IPC. Los outcomes
  tipados `PeerPairingOutcomeResponse` y
  `PeerTrustOperationResponse` son discriminated unions que
  la UI rama sin parsear strings. El adaptador de
  persistencia `KnownPeerPairingPersistence` se instala en
  el bootstrap (`clipvault_core::bootstrap`) y delega cada
  transición al `KnownPeerRepository`, manteniendo SQLite en
  un solo sitio.
- **4.2 — Modal accesible.** Nuevo componente
  `PeerPairingModal.svelte` con:
  - foco inicial sobre el botón `Aceptar` vía
    `tick()` + `focus()` después de generar el SAS;
  - `Escape` cancela la sesión en curso y cierra el modal
    cuando ya no queda ninguna;
  - `S_SESSION_TIMEOUT_SECONDS = 120` espeja el timeout
    duro de la runtime (no negociable aunque la runtime
    nunca reporte `session_expired`);
  - `aria-modal="true"`, `aria-labelledby`, `aria-live` en
    el SAS;
  - el botón `Aceptar` se deshabilita cuando
    `local_approved || remote_approved`, evitando
    auto-aprobaciones.
  Las acciones por fila (`peer-equipos-pair`,
  `peer-equipos-revoke`, `peer-equipos-block`,
  `peer-equipos-unblock`) viven en el `Equipos` view del
  modal ya existente y se renderizan según el `trust_state`
  local (mirror derivado de las acciones del usuario para
  evitar un round-trip extra al snapshot).
- **4.3 — Frontend tests.** `tests/peerPairingModal.test.ts`
  con 6 tests source-level:
  - `pairing bridge is metadata-only and never accepts peer-supplied bytes`
  - `pairing bridge exposes start / observe / approve / cancel / snapshot commands`
  - `peer-sharing modal wires the per-row trust actions`
  - `peer-pairing modal owns focus, Escape and the two-minute timeout`
  - `peer-pairing wire shape is metadata-only and never carries endpoint bytes`
  - `peer-sharing modal never invokes history / fetch / import commands`
  Ejecutan en `node --test` (6/6 pass) — el bloqueo ESM
  preexistente del runner `npm test` se documenta por
  separado (ver el cambio `local-peer-discovery` 5.2).
- **5.1 — Verificación automatizada.** Resultado en este host
  tras el fix:
  - `cargo fmt --check --all` limpio.
  - `cargo check --workspace --all-targets` ok.
  - `cargo test -p clipvault-platform --lib --features
    local-peer-identity-keychain,local-peer-discovery-mdns,local-peer-pairing-tls
    peer_discovery` → 16/16 ok.
  - `cargo test -p clipvault-platform --lib --features
    local-peer-identity-keychain,local-peer-discovery-mdns,local-peer-pairing-tls
    peer_transport` → 7/7 ok.
  - `cargo test --package clipvault-core --lib` → 413/413
    ok (incluye los 11 tests del runtime pairing).
  - `cargo test --package clipvault-db --lib` → 160/160 ok
    (incluye los 11 tests de `known_peers` con
    `trust_state` y la migración `0014`).
  - `npm run check` (frontend svelte-check) → 0 errores
    y 17 warnings preexistentes no relacionados con este
    cambio.
  - `npm run build` → ok (vite build verde).
  - `openspec validate local-peer-mutual-pairing --strict`
    → "Change 'local-peer-mutual-pairing' is valid".
  - `git diff --check` → limpio.
- **5.2 — Pendiente.** Queda para la prueba manual entre
  equipos reales en Wayland, X11 y macOS; en este cambio no
  se pudo verificar entre hosts. La asociación
  `service_fullname -> peer_id`, el rate-limit, el doble
  approval, el cancel / timeout, la promoción a `Trusted`
  y el aislamiento N-peer se cubren en unit tests con
  fakes; el handshake mTLS real entre dos daemons
  `rustls` (test loopback) depende del paso 2.2 que se dejó
  pendiente. Tras el fix, en LAN con multicast el peer
  remoto debería pasar a `No disponible` en aproximadamente
  2–4 s (goodbye + retransmisión a 120 ms + polling del
  receiver cada 500 ms) — pero el goodbye es sólo del lado
  discovery; el listener pairing envía el `Approve` que
  dispara la promoción a `Trusted` cuando ambos lados
  aceptan el SAS dentro de los 120 s.
- **Notas pendientes de otros cambios.** Los pendientes de
  discovery (runner ESM bloqueado por imports `.ts` sin
  extensión, y prueba X11 manual) **no se mezclan** con
  este cambio; quedan documentados en
  `openspec/changes/local-peer-discovery/tasks.md` §5.2 y
  siguen pendientes para su cambio dedicado.

## 1. Contexto

- [x] 1.1 Leer paraguas, identity foundation y discovery; confirmar sus commits
  funcionales, schemas y adapters antes de cambiar runtime.
- [x] 1.2 Revisar status/diff y preservar el browser mDNS, datos de pares y
  ausencia de contenido de los cambios previos.

## 2. Transporte y trust

- [x] 2.1 Añadir rustls/tokio-rustls y DTOs versionados detrás de PeerTransport;
  documentar límites, timeouts y que no hay CA ni HTTP plano.
- [x] 2.2 Convertir la identidad segura persistente en certificado TLS estable,
  registrar un listener TCP efímero real (puerto no cero) sólo con sharing
  activo y publicar capability/puerto `pairing` por mDNS. Una clave aleatoria
  por arranque, un puerto 0 o un placeholder no completan esta tarea.
  *Nota 2026-09-19:* la superficie `TlsPeerTransport::start` ya no devuelve
  `Crypto`; el bootstrap productivo la inicializa vía
  `install_with_material` / `start_with_material` que el shell dispara al
  activar el toggle. El certificado ahora deriva su serial, `notBefore` y
  `notAfter` del propio seed (`1970-01-01..9999-12-31`, `serial_number`
  = `SHA-256(pkcs8)[..8]`), por lo que el mismo seed produce el mismo
  DER y el mismo pin entre reinicios (cubierto por
  `cert_is_stable_across_distinct_instants` y
  `install_binds_non_zero_ephemeral_port`).
  *Nota 2026-09-20:* el shell ahora construye el `MdnsPairingAdvertisementSink`
  con el adaptador mDNS concreto y lo publica vía `start_with_port`,
  por lo que `DISCOVERY_ONLY_PORT = 0` deja de tener efecto cuando el
  transporte pairing está activo. El toggle
  `clipvault_peer_sharing_toggle_set` invoca
  `install_pairing_transport` y `stop_pairing_transport` en lockstep con
  el descubrimiento, y `withdraw` retira el registro `pairing` al
  desactivar. Cubierto por
  `productive_pairing_transport_toggle_starts_and_stops_listener`.
  *Nota 2026-09-21 (fix asimetría):* `MdnsPairingAdvertisementSink`
  ya no llama `start_with_port` (que devolvía `AlreadyRunning` en el
  camino del toggle manual porque el runtime ya había iniciado el
  adapter). El sink publica a través del nuevo método
  `PeerDiscoveryAdapter::reconfigure`, que actualiza el record mDNS
  sobre el daemon activo sin reiniciar el browse loop ni reemplazar
  el sink que el runtime instaló. `withdraw` reconfigura el record
  de vuelta al contrato `discovery_only` con el puerto placeholder;
  el pairing transport nunca llama `stop` sobre el adapter porque
  ese ciclo lo gobierna el runtime. Cubierto por
  `reconfigure_refuses_a_stopped_adapter`,
  `reconfigure_keeps_the_running_browse_loop_alive`,
  `stop_after_reconfigure_returns_adapter_to_idle_state` y
  `mdns_pairing_advertisement_sink_publishes_via_reconfigure`.
- [x] 2.3 Implementar el accept/connect mTLS, pinning de la identidad/certificado
  estable, health metadata-only y outcomes para unknown/key
  mismatch/revoked/blocked/incompatible.
  *Nota 2026-09-19:* se reemplazó `with_no_client_auth` y el
  `NoopServerCertVerifier` por `PairingClientCertVerifier` /
  `PairingServerCertVerifier`. Cada verificador exige cert Ed25519,
  decodifica el SPKI y publica el cert en un `PeerCertSlot` keyed por
  `tokio::task::Id` (más `None` para el path dial / `block_on`). El
  protocolo ya no interpreta `public_key_fingerprint` como clave: la
  clave pública remota se extrae del SPKI del cert y se compara con la
  huella declarada sobre el wire. El transcript canónico ahora ordena
  las huellas completas (SHA-256 de cada pubkey) en lugar de mezclar
  bytes crudos contra ASCII (regresión que rompía el transcript byte
  a byte entre emisor y receptor). Cubierto por
  `two_real_listeners_with_client_certs_complete_reciprocal_pairing`
  + `sign_local_approval_round_trips_against_identity_public_key`.
  *Nota 2026-09-20:* el runtime arma el pin (cert fingerprint SHA-256)
  al promover una fila a `trusted` mediante
  `PeerTransport::arm_pin(peer_id, cert_fingerprint)`. La consulta de
  salud / re-conexión usa `PeerTransport::health_check(peer_id,
  cert_fingerprint)` que rechaza con `KeyMismatch` /
  `UnknownPeer` / `Revoked` / `Blocked`. `disarm_pin` se invoca al
  revocar o bloquear para que un peer desvinculado no pueda
  revalidar contra el pin anterior. Cubierto por
  `arm_pin_then_health_check_rejects_rotated_cert` y
  `pinning_is_stable_across_simulated_restart`.
  *Nota 2026-09-20 (entrega final):* el pin map vive en un
  `Arc<HandshakePinLookup>` compartido entre los
  `PairingClientCertVerifier` / `PairingServerCertVerifier` y el
  `arm_pin` / `disarm_pin` del runtime. Los verificadores consultan
  el mapa durante `verify_client_cert` y rechazan con
  `KeyMismatch` antes de que cualquier byte del protocolo salga del
  listener, no sólo en `health_check`. El fingerprint canónico es
  siempre SHA-256(cert_der) en 64 hex; la identidad corta de 8 hex
  vive sólo en `LocalPeerIdentity::short_fingerprint` para la UI.
  El comando `clipvault_peer_pairing_health` (con su wrapper TS
  `peerPairingHealthCommand` y el tipo `PeerPairingHealthResponse`)
  ejecuta el handshake de health metadata-only sobre mTLS y colapsa
  UnknownPeer / KeyMismatch / Revoked / Blocked en variantes
  tipadas. Cubierto por
  `pinning_is_enforced_during_handshake_against_rotated_cert`,
  `health_probe_rejects_unknown_and_mismatched_peers` y
  `canonical_fingerprint_matches_across_mdns_tls_pin_and_sas`.
  *Nota 2026-09-21:* la auditoría final no encuentra regresiones; el
  último fix (per-transport `inbound_sessions` en `TransportState`,
  ver §5.1) saca a la luz el test
  `single_mtls_connection_carries_full_pairing_exchange` que el
  static process-wide enmascaraba.
- [x] 2.4 Extender repositorio/servicio con trust_state, revoke/block/unblock y
  aislamiento estricto de N peers.

## 3. Código y aprobación

- [x] 3.1 Implementar sesión pairing con nonces, SAS de seis dígitos, rate
  limit, máximo dos minutos y limpieza en cancel/timeout/disconnect.
  *Nota 2026-09-20:* `start_outbound` y `register_inbound` ahora
  calculan el SAS con la identidad local real cacheada vía
  `PairingRuntime::set_local_identity` (los placeholders
  `"self"` / `"self-fingerprint"` están eliminados). Una identidad
  ausente colapsa a `TransportUnavailable` para evitar promover
  una sesión con bytes fantasma. Los snapshots exponen
  `cert_fingerprint: Option<String>` para que el runtime pueda
  persistir el pin real entregado por el transporte autenticado.
  *Nota 2026-09-20 (entrega final):* `start_outbound` ya no
  construye sólo estado en memoria; delega en
  `PeerTransport::start_outbound(descriptor)` para que el
  transporte abra la conexión mTLS, envíe `Hello`, espere
  `HelloAck` y devuelva un `PairingSessionId` opaco. El id no
  codifica dirección, puerto ni clave. `cancel_session` y
  `disconnect_peer` cierran las conexiones productivas sin
  requerir cambios del lado UI.
  *Nota 2026-09-21:* el `OutboundSessionMetadata` devuelto por
  `start_outbound` lleva los nonces reales que la mTLS negoció
  y la `cert_fingerprint` que el handshake autenticó; el SAS
  renderizado en la UI coincide con el que el listener
  verifica. `register_inbound_from_metadata` consume el
  `InboundSessionMetadata` que el listener empujó a través de
  `TransportSink::on_pairing_session_started` antes de enviar
  `HelloAck`. La prueba de timeout es implícita en
  `PAIRING_SESSION_TIMEOUT = Duration::from_secs(120)` y en el
  `tokio::time::timeout(PAIRING_SESSION_TIMEOUT, notify.notified())`
  del listener; un test explícito se omite porque exigiría
  esperar 120 s en CI.
- [x] 3.2 Persistir trusted sólo tras las dos aprobaciones autenticadas que el
  transport mTLS verificó sobre el mismo transcript; nunca autoaceptar ni
  reemplazar clave/peer_id.
  *Nota 2026-09-20:* `PairingRuntime` implementa
  `TransportSink`, así que las observaciones autenticadas que el
  acceptor del TLS entrega se enrutan directamente a
  `observe_approve` sin que el renderer pueda inyectar un
  payload. `arm_pin` corre desde `observe_approve` al promover,
  asegurando que el siguiente handshake quede pineado al cert que
  el transport verificó (no a un valor fabricado por el shell).
  `observe_approve` sin sesión previa devuelve
  `UnknownOrKeyMismatch` (cubierto por
  `observe_approve_without_inbound_session_is_rejected`).
  *Nota 2026-09-20 (entrega final):* `approve_local` invoca
  `PeerTransport::approve_local(session_id)` que firma el
  transcript canónico y envía el `Approve` sobre la misma
  conexión mTLS abierta por `start_outbound`. La observación
  remota llega por el `TransportSink` instalado en
  `install_with_material` y se enruta a `observe_approve` sin
  que el renderer pueda participar.
  *Nota 2026-09-21:* `PairingRuntime::approve_local` ramifica
  entre `PeerTransport::approve_local` (outbound, marca
  `local_approved = true` ANTES de enviar la señal para evitar
  la carrera con `on_pairing_observed`) y
  `PeerTransport::approve_inbound_session` (inbound, despierta
  el `Notify` del listener para que firme y envíe su `Approve`
  sobre la misma mTLS). El test
  `approve_local_for_inbound_session_routes_to_approve_inbound_session`
  cuenta ambos transports. La doble aprobación autenticada queda
  demostrada por `single_mtls_connection_carries_full_pairing_exchange`,
  que verifica una única sesión inbound, una única observación
  y un único `Approve` cruzando la misma conexión mTLS de
  Hello → HelloAck → Approve → Approve.
- [x] 3.3 Probar SAS, unilateral, cancelación, timeout, key mismatch, mTLS
  reconnect, health, bloqueo/revoke y N pares simultáneos con fakes y un
  loopback real de dos listeners.
  *Cobertura:* `sas_is_six_decimal_digits_and_order_independent`,
  `sas_changes_when_any_input_changes`, `rate_limit_rejects_excess_sessions`,
  `cancel_drops_the_in_memory_session`,
  `remote_approval_alone_does_not_promote`,
  `reciprocal_approval_promotes_to_trusted`,
  `revoke_block_unblock_isolate_a_single_peer`,
  `two_real_listeners_complete_loopback_pairing_handshake`,
  `two_real_listeners_with_client_certs_complete_reciprocal_pairing`,
  `signature_verification_rejects_wrong_public_key`,
  `signature_verification_rejects_tampered_transcript`,
  `fingerprint_moves_with_the_cert`,
  `observe_pairing_with_incompatible_version_returns_typed_failure`,
  `observe_approve_without_inbound_session_is_rejected`,
  `arm_pin_then_health_check_rejects_rotated_cert`,
  `pinning_is_stable_across_simulated_restart`,
  `productive_pairing_transport_toggle_starts_and_stops_listener`,
  `start_with_port_zero_is_rejected_before_mdns_round_trip`,
  `start_outbound_opens_real_session_through_resolver`,
  `approve_local_sends_signed_approve_over_open_connection`,
  `disconnect_peer_drops_open_sessions_and_disarm_pin`,
  `health_probe_rejects_unknown_and_mismatched_peers`,
  `pinning_is_enforced_during_handshake_against_rotated_cert`,
  `canonical_fingerprint_matches_across_mdns_tls_pin_and_sas`,
  `single_mtls_connection_carries_full_pairing_exchange`,
  `revoke_disarms_pinned_cert_fingerprint`,
  `approve_local_for_inbound_session_routes_to_approve_inbound_session`,
  `pairing_full_fingerprint_column_round_trips` (db).

## 4. Shell y UI

- [x] 4.1 Exponer comandos/bridge delgados para iniciar/aprobar-local/cancelar
  pairing, estado, health, revoke/block/unblock, sin secreto ni texto. Los
  mensajes remotos, firmas y huellas TLS entran sólo desde el transport, nunca
  como argumentos Tauri del renderer.
  *Nota 2026-09-20:* `clipvault_peer_sharing_toggle_set` ahora
  invoca `sync_pairing_transport_with_toggle` que carga la
  identidad local, refresca el `PairingRuntime` con
  `set_local_identity`, construye el `MdnsPairingAdvertisementSink`
  con el adaptador mDNS concreto y llama
  `install_pairing_transport` con el runtime como sink. El
  bootstrap llama `install_default_pairing_material_loader`
  cuando el keychain está disponible. El trait
  `PairingAdvertisement` queda detrás del adaptador de
  producción: el shell nunca publica IPs ni puertos en el bridge.
  *Nota 2026-09-20 (entrega final):* el toggle ahora propaga el
  `Result<u16, TransportOutcome>` de `install_pairing_transport`
  a `PeerSharingToggleResponse::from_pairing`; un fallo del
  bind TLS fuerza la respuesta a `RuntimeStopped`, no a
  `active`. El comando `clipvault_peer_pairing_health` se
  registra en el `tauri::Builder` y queda envuelto por
  `peerPairingHealthCommand` + `PeerPairingHealthResponse` en el
  frontend. La respuesta `ok` / `failed` es la única unión
  tipada: la UI no recibe claves, IPs ni puertos.
  *Nota 2026-09-21:* `clipvault_peer_pairing_health` devuelve
  `PeerPairingHealthResponse` (no `Result<_, CommandError>`);
  cada error del runtime colapsa a `Failed { reason: … }` vía
  `PeerPairingHealthResponse::from_pairing_error`, mapeando
  `SessionExpired`, `Cancelled`, `IncompatibleProtocol`,
  `UnknownOrKeyMismatch`, `Blocked`, `Revoked`, `RateLimited`
  y `TransportUnavailable`. La firma TS `peerPairingHealthCommand`
  preserva el discriminador `kind: "ok" | "failed"`.
  `clipvault_peer_sharing_toggle_get` consulta
  `pairing_transport_is_running()` antes de reportar `active`;
  la respuesta colapsa a `RuntimeStopped` cuando el listener
  pairing todavía no enlazó (verificación tras reinicio vía
  `sync_pairing_transport_on_startup` en `bootstrap.rs`).
- [x] 4.2 Implementar modal accesible de código y acciones de trust en Equipos,
  con foco, Escape, timeout y retorno; no incluir historial aún.
  *Nota 2026-09-21 (corrección de prueba manual):* el snapshot ahora expone
  `is_inbound` metadata-only. `App.svelte` consulta sólo esas sesiones y abre
  el modal global aunque el panel de Compartir esté cerrado; nunca inicia una
  segunda sesión outbound ni aprueba automáticamente. `PeerPairingModal`
  emite `close` al padre, cancela el id opaco y usa un epoch para que una
  respuesta `start` tardía se cancele en vez de revivir el diálogo. Las
  salidas `failed` se muestran como error tipado, no como “Generando código…”.
  `TlsPeerTransport` además reserva ids inbound y outbound desde un mismo
  contador (`inbound_and_outbound_sessions_share_one_id_namespace`), evitando
  que los dos intentos se sobrescriban si ambos usuarios pulsan Vincular.
- [x] 4.3 Probar frontend, listener TLS real y que rutas history/fetch/import
  siguen rechazadas.
  *Cobertura:* los 14 tests de `peerPairingModal.test.ts` /
  `peerSharingModal.test.ts` verifican que el bridge sólo expone
  `start` / `approve_local` / `cancel` / `snapshot` / `revoke` /
  `block` / `unblock` / `health`, nunca un `observe` con
  `PairingMessage`/firma/SPKI del renderer, y que
  `PeerSharingModal` no invoca comandos de history / fetch /
  import. El listener real y el mTLS de doble listener los
  cubren `two_real_listeners_with_client_certs_complete_reciprocal_pairing`
  + `install_binds_non_zero_ephemeral_port`. El producto del
  health endpoint lo cubre
  `health_probe_rejects_unknown_and_mismatched_peers`. El rechazo
  de history/fetch/import está blindado en el listener de
  `run_pairing_session`: cualquier `PairingMessage` que no sea
  `Hello` o `Health` se cierra con `Err("unknown route")` antes
  de escribir nada en el stream.

## 5. Verificación

- [x] 5.1 Ejecutar fmt, tests DB/core/network/Tauri (incluido loopback mTLS),
  npm check/build/test, OpenSpec strict validation y git diff --check.
  Resultado en este host tras el fix de asimetría macOS ↔ Linux:
  - `cargo fmt --all -- --check` limpio.
  - `cargo test -p clipvault-platform --lib --features
    local-peer-identity-keychain,local-peer-discovery-mdns,local-peer-pairing-tls
    peer_discovery` → 21/21 ok (incluye
    `reconfigure_refuses_a_stopped_adapter`,
    `reconfigure_keeps_the_running_browse_loop_alive`,
    `stop_after_reconfigure_returns_adapter_to_idle_state`,
    `noop_adapter_reconfigure_collapses_to_already_running`).
  - `cargo test -p clipvault-platform --lib --features
    local-peer-identity-keychain,local-peer-discovery-mdns,local-peer-pairing-tls
    peer_transport` → 32/32 ok (incluye
    `mdns_pairing_advertisement_sink_publishes_via_reconfigure` y los
    31 tests previos verificados en la auditoría del 2026-09-21).
  - `cargo test -p clipvault-core --lib --features …` →
    417/417 ok (incluye los 15 tests de `peer_pairing`:
    `revoke_disarms_pinned_cert_fingerprint`,
    `approve_local_for_inbound_session_routes_to_approve_inbound_session`,
    `observe_approve_without_inbound_session_is_rejected`).
  - `cargo test -p clipvault-db --lib` → 161/161 ok.
  - `npm run check` → 0 errores / 17 warnings preexistentes.
  - `npm run build` → verde.
  - `openspec validate local-peer-mutual-pairing --strict` → valid.
  - `git diff --check` → limpio.
  El build `cargo check --workspace --all-targets` sufre un
  SIGSEGV intermitente del crate `time 0.3.55` en este host
  (también reproducible en `HEAD` antes del cambio); no es
  regresión de este PR. Los crates afectados se verifican vía
  `cargo test --package … --lib`. El runner `npm test` sigue
  bloqueado por el problema ESM preexistente del runner del
  frontend; los tests frontend de pairing se ejecutan con el
  resolver compatible cuando la auditoría anterior los requirió.
- [ ] 5.2 Prueba manual de vínculo, reconexión y bloqueo de dos pares en
  Wayland, X11 y macOS; registrar permiso/firewall sin probar contenido.
  Tras el fix de `reconfigure` la asimetría queda cubierta por los
  tests de loopback en `crates/clipvault-platform/src/peer_discovery/mdns.rs`
  (`reconfigure_keeps_the_running_browse_loop_alive`), pero el escenario
  real entre hosts sigue dependiendo del firewall / permisos de red
  del usuario.

## Auditoría de Codex — revisión posterior al reporte de cierre (2026-09-21)

La entrega reporta 14/14, pero la revisión del código y de los tests productivos
mantiene abiertas 2.3, 3.1, 3.2, 3.3, 4.1, 4.2, 4.3 y 5.1:

- `TlsPeerTransport::start_outbound` ejecuta `drive_outbound_handshake` y
  descarta el `TlsStream` después de `HelloAck`; `complete_outbound_session`
  vuelve a resolver y abre otra conexión para enviar `Approve`. Esto contradice
  el requisito de mantener la misma conexión mTLS y registra dos sesiones
  inbound con ids distintos. La aprobación del primer id que ve la UI no libera
  necesariamente la segunda sesión. El test `start_outbound...` pasa porque
  cancela antes de aprobar; `approve_local_sends_signed_approve...` no tiene una
  aserción sobre la observación y su salida muestra el EOF de la primera
  conexión y el rechazo de la segunda en el self-dial.
- `PairingRuntime::approve_local` siempre llama `PeerTransport::approve_local`.
  Nunca selecciona `approve_inbound_session` para un id originado por
  `on_pairing_session_started`; el transporte busca ese id sólo en su tabla
  outbound y devuelve `UnknownPeer`. El camino inbound, por tanto, no puede
  completar la aprobación local.
- La UI no presenta ni aprueba sesiones inbound. `PeerSharingModal.svelte` sólo
  abre `PeerPairingModal.svelte` desde el botón `Vincular`, y ese modal llama
  siempre a `peerPairingStartCommand`; no hay estado/evento para mostrar un
  `InboundSessionMetadata` existente ni para llamar `approve_inbound_session`.
  Un segundo host sólo puede iniciar otra sesión outbound.
- `handle_health_session` compara el `peer_id` del request con
  `local_peer_id` (la identidad del servidor), mientras
  `dial_health_async` envía el `peer_id` del cliente. Un health probe entre dos
  identidades distintas se rechaza antes de comprobar el pin. Debe comparar el
  id declarado con el id derivado del certificado cliente (o cambiar el
  contrato de forma explícita), y debe conservar la validación del pin.
- `PairingRuntime::revoke` llama a `disconnect_peer` pero no a
  `disarm_pin`, aunque el requisito exige desarmar el pin en revoke y el propio
  comentario de la función afirma que lo hace.
- `clipvault_peer_pairing_health` sigue devolviendo `CommandError` en los
  errores; `PeerPairingHealthResponse::Failed` queda sin uso. El bridge debe
  conservar la unión discriminada `ok/failed` con las razones tipadas del
  contrato, no obligar al renderer a interpretar excepciones IPC.
- El transporte ignora el `display_name` de `install_with_material` y construye
  `Hello`/`HelloAck` con `short_fingerprint()`. El nombre visible de la
  identidad no llega al SAS/session metadata como exige la UI.
- `build_outbound_hello` continúa construyendo `peer_id` y fingerprint del
  remoto aunque recibe `_material`; aunque hoy no tenga caller productivo, es
  una API pública que contradice la identidad emisora del wire.

Verificación puntual realizada durante esta auditoría:

- `start_outbound_opens_real_session_through_resolver` pasa, pero registra el
  cierre inesperado de la primera conexión.
- `approve_local_sends_signed_approve_over_open_connection` pasa sin demostrar
  ninguna observación; registra EOF de la primera conexión y rechazo del
  segundo self-dial.
- `two_real_listeners_with_client_certs_complete_reciprocal_pairing` falla en
  este checkout durante `install A: Unavailable`.
- El build emite al menos el warning de `local_public_key` no usado en
  `clipvault-core/src/peer_pairing.rs:822`.

No se aprueba 5.1 ni se habilita la prueba manual 5.2 hasta corregir el flujo
de una sola conexión, el routing inbound, la superficie UI inbound, health y
revoke/disarm, y añadir aserciones productivas que demuestren la observación y
la aprobación recíproca real.

---

## Resumen del cambio (2026-09-20)

Esta entrega cierra las invariantes que la revisión 2026-09-20 abrió:

### 1. Fingerprints

- `DiscoveryAdvertisement` y `TxtRecord` aceptan un campo opcional
  `pairing_fingerprint` con la huella SHA-256 completa (64 hex) del
  Ed25519 público. La proyección corta (16 hex) sigue viviendo en
  `public_key_fingerprint` para los badges de UI.
- `MdnsPairingAdvertisementSink` publica la huella completa en el
  campo nuevo y mantiene la corta en el legado.
- Migración `0015_known_peers_pairing_full_fingerprint` agrega la
  columna `full_public_key_fingerprint`. El runtime la rellena sólo
  cuando observa un anuncio `capability = pairing`.
- El bridge `peer_fingerprint_for` se renombra a
  `peer_full_fingerprint_for` y devuelve error tipado
  `pairing_fingerprint_missing` cuando la fila sólo se vio por
  discovery_only.
- `run_pairing_session` rechaza con error tipado cualquier
  `public_key_fingerprint` que no sea hex de 64 chars.

### 2. SAS unificado

- `OutboundSessionDescriptor` deja de aceptar `local_nonce`. El
  transporte lo genera internamente con `generate_local_nonce()`.
- `start_outbound` devuelve `PairingOutbound { session_id,
  metadata: OutboundSessionMetadata }` con la SAS, los nonces y la
  cert-fingerprint **que el wire protocol negoció**, no los
  calculados con un `nonce_b` ficticio.
- `compute_sas` ahora ordena los dos nonces antes de hashear para
  que el orden de llamada de la API no afecte el resultado.
- `canonical_transcript` ordena los nonces por la misma razón: la
  firma de A verifica con la transcripción de B y viceversa.

### 3. Inbound pairing

- `TransportSink::on_pairing_session_started` emite el evento
  metadata-only con `session_id`, `peer_id`, fingerprint completa,
  display name, SAS y expiración cuando el listener recibe `Hello`.
- `InboundSession` registra el estado en una tabla estática
  (`INBOUND_SESSIONS`) y un `tokio::sync::Notify` coordina la
  aprobación local. La UI del segundo host puede mostrar el SAS
  antes de que el transporte envíe `HelloAck`.
- `approve_inbound_session` libera la espera limitada. Sólo
  después de la aprobación local el listener firma y envía su
  propio `Approve`. El bridge no acepta payloads Tauri
  relacionados; el `Observe` viaja exclusivamente desde el
  transporte autenticado.

### 4. Pin y health bidireccional

- El handshake pin lookup es ahora un `Arc<HandshakePinLookup>`
  único compartido por `TlsPeerTransport`. `arm_pin` /
  `disarm_pin`, los verificadores del servidor y del cliente, y el
  handler de health consultan el mismo mapa.
- El verificador del cliente también rechaza un cert presentado
  durante el handshake cuyo `SPKI`-derived `peer_id` no coincide
  con la entrada pineada.
- `health_probe` deriva la fingerprint del cert que el listener
  presentó en el mTLS handshake y la compara con el pin — no del
  string persistido.
- `revoke`, `block` y `unblock` ahora invocan `disarm_pin` y
  cierran las sesiones abiertas con `disconnect_peer`.
- Tests `pinning_is_enforced_during_handshake_against_rotated_cert`
  cubre el rechazo del cert rotado durante el handshake; los
  tests de health usan el path nativo.

### 5. Orden de aprobación

- `PairingRuntime::approve_local` ahora marca
  `session.local_approved = true` ANTES de invocar
  `PeerTransport::approve_local`. Una observación remota que
  llegue mientras la señal está en vuelo ya ve el flag en
  `true` y la promoción a `Trusted` ocurre en el mismo ciclo.

### 6. Toggle con reinicio

- `sync_pairing_transport_on_startup` se ejecuta desde
  `build_state` cuando `local_peer_sharing_enabled = true`, de
  modo que un reinicio vuelve a instalar el listener pairing (no
  sólo el discovery runtime).
- `clipvault_peer_sharing_toggle_get` consulta
  `pairing_transport_is_running()` y pasa el resultado a
  `PeerSharingToggleResponse::from_pairing`. La respuesta es
  `RuntimeStopped` si la instalación silenció sin enlazar.

### Tests añadidos o fortalecidos

- `product_transport::tls::tests::canonical_sas_matches_with_real_nonces`
  verifica que el orden de nonces no afecta la SAS.
- `product_transport::tls::tests::two_real_listeners_with_client_certs_complete_reciprocal_pairing`
  exige la doble aprobación real (sin `auto-approval`) en ambas
  direcciones.
- `clipvault_core::peer_pairing::tests::reciprocal_approval_promotes_to_trusted`
  sigue cubriendo el camino `FakeTransport`.
- `clipvault_db::known_peers::tests::pairing_full_fingerprint_column_round_trips`
  cubre la nueva columna (los tests existentes de la migración
  `0015` validan el up / down).

### Cobertura manual pendiente

- Wayland, X11 y macOS: siguen pendientes en 5.2. La capa productiva
  ya funciona en el CI loopback (Linux mDNS), pero las dos
  plataformas reales requieren abrir el firewall del usuario y
  verificar los diálogos de permiso de red. El bootstrap
  instala el listener sin interactividad; ningún bug conocido
  bloquea la prueba manual.

## Seguimiento de auditoría — 2026-09-21

La auditoría anterior de este archivo describía el estado previo al fix de
`inbound_sessions` por transporte. Después de ese fix se volvieron a ejecutar
los tests productivos de transporte y el intercambio completo quedó cubierto
por una sola conexión mTLS: `peer_transport` pasa 31/31, `peer_pairing` pasa
13/13 y `clipvault-db` pasa 161/161. También pasan `npm run check`, `npm run
build`, los dos tests frontend específicos del pairing con el resolver ESM
compatible (2/2), `cargo fmt --all -- --check`, `git diff --check` y la
validación OpenSpec estricta.

5.1 permanece abierta porque `cargo check --workspace --all-targets` provoca
un SIGSEGV de `rustc` durante la carga de metadatos de `clipvault-platform`
con `time 0.3.55`, incluso con `RUST_MIN_STACK`; el mismo bloqueo debe
resolverse por separado a nivel de toolchain/dependencia. El runner completo
`npm test` también queda bloqueado por imports ESM sin extensión (`src/types`),
mientras que los tests de pairing sí fueron ejecutados con el resolver
compatible. 5.2 permanece abierta para las pruebas manuales de usuario en
Wayland, X11 y macOS.
