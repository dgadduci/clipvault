# Tareas

## 1. Relevamiento

- [x] 1.1 Revisar el estado de Git y los cambios activos antes de modificar el
  menú, la configuración general o el empaquetado Linux.
- [x] 1.2 Confirmar los puntos de entrada actuales de Development y Atajos de
  teclado en `DesktopToolbar`, `App.svelte` y `GeneralSettingsModal`.
- [x] 1.3 Inventariar los bindings editables, el matcher de la ventana
  principal y las reglas actuales de colisión.
- [x] 1.4 Inspeccionar en un bundle Linux el archivo `.desktop`, su icono y la
  identidad GTK/Wayland de la ventana principal; distinguirlos del icono de
  bandeja y del `skipTaskbar` de Quick Paste.
- [x] 1.5 Revisar los estados de carga de colecciones y el orden de arranque
  Tauri; confirmar cómo mostrar el splash antes de cualquier tarea bloqueante.

## 2. Menú y atajo de Development

- [x] 2.1 Quitar `Development` del menú de puntos suspensivos sin cambiar el
  modal, sus comandos ni el retorno de foco.
- [x] 2.2 Abrir el mismo modal con `⌃⌥⌘⇧D` en macOS y
  `Ctrl+Alt+Shift+D` en Linux, limitado a la ventana principal enfocada.
- [x] 2.3 Ignorar el atajo si otro modal está abierto o el foco está en un
  campo editable; no registrar un hotkey global ni un listener duplicado.
- [x] 2.4 Reservar el chord para Development y rechazarlo al guardar bindings
  editables que colisionen.
- [x] 2.5 Quitar de los cinco catálogos solo la etiqueta de menú que ya no se
  renderiza; conservar las cadenas del modal y sus diagnósticos.

## 3. Entrada a Atajos de teclado

- [x] 3.1 Retirar de `GeneralSettingsModal` la tarjeta y el botón duplicados.
- [x] 3.2 Mantener el elemento independiente del menú, el modal, el atajo
  `⌘⇧K` / `Ctrl+Shift+K` y el flujo editable actual.
- [x] 3.3 Actualizar pruebas y especificación canónica para afirmar que no hay
  acceso duplicado en General Settings.

## 4. Icono e identidad KDE

- [x] 4.1 Determinar qué identidad no coincide entre la ventana de Tauri y el
  `.desktop` generado por el paquete que KWin usa para agrupar ventanas.
- [x] 4.2 Alinear el app ID/identidad de la ventana principal y la clave `Icon`
  del lanzador usando el asset Linux existente y configuración soportada.
- [x] 4.3 Si se habilita `enableGTKAppId`, verificar su efecto de instancia
  única y preservar la activación de una instancia existente.
- [x] 4.4 Mantener sin cambios el icono de Estado y notificaciones y excluir
  Quick Paste del Gestor de tareas y Alt+Tab.
- [x] 4.5 Añadir una comprobación reproducible de los metadatos `.desktop` y
  del icono incluido en el paquete.
- [x] 4.6 Confirmar que `enableGTKAppId` publica `com.clipvault.desktop` y que
  el lanzador generado se llama `ClipVault.desktop`; documentar el desajuste
  Wayland con el nombre base esperado por `xdg_toplevel.set_app_id`.
- [x] 4.7 Instalar una entrada `.desktop` llamada
  `com.clipvault.desktop.desktop` en los bundles Linux y comprobar nombre,
  `Icon`, `StartupWMClass` e icono empaquetado.
- [x] 4.8 Añadir un helper optativo para registrar la entrada correspondiente
  al ejecutar `cargo tauri dev` en KDE Wayland.

## 5. Carga de colecciones

- [x] 5.1 Identificar la carga y refresco de Historial, Favoritos, colecciones
  locales del usuario y equipos remotos, junto al spinner circular existente.
- [x] 5.2 Reutilizar el spinner en el panel de contenido durante la carga de
  cada colección local o remota, y quitarlo al resolverse la operación.
- [x] 5.3 Evitar que durante el cambio se muestre contenido anterior como si
  perteneciera a la colección recién seleccionada.
- [x] 5.4 Añadir el estado accesible localizado `collections.loading` a los
  cinco catálogos y respetar movimiento reducido.
- [x] 5.5 Cubrir con pruebas carga local/remota y finalización exitosa o con
  error, sin interferir con las acciones de las colecciones.

## 6. Splash de inicio

- [x] 6.1 Mostrar el splash antes de la carga bloqueante de arranque; verificar
  si el `build_state()` síncrono actual en Tauri `setup` debe diferirse para
  que el splash llegue a pintarse durante el inicio.
- [x] 6.2 Mantener oculta la ventana principal desde el primer frame y mostrar
  el logo transparente aprobado junto a la frase localizada
  `app.splash.tagline`,
  como otra ventana del mismo proceso y sin duplicar la bandeja.
- [x] 6.3 Añadir la frase a en/es/pt/de/fr, usar la preferencia local guardada
  y aplicar inglés cuando todavía no haya una preferencia válida.
- [x] 6.4 Coordinar el fin de la carga inicial con un mínimo de tres segundos
  desde que el splash se ve; cerrar el splash y mostrar/enfocar el desktop solo
  cuando ambas condiciones se cumplan.
- [x] 6.5 Si la carga inicial termina con el error recuperable existente,
  respetar el mínimo y abrir el desktop con su error y acción de reintento.
  No dejar un splash permanente ni mostrar la pantalla de conexión como vista
  de arranque.
- [x] 6.6 Añadir pruebas de inicio rápido, inicio demorado y error usando
  temporizador controlable, y validar duración y textos en los cinco idiomas.
- [x] 6.7 Reemplazar la fuente del logo por el PNG aprobado, generar el icono
  cuadrado con fondo oscuro y regenerar los formatos macOS/Linux; hacer que el
  splash use el logo transparente original.

## 7. Verificación

- [x] 7.1 Añadir pruebas frontend para ausencia del item Development, apertura
  mediante los chords por plataforma, guards de foco y colisiones.
- [x] 7.2 Añadir pruebas frontend que confirmen que General Settings no abre
  Atajos de teclado y el item independiente del menú sigue abriéndolo.
- [x] 7.3 Ejecutar verificación de catálogos, `npm run check`, `npm run build`,
  `npm test`, checks de Rust/Tauri afectados y validación OpenSpec estricta.
- [x] 7.4 Revisar `git diff --check` y confirmar que no se agregan dependencias,
  red ni telemetría; validar formatos de icono generados y que el único permiso
  nuevo, `core:window:allow-close`, sigue limitado a `startup-splash`.
- [x] 7.5 Verificar el `.deb` y AppImage para la entrada cuyo nombre base
  coincide con el app ID, y validar `icon.icns`, `icon.ico` y los PNG Linux.

## 8. Verificación manual

- [x] 8.1 En macOS, probar el chord Development, sus guards y la ausencia del
  item en el menú; confirmar que Atajos de teclado sigue abriendo desde el
  menú y no desde General Settings.
- [ ] 8.2 Repetir en KDE Plasma Wayland, con el bundle nuevo o después de registrar la
  entrada local de desarrollo, confirmar el icono de ClipVault en el panel y
  Alt+Tab, que las ventanas se agrupen correctamente y que Quick Paste no
  agregue una entrada. La prueba anterior encontró el splash en blanco, el
  fondo opaco en la bandeja, icono incorrecto en panel/Alt+Tab y spinner sin
  movimiento perceptible; volver a comprobar los cuatro puntos tras 9.1–9.4.
- [ ] 8.3 En KDE Plasma X11, repetir las comprobaciones de identidad e icono
  cuando el host esté disponible.
- [x] 8.4 Confirmar que el icono de bandeja permanece en Estado y
  notificaciones y conserva sus acciones.
- [x] 8.5 En macOS y Ubuntu, comprobar en un inicio normal que aparece el splash
  localizado, que la ventana principal no se ve antes de tiempo y que el texto
  «Conectando con el backend» ya no aparece.
- [ ] 8.6 En una sesión con carga inicial demorada, confirmar que el splash
  permanece visible después de los tres segundos y se cierra al terminar la
  carga.
- [x] 8.7 Navegar por Historial, Favoritos, colecciones locales y un equipo
  remoto; confirmar el spinner al cargar, que desaparece al terminar y que
  nunca muestra contenido de la selección anterior como contenido nuevo.

## 9. Correcciones tras la prueba manual en KDE

- [x] 9.1 Aplicar fondo oscuro nativo y en el HTML al splash, y cargar el
  frontend del splash mediante un import dinámico independiente de `App.svelte`.
- [x] 9.2 Crear un icono de bandeja transparente a partir del logo aprobado y
  usarlo desde `TrayIconBuilder`; mantener el icono cuadrado para ventana y
  lanzador.
- [x] 9.3 Hacer visible la entrada `.desktop` que coincide con el app ID;
  ocultar el lanzador generado `ClipVault.desktop` en `.deb` y AppImage para
  evitar duplicados; actualizar helper y verificadores.
- [x] 9.4 Aumentar la claridad visual del arco animado del spinner en el modo
  normal y conservar el indicador estático con movimiento reducido; comprobar
  que el busy state sigue el fin real de la carga asíncrona.
- [x] 9.5 Ejecutar pruebas frontend, checks/build de frontend y Rust, bundles
  Linux, verificadores de `.deb`/AppImage, `desktop-file-validate`, OpenSpec
  estricto y `git diff --check`.
- [ ] 9.6 Repetir prueba manual en KDE Plasma Wayland: splash completo desde
  el primer frame, icono de bandeja transparente, icono correcto en panel y
  Alt+Tab, y spinner animado durante una carga demorada. Mantener X11 pendiente
  si no está disponible.
- [ ] 9.7 Repetir el inicio y comprobar la bandeja en macOS y Ubuntu, porque el
  splash y el icono de bandeja cambiaron desde la última prueba manual.

La creación del AppImage y del `.deb` terminó y ambos artefactos pasaron sus
verificadores. `cargo tauri bundle` reportó `Text file busy (os error 26)` al
finalizar después de generar el AppImage; la salida del AppImage y la carpeta
AppDir resultante sí se validaron.

## 10. Rediseño del icono para distinguir ClipVault

- [x] 10.1 Actualizar la propuesta, el diseño y los requisitos para reemplazar
  el símbolo de hoja y pluma por un emblema distintivo de ClipVault, sin forma
  de documento o procesador de texto.
- [x] 10.2 Crear un PNG transparente de alta resolución con una silueta simple
  que combine un clip y el disco de una bóveda; conservar el arte anterior sin
  sobrescribirlo.
- [x] 10.3 Regenerar el icono cuadrado, el icono monocromático transparente de
  bandeja, los tamaños PNG Linux, ICO e ICNS a partir del nuevo símbolo.
- [x] 10.4 Actualizar las pruebas y verificadores para comprobar los nuevos
  assets y sus fondos/transparencias, y ejecutar las validaciones afectadas.
- [ ] 10.5 Probar manualmente en KDE Plasma que el símbolo aparece en bandeja,
  panel y Alt+Tab; repetir una comprobación de icono en macOS y Ubuntu.

## 11. Aplicar el icono correcto a la ventana KDE

- [x] 11.1 Revisar la captura de KDE y confirmar que la bandeja usa el nuevo
  símbolo mientras la ventana principal muestra el icono genérico de hoja.
- [x] 11.2 Aplicar explícitamente `icons/icon.png` a la ventana principal Linux
  durante la configuración inicial, sin bloquear el arranque si el compositor
  rechaza el cambio.
- [x] 11.3 Reaplicar el icono al abrir o recrear la ventana desde la bandeja.
- [x] 11.4 Añadir una prueba de lectura del PNG integrado y verificar que la
  ruta de ventana usa el mismo arte que el lanzador.
- [x] 11.5 Ejecutar tests frontend, tests/check de Rust, OpenSpec estricto y
  `git diff --check`.
- [ ] 11.6 Repetir en KDE Plasma la captura: comparar bandeja, panel y Alt+Tab;
  confirmar que el icono de hoja ya no aparece.

El test unitario del binario y `cargo check -p clipvault-app` pasaron. La suite
completa de integración de Cargo no compila por tres inicializadores
preexistentes incompletos de `AppState` en
`tests/capture_tick_command.rs` (falta `kde_kwin_integration`); el test unitario
enfocado se ejecutó con `--bin clipvault-app` y pasó.
