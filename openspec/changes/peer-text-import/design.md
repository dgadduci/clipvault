# Diseño: importación explícita de texto desde un par

## Dependencia y fetch

Depende de peer-text-history-browser. fetch_text se habilita sólo sobre la
conexión mTLS de un peer trusted Activo y únicamente tras click Importar. El
host vuelve a verificar que la entrada exista, sea transferible y mida como
máximo 1 MiB UTF-8. Devuelve remote_entry_id, contenido y título opcional; no
devuelve tags, collections, favorites, source app, assets ni hash remoto.

El texto se mantiene en memoria sólo durante la request/transaction. Ningún
evento, log, error o toast lo incluye.

## Persistencia

La siguiente migración disponible agrega:

    peer_collection_bindings(peer_id PK, collection_id UNIQUE, created_at, updated_at)
    remote_imports(
      peer_id, remote_entry_id, imported_content_hash, local_entry_id, imported_at,
      PRIMARY KEY(peer_id, remote_entry_id, imported_content_hash)
    )

Los FKs apuntan a known_peers, collections y entries. Borrar una collection
hace cascade sólo de su binding; nunca borra entry, assets o remote_imports.
Down elimina únicamente las tablas nuevas en orden seguro.

## Transacción de importación

PeerImportService valida identidad, UTF-8, máximo, title y entrada transferible.
Con el detector y hash canónicos locales, dentro de una transacción:

1. Busca una entrada local por hash.
2. Si no existe, crea una entrada local de historial con hora de importación,
   sin source_app falso y con title remoto sólo si pasa la validación existente.
3. Si ya existe, la reutiliza sin tocar título, timestamps, favoritos, tags,
   collections, source metadata ni assets.
4. Obtiene binding por peer_id. Si no existe, crea una colección user con el
   nombre visible del peer; ante colisión usa el sufijo (equipo), persiste su ID
   y no vuelve a renombrarla por un cambio remoto.
5. Inserta membership idempotente y provenance por peer_id, remote_entry_id y
   hash. Un snapshot idéntico no añade fila nueva; una edición remota crea un
   nuevo snapshot sujeto al mismo dedupe local.

Esta ruta no es watcher de clipboard ni captura normal: no invoca PrivacyGate
dependiente de foco, no escribe clipboard y no pega. Tras commit emite los
eventos existentes history-updated y organization-updated con payload vacío.

## UI y verificación

RemoteHistory agrega Importar con estado busy/error seguro. La acción permanece
aislada de cards locales. El resultado identifica sólo la colección destino y
si fue nuevo/deduplicado, no el contenido.

Tests verifican fetch auth/límite, nuevo/reuse, título, nombres en colisión,
membership, provenance, rollback, reimport, snapshot editado y N peers. Manual
en Wayland, X11 y macOS cubre persistencia/reinicio/revoke/block y confirma que
no se modifica clipboard ni assets existentes.
