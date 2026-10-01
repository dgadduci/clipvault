# Tareas: compatibilidad Secret Service para la identidad

Codex documentó la decisión arquitectónica y la evidencia de KWallet. MiniMax
implementa únicamente tras la aprobación del usuario. No archivar, commitear ni
publicar automáticamente.

## 1. Baseline y contrato

- [x] 1.1 Revisar el estado y diff existentes sin alterar los cambios previos
  del usuario; confirmar la única entrada KWallet por búsqueda D-Bus sin
  recuperar ni imprimir su secreto.
- [x] 1.2 Implementar helpers privados y testeables para codificar/decodificar
  el formato ASCII v1 y leer semillas legacy.

## 2. Adaptador de almacenamiento seguro

- [x] 2.1 Usar el formato ASCII v1 al persistir semillas nuevas en la entrada
  existente de `keyring`.
- [x] 2.2 Leer el formato v1, migrar semillas legacy de 32 bytes preservando la
  identidad y recuperar entradas vacías con una semilla nueva.
- [x] 2.3 Rechazar codificaciones no vacías desconocidas sin sobrescribirlas;
  conservar errores tipados y no registrar material privado.
- [x] 2.4 Compartir el mismo flujo de persistencia para perfil y material TLS.

## 3. Regresiones automatizadas

- [x] 3.1 Probar round-trip v1, legacy/migración estable, entrada vacía,
  prefijo, hex y longitudes inválidas con almacenamiento aislado.
- [x] 3.2 Ejecutar tests relevantes del crate platform/core, `cargo fmt`, check
  de compilación Linux y validación estricta de OpenSpec.
- [x] 3.3 Revisar el diff, errores/warnings y confirmar que no hay secretos ni
  datos persistidos del usuario modificados.

## 4. Validación manual pendiente

- [x] 4.1 En Arch KDE Plasma Wayland, iniciar ClipVault tras el cambio y usar
  **Reintentar identidad**; confirmar peer_id/fingerprint y estabilidad tras
  reinicio. Prueba manual aprobada por el usuario el 2026-09-29.
- [x] 4.2 Verificar sólo la longitud del secreto persistido, sin mostrarlo; la
  nueva codificación debe medir 87 bytes. Prueba manual aprobada por el usuario
  el 2026-09-29; se confirmó la longitud sin exponer el valor.
