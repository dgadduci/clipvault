# Tareas

- [x] 1. Extender el protocolo local GNOME con `quick_paste` posterior al
  handshake y un callback testeable.
- [x] 2. Registrar y liberar el acelerador GNOME, preservando el orden de foco
  y sin transmitir contenido de portapapeles.
- [x] 3. Conectar el callback al evento Tauri existente y omitir el backend
  X11 de hotkeys en Wayland.
- [x] 4. Agregar regresiones y ejecutar formato, tests/checks, build frontend
  y validación OpenSpec.

> Verificación manual reportada el 2026-09-14: `Ctrl+Shift+V` aprobado y
> funcionando correctamente en Wayland y X11.
