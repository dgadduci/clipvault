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
tareas o Alt+Tab. La implementación debe medir por separado el runtime de
`cargo tauri dev` y la aplicación empaquetada:

1. Inspeccionar el nombre del archivo `.desktop`, sus claves `Name`, `Icon` y
   `StartupWMClass`, y la identidad GTK/Wayland que expone la ventana
   principal.
2. Confirmar cuál identidad espera KWin para asociar la ventana con la entrada
   instalada; usar el icono Linux ya incluido en la configuración de bundle.
3. Alinear la identidad de la ventana y del lanzador mediante configuración
   soportada por Tauri o metadatos de empaquetado, sin añadir una dependencia
   nativa nueva si no hace falta.
4. Linux habilita `enableGTKAppId` con `com.clipvault.desktop` y usa ese
   mismo valor en `StartupWMClass`. Tauri registra el ID GTK en el bus de
   sesión y evita una segunda instancia; una activación posterior dirige la
   ventana principal al frente, o mantiene al frente el splash si el inicio
   sigue en curso.

El template del lanzador conserva `Icon={{icon}}`, que Tauri resuelve al asset
Linux existente (`clipvault-app`). El icono de bandeja no cambia. La
configuración GTK se limita al override Linux y no cambia el identificador
persistente de ClipVault.

El cambio se limita a la ventana principal de ClipVault. La ventana
`quick-paste` conserva `skipTaskbar`, no crea una entrada adicional en el
Gestor de tareas ni en Alt+Tab, y el icono de Estado y notificaciones no
cambia. No se deben regenerar ni reemplazar los assets persistidos de
imágenes; el icono de aplicación existente se conserva.

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

Se usa el logo de aplicación existente, sin conexión de red ni recurso remoto.
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

## No objetivos

- Cambiar nombres, comandos o contenido del modal Development.
- Cambiar el atajo configurable para abrir el modal de Atajos de teclado.
- Agregar un hotkey global o un servicio de sistema para abrir Development.
- Modificar el icono de bandeja o crear una entrada de tarea para Quick Paste.
- Cambiar el comportamiento de integración GNOME/KWin para atribuir la
  aplicación de origen de las capturas.
- Añadir texto de producto nuevo, dependencias, telemetría o servicios de red.
