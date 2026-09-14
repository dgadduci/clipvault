# Propuesta: conservar el hotkey de Quick Paste durante grabs activos X11

## Problema

La prueba manual aprobada confirma que `Ctrl+Shift+V` abre Quick Paste en
GNOME Wayland mediante la extensión local. En GNOME X11, la misma build
informa que el registro del atajo fue exitoso, pero al pulsarlo no llega la
activación ni se emite `clipvault://quick-search`.

El backend actual `global-hotkey` 0.6 usa un grab pasivo X11 (`XGrabKey`).
El servidor solo activa ese grab si ningún otro cliente posee un
`XGrabKeyboard` activo. Un menú, bloqueo de pantalla, cliente de escritorio
remoto u otro cliente que tome el teclado deja el registro aparentemente
correcto pero impide que el evento alcance a ClipVault.

## Cambio propuesto

- Usar, dentro del adaptador Linux X11 de `clipvault-platform`, un único
  manager `x11rb` que conserva el grab pasivo para la ruta normal y observa
  `RawKeyPress`/`RawKeyRelease` para el caso bloqueado. Ambas rutas comparten
  la misma conexión X11 y el mismo estado de binding. Un `RawKeyPress`
  candidato espera brevemente al `KeyPress` pasivo; su ausencia identifica el
  caso bloqueado sin duplicar callbacks.
- Usar el fallback únicamente cuando se compruebe que un cliente ajeno tiene
  un grab activo del teclado. En esa condición, entregar una sola activación
  al callback de hotkeys existente; fuera de ella, no duplicar ni dejar pasar
  una segunda activación.
- Mantener el contrato Tauri existente: la activación solo emite
  `clipvault://quick-search` sin contenido de clipboard ni metadatos de
  ventana. Wayland conserva exclusivamente la ruta de la extensión GNOME.
- Habilitar la extensión `xinput` de la dependencia `x11rb` ya presente; no
  se agrega un crate, llamada de red, servicio ni permiso nuevo.

## Impacto y límites

El cambio queda limitado a Linux X11 y a la frontera de plataforma. XInput2
no disponible conserva el grab pasivo del mismo manager y expone un
diagnóstico técnico sanitizado; no se simulará un registro exitoso ni se
afectará Wayland, macOS, clipboard, pegado, frontend, cards o assets
persistidos. `global-hotkey` sigue siendo el adaptador para macOS y queda como
degradación solo si no puede inicializarse el manager X11.

No se usará una dependencia Git ni se consumirá la corrección upstream sin
publicar: aunque existe una propuesta upstream para este mismo problema, aún
no forma parte de una release estable. La adaptación local usa el `x11rb`
maduro que ClipVault ya emplea para X11 y permite probar el comportamiento
aisladamente.
