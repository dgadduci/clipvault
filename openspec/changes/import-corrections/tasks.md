## 1. Import readiness

- [x] 1.1 Propagar una señal de preparación de importación ligada a la
  generación actual del peer desde `RemoteHistoryRail` a cada card remota.
- [x] 1.2 Bloquear los comandos de importación de texto e imagen hasta que esa
  señal esté lista, preservando los gates del core y el preview metadata-only.

## 2. Feedback y regresión

- [x] 2.1 Añadir texto localizado en `en`, `es`, `pt`, `de` y `fr` para el
  estado transitorio de preparación y exponerlo de forma accesible.
- [x] 2.2 Añadir una regresión frontend que pruebe la carrera de preview e
  importación y la invalidación por cambio de peer.

## 3. Verificación

- [x] 3.1 Ejecutar los tests frontend relevantes, validar OpenSpec y revisar
  el diff sin registrar contenido remoto ni archivos generados.

## 4. Rechazos de fetch de capturas existentes

- [x] 4.1 Conservar `not_found` y `not_transferable` como rechazo tipado desde
  TLS hasta `PeerImportOutcome::NotTransferable`.
- [x] 4.2 Conservar `not_available` como resultado tipado de host sin capacidad
  de importación y exponerlo sin filtrar detalles del protocolo.
- [x] 4.3 Mostrar feedback localizado para captura no transferible y host que
  requiere actualización, con regresiones de frontend y core.
- [ ] 4.4 Ejecutar los tests Rust/frontend relevantes, validar OpenSpec y
  revisar el diff final.

## 5. Orden de estado del peer

- [x] 5.1 No sincronizar un estado de importación antes de conocer el snapshot
  completo; revocar el estado cuando un snapshot conocido no contenga al peer.
- [x] 5.2 Serializar por peer las escrituras de estado hacia los servicios de
  historial e importación y mantener deshabilitada la acción hasta que la
  instantánea actual termine.
- [x] 5.3 Añadir una regresión frontend que cubra el snapshot tardío y la
  finalización invertida de las sincronizaciones.
- [ ] 5.4 Ejecutar las verificaciones relevantes y revisar el diff final.
