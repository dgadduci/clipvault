# editable-text-captures

Edicion persistente de capturas textuales desde las cards del historial.

La primera version usa un `<textarea>` nativo dentro de un modal. No agrega un
editor de terceros porque el alcance es texto plano y el WebView de Tauri ya
provee edicion, seleccion, portapapeles y accesibilidad multiplataforma.
