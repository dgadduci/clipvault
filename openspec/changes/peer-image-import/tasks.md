# Tareas: importación explícita de imágenes desde un par local

## 1. Contratos y baseline

- [x] 1.1 Leer los artefactos archivados de `peer-text-import`,
  `peer-text-history-browser`, `peer-import-collection-visibility` y
  `clipboard-rich-content`; confirmar contratos de transporte, assets,
  dedupe, provenance, colecciones y eventos.
- [x] 1.2 Revisar `git status --short`, `git diff --check` y los baselines de
  cards, drag-and-drop, clipboard y persistencia de assets antes de modificar
  código.
- [x] 1.3 Confirmar que no se requiere una migración nueva; si el esquema
  actual no soporta snapshots de imagen con `remote_imports`, pausar y
  actualizar este cambio antes de implementar.

  > **Nota 2026-09-24:** la confirmación final del cambio añadió la
  > migración `0018_known_peers_caps_extra` para que el campo TXT
  > aditivo `caps_extra` se persista en `known_peers` sin tocar el
  > campo legacy `capability`. El cambio NO necesitó una migración
  > nueva para los snapshots de imagen: el hash canónico del PNG
  > persistido se almacena como `imported_content_hash` y la fila
  > `clipboard_entries` se reutiliza con `content_type = 'image'`
  > gracias a la migración `0008`. El `up`/`down` de la `0018`
  > preserva todos los demás datos de `known_peers`
  > (fingerprint, display_name, capability, trust_state, pin,
  > paired_at, full_public_key_fingerprint, cursor_secret y los
  > índices), como verifican los tests de
  > `crates/clipvault-db/src/registry.rs`.

## 2. Transporte y host

- [x] 2.1 Añadir la capacidad `image_import` y los DTOs metadata-only para
  listar imágenes remotas sin romper los endpoints de texto.
- [x] 2.2 Implementar la proyección de entradas de imagen con revalidación de
  asset, namespace, PNG, tamaño y dimensiones.
- [x] 2.3 Implementar el fetch autenticado de PNG sólo para Importar, con
  límites del asset store y del transporte, timeout y outcome tipado.
- [x] 2.4 Cubrir autorización mTLS, peer inactivo/revocado/bloqueado,
  capability ausente, entrada eliminada y payload inválido sin fugas en logs.
- [x] 2.5 Mantener `capability = pairing` como valor legacy y anunciar
  `image_import` mediante un campo TXT aditivo (`caps_extra`) para no romper
  parsers estrictos; combinar ambos campos en el resolver y rechazar tokens
  desconocidos.

## 3. Core y persistencia

- [x] 3.1 Implementar la ruta `PeerImportService::import_image` usando el
  pipeline local de normalización y `ClipboardAssetStore`.
- [x] 3.2 Integrar create/reuse por hash canónico, título sólo en creación,
  `remote_imports`, provenance y collection binding existente.
- [x] 3.3 Garantizar atomicidad y limpieza de staging ante fallo de asset o
  SQLite, sin borrar ni renombrar assets existentes.
- [x] 3.4 Emitir `history-updated` y `organization-updated` únicamente después
  del commit, con payload metadata-only y sin clipboard/paste/watcher.
- [x] 3.5 Sincronizar la reserva de cleanup de assets con nuevos stages y con
  capturas locales hasta su commit SQLite, sin abrir una ventana de unlink.
- [x] 3.6 Actualizar `known_peers.caps_extra` en observaciones compatibles
  existentes para que los peers emparejados antes de `image_import` reciban la
  capacidad en su siguiente anuncio mDNS.

## 4. Tauri y frontend

- [x] 4.1 Exponer un comando/bridge delgado para importar una imagen y
  outcomes discriminados, sin bytes, rutas ni `asset_ref` remoto en la
  respuesta.
- [x] 4.2 Extender el modelo de filas remotas para imágenes y renderizar el
  placeholder estático común, dimensiones, tamaño y origen.
- [x] 4.3 Habilitar `Importar` para filas elegibles con busy, retry/error seguro,
  refresh de Historial y collections después de commit.
- [x] 4.4 Mantener la tarjeta remota sin drag, pin, edición, copia o pegado y
  conservar intactos los contratos del drag-and-drop de cards locales.
- [x] 4.5 Exponer `caps_extra` en el snapshot metadata-only de peers y
  combinarlo con el campo legacy en el gate de Importar del frontend.
- [x] 4.6 Si el endpoint de imagen responde `not_available`, aplicar la página
  de texto que sí tuvo éxito y agotar sólo el stream de imágenes.

## 5. Tests de dominio y transporte

- [x] 5.1 Añadir tests Rust de listado metadata-only, capability gating,
  revalidación de asset y límites PNG/bytes/dimensiones.
- [x] 5.2 Añadir tests de importación nueva, reuse local, mismo snapshot,
  snapshot editado, title, varios peers, provenance y collection binding.
- [x] 5.3 Añadir tests de rollback, asset temporal, fallo SQLite, peer
  unavailable, revoke/block y preservación de assets existentes.
- [x] 5.4 Añadir tests de transporte mTLS/DTO para asegurar que no cruzan
  `asset_ref`, rutas, hashes, thumbnails ni contenido en previews o errores.
- [x] 5.5 Añadir tests frontend del placeholder idéntico, acción Importar,
  busy/retry, stale response, refresh de rail y ausencia de mutaciones del
  clipboard.
- [x] 5.6 Añadir tests del campo TXT aditivo (`caps_extra`) para
  `image_import` y de interoperabilidad con el parser legacy que sólo
  acepta `capability = pairing`.
- [x] 5.7 Añadir regresiones de filas elegibles intercaladas a través de
  `WorkLimitReached`, de un stage iniciado durante el cleanup reservado,
  de una captura local concurrente con rollback y de una página vacía que
  lleva cursor de continuación.
- [x] 5.8 Añadir regresiones de refresh de capability en peers existentes,
  proyección de `caps_extra` al snapshot y preservación de filas de texto
  cuando imágenes no están disponibles.

## 6. Verificación y regresiones

- [x] 6.1 Ejecutar `cargo fmt --all -- --check`, tests Rust relevantes,
  `cargo clippy` aplicable y `git diff --check`.
- [x] 6.2 Ejecutar `npm run check`, `npm run build` y `npm test` en el
  frontend, incluyendo regresiones de drag-and-drop, cards e imágenes.
- [x] 6.3 Ejecutar `openspec validate peer-image-import --strict --type change`
  y revisar el diff completo sin incluir el cambio no relacionado de
  `quick-paste-text-editing`.
- [x] 6.4 Verificar que los tests de imágenes usan directorios temporales y no
  escriben, renombran ni limpian `~/.clipvault`.
- [x] 6.7 Reejecutar los tests enfocados de paginación, leases SQLite y el
  contrato frontend de cursor no agotado en página vacía.
- [x] 6.8 Ejecutar tests DB/core/frontend, `npm run check`, build afectado,
  regresión de drag-and-drop, formato, `git diff --check` y validación
  OpenSpec.
- [ ] 6.5 Ejecutar la matriz manual en macOS, Linux X11 y Linux Wayland cuando
  estén disponibles: importación, dedupe, edición remota, restart,
  revoke/block, peer unavailable, asset inválido y fallo de persistencia.
- [ ] 6.6 Confirmar manualmente que el preview remoto usa siempre el
  placeholder, que la imagen real sólo aparece tras importar, y que no se
  modifica clipboard, pegado, drag payload ni assets existentes.

> **Nota:** las tareas 6.5 y 6.6 son pruebas manuales que requieren
> intervención humana en macOS, Linux X11 y Linux Wayland. Permanecen
> abiertas hasta que una persona las ejecute en cada plataforma; la
> cobertura automática NO las marca como completadas.

## 7. Entrega

- [x] 7.1 Documentar verificaciones ejecutadas, fallos preexistentes y pruebas
  no necesarias.
- [x] 7.2 Dejar las tareas verificadas marcadas inmediatamente y no archivar ni
  sincronizar este cambio sin una instrucción posterior.
