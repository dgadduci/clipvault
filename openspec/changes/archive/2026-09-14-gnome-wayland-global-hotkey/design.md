# Diseño: hotkey global de Quick Paste en GNOME Wayland

## Decisión

La extensión registrará `Ctrl+Shift+V` mediante
`global.display.grab_accelerator`, permitirá el binding en los modos
`NORMAL | OVERVIEW` y escuchará `accelerator-activated`. Enviará por el socket
Unix ya existente una línea JSON sin datos de usuario:

```json
{"v":1,"kind":"quick_paste"}
```

El listener de `clipvault-platform` aceptará este tipo solo después de `hello`
y lo expondrá mediante un callback. El adaptador Tauri conectará el callback
con `clipvault://quick-search`. En Wayland, `build_hotkey` devolverá el
manager no-op y no creará el backend X11. En X11 se conserva el preflight y
registro actual.

```text
Ctrl+Shift+V -> Mutter/extensión -> socket Unix local -> listener
  -> callback Tauri -> clipvault://quick-search -> Quick Paste existente
```

La extensión publica primero el app-id de foco pendiente y después
`quick_paste`, preservando el orden existente.

## Ciclo de vida

- El grab se crea al habilitar la extensión y se libera, junto con su señal,
  al deshabilitarla.
- Si no hay conexión o se pierde, cualquier activación pendiente se descarta.
- Los fallos o colisiones al registrar el acelerador no pueden derribar GNOME
  Shell.
- Se incrementa la versión de metadata de la extensión para exigir una
  reinstalación local explícita.

## Alternativas descartadas

1. Seguir con `global-hotkey` en Wayland: el crate soporta Linux solo mediante
   X11 y no recibe teclas de superficies Wayland nativas.
2. `Main.wm.addKeybinding` con GSettings: exige añadir, instalar y compilar un
   schema para un único atajo.
3. Un backend genérico Wayland: los protocolos no son uniformes entre
   compositores y excede la integración GNOME consentida.
