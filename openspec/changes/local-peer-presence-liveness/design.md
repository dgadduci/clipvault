# Diseño: liveness confiable de pares locales

## Causa y decisión

`PeerDiscoveryRuntime` almacena `last_observed` en memoria y hoy infiere
presencia con `now - last_observed <= PRESENCE_TTL` (120 s). El
`MdnsPeerDiscoveryAdapter`, en cambio, sólo puede entregar `Observed` cuando
su daemon resuelve o actualiza el record. Esos dos relojes no forman un
contrato: una instalación sana puede continuar en la cache del browser sin
emitir otra resolución antes del vencimiento que eligió el core.

Se cambia el contrato de presencia:

1. `DiscoveryEvent::Observed` incorpora o actualiza metadata y marca presente.
2. `DiscoveryEvent::Removed` es la única transición normal que marca ausente.
   El adaptador lo emite por goodbye DNS-SD, expiración de cache o fallo de una
   verificación DNS-SD acotada.
3. El core no elimina ni degrada presencia por edad de su último `Observed`.
   Conserva una tabla en memoria de pares presentes y la limpia sólo al recibir
   `Removed`, al detener el runtime o al descartar la identidad local. El
   snapshot continúa derivando `is_present` de esa tabla; SQLite conserva sólo
   la observación histórica, no endpoints ni una lease de presencia.

Así la plataforma sigue siendo dueña de red y liveness; core sigue siendo
puro, testeable y ajeno a `mdns-sd`, sockets, Tauri y timers de DNS.

## Confirmación mDNS acotada

El adaptador productivo conserva, exclusivamente en memoria, el mapping
`service_fullname -> peer_id` ya necesario para traducir removals. Mientras
sharing está activo, programa una comprobación de las instancias resueltas a
una cadencia fija y moderada: primera comprobación no antes de 60 s desde la
última resolución o el último intento, y como máximo una verificación pendiente
por instancia. La espera por verificación es de 5 s.

La verificación usa únicamente una consulta DNS-SD/mDNS del `fullname` ya
descubierto. No usa IP manual, no intenta un puerto TCP, no invoca health TLS,
no abre la API de historial y no persiste hostname/dirección/puerto. Un timeout
hace que el daemon invalide el record y que el adaptador traduzca su
`ServiceRemoved` en `DiscoveryEvent::Removed { peer_id }`.

`ServiceDaemon::verify` es asíncrono: su `Result<()>` sólo confirma que el
comando se encoló. Cuando una respuesta renueva un record sin modificar sus
datos, `mdns-sd` puede refrescar su cache sin emitir otro `ServiceResolved`.
Por tanto, el scheduler NO puede usar `ServiceResolved` como el único modo de
limpiar `in_flight`, ni puede llamar a un intento exitoso una confirmación
observable. Cada intento lleva un `verify_deadline`; al vencer, el scheduler
limpia su guardia y programa el próximo intento desde ese intento anterior. Si
llega `ServiceRemoved`, éste tiene precedencia y elimina la fila. Si llega un
nuevo `ServiceResolved`, refresca metadata y adelanta la siguiente
comprobación, pero no es requisito para que la liveness periódica continúe.
Así, una respuesta sana no atasca el scheduler y una caída posterior sigue
siendo comprobada dentro de `intervalo + timeout`.

El comando puro que `step` devuelve lleva un token de intento opaco (puede ser
el deadline o una generación privada). Justo antes de llamar
`daemon.verify`, el wrapper productivo vuelve a comprobar bajo lock que la fila
sigue existiendo y posee ese mismo token. Si `Removed` o un `ServiceResolved`
más reciente ganó la carrera después del snapshot, descarta el comando; nunca
emite una consulta atrasada para una fila ya removida o supersedida.

La versión actual `mdns-sd 0.11.5` no expone la operación de verificación de
instancia que requiere este contrato. MiniMax actualizará la dependencia
opcional a una versión compatible con el MSRV del workspace que exponga
`ServiceDaemon::verify`, conservará el feature
`local-peer-discovery-mdns`, y adaptará sólo las diferencias de API que sean
necesarias. La dependencia permanece aislada en `clipvault-platform`.

`ServiceDaemon::verify(instance_fullname: String, timeout: Duration) -> Result<()>`
apareció en `keepsimple1/mdns-sd` PR #267 (mergeado en noviembre 2024) y se
publicó por primera vez en la versión 0.12.0. El workspace declaraba
`mdns-sd = "0.11"`, así que el bump mínimo necesario era `0.12`. Se eligió
`mdns-sd = "0.13"` porque: (a) `0.12` ya quedó sin releases de parches;
(b) `0.13.11` (la última 0.13.x en el momento de implementar el cambio)
sigue declarando `rust-version = "1.71"`, muy por debajo del MSRV del
workspace (`1.85`); (c) `ServiceInfo::new`, `enable_addr_auto`, `unregister`,
`browse`, `register` y `shutdown` no cambiaron de firma entre 0.11 y 0.13,
así que el bump queda limitado a la API `verify`. `cargo update -p mdns-sd`
resuelve a `0.13.11` y `cargo check -p clipvault-app` termina sin warnings
adicionales. La dependencia sigue aislada en `clipvault-platform` mediante
el feature `local-peer-discovery-mdns`; ni `clipvault-core` ni `clipvault-db`
ni el frontend la enlazan.

Alternativas descartadas:

- Aumentar `PRESENCE_TTL`: posterga el falso negativo y demora aún más las
  bajas reales; no establece un contrato de liveness.
- Emitir un heartbeat desde core/frontend: duplica lógica de red fuera del
  adaptador y no prueba que el servicio DNS-SD siga publicado.
- Polling TCP/TLS de todos los peers: añade conexiones y consumo continuos,
  mezcla pairing con discovery y excede el opt-in de descubrimiento.
- Deshabilitar App Nap o idle sleep: la regresión afecta también Linux y no
  verifica la visibilidad DNS-SD.

## Ciclo de vida y errores

`start` instala browse, registro y scheduler de confirmación como una sola
unidad. `stop` cancela primero las confirmaciones, luego ejecuta el goodbye
actual (unregister) y finalmente cierra daemon y workers. Un callback tardío
de una generación anterior no puede reinsertar presencia después de `stop` o
de un `Removed`; se usa cancelación/generación privada en el adaptador.

Si el daemon no puede iniciar, se mantiene el outcome tipado actual
`MulticastUnavailable`. Si una verificación falla por un error transitorio de
la API local, el adaptador registra sólo una razón técnica segura y permite el
siguiente intento; sólo el timeout/protocolo de verificación que invalida el
record causa `Removed`. Los logs no contienen fullname, hostname, IP, puerto,
peer_id, contenido ni secretos.

No se cambia el cierre ordenado: `stop_network_subsystems` mantiene el orden
pairing → discovery y el goodbye sigue llevando al `Removed` remoto con la
latencia propia de mDNS. La UI conserva su polling metadata-only de 2 s; éste
sólo proyecta snapshots, nunca inicia la comprobación de red.

## Pruebas

Tests puros del core demuestran que una observación sigue presente más allá de
la antigua ventana de 120 s hasta un `Removed`, y que `Removed` la vuelve no
disponible de inmediato. Se elimina/actualiza el test que codificaba la
expiración local como comportamiento deseado.

Tests del adaptador usan un doble del daemon/scheduler para demostrar:

- una instancia resuelta recibe comprobaciones con la cadencia y timeout
  acotados, sin operaciones TCP;
- una respuesta mDNS sin cambio no deja `in_flight` atascado y el intento
  siguiente se programa aun si no llega otro `ServiceResolved`;
- un `Removed` o `Resolved` que ocurre entre `step` y el dispatch invalida el
  comando snapshot y evita el `verify` atrasado;
- timeout/expiración produce un único `Removed` con el `peer_id` correcto;
- un goodbye ordenado sigue produciendo `Removed` sin esperar la confirmación;
- stop cancela tareas y ningún callback tardío reanima la instancia.

Una integración loopback de dos adapters, cuando multicast está permitido,
mantiene ambos detectados durante al menos 360 s o usa reloj/scheduler
controlable que prueba tres ventanas de 120 s sin volver la suite lenta. La
prueba manual se repite en macOS, Linux Wayland y Linux X11: equipos quietos
por al menos seis minutos permanecen activos; apagar red o proceso confirma
la transición acotada a no disponible; compartir desactivado confirma goodbye.
