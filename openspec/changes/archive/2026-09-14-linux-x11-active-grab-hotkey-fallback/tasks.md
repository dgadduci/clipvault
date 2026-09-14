# Tareas: fallback de hotkey X11 ante grab activo

## 1. Relevamiento y contratos

- [x] 1.1 Confirmar en la build X11 nueva que el registro es `registered` y
  que la ausencia de activación ocurre antes de `clipvault://quick-search`.
- [x] 1.2 Revisar el ciclo de vida de `GlobalHotkeyManagerAdapter`, el
  dispatcher global y el preflight `XInitThreads` para que el fallback no
  cree un segundo callback ni una llamada Xlib tardía.
- [x] 1.3 Confirmar el soporte XInput2 del servidor y documentar la
  degradación cuando no esté disponible sin registrar datos de usuario.

## 2. Adaptador Linux X11

- [x] 2.1 Habilitar únicamente `x11rb/xinput` bajo el feature Linux X11, sin
  agregar crates ni afectar builds macOS/Wayland.
- [x] 2.2 Implementar el manager X11 unificado con estado raw XInput2 de
  modificadores reconstruible, consulta segura de grab activo y cancelación
  explícita.
- [x] 2.3 Mantener las rutas pasiva y raw en la misma conexión para una sola
  activación por pulsación y callbacks con identificador opaco; el candidato
  raw espera al evento pasivo antes de entregarse.
- [x] 2.4 Mantener el grab pasivo como ruta primaria y habilitar la ruta raw
  exclusivamente ante `AlreadyGrabbed`; fallos de XInput2 conservan el
  comportamiento pasivo y un diagnóstico sanitizado.
- [x] 2.5 Garantizar shutdown, desregistro y drop idempotentes sin hilos o
  callbacks retenidos.

## 3. Contrato de Quick Paste

- [x] 3.1 Conservar la emisión única de `clipvault://quick-search` con
  payload `()` y el listener único del frontend.
- [x] 3.2 Verificar que el fallback no ejecuta APIs de Tauri/ventana desde su
  hilo X11 ni transmite clipboard, rutas, títulos, hashes o datos de assets.
- [x] 3.3 Confirmar que Wayland sigue omitiendo el backend X11 y usa solo el
  bridge GNOME consentido.

## 4. Pruebas automatizadas

- [x] 4.1 Añadir tests puros para la máquina de estado de modificadores,
  incluyendo `Ctrl+Shift+V`, locks ignorables, secuencias de release y
  repetición de teclas.
- [x] 4.2 Añadir tests de decisión: sin grab ajeno el raw no activa; con
  `AlreadyGrabbed` el raw activa una vez; el release habilita la siguiente
  pulsación; XInput2 no disponible conserva el camino pasivo.
- [x] 4.3 Añadir regresiones de dispatcher que prueben que una misma
  pulsación no duplica el callback entre rutas pasiva y raw.
- [x] 4.4 Ejecutar los tests Rust relevantes con `linux-x11`, `xinput`,
  `hotkey-global` y `linux-xlib-init`, sin abrir un display desde tests puros.

## 5. Verificación y entrega

- [x] 5.1 Ejecutar formato, checks Rust relevantes, `openspec validate
  linux-x11-active-grab-hotkey-fallback --strict --type change` y
  `git diff --check`.
- [x] 5.2 En X11, con instancia anterior cerrada y build actual del checkout,
  comprobar el atajo sin grab ajeno y con un cliente de prueba que mantenga
  `XGrabKeyboard`; ambos abren Quick Paste exactamente una vez y el proceso
  permanece vivo.
- [x] 5.3 Ejecutar la regresión manual del hotkey GNOME Wayland ya aprobada;
  confirmar que no se inicializa el fallback X11 allí.
- [x] 5.4 Revisar el diff: sin secretos, contenido de clipboard, assets
  persistidos ni archivos generados fuera de alcance.

> Verificación manual reportada el 2026-09-14: `Ctrl+Shift+V` aprobado y
> funcionando correctamente en Wayland y X11.
