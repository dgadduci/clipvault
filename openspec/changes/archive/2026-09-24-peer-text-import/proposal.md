# Propuesta: importación explícita de texto desde un par

## Problema

La navegación remota permite identificar una captura, pero aún no la incorpora
al historial local. La importación debe ser explícita, persistente, idempotente
y no puede sobrescribir datos locales ni convertirse en sync.

## Objetivo

Permitir obtener el texto completo de una fila remota elegida e importarlo como
snapshot local. La entrada se asocia a una colección vinculada internamente al
peer_id y conserva un título remoto válido cuando es una creación nueva.

## Alcance

- Añadir fetch_text autenticado, únicamente por acción Importar.
- Crear peer_collection_bindings y remote_imports con migración reversible.
- Crear/reutilizar entrada local en una única transacción por hash canónico.
- Crear colección inicial con nombre visible del par, resolver colisión y fijar
  su binding por peer_id.
- Registrar provenance e importar idempotentemente.
- Mantener snapshots si el peer se desconecta, revoca, bloquea o se elimina su
  colección vinculada.

## Fuera de alcance

- Auto-import, sync, push, clipboard write/paste, PrivacyGate dependiente de
  aplicación activa, búsqueda remota, imágenes/rich text, transferir tags,
  collections, favoritos, source app o actualización de snapshots previos.

## Criterio de aceptación

Importar desde cualquiera de N pares activos crea o reutiliza sólo la entrada
local correcta, sin duplicados ni mutar metadata local existente. La colección
y proveniencia del peer quedan persistentes y sobreviven a revocar/bloquear.
