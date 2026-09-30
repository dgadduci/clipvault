# Tareas: actualizar el nombre visible de pares conocidos

## 1. Contrato y baseline

- [x] 1.1 Revisar los flujos existentes de edición local, anuncio mDNS,
  observación de peers, persistencia de `known_peers` y proyección en UI; no
  alterar los cambios locales ajenos.
- [x] 1.2 Confirmar que un nombre distinto con identidad estable se rechaza
  actualmente como conflicto y registrar qué metadata de confianza debe
  preservarse.

## 2. Publicación local

- [x] 2.1 Actualizar el anuncio DNS-SD/mDNS activo al guardar un nombre válido,
  conservando peer_id, huellas, capacidades y el browse loop.
- [x] 2.2 Actualizar el nombre usado por nuevos intercambios de pairing sin
  reinstalar la identidad ni modificar certificado o confianza.
- [x] 2.3 Cubrir el cambio con un test de runtime/adaptador y un test del flujo
  de Settings; invalidar nombres no debe cambiar el anuncio.

## 3. Recepción y persistencia

- [x] 3.1 Aceptar una observación con nombre diferente solo si el peer_id y las
  huellas conocidas coinciden y protocolo/capacidad siguen siendo compatibles.
- [x] 3.2 Actualizar el nombre visible de peers emparejados y no emparejados
  conservando trust, certificado fijado, pairing metadata, first_seen_at,
  colecciones e imports.
- [x] 3.3 Mantener los conflictos de huella, protocolo, identidad malformada y
  transiciones de capacidad no permitidas.
- [x] 3.4 Verificar que las superficies que proyectan `known_peers.display_name`
  muestren el último nombre, también cuando un peer vuelve a estar disponible.

## 4. Verificación y cierre

- [x] 4.1 Ejecutar tests representativos de repositorio, runtime, settings y
  pairing; correr formato, checks afectados y validación estricta de OpenSpec.
- [x] 4.2 Revisar diff y confirmar que un cambio de nombre no modifica
  peer_id/huellas, trust, certificados fijados ni contenido local.
- [ ] 4.3 Prueba manual multi-equipo: renombrar un equipo mientras comparte y
  confirmar que los pares descubiertos/emparejados ven el nombre nuevo sin
  reinicio ni nuevo pairing; repetir con un par que vuelve a la red.
- [ ] 4.4 Confirmar que un anuncio con otra huella sigue rechazándose y no
  sobrescribe el peer confiable.
