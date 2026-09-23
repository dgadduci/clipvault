# Tareas: estabilidad de presencia de pares locales

## 1. Contexto y contrato

- [x] 1.1 Confirmar manualmente macOS ↔ Linux que TLS sigue funcional cuando
  presencia se degrada, y atribuir la baja al verify periódico, no clipboard.
- [x] 1.2 Documentar que `ServiceRemoved` del browser es la única transición
  autoritativa de ausencia.

## 2. Adaptador

- [x] 2.1 Retirar scheduler, estado, reloj, token y `ServiceDaemon::verify`
  sin modificar browse ni fullname → peer_id.
- [x] 2.2 Conservar shutdown y `ServiceResolved` / `ServiceRemoved`, incluido
  cleanup de endpoint de pairing.

## 3. Pruebas

- [x] 3.1 Actualizar las pruebas de plataforma: presencia sostenida sin
  scheduler y eliminación ante `ServiceRemoved`.

## 4. Verificación

- [x] 4.1 Ejecutar formato, tests Rust relevantes, OpenSpec estricto y diff.
- [ ] 4.2 Prueba manual humana macOS ↔ Linux por seis minutos; no completar
  automáticamente.
