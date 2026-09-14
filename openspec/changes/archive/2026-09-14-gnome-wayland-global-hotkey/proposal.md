# Propuesta: hotkey global de Quick Paste en GNOME Wayland

## Problema

En GNOME Wayland, `Ctrl+Shift+V` solo abre Quick Paste en clientes XWayland,
como Warp o xterm. No funciona desde el escritorio ni clientes Wayland nativos
como GNOME Terminal, Chrome o Files. El adaptador Rust usa `global-hotkey`,
cuya implementación Linux es exclusivamente X11.

## Cambio propuesto

- Extender la integración opcional de GNOME Shell para capturar el atajo con
  Mutter y reenviar un evento local, sin contenido de portapapeles, por su
  socket Unix existente.
- Transformar ese evento validado en la emisión Tauri ya usada por Quick Paste.
- En Wayland, no inicializar el backend X11 de `global-hotkey`.
- Conservar el flujo X11 actual y el protocolo de foco existente.

## Impacto y límites

La extensión GNOME deberá reinstalarse y la sesión de GNOME reiniciarse para
activar su nuevo código. El evento solo expresa `quick_paste`: no transmite
texto, imágenes, rutas, hashes ni datos de ventana. No se implementan hotkeys
para otros compositores Wayland, atajos configurables ni cambios de clipboard.
