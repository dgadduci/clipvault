# Tareas: importación explícita de texto desde un par

## 1. Contexto

- [x] 1.1 Leer paraguas y los cuatro cambios predecesores; confirmar que browse
  de previews y mTLS trusted Activo están verificados antes de añadir fetch.
- [x] 1.2 Revisar dedupe/hash/type, creación de entradas, organización,
  migraciones, eventos y status/diff sin alterar baselines.

## 2. Fetch y persistencia

- [x] 2.1 Añadir fetch_text autenticado con revalidación, outcome tipado y
  máximo 1 MiB; no incluir campos/protocolos fuera de alcance.
- [x] 2.2 Agregar siguiente migración libre para peer_collection_bindings y
  remote_imports, con FKs, índices, up/down y tests desde base previa.
- [x] 2.3 Implementar PeerImportService transaccional: validación, hash/tipo,
  create/reuse, título, binding/colisión, membership y provenance.
- [x] 2.4 Asegurar que import no usa PrivacyGate de foco, watcher, clipboard,
  paste ni source_app falso, y que failure/rollback no deja datos parciales.

## 3. Shell y UI

- [x] 3.1 Exponer fetch/import Tauri y bridge con outcomes discriminados; emitir
  history-updated/organization-updated vacíos sólo después de commit.
- [x] 3.2 Agregar Importar a RemoteHistory con busy, reintento/error seguro,
  feedback metadata-only y sin acciones de card local. Reabierto: la primera
  entrega no sincronizó la caché de elegibilidad del importador al seleccionar
  el peer, por lo que Importar devolvía `no_known_peer` antes de dialeo.
- [x] 3.3 Probar bridge/UI, refresh de rail/collections y que texto no entra en
  logs, toasts, eventos ni payload drag. Reabierto: falta cubrir la
  sincronización conjunta de las cachés de historial e importación.

## 4. Tests de dominio

- [x] 4.1 Probar fetch trust/eligibility/límite y nuevo import, dedupe local,
  mismo snapshot, snapshot editado, Unicode, title inválido y rollback.
- [x] 4.2 Probar binding por peer_id, colisión, collection borrada, revoke/block
  y N peers independientes con entradas/provenance preservadas.

## 5. Verificación

- [x] 5.1 Ejecutar fmt, tests DB/core/network/Tauri, npm check/build/test,
  OpenSpec strict validation y git diff --check. Reabierto para verificar la
  corrección de la sincronización de estado de importación.
- [x] 5.2 Manual Wayland, X11 y macOS: importar de varios pares, reiniciar,
  bloquear/desvincular, borrar collection y confirmar no clipboard/paste ni
  cambios a assets existentes. Aprobada manualmente en la matriz indicada:
  los pares permanecen disponibles, la importación funciona entre Linux y
  macOS, y no se observaron mutaciones del clipboard ni de assets existentes.
