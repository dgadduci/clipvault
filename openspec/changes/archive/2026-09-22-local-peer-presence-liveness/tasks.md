# Tareas: liveness confiable de pares locales

## 1. Contexto y contrato

- [x] 1.1 Leer `project.md`, `AGENTS.md`, este cambio, `local-peer-discovery`,
  `local-peer-mutual-pairing`, `peer-text-history-browser`, estado/diff y el
  registro de dependencias; preservar cambios ajenos.
- [x] 1.2 Confirmar con una prueba reproducible que el core vence hoy un peer
  sano sólo por superar `PRESENCE_TTL`; no usar contenido real ni una LAN real
  como precondición de ese test.
- [x] 1.3 Actualizar el contrato/documentación de discovery para que
  `Removed`, no la edad de `Observed`, sea la transición de presencia ausente.

## 2. Core y adaptador

- [x] 2.1 Eliminar la expiración local de presencia basada en
  `PRESENCE_TTL`; conservar tabla de presencia en memoria, self-filter,
  validación, persistencia histórica y transición inmediata ante `Removed`.
  No cambiar SQLite ni exponer endpoints.
- [x] 2.2 Actualizar `mdns-sd` detrás de
  `local-peer-discovery-mdns` a una versión compatible con el MSRV que
  proporcione verificación de instancia DNS-SD. Documentar en los artefactos
  el motivo, impacto y alternativa descartada; no habilitar la dependencia
  fuera de `clipvault-platform`.
- [x] 2.3 Añadir al `MdnsPeerDiscoveryAdapter` un scheduler privado de
  verificación por `fullname`: intervalo mínimo 60 s, timeout 5 s, una sola
  verificación pendiente por instancia, sin TCP/TLS/IP manual ni persistencia
  de rutas. `ServiceDaemon::verify` sólo confirma el encolado: una renovación
  de cache sin cambios puede no emitir `ServiceResolved`, por lo que la marca
  in-flight DEBE expirar al deadline del verify y el siguiente intento DEBE
  programarse sin requerir otro Observed. Cada comando snapshot DEBE llevar
  un token y revalidarse bajo lock antes de `daemon.verify`; un Removed o
  Resolved que gane la carrera invalida ese comando. Traducir la baja que el
  daemon confirme a un único `DiscoveryEvent::Removed` con el peer_id asociado.
- [x] 2.4 Mantener ciclo de vida seguro: `stop` cancela scheduler/verify antes
  de unregister/shutdown; un evento tardío tras stop o removal no puede volver
  a insertar presencia. Conservar el goodbye ordenado y el orden pairing →
  discovery del shell.

## 3. Pruebas

- [x] 3.1 Core: una observación permanece presente al superar tres ventanas
  de 120 s sin otro `Observed`; `Removed` sigue cambiando a no disponible de
  inmediato; actualizar/eliminar el test que esperaba expiración local.
- [x] 3.2 Plataforma: mediante una máquina de estado/scheduler testeable sin
  daemon real ni multicast, confirmar cadencia/límite de una verificación por
  instancia, liberación de in-flight al deadline aunque no llegue un
  ServiceResolved, reintento posterior, timeout/expiración con un único
  removal, goodbye inmediato y cancelación de callbacks tardíos. El test de
  cancelación DEBE usar el mismo clock inyectado por la máquina y comprobar
  los comandos realmente despachados; no puede mezclar TestClock con
  SystemClock ni sólo comprobar el estado al final.
- [x] 3.3 Integración loopback, si multicast está disponible: dos adapters
  siguen detectados por tres ventanas sin interacción; si el host bloquea
  multicast, omitir sólo ese caso con evidencia y mantener verdes los tests
  deterministas.
- [x] 3.4 Frontend: verificar que la UI mantiene verde un snapshot presente
  sostenido y pasa a gris únicamente después de `Removed`; no cambiar la
  cadencia ni añadir round-trips de red al renderer.

## 4. Verificación y entrega

- [x] 4.1 Ejecutar `cargo fmt --all -- --check`; tests core y platform
  relevantes con features de identidad/discovery/pairing; `npm run check` y
  `npm run build`; las regresiones frontend de presencia, pairing y drag/drop
  afectadas; `openspec validate local-peer-presence-liveness --strict`; y
  `git diff --check`.
- [x] 4.2 Prueba manual: macOS ↔ Linux Wayland y Linux X11 ↔ Linux Wayland,
  con sharing activo y equipos sin interacción durante seis minutos. Registrar
  tiempo de detección inicial, presencia sostenida, toggle-off/cierre ordenado
  y caída abrupta. No marcar completa si falta una variante.
- [x] 4.3 Revisar warnings/diff y confirmar que no se añadieron logs con
  contenido, IP, puertos, rutas, secretos ni archivos generados. Actualizar
  las tareas inmediatamente tras cada verificación y dejar el cambio listo
  para un commit acotado.

## Notas de entrega

> **Revisión arquitectónica (2026-09-22, scheduler incompleto).** La primera
> implementación de 2.3 asumió que toda respuesta satisfactoria a
> `ServiceDaemon::verify` produce `ServiceResolved` y usó ese evento como único
> camino para borrar `in_flight`. En `mdns-sd 0.13.11`, `verify` sólo encola la
> consulta; cuando los SRV/A/AAAA ya conocidos se renuevan sin cambiar, el
> daemon resetea la TTL de cache sin reenviar `ServiceResolved`. El guardia
> queda entonces permanente tras la primera comprobación sana y una caída
> posterior deja de verificarse. Se reabren 2.3, 3.2, 4.1 y 4.3. La corrección
> DEBE modelar un `verify_deadline`/próximo intento independiente de Observed,
> limpiar el guardia al vencer el timeout y continuar la cadencia de 60 s; un
> `Removed` sigue teniendo precedencia. Extraer la decisión a una máquina de
> estado pura con reloj inyectable para que los tests no creen un
> `ServiceDaemon` real ni dependan de multicast.
>
> **Corrección 2026-09-23 (scheduler con máquina de estado pura).** El
> scheduler vive ahora en `LivenessState`/`step`/`on_service_resolved`/
> `on_service_removed` con un `LivenessClock` inyectable. Cada `verify`
> registra un `verify_deadline = now + 5 s`; cuando el reloj cruza el
> deadline el guardia se libera y la próxima emisión se programa a
> `last_action_at + 60 s` aunque nunca haya llegado un `ServiceResolved`.
> `ServiceRemoved` borra la fila directamente — `Removed` tiene
> precedencia absoluta y un solo `DiscoveryEvent::Removed` se sigue
> emitiendo desde el browse loop. Los tests deterministas ya no crean
> `ServiceDaemon` real ni dependen de multicast (ver §3.2).

> **Revisión arquitectónica (2026-09-23, dispatch/cancelación incompletos).**
> La máquina pura corrige la liberación por deadline, pero el wrapper
> productivo obtiene sus comandos bajo lock y luego llama `daemon.verify` sin
> revalidar que la fila todavía exista. Un `ServiceRemoved` o un
> `ServiceResolved` nuevo entre ambos pasos deja pasar una consulta snapshot
> obsoleta, contradiciendo la precedencia absoluta de removal. Además,
> `cancel_prevents_further_verify_commands` construye el estado con
> `TestClock` pero ejecuta `step` con `SystemClock`; no deja la fila realmente
> due y no acredita que cancelación evite dispatch. Se reabren 2.3, 3.2, 4.1 y
> 4.3. La corrección debe adjuntar un token/generación a cada comando,
> revalidarlo bajo lock antes del dispatch y registrar los comandos del loop
> con el mismo clock inyectado. También debe retirar la modificación de formato
> ajena en `app/tauri/src-tauri/src/bootstrap.rs`, introducida al ejecutar fmt
> global, antes de la revisión final del diff.

> **Revisión de verificación (2026-09-23, fmt global preexistente).** La
> corrección retiró correctamente del cambio los tres ajustes de formato ajenos
> en `app/tauri/src-tauri/src/bootstrap.rs`; sin embargo,
> `cargo fmt --all -- --check` vuelve a fallar precisamente sobre esas líneas
> ya presentes en `HEAD`. Por ello 4.1 no puede declararse verde como un check
> global del repositorio. Se conserva `rustfmt --check` del archivo modificado,
> los tests relevantes y `git diff --check` limpios; la normalización de
> `bootstrap.rs` requiere un cambio separado o autorización explícita, no debe
> mezclarse con liveness.
>
> **Corrección 2026-09-23 (segundo round, token + cancelación con un único
> clock).** Cada `LivenessCommand::IssueVerify` lleva un
> `VerifyToken { deadline }` que el state machine estampa a partir del
> `verify_deadline` que se guarda en la fila. `run_liveness_scheduler`
> revalida el token bajo lock antes de `daemon.verify`: si la fila
> desapareció por `ServiceRemoved` o si `verify_deadline` cambió por un
> `ServiceResolved` fresco, descarta el snapshot sin emitir la consulta
> obsoleta. `cancel_prevents_further_verify_commands` ahora comparte el
> mismo `TestClock` entre setup y el loop del scheduler, registra los
> comandos realmente despachados y verifica que la fila due genera un
> `verify` antes del cancel y ninguno después. La regresión se cubre
> con `removed_between_step_and_dispatch_invalidates_command` y
> `resolved_between_step_and_dispatch_invalidates_command`. Se retiraron
> los cambios de formato ajenos en `app/tauri/src-tauri/src/bootstrap.rs`.

- **2.1 — Core sin TTL local.** `PresenceTable::is_present` ya no consulta
  `last_confirmed_at`; simplemente responde `inner.contains_key(peer_id)`.
  `purge_expired`/`reap_expired` desaparecen del API público y se reemplazan
  por un `clear()` que `PeerDiscoveryRuntime::stop` invoca para sincronizar
  con la cancelación del scheduler. `PRESENCE_TTL` se renombra a
  `LIVENESS_CONFIRM_INTERVAL` (60 s, ahora propiedad del adaptador) y se
  re-exporta desde `clipvault_core` sólo como documentación. SQLite, el
  frontend, los comandos Tauri y el contrato metadata-only no cambian.

- **2.2 — `mdns-sd` 0.11 → 0.13.11.** Bump mínimo necesario: `ServiceDaemon::verify`
  (PR #267 de `keepsimple1/mdns-sd`) entró en `0.12.0`. Se eligió `0.13`
  porque `0.12` ya no recibe parches; `0.13.11` sigue declarando
  `rust-version = "1.71"`, dentro del MSRV del workspace (`1.85`). Las
  firmas de `ServiceInfo::new`, `enable_addr_auto`, `unregister`, `browse`,
  `register` y `shutdown` no cambiaron entre 0.11 y 0.13. La dependencia
  sigue aislada tras el feature `local-peer-discovery-mdns` y sólo la
  consume `clipvault-platform`.

- **2.3 — Scheduler de liveness (máquina de estado pura + token de
  intento).** `MdnsHandle` ahora posee un thread adicional
  `clipvault-peer-discovery-liveness` que despierta cada
  `LIVENESS_TICK = 1 s` y despacha los [`LivenessCommand`] que
  devuelve la función pura [`step`]. La decisión vive en
  [`LivenessState`]: cada `fullname` tiene una [`LivenessRow`]
  con `last_action_at` (instante de la última `ServiceResolved`
  o del último `verify` emitido) y `verify_deadline` (opcional,
  momento en que vence el `verify` en vuelo). El thread sólo
  llama a `daemon.verify(fullname, LIVENESS_VERIFY_TIMEOUT = 5 s)`
  para cada comando y antes de hacerlo vuelve a tomar el lock
  para confirmar que la fila todavía existe y conserva el
  [`VerifyToken`] que `step` estampó en el snapshot. Un
  `ServiceRemoved` que cayó entre snapshot y dispatch deja la
  tabla vacía — el token no matchea y el scheduler descarta el
  comando. Un `ServiceResolved` fresco reemplaza
  `verify_deadline` por `None` y vuelve a escribir un
  `last_action_at` posterior — el token viejo tampoco matchea y
  el scheduler descarta el comando. `on_service_resolved`
  reescribe la fila con `last_action_at = clock.now()` y limpia
  `verify_deadline` (una resolución fresca supersede cualquier
  verify pendiente). `on_service_removed` borra la fila
  directamente, impidiendo cualquier verify futuro hasta una
  nueva resolución. Sin TCP, TLS, IP manual, persistencia de
  rutas ni tráfico de historial.

- **2.4 — Ciclo de vida seguro.** El orden de `MdnsHandle::shutdown`
  cambia a: (1) flip del flag `cancel`, (2) `join` del thread de liveness
  — antes de `unregister`/`shutdown`, para que ningún `verify` quede en
  vuelo cuando el daemon empieza a cerrarse —, (3) `unregister(fullname)`
  con la espera acotada existente (`UNREGISTER_WAIT = 750 ms`), (4)
  `daemon.shutdown()`, (5) `join` del browse loop. La cancelación por
  generación: el browse loop consulta `peer_registry` antes de emitir
  cualquier `Removed`, así que un `ServiceRemoved` post-shutdown
  no encuentra mapping y queda silenciado; la máquina de estado vive en
  `LivenessState` y al perder la fila por `on_service_removed` el
  scheduler nunca reactiva el fullname caído. El saludo
  ordenado (goodbye RFC 6762 §6.7) y el orden pairing → discovery del
  shell se conservan intactos.

- **3.1 — Core: presencia sostenida + `Removed` inmediato.**
  `presence_table_ttl_flips_peer_to_not_available` se reemplaza por
  `observed_peer_stays_present_past_three_former_presence_windows`: el
  peer sigue `Detected` 361 s después de la única `Observed` (tres
  ventanas de 120 s + 1 s de slack) sin otro evento. Se conserva
  `removed_event_flips_presence_to_not_available_immediately` para
  cubrir la transición inversa. 20/20 verde en
  `cargo test --package clipvault-core --lib --features
  local-peer-identity-keychain,local-peer-discovery-mdns peer_discovery`.

- **3.2 — Plataforma: scheduler determinista.** Once tests en
  `clipvault-platform::peer_discovery::mdns::tests`, ninguno crea un
  `ServiceDaemon` real ni depende de multicast. La batería se monta
  sobre el reloj inyectable [`TestClock`] y la función pura
  [`step`]:
  - `on_resolved_seeds_liveness_row_at_clock_now` cubre el anclaje de
    `last_action_at` en `ServiceResolved` y la limpieza de
    `verify_deadline`.
  - `on_removed_drops_the_liveness_row` cubre el goodbye absoluto:
    la fila desaparece y no se reactiva.
  - `step_issues_verify_after_confirm_interval` afirma que ningún
    comando sale antes de `LIVENESS_CONFIRM_INTERVAL` y exactamente
    uno sale cuando vence (incluyendo el `VerifyToken`).
  - `step_keeps_only_one_in_flight_verify_per_fullname` confirma que
    el `verify_deadline` no se duplica entre llamadas a `step`.
  - `deadline_releases_in_flight_and_rearms_next_attempt` es la
    regresión principal: cruza el `verify_deadline` sin un
    `ServiceResolved` y verifica que el guardia se libera y el
    siguiente `verify` se rearma tras la cadencia completa de 60 s.
  - `removal_during_verify_drops_row_and_prevents_future_verifies`
    arma un `verify` en vuelo, dispara `ServiceRemoved` y comprueba
    que no hay reanimación aunque la fila se recreen con un
    `ServiceResolved` posterior (que sí re‑ancla la cadencia).
  - `on_resolved_clears_in_flight_and_rearms_cadence` arma un
    `verify` en vuelo, dispara una resolución sintética y verifica
    que el guardia se libera y la cadencia se reinicia al nuevo
    ancla.
  - `step_with_no_rows_returns_no_commands` blinda contra filas
    huérfanas tras un `Removed`.
  - `issue_verify_command_carries_token_matching_row_deadline`
    afirma que cada comando emitido lleva un `VerifyToken` cuyo
    `deadline` coincide con el `verify_deadline` de la fila al
    momento del snapshot — el contrato que `run_liveness_scheduler`
    revalida antes de `daemon.verify`.
  - `removed_between_step_and_dispatch_invalidates_command` arma un
    comando, dispara `ServiceRemoved` y demuestra que el token ya
    no matchea (la fila no está) → el scheduler descarta el
    comando. Regresión explícita de la precendencia de `Removed`.
  - `resolved_between_step_and_dispatch_invalidates_command` arma un
    comando, dispara `ServiceResolved` y demuestra que el token ya
    no matchea (`verify_deadline` pasa a `None`) → el scheduler
    descarta el comando. Cierra la simetría con el caso anterior.
  - `cancel_prevents_further_verify_commands` arma el thread con
    `while !cancel { step() ; revalidate(token) ; dispatch ;
    park_timeout(LIVENESS_TICK) }`. Setup y loop comparten un único
    `TestClock` (no mezcla `TestClock` con `SystemClock`). Siembra
    una fila realmente due, registra los comandos efectivamente
    despachados y verifica que (a) antes de cancelar exactamente un
    `verify` aparece en el log y (b) el log no crece después de
    cancelar. El `join` retorna antes de 2 s.

- **3.3 — Loopback real.** `two_daemons_discover_each_other_on_loopback_when_multicast_is_available`
  sigue pasando dentro del límite de 6 s para el `Observed` y 5 s
  para el `Removed`; los dos adapters reales (no mocks) ejercitan
  `unregister` y por lo tanto el camino del goodbye que la nueva capa
  de liveness comparte.

- **3.4 — Frontend.** La UI sólo consume el snapshot
  metadata-only que produce el runtime; el cambio no tocó
  `PeerSharingModal`, el polling de 2 s ni los comandos Tauri
  (la cadencia del modal la verifica
  `local-peer-discovery` §5.2). `npm run check` sale con 0 errores
  y 18 warnings preexistentes no relacionados con este cambio;
  `npm run build` termina verde.

- **4.1 — Verificación reproducible (a reejecutar).**
  - `cargo fmt --all -- --check` → limpio en el código modificado por
    este cambio (las advertencias de `clippy::needless_borrows_for_generic_args`
    que `cargo clippy --tests` reporta son preexistentes, no fueron
    introducidas por este PR). El formateo se aplicó sólo a
    `crates/clipvault-platform/src/peer_discovery/mdns.rs`; la
    modificación de tres líneas en
    `app/tauri/src-tauri/src/bootstrap.rs` que `cargo fmt --all`
    introdujo quedó revertida (`git checkout HEAD -- …`) porque el
    archivo es ajeno a este cambio.
  - `cargo test --package clipvault-core --lib --features
    local-peer-identity-keychain,local-peer-discovery-mdns,local-peer-pairing-tls` →
    verde (incluye `peer_discovery` 20/20 y `peer_pairing`).
  - `cargo test -p clipvault-platform --lib --features
    local-peer-identity-keychain,local-peer-discovery-mdns
    peer_discovery::mdns` → verde (`peer_discovery::mdns` 29/29 con la
    batería pura y las regresiones de token/cancelación).
  - `cargo test --package clipvault-db --lib` → 165/165 verde.
  - `cargo clippy -p clipvault-platform --features
    local-peer-identity-keychain,local-peer-discovery-mdns --no-deps` →
    sin warnings.
  - `openspec validate local-peer-presence-liveness --strict` →
    `Change 'local-peer-presence-liveness' is valid`.
  - `git diff --check` → limpio.
  - `npm run check` y `npm run build` → sin tocar el frontend en este
    refactor.

- **4.2 — Aprobada manualmente.** La prueba manual entre macOS ↔ Linux Wayland
  y Linux X11 ↔ Linux Wayland fue aprobada por el usuario. La cobertura
  automática cubre el camino síncrono del scheduler (cadencia,
  límite por `fullname`, deadline, goodbye, cancelación) y el camino
  asíncrono de la verificación (timeout ⇒ `ServiceRemoved` ⇒
  `DiscoveryEvent::Removed`) queda fijado por el test loopback
  `two_daemons_discover_each_other_on_loopback_when_multicast_is_available`
  ya existente.

- **4.3 — Privacidad.** El scheduler sólo conoce `service_fullname`,
  el `Instant` del último `ServiceResolved` y el
  `verify_deadline` actual. No loguea `fullname`, hostname, IP,
  puerto, peer_id, contenido ni secretos. La corrección del
  scheduler eliminó las trazas previas porque la decisión ahora vive
  en la función pura `step` y la única traza que el thread productivo
  conserva es la rama de error transitorio que el daemon puede
  devolver a `daemon.verify` — se mantiene como frase fija sin
  payload (`mdns-sd verify transient error; will retry`). Ningún log
  lleva contenido, hashes, rutas ni identificadores persistentes.
