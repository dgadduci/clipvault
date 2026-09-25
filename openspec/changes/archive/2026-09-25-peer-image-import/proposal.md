# Propuesta: importación explícita de imágenes desde un par local

## Por qué

ClipVault ya permite navegar previews de texto en un par vinculado y activo e
importar una captura de texto de forma explícita. Las capturas de imagen del
peer siguen excluidas aunque ya existe un pipeline local para validar, guardar,
deduplicar y mostrar assets PNG.

La imagen debe cruzar la red sólo cuando el usuario lo solicita, sin convertir
la navegación en sincronización automática ni exponer rutas o referencias de
assets del otro equipo.

## Qué cambia

- Añadir la capacidad autenticada `image_import` a peers compatibles.
- Publicar filas remotas de imagen con metadata acotada: identificador remoto,
  título, fecha, tamaño y dimensiones.
- Añadir un fetch explícito de la imagen original persistida como PNG.
- Reutilizar el pipeline local de assets, hash canónico, deduplicación,
  provenance, colección vinculada y eventos ya implementados para texto.
- Mostrar una imagen placeholder estática, idéntica para todos los previews
  remotos de imagen; no se transfieren thumbnails en este cambio.
- Mantener separados los endpoints de texto y de imagen para compatibilidad
  con peers anteriores y evitar un refactor prematuro a blobs genéricos.

## Alcance

La importación sólo estará disponible para un peer descubierto, emparejado,
trusted y activo mediante el transporte mTLS existente. El host revalidará la
entrada y el asset en cada request.

El receptor validará el PNG, lo normalizará mediante las reglas locales,
persistirá el asset dentro de `assets/clipboard/` y creará o reutilizará una
entrada de imagen. El resultado se asociará a Historial y a la colección
vinculada por `peer_id`.

La operación será idempotente por peer, entrada remota y hash del PNG
canónico persistido. Una segunda importación no creará filas, assets,
membership ni provenance duplicados.

## Fuera de alcance

- Sin sincronización automática ni descargas en segundo plano.
- Sin thumbnails remotos; se planificarán en un spec posterior.
- Sin transferencia de `asset_ref`, rutas, hashes remotos, tags, favoritos,
  colecciones, aplicación de origen ni bytes de rich text.
- Sin escritura al clipboard, pegado automático o interacción con
  `PrivacyGate`/watcher.
- Sin JPEG, GIF animado, WebP, video, HTML, RTF ni edición de imágenes.
- Sin cambios al payload ni al controlador de drag-and-drop.

## Dependencias

- `peer-text-history-browser`: listado, paginación, estado del peer y tarjeta
  remota.
- `peer-text-import`: transacción de importación, colección vinculada,
  provenance e idempotencia ya verificadas.
- `peer-import-collection-visibility`: visibilidad de la colección y origen
  remoto en el desktop.
- `clipboard-rich-content`: `EntryRecord` de imagen y `ClipboardAssetStore`.

## Criterio de aceptación

Desde la lista remota, un usuario puede importar una captura de imagen válida
de un peer activo. La imagen queda disponible como asset local, aparece en
Historial y en la colección del peer, conserva provenance y no duplica datos al
repetir la acción. Las entradas inválidas, demasiado grandes, no autorizadas o
inaccesibles producen un resultado tipado y seguro sin mutaciones parciales.
