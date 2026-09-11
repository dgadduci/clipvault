# Diseño: baseline del runtime Ubuntu Wayland

## Baseline aislado

La investigación comienza desde una rama creada sobre `main`.
Antes de iniciar se confirma árbol limpio y se detienen por PID explícito sólo
instancias anteriores de ClipVault, Vite y `cargo tauri dev` del workspace.

La primera ejecución es:

```bash
CARGO_BUILD_JOBS=1 cargo tauri dev
```

No se fuerza `GDK_BACKEND` ni features GNOME. La aparición se evalúa
visualmente: `wmctrl` es una herramienta X11/XWayland y no demuestra la
ausencia de una superficie Wayland nativa.

## Diagnóstico seguro

Si el baseline falla, se habilita una única traza
`clipvault_window_lifecycle`, desactivada por defecto.
Informa sólo etapas normalizadas de la ventana `main`: creación configurada,
entrada/salida de `setup`, disponibilidad booleana de monitor y eventos
`focused`, `resized`, `moved`, `destroyed` y `close_requested`.

La traza nunca incluye contenido de clipboard, títulos, PID, rutas absolutas,
hashes, `asset_ref`, `source_app`, identificadores externos de ventana ni
variables de entorno. Se activa con un booleano documentado.

## Política de corrección

No se agregan `show`, `unminimize`, foco, tamaño, posición ni bridge Svelte
por especulación. La traza Ubuntu decide la corrección:

1. sin ventana creada: revisar configuración o builder;
2. ventana creada pero oculta: una sola solicitud idempotente en el punto
   probado;
3. superficie no mapeada: conservar el baseline y documentar una alternativa
   respaldada por Tauri/GTK;
4. sin `primary_monitor` durante `setup`: no mutar la ventana antes de tener
   evidencia de que esa operación es requerida.

La restauración desde el tray queda separada del arranque inicial. El cambio
preserva macOS, X11, Quick Paste, captura, assets, imágenes, tags, colecciones,
favoritos, búsqueda y drag and drop.

## Validación

La validación manual ocurre en Ubuntu: primer inicio visible, ocultar,
restaurar desde tray, cierre sin proceso residual y reinicio. Después se hace
un smoke test X11. Una corrección funcional incrementa una sola vez el patch;
este plan no cambia versión.
