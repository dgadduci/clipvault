# code-language-detection

Propuesta para que ClipVault reconozca capturas de código fuente comunes,
persista opcionalmente su lenguaje y lo utilice para el resaltado local en la
interfaz.

El cambio reutiliza `ContentType::Code` como categoría estable y agrega
`code_language` como metadata opcional. No crea una variante de enum por cada
lenguaje.

Estado: implementado y verificado manualmente en macOS. La verificación de
Linux X11/Wayland permanece pendiente. No sincronizar ni archivar
automáticamente.
