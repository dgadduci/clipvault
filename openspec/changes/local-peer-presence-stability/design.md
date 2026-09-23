# Diseño: estabilidad de presencia de pares locales

El core ya mantiene presencia sin TTL: una resolución válida inserta el
`peer_id` y sólo `DiscoveryEvent::Removed` lo quita. El adaptador no debe
fabricar bajas a partir de un plazo local.

`ServiceDaemon::verify` puede vaciar la caché y emitir `ServiceRemoved` si no
recibe respuesta antes de su timeout. Eso no es un heartbeat confiable entre
macOS y Linux. Se retira el scheduler de verify.

El browse loop conserva exclusivamente:

- `ServiceResolved`: valida metadata y actualiza endpoint si el puerto no es
  cero.
- `ServiceRemoved`: elimina endpoint y emite el único `Removed` del peer.

El daemon conserva su expiración natural y los goodbyes. El shutdown ordenado
permanece; al no haber scheduler se eliminan también su thread, join, reloj,
estado y tokens.
