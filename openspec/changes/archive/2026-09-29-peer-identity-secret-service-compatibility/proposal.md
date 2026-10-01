# Propuesta: compatibilidad del secreto de identidad con KWallet

## Why

En Arch KDE Plasma Wayland, la integración de KWallet con Secret Service
acepta la creación de la entrada de identidad, pero la lectura segura actual
devuelve cero bytes después de reintentar. Una prueba anterior con
`secret-tool store` confirmó texto simple, pero no cubrió los 32 bytes
aleatorios que ClipVault guarda con `Entry::set_secret`. Sin esos bytes, el
adaptador no puede derivar `peer_id` ni fingerprint.

La búsqueda D-Bus de solo lectura encontró una entrada desbloqueada en
`kdewallet`; `secret-tool lookup` terminó correctamente pero no emitió datos.
El comportamiento apunta a incompatibilidad al persistir este payload binario
mediante la interfaz Secret Service de KWallet. La decisión archivada en
`local-peer-identity-foundation` de guardar siempre los 32 bytes crudos no
cubre este proveedor.

## What Changes

Persistir la misma semilla Ed25519 dentro del almacén seguro usando un formato
ASCII versionado que KWallet preserve, manteniendo la identidad estable y sin
exponerla fuera del adaptador de plataforma.

## Alcance

- Guardar nuevas semillas como `clipvault-peer-seed-v1:` seguido de 64
  caracteres hexadecimales minúsculos.
- Leer ese formato y las semillas legacy de exactamente 32 bytes; migrar las
  legacy al formato versionado sin cambiar su identidad.
- Recuperar una entrada vacía, que no puede representar una semilla válida,
  creando y persistiendo una semilla nueva en el formato versionado.
- Rechazar valores no vacíos con formato o longitud inválidos; nunca
  sobrescribirlos silenciosamente.
- Mantener `keyring`, el almacén seguro nativo y la ausencia de fallback a
  archivos ordinarios.

## Fuera de alcance

- Cambiar el flujo de descubrimiento, emparejamiento, red o la UI.
- Agregar dependencias, registrar secretos o modificar datos de usuario.
- Rotar automáticamente una identidad válida o reparar payloads no vacíos
  desconocidos.
