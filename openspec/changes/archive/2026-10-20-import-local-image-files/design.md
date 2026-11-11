# Design: import-local-image-files

## Flow

1. El botón de foto del toolbar solicita al comando Tauri el selector nativo de
   archivos con selección múltiple y filtros para PNG, JPEG, WebP, GIF, BMP y
   TIFF. Tauri almacena temporalmente las rutas seleccionadas en memoria y
   devuelve al frontend un token de sesión, identificadores opacos y los nombres
   base de los archivos. Si hay nombres repetidos, la interfaz agrega un ordinal
   temporal para distinguirlos. Nunca devuelve directorios ni rutas completas.
2. El frontend presenta una lista de revisión con controles para subir y bajar
   cada archivo. Los selectores nativos de macOS, Linux y Windows no ofrecen un
   contrato común que garantice el orden de clics; por eso el usuario confirma
   el orden final en ClipVault antes de iniciar la importación. Cancelar esta
   revisión descarta las rutas pendientes.
3. Al confirmar, el frontend envía el token y la secuencia de identificadores.
   Tauri valida que sea una permutación exacta de los archivos pendientes,
   consume las rutas y pasa cada archivo al servicio del core en ese orden
   lógico. El core identifica el formato por la firma del archivo y rechaza
   formatos no admitidos, incluido HEIF/HEIC.
4. El decodificador respeta la orientación de la imagen, limita dimensiones y
   memoria según las cotas de imágenes existentes y convierte los píxeles a
   RGBA.
5. El servicio reutiliza la persistencia de imagen actual: normalización y
   codificación PNG, almacenamiento por hash, captura en Historial y
   deduplicación. La colección de captura local activa se añade como una
   asociación adicional; si no hay colección activa, queda solo en Historial.
6. Los archivos se procesan en el orden confirmado. Como el historial ordena
   las capturas nuevas de más reciente a más antigua, el core asigna marcas de
   tiempo con una diferencia de 10 nanosegundos descendente según la posición
   del archivo. Así el historial muestra las capturas nuevas en el orden final
   confirmado y la primera aparición de un duplicado dentro del lote crea la
   captura reutilizada. Un archivo ilegible,
   demasiado grande o no compatible se omite y no impide procesar los demás.
   El backend devuelve únicamente las cantidades importadas, duplicadas y
   omitidas. La interfaz actualiza la lista y muestra el resumen localizado.

## Toolbar text-capture action

The manual text-capture button keeps its existing toolbar position, callback,
accessible name, tooltip and `aria-keyshortcuts` metadata. Its visible content
is only the existing document icon; the visible “Texto” label and shortcut
badge are removed so the toolbar action remains compact.

## Data and privacy

- Todo el trabajo de archivos ocurre localmente en Rust. Durante la revisión,
  el WebView recibe solo identificadores opacos y nombres base para que el
  usuario distinga y ordene archivos; nunca recibe directorios, rutas
  completas, bytes, hashes ni contenido de imagen. Los nombres e
  identificadores son transitorios y no se persisten.
- Tauri consume las rutas pendientes tras una confirmación válida y las elimina
  al cancelar. Si la secuencia confirmada está incompleta, contiene duplicados
  o referencias desconocidas, rechaza la confirmación sin importar archivos.
- No se conserva la ruta original. El asset permanece bajo el almacén PNG
  existente, sujeto a sus límites y a la recolección actual de assets.
- Los duplicados siguen la regla de captura existente y reutilizan la entrada
  por hash; también se asignan a la colección activa cuando corresponda.
- No se añade telemetría, red ni una nueva persistencia de metadatos.
- La garantía de orden se refiere al orden final que el usuario revisa y
  confirma, no al orden de clic que expone el selector del sistema. Las
  capturas duplicadas reutilizadas mantienen su ubicación anterior en el
  historial; la secuencia visible garantizada corresponde a las capturas
  nuevas del lote. Si el mismo contenido aparece varias veces en el lote, la
  deduplicación crea una sola captura en la posición de su primera aparición.

## Dependency

Se fija `image =0.25.10` con `default-features = false` y solo los codecs BMP,
GIF, JPEG, PNG, TIFF y WebP. La dependencia convierte varios formatos a la
representación RGBA que ya recibe el core; la escritura se mantiene en el
codificador PNG existente. Se descartan decodificadores separados por formato
y herramientas externas porque duplicarían código y saldrían del flujo
offline autocontenido. HEIF/HEIC no se habilita.
