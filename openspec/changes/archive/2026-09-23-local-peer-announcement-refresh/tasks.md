# Tareas: renovación de anuncios mDNS

## 1. Diseño

- [x] 1.1 Documentar la expiración de TTL y por qué `verify` no es apropiado.
- [x] 1.2 Definir reanuncio local, sincronización de reconfigure y shutdown.

## 2. Adaptador

- [x] 2.1 Conservar el `ServiceInfo` publicado y reanunciarlo cada 45 s.
- [x] 2.2 Actualizar el registro retenido al reconfigurar y unir el worker
  antes del goodbye.
- [x] 2.3 Usar sólo logs seguros de frase fija ante error de reanuncio.

## 3. Pruebas

- [x] 3.1 Cubrir que el estado de reanuncio se actualiza y no conserva el
  anuncio anterior tras reconfigure.
- [x] 3.2 Conservar las regresiones de browse, Removed y shutdown.

## 4. Verificación

- [x] 4.1 Ejecutar formato, tests Rust relevantes, OpenSpec estricto y diff.
- [x] 4.2 Prueba manual humana macOS ↔ Linux por más de seis minutos: ambos
  equipos permanecen en línea y comparten archivos.
