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
> - 2.3 — Dial mTLS productivo y aceptación en el listener.
> - 3.1 — Comando Tauri invoca el transporte autenticado (no SQLite local).
> - 3.3 — Selección de peer dispara el dial real; rechazo tipado para
>   revoked / blocked / not trusted / cursor fabricado.
> - 3.5 — Apagado correcto de los subsistemas de red en el cierre
>   normal del shell para que el peer remoto reciba el `goodbye`
>   en lugar de esperar al TTL de mDNS.
> - 4.1 — Validación final del bloque 1 + 2 (fmt, tests Rust, npm
>   check/build, regression `pointerDragAndDrop`, e2e TLS A/B,
>   OpenSpec strict, git diff --check).
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
- [ ] 2.3 Dial mTLS productivo: el cliente diala al peer trusted y activo,
  presenta el cert client-side y verifica el pin SHA-256; el listener
  acepta `ListRecentText`, autentica al peer contra el pin y delega en un
  trait de core (`HostHistorySource`) inyectado por el bootstrap.
  platform no depende de SQLite ni de Tauri.
- [ ] 2.4 Reemplazar el cursor percent-encoded por HMAC-SHA256 sobre
  `(peer_id, created_at, id)` con secreto de 32 bytes por peer; rotar el
  secreto en `Revoked`; persistir el secreto ligado al peer; devolver
  `invalid_cursor` ante firma inválida, timestamp manipulado o secreto
  rotado.
- [ ] 2.5 Reemplazar `snapshot_id` por fingerprint SHA-256 no reversible
  sobre el header de la página transferible; excluir `Html` del set
  elegible (aunque sea textual) y cubrirlo con test.
- [ ] 2.6 Tests: orden, cursor firmado (válido, inválido, secreto rotado,
  timestamp manipulado), boundaries, exclusiones (image / rich-text /
  Html), escape, snapshot id no reversible y que DTOs / eventos /
  diagnósticos no exponen contenido completo.

## 3. Shell y UI

- [ ] 3.1 Comando Tauri invoca el transporte autenticado
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
  >   `PeerDiscoveryRuntime`, TTL, pairing ni mTLS. La transición
  >   tras un cierre abrupto sigue dependiendo del TTL vigente.
- [ ] 3.3 Selección de peer dispara el dial real con outcomes tipados;
  rechazo para revoked / blocked / not trusted / pin inválido / peer
  ausente / cursor fabricado; respuesta tardía no puede sobrescribir
  el panel del peer activo.
- [x] 3.5 El cierre normal del shell (`⌘Q`, *tray Salir*, `Ctrl-C`)
  llama a `stop_network_subsystems` antes de la pasada de
  retention: primero `stop_pairing_transport()` y después
  `PeerDiscoveryRuntime::stop()`. El peer remoto observa el
  `ServiceRemoved` en ≤ 5 s y el desktop refleja `No disponible`
  sin esperar al TTL de mDNS (120 s). Ambas paradas son
  best-effort: un fallo en una de las dos sólo registra un
  `warn!` sin IP, puerto, `peer_id` ni contenido; nunca impide
  la salida. La regresión vive en
  `app/tauri/src-tauri/src/bootstrap.rs::tests` y combina un
  adapter de discovery y un transporte de pairing instrumentados
  para asegurar el orden y la idempotencia. El TTL, el
  protocolo mDNS, el pairing y mTLS no se tocan; una caída
  abrupta, una suspensión, Wi-Fi apagado o `kill -9` siguen
  dependiendo del TTL vigente.
- [ ] 3.4 `RemotePreviewCard` con esqueleto visual de card local pero
  sin reutilizar `HistoryCard` ni habilitar drag/drop, pin, editar,
  copy/paste o acciones locales. El único menú muestra `Importar
  (próximamente)` deshabilitado. La fecha se formatea igual que la
  card local. Cuando el panel principal muestra historial remoto, los
  filtros locales (búsqueda / source-app / tag) no se aplican al rail
  remoto.

## 4. Verificación

- [ ] 4.1 Ejecutar fmt, tests Rust relevantes, npm check/build/test,
  OpenSpec strict validation y git diff --check. Regresión
  `pointerDragAndDrop` y test de layout frontend (dos columnas,
  scrollers, toolbar horizontal, rail restaurada).
- [ ] 4.2 Prueba manual en Wayland, X11 y macOS: lista reactiva de
  pares, puntos activo/no disponible, reemplazo del panel principal,
  previews horizontales, páginas, falla de red, menú Importar
  deshabilitado, ausencia de mutación local y dial mTLS real entre
  dos máquinas.
