# Design: persistent image rotation

## User interaction

- El preview compartido agrega dos botones SVG con flechas de giro, solo para
  capturas de imagen. Sus nombres accesibles identifican el sentido del giro.
- Cada activación aplica exactamente 90° y guarda el resultado de inmediato.
  Los botones se deshabilitan mientras la operación está en curso; un fallo
  mantiene la imagen visible sin cambiar la entrada y presenta un mensaje
  localizado.
- Tras guardar, el preview reemplaza su metadata local con la respuesta del
  core y solicita el asset nuevo. Un spinner accesible permanece visible desde
  que empieza el giro hasta que termina la recarga del PNG; durante ese estado
  no se muestra el texto de carga inicial. El modal sigue abierto. Al cerrarlo
  y abrirlo otra vez, la base de datos ya contiene la orientación resultante.

## Persistence and ownership

- El comando Tauri acepta solo el ID numérico de la entrada y el sentido
  (`clockwise` o `counterclockwise`). No acepta rutas, bytes ni referencias de
  assets suministradas libremente por la GUI.
- El core recupera la entrada y valida que siga siendo una imagen renderizable.
  Lee su PNG a través de `ClipboardAssetStore`, lo rota sobre su bitmap RGBA y
  lo normaliza usando la codificación PNG existente.
- El PNG nuevo se guarda antes de actualizar SQLite, bajo su hash de contenido.
  Una actualización condicional reemplaza `asset_ref`, `content_hash`,
  `content_size`, `mime_type` y dimensiones en la misma fila, conservando el ID
  y el resto de sus columnas. La referencia anterior se recoge solo después del
  commit y únicamente si ninguna fila sigue usándola.
- La codificación conserva `pHYs`, el perfil `iCCP` o marcador `sRGB`, y los
  valores `gAMA`/`cHRM` cuando aplican. En un cuarto de vuelta intercambia los
  ejes X/Y de resolución física; descarta metadatos de texto u orientación que
  ya no describirían la imagen transformada.
- Si la fila desaparece o cambia mientras se prepara el giro, la escritura
  condicional no sobrescribe esa modificación. Un archivo nuevo que quede sin
  referencia es un huérfano seguro y podrá retirarse mediante la recolección
  normal de assets.
- Si el hash transformado ya pertenece a otra captura, el índice único actual
  impide el cambio. El giro falla de forma segura y deja intactas ambas
  capturas; no las fusiona ni reasigna sus asociaciones.
- Las rotaciones cambian los píxeles persistidos; por ello el hash y las
  dimensiones se actualizan y el ancho/alto se intercambian en cada cuarto de
  vuelta. La entrada mantiene asociaciones, nota, título, favorito, timestamps
  de captura y atribución de origen.
- La rotación no altera assets antiguos compartidos por otras capturas. Si el
  resultado coincide con un asset existente, el almacenamiento lo reutiliza.

## Architecture

- El core contiene transformación, validación y persistencia; el comando Tauri
  solo adapta argumentos/resultados y emite el evento existente de historial
  actualizado tras el commit.
- El preview recibe una respuesta metadata-only con la entrada actualizada;
  el PNG sigue detrás del bridge validado ya utilizado para mostrar imágenes.
- Se reutiliza `image` ya declarado en el workspace y las APIs de
  `ClipboardAssetStore`; no se agrega una dependencia ni una migración.
- Todos los labels accesibles y mensajes de error nuevos se agregan a los cinco
  catálogos soportados. El spinner reutiliza como etiqueta accesible la
  traducción existente para «cargando imagen» y no añade texto visible.

## Verification

- Verificar los paths de giro horario y antihorario, las dimensiones, el cambio
  de referencia persistido y que no se alteren otras columnas de la entrada.
- Ejecutar el check del frontend, `cargo check` para el core y la app Tauri,
  validación estricta de OpenSpec y `git diff --check`.
- La revisión manual del preview y la persistencia entre reinicios queda como
  comprobación de interfaz.
