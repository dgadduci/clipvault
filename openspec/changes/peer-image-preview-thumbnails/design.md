# Design: peer-image-preview-thumbnails

## Contrato y separación de rutas

La lista existente (`ListRecentImages`) sigue devolviendo metadata: nunca
incluye bytes, miniaturas, `asset_ref`, rutas ni hashes. La miniatura usa un
request/response separado en el transporte pairing mTLS; no se reutiliza ni se
relaja `FetchImage`, que continúa reservado a la importación explícita del PNG
original.

El request identifica únicamente la entrada remota mediante su referencia
opaca. El host obtiene internamente la referencia local del asset, verifica
de nuevo la autorización y elegibilidad actuales, valida el PNG con el
pipeline existente, reduce y codifica el resultado en memoria. No persiste el
derivado ni crea una fila, binding o provenance.

```text
ListRecentImages ──> metadata-only row ──> placeholder estático
                                           │ al entrar al viewport
                                           v
FetchImageThumbnail ──mTLS──> validación + resize ──> PNG reducido

Importar ──> FetchImage existente ──> PNG original ──> pipeline local
```

## Capability y compatibilidad

- Agregar `image_preview_thumbnail` a `caps_extra`; preservar el valor legacy
  exacto `capability=pairing`.
- El parser de discovery acepta este token junto a `image_import` y sigue
  rechazando tokens desconocidos. La capability sólo habilita la ruta de
  thumbnail; el host exige además el estado trusted/active y `image_import`.
- Las builds que aún sólo anuncian `image_import` funcionan sin cambios y
  conservan el placeholder. El cliente no invoca el nuevo endpoint sin ambas
  capabilities. Los hosts nuevos no sirven miniaturas a callers sin ellas.
- La respuesta usa variantes tipadas para unavailable/not-found/invalid/busy;
  los errores no contienen el PNG, referencia de asset, ruta ni hash.

## Generación y límites

- Reutilizar el decoder, validador de assets y encoder PNG de `clipvault-core`.
  Reducir manteniendo aspect ratio, con lado mayor de como máximo 256 px y
  transparencia preservada. Implementar una reducción determinística y
  acotada con las primitivas/dependencias existentes; no incorporar una
  dependencia de codec grande por conveniencia.
- Límite del body PNG derivado: 384 KiB. Límite del envelope serializado:
  544 KiB como mínimo, para incluir el body en base64 y el framing. Ambos
  límites se comprueban antes de aceptar/enviar el payload. Si el resultado
  excede el límite o no se puede validar/generar, responder con un resultado
  tipado y dejar el placeholder; no recurrir al PNG original.
- El input conserva los límites actuales del asset local. El host limita a
  dos los trabajos simultáneos de decode/resize por peer; el cliente limita a
  dos las solicitudes simultáneas desde el rail activo. Los excesos se
  responden como busy/deferred, sin bloquear el transporte ni la UI.
- Prohibido loguear bytes, PNG, hash, `asset_ref`, ruta o contenido visual.
  Logs permitidos: outcome estable, dimensiones derivadas y tamaño en bytes.

## Ciclo de vida del cliente

- Solicitar una miniatura sólo cuando una card de imagen con ambas capabilities
  intersecta el viewport del scroller del rail. El estado inicial y los
  estados loading/error/busy muestran el placeholder común.
- El preview usa datos locales en memoria/Object URL, sin navegación a un URL
  remoto ni a un asset local del host. Ignorar respuestas si cambió peer,
  fila, página o componente; liberar Object URLs al reemplazar la miniatura,
  desmontar el card o cambiar de peer/página.
- No mantener cache persistente ni cache cross-peer. El conjunto renderizado
  conserva el tope actual de 50 filas combinadas, y las miniaturas quedan
  limitadas a la vida de esas cards.
- No mostrar un error global del rail por una miniatura fallida. `Importar`
  permanece separado y obtiene el PNG original mediante el servicio existente.

## Seguridad y privacidad

Cada request vuelve a verificar peer mTLS/fingerprint fijado, trusted, active,
no revocado ni bloqueado, capabilities, existencia y elegibilidad de la
entrada y validez/namespace del asset. Un ID stale o una imagen borrada/editada
produce un resultado seguro; nunca se confía en valores locales suministrados
por el cliente salvo la referencia opaca.

La generación no toca clipboard, watcher, paste, foco de aplicación, historial
local, colección, import provenance, drag/drop ni assets persistidos. El
payload de drag de cards locales no cambia. Las pruebas de filesystem usan
temporales, nunca `~/.clipvault`.

## Persistencia y dependencias

No se añade migración ni campo SQLite. No se guardan PNG derivados en el
namespace de assets; por lo tanto no requieren lifecycle ni garbage
collection. El protocolo agrega mensajes y una capability aditiva. Si durante
la implementación se comprueba que la reducción requerida no puede hacerse
con el decoder/encoder existente de manera segura, MiniMax debe pausar y pedir
una actualización de este diseño antes de agregar una dependencia de codec.

## Verificación manual

En builds reales para macOS, Linux X11 y Linux Wayland, revisar un peer nuevo y
uno antiguo, miniatura al hacer scroll, placeholder en error/no capability,
cambio de peer durante una respuesta, e importación del original. Confirmar
que la imagen importada conserva el contrato existente y que clipboard,
pegado, drag payload y assets locales ajenos permanecen intactos.
