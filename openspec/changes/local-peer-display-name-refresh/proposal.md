# Propuesta: actualizar el nombre visible de pares conocidos

## Why

Cuando una instalación cambia su nombre visible, conserva el nuevo valor en
Settings, pero los equipos que ya la descubrieron o emparejaron pueden seguir
mostrando el nombre inicial. El anuncio mDNS contiene el nombre, aunque el
runtime solo actualiza su snapshot local al editarlo. Además, la persistencia
trata un nombre distinto como conflicto aunque el peer_id y la huella
permanezcan iguales.

El nombre visible es metadata editable; no identifica ni autentica al equipo.
Rechazar una observación solo por el cambio de nombre deja obsoletos los datos
locales de pares conocidos y puede confundir un cambio legítimo con una
colisión de identidad.

## What Changes

- Al guardar un nombre válido con el uso compartido activo, actualizar el
  anuncio DNS-SD/mDNS y el nombre usado en nuevos intercambios de pairing, sin
  reiniciar el runtime ni modificar peer_id, huella, certificado o confianza.
- Aceptar un nombre válido nuevo de un peer existente cuando coincidan su
  peer_id, huella pública y versión de protocolo, y la transición de capacidad
  sea compatible.
- Actualizar el nombre visible de pares emparejados y no emparejados, mantener
  intactos su confianza y metadata de pairing, y mostrar el nuevo valor en las
  superficies que usan el nombre del peer.
- Mantener el rechazo de anuncios con identidad o protocolo incompatibles.
- Almacenar el nombre nuevo de pares temporalmente ausentes cuando vuelvan a
  anunciarse.

## Fuera de alcance

- Cambiar el peer_id, las claves, huellas o certificados al renombrar.
- Añadir un campo de red, endpoint, capability o mecanismo de sincronización
  distinto del anuncio DNS-SD/mDNS existente.
- Cambiar los nombres locales de colecciones, dispositivos o capturas.
- Alterar la confianza, revocación, pairing o acceso a datos como efecto de un
  cambio de nombre.
