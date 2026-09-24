# Propuesta: fundamento de identidad de pares locales

## Problema

La futura compartición local no puede confiar en nombres de equipos, hostnames o
direcciones IP. Antes de descubrir o vincular pares, ClipVault necesita una
identidad estable que el usuario pueda presentar y que no exponga la clave
privada fuera del almacén seguro del sistema.

## Objetivo

Agregar un perfil de dispositivo local con nombre visible editable e identidad
criptográfica estable guardada en el almacén seguro de macOS o Linux. Esta
entrega no descubre, anuncia, escucha ni conecta por red.

## Alcance

- Definir PeerIdentityStore y una identidad Ed25519 con peer_id y fingerprint
  derivados de la clave pública.
- Implementar adaptadores Keychain de macOS y Secret Service de Linux, más
  fake en memoria para tests.
- Persistir sólo el nombre visible validado en app_settings.
- Exponer un perfil local metadata-only al shell y a Settings.
- Conservar la posibilidad de que los siguientes cambios agreguen el toggle
  Compartir en red local sobre esta identidad.

## Fuera de alcance

- mDNS, sockets, listeners, TLS, pairing, pares persistidos, historial remoto
  e importación.
- Cualquier clave, secreto o certificado en SQLite, archivos normales, logs,
  fixtures, errores o frontend.

## Criterio de aceptación

Una instalación puede definir su nombre visible y obtener el mismo peer_id al
 reiniciar. Su material privado nunca abandona el secure store. Un sistema sin
 secure store devuelve una guía tipada y no crea identidad insegura.
