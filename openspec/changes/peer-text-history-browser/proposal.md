# Propuesta: navegación de historial textual de un par

## Problema

Después de vincular un par, el usuario necesita decidir qué captura desea
transferir. No debe descargar ni sincronizar todo el historial para ello, ni
recibir imágenes o rich text fuera del alcance inicial.

## Objetivo

Permitir que un par trusted y activo navegue páginas recientes de metadata y
preview directo de capturas de texto transferibles desde el desktop principal.
Esta entrega no descarga el texto completo, no modifica la base local y no
importa contenido.

El camino de transporte es mTLS de extremo a extremo: el cliente resuelve y
diala al par trusted y activo mediante mTLS, el listener autentica y autoriza
al peer según trust/pin vigente, y proyecta únicamente su propio historial
local. No se reutiliza la SQLite del solicitante como fuente de los previews.

## Alcance

- Añadir el endpoint autenticado list_recent_text al transporte mTLS
  productivo. El dial, la autorización y la proyección remota se implementan
  en este mismo cambio.
- Proyectar sólo texto elegible, tipo, fecha, título opcional y preview escapado.
- Usar cursor firmado por el host (HMAC-SHA256 con secreto por peer),
  orden newest-first y un límite de 50 filas. Un cursor no emitido por el
  host, manipulado o que excede el window del secreto actual retorna
  `invalid_cursor`.
- Mostrar los pares vinculados debajo de las colecciones en el desktop, con
  estado Activo/No disponible, punto verde sólo cuando el par es
  `trusted && is_present`, gris para trusted no disponible, y actualización
  explícita tras un vínculo nuevo sin recargar la aplicación.
- Al seleccionar un par activo, reemplazar el contenido principal actual por
  una tira horizontal read-only de previews remotos, con paginación y errores.
- Mantener el lenguaje visual de las cards locales sin reutilizar sus acciones
  ni su controlador de drag/drop; el menú remoto sólo muestra `Importar
  (próximamente)` deshabilitado.

## Fuera de alcance

- fetch de texto completo, Importar, colecciones de pares, provenance,
  deduplicación, búsqueda remota, imágenes/rich text, copy/paste o edición
  remota.

## Criterio de aceptación

Un usuario ve sus pares vinculados junto a las colecciones, identifica su
disponibilidad y puede seleccionar un par activo para recorrer previews
seguros — obtenidos del host remoto vía mTLS y nunca de la base local — en el
mismo contenido principal. La vista no crea entradas locales ni transporta
el cuerpo completo de una captura. Seleccionar una colección local o
`Historial` restaura la rail local.