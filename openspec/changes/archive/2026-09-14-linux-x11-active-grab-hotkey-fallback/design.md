# Diseño: fallback XInput2 para el hotkey X11

## Diagnóstico confirmado

En la build actual de X11, el inicio registra `quick_search` como
`registered`, pero una pulsación no produce los logs `global hotkey activated`
ni `quick-search event emitted`. Esto sitúa el fallo antes de Tauri y del
listener de Quick Paste.

`global-hotkey` 0.6 registra `Ctrl+Shift+V` con `XGrabKey`, un grab pasivo.
Un grab pasivo no se activa mientras otro cliente mantiene un
`XGrabKeyboard` activo. El resultado es silencioso para el backend actual:
el registro ya respondió con éxito antes de que aparezca el grab ajeno.

## Decisión

Un único `LinuxX11HotkeyManagerAdapter`, contenido en `clipvault-platform` y
compilado solo con Linux X11, conserva el grab pasivo y el observador XInput2
en la misma conexión X11:

```text
Ctrl+Shift+V, sin grab ajeno
  -> XGrabKey / manager X11 -> callback -> Tauri

Ctrl+Shift+V, con XGrabKeyboard ajeno
  -> RawKeyPress XInput2 -> manager X11 -> callback -> Tauri
```

El manager selecciona `RawKeyPress` y `RawKeyRelease` sobre la ventana raíz.
Un `GrabKeyboard` temporal identifica que el servidor tiene un grab activo,
pero ese estado también puede corresponder al grab pasivo recién activado de
ClipVault. Por eso la ruta raw queda pendiente hasta que llegue el
`KeyPress` pasivo correspondiente; ese evento cancela el fallback. Si no
llega antes de la siguiente release o de una espera breve, se entrega el
callback raw. Así la ausencia de ruta pasiva, y no una inferencia sobre el
propietario del grab, determina el fallback.

XInput2 no incluye el estado de modificadores en estos eventos. El adaptador
construirá una tabla keycode→modifier a partir de la asignación del servidor y
mantendrá el estado con cada press/release. Solo el conjunto normalizado
`Control|Shift` más las variantes de locks ignorables coincidirá con la
binding solicitada. La liberación siempre restablece el estado publicado,
incluso si el grab ajeno desaparece entre press y release.

## Integración y ciclo de vida

- El manager X11 inicia solo después del preflight `XInitThreads` ya existente
  y solo en `DisplayServer::X11`.
- El manager almacena callbacks por binding opaca; no conoce Tauri, ventanas
  ni clipboard.
- La ruta raw se activa una vez por press y marca el binding como presionado.
  La ruta pasiva cancela el candidato raw antes de activar y la release
  desmarca el binding. Un evento del grab pasivo y otro raw nunca deben
  invocar el callback dos veces para la misma pulsación.
- Al desregistrar o destruir el manager se detiene el hilo, se sueltan las
  selecciones XInput2 y se descartan callbacks. No se guarda contenido ni
  datos de ventana.
- Si XInput2 no está disponible, el manager conserva su grab pasivo y el
  diagnóstico solo informa
  `xinput_unavailable` o `xinput_connection_failed`.

## Dependencias y alternativas descartadas

Habilitar `x11rb/xinput` extiende una dependencia X11 que ya existe para el
probe de aplicación y XTEST. Evita otra biblioteca nativa, FFI nuevo o
conexiones de red. Su impacto queda restringido al feature Linux X11.

1. Actualizar a una corrección upstream aún no publicada o usar una
   dependencia Git: se descarta porque no es una release estable y afectaría
   la reproducibilidad del build.
2. Un observador XInput2 en otra conexión: se descarta porque al sondear un
   grab activo no comparte el estado de la binding pasiva y puede duplicar el
   callback. Un único manager puede dar precedencia al evento pasivo antes de
   entregar el fallback raw.
3. Confiar solo en `XGrabKey`: se descarta porque no entrega eventos bajo un
   grab activo, el caso reproducido.
4. Resolverlo desde Svelte/Tauri: se descarta porque la pérdida ocurre antes
   de que exista un evento de shell y violaría la frontera de plataforma.

## Pruebas y verificación

La lógica que reconstruye modificadores y decide entre grab pasivo/raw será
pura y testeable sin display. Las pruebas de protocolo/adaptador simularán el
estado `AlreadyGrabbed`, la disponibilidad de XInput2 y secuencias de
press/release; no abrirán una sesión real ni leerán el portapapeles.

La verificación manual X11 debe arrancar la build nueva sin una instancia
anterior y comprobar tanto una activación normal como una activación con un
cliente de prueba que mantenga temporalmente `XGrabKeyboard`. Cada caso debe
abrir Quick Paste una sola vez, mantener vivo el proceso y no mostrar la
aserción XCB corregida por el cambio anterior. Wayland solo requiere
regresión del bridge GNOME ya aprobado; el fallback no se construye ni se
inicializa allí.
