# Diseño: accesos, carga y bienvenida del desktop e identidad de ventana en KDE

## Acceso a Development

El modal y sus contenidos de diagnóstico permanecen intactos. Se elimina su
acción visible del menú de puntos suspensivos y se abre desde un chord local
de la ventana principal:

| Plataforma | Combinación |
| --- | --- |
| macOS | `Control + Option + Command + Shift + D` (`⌃⌥⌘⇧D`) |
| Linux | `Ctrl + Alt + Shift + D` |

El atajo no se registra como hotkey del sistema: solo funciona cuando la
ventana principal tiene el foco. Se ignora si hay un modal abierto o el foco
está en un campo editable. El matcher reutiliza el ciclo de vida central de
atajos del desktop, no crea listeners por montaje y no abre un segundo modal.
La combinación queda reservada para que una preferencia de atajo editable no
pueda interceptarla. No se incluye una fila para `Development` en el catálogo
visible de Atajos de teclado.

## Acceso a Atajos de teclado

El menú de puntos suspensivos conserva el elemento independiente que abre
`KeyboardShortcutsModal` (hoy etiquetado `Atajo de pegado rápido`). Se elimina
solamente la tarjeta/botón que `GeneralSettingsModal` presenta hoy. Se
mantienen su etiqueta y callback, el atajo `⌘⇧K` en macOS,
`Ctrl+Shift+K` en Linux, la edición de bindings y el acceso a ajustes no
relacionados.

## Identidad de ventana Linux y KDE

El icono de la bandeja no representa el icono de la ventana en el Gestor de
tareas o Alt+Tab. La primera implementación habilitó `enableGTKAppId` y añadió
`StartupWMClass=com.clipvault.desktop`, pero la prueba manual en Plasma confirmó
que el icono de la ventana aún falta. El bundle genera el lanzador
`ClipVault.desktop`, mientras que la superficie Wayland anuncia
`com.clipvault.desktop`. El protocolo `xdg_toplevel` recomienda que el app ID
coincida con el nombre base del archivo `.desktop`; `StartupWMClass` cubre la
ruta de asociación tradicional de X11.

El paquete Linux instala una entrada canónica llamada
`com.clipvault.desktop.desktop`, con `Icon=clipvault-app` y
`StartupWMClass=com.clipvault.desktop`. Se incluye en los bundles `.deb` y
AppImage y queda visible para que KWin pueda resolver el app ID. El lanzador
generado `ClipVault.desktop` se sobrescribe con un archivo de escritorio
estático y válido que incluye
`NoDisplay=true`; así no aparece una segunda entrada en el menú. La
comprobación del paquete verifica la entrada visible, el lanzador oculto,
`StartupWMClass`, `Icon` y el icono PNG instalado. Un helper local de KDE
instala la misma entrada visible en el directorio de aplicaciones del usuario
para que una ejecución de desarrollo también pueda asociarse en Wayland.

El template generado por Tauri conserva `Icon={{icon}}`, que se resuelve al
icono Linux `clipvault-app`. El icono de ventana y el del lanzador usan la
variante cuadrada del logo; el icono de bandeja usa un PNG separado de 64×64
con el símbolo transparente, sin el fondo de la aplicación. Tras la prueba
visual del logo entregado, el nuevo arte será un símbolo geométrico propio que
combine un clip con el disco circular de una bóveda. No incluirá hojas, líneas
de texto, pluma ni otros rasgos de un editor de documentos. Esto hace que la
silueta sea reconocible en panel, Alt+Tab y tamaños pequeños. Esto evita que el
fondo cuadrado del icono de ventana aparezca en Estado y notificaciones. La
ventana `quick-paste` conserva `skipTaskbar`, no crea una entrada adicional
en el Gestor de tareas ni en Alt+Tab, y la configuración GTK se limita al
override Linux sin cambiar el identificador persistente de ClipVault.

La captura de regresión muestra que el icono transparente de bandeja se carga
correctamente mientras KWin sigue mostrando el icono genérico de documento en
la ventana. Linux aplicará explícitamente `icons/icon.png` mediante
`WebviewWindow::set_icon` al configurar la ventana principal y después de
crear/reobtener la ventana desde la acción de bandeja. El icono del lanzador
seguirá apuntando al nombre instalado `clipvault-app`, que corresponde a la
misma composición cuadrada. La operación es de mejor esfuerzo: un fallo al
aplicar el icono se registra y no impide mostrar la ventana.

Referencia técnica a verificar durante la implementación: la opción
[`enableGTKAppId` de la configuración de Tauri](https://v2.tauri.app/reference/config/#enablegtkappid)
permite asociar la aplicación GTK con su entrada `.desktop` y activa el
comportamiento de instancia única en Linux.

## Carga de colecciones

El indicador circular de `RemoteHistoryRail` se reutiliza como feedback de
carga para cualquier colección seleccionada: Historial, Favoritos,
colecciones locales definidas por el usuario y el historial de un equipo
remoto. Debe aparecer mientras se solicitan o actualizan los datos de la
colección seleccionada y desaparecer cuando la operación termine, con éxito
o error. No se muestra cuando la vista está inactiva. Durante una transición
no se debe presentar el contenido de la colección anterior como si perteneciera
a la nueva selección.

El contenedor informa su estado ocupado a tecnologías de asistencia y usa
una etiqueta traducida mediante el catálogo local (`collections.loading`),
con claves equivalentes en en/es/pt/de/fr. El indicador y su animación
respetan la preferencia de movimiento reducido del sistema.

## Bienvenida de inicio

Al abrir la aplicación se muestra una ventana splash propia con el logo local
de ClipVault en tamaño prominente y una frase corta. La ventana principal de
trabajo permanece oculta mientras se ejecuta la carga inicial. El splash se
cierra y la ventana principal se muestra enfocada cuando se cumplen ambas
condiciones: han transcurrido al menos 3 segundos desde que el splash se hizo
visible y la carga inicial del backend y del desktop terminó. Equivale a
esperar `max(3 segundos, duración de la carga inicial)`.

Si la carga inicial termina antes de los 3 segundos, el splash permanece
visible hasta completar ese mínimo. Si tarda más, permanece hasta que termine
la carga. Si la carga concluye con un error recuperable, se respeta el mínimo,
se muestra el desktop con su estado de error y la acción actual de reintento.
El texto «Conectando con el backend» no aparece como pantalla intermedia, y
la ventana principal no puede mostrarse brevemente antes del splash.

La frase se entrega mediante una clave de traducción local (`app.splash.tagline`)
y los cinco catálogos. Texto recomendado:

| Idioma | Frase |
| --- | --- |
| Inglés | “Little clips, safely tucked away.” |
| Español | “Pequeños recortes, bien guardados.” |
| Portugués | “Pequenos recortes, bem guardados.” |
| Alemán | “Kleine Schnipsel, sicher verstaut.” |
| Francés | “De petits extraits, bien à l’abri.” |

Se usa el símbolo propio de ClipVault, sin conexión de red ni recurso remoto.
Para los iconos cuadrados del sistema se compone el mismo símbolo sobre el
fondo oscuro de la aplicación; el splash muestra el PNG transparente
directamente sobre su fondo. La nueva imagen se guarda con nombre propio; el
logo entregado anteriormente se conserva como fuente histórica y no se
sobrescribe.
El splash aparece en el idioma guardado; si todavía no hay preferencia, usa
inglés, de acuerdo con la inicialización de idioma existente.

El splash es una ventana del mismo proceso Tauri, no una segunda instancia ni
otra aplicación. No instala un segundo icono de bandeja. `build_state()` se
ejecuta en un hilo de trabajo después de crear las ventanas; la configuración
de adaptadores que usa APIs de Tauri vuelve al hilo principal antes de
publicar el estado `ready`. La ventana principal consulta ese estado antes de
invocar comandos y comienza la carga del desktop solo después de que el
backend esté administrado.

El splash necesita cerrar su propia ventana cuando termina la carga. Se agrega
`core:window:allow-close` en una capacidad Tauri separada que aplica únicamente
a `startup-splash`; no se amplía ese permiso a la ventana principal ni a Quick
Paste.

El documento HTML establece el fondo oscuro antes de ejecutar JavaScript y
`WindowConfig.backgroundColor` aplica el mismo color al webview nativo. La
entrada de frontend detecta la ventana y carga `StartupSplash.svelte` o
`App.svelte` mediante imports dinámicos; el splash no descarga ni evalúa el
módulo completo del desktop antes de renderizarse. Así el fondo aparece desde
la creación de la ventana y la frase/logo no esperan el montaje de la
aplicación principal.

El indicador circular mantiene su estado busy ligado a las promesas de carga.
En movimiento normal, el arco coloreado tiene contraste suficiente para que
se perciba la rotación; cuando se solicita movimiento reducido, permanece el
indicador estático accesible. El estado ocupado no depende de la duración de
la animación CSS.

## No objetivos

- Cambiar nombres, comandos o contenido del modal Development.
- Cambiar el atajo configurable para abrir el modal de Atajos de teclado.
- Agregar un hotkey global o un servicio de sistema para abrir Development.
- Crear una entrada de tarea para Quick Paste.
- Cambiar el comportamiento de integración GNOME/KWin para atribuir la
  aplicación de origen de las capturas.
- Añadir texto de producto nuevo, dependencias, telemetría o servicios de red.
