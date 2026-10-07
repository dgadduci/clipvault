# Propuesta: accesos, carga y bienvenida del desktop e identidad de ventana en KDE

## Why

El menú principal expone `Development` como una acción cotidiana, aunque es
una vista de diagnóstico. Además, la entrada a Atajos de teclado está
duplicada dentro de Configuración general y como elemento independiente del
menú.

En KDE Plasma, el icono de ClipVault aparece en Estado y notificaciones, pero
no identifica la ventana en el Gestor de tareas ni al alternar ventanas con
Alt+Tab. El primer ajuste alineó el ID GTK y `StartupWMClass`, pero la prueba
manual confirmó que sigue faltando el icono en esas superficies. El ID de
ventana Wayland también debe coincidir con el nombre base de un archivo
`.desktop` instalado; el bundle actual genera `ClipVault.desktop` mientras
Tauri anuncia `com.clipvault.desktop`.

Además, a partir de la observación del usuario de que el logo entregado parece
una hoja de procesador de texto, se diseñará un símbolo propio y más distintivo
para ClipVault. El nuevo símbolo combinará visualmente un clip con una bóveda,
sin hojas, documentos ni plumas, y sustituirá el arte en iconos de ventana,
bandeja y splash en todas las plataformas soportadas.

La prueba posterior en KDE Plasma encontró cuatro fallos concretos: el splash
queda visualmente en blanco durante la mayor parte de su duración; el icono
de bandeja conserva el fondo cuadrado; el Gestor de tareas/Alt+Tab no muestra
el icono de ClipVault; y el spinner de carga no comunica movimiento de forma
clara.

Una captura manual posterior confirma que el icono nuevo ya aparece en Estado
y notificaciones, pero el Gestor de tareas todavía muestra una hoja genérica.
La ventana principal y la bandeja reciben sus iconos por rutas distintas; la
ventana debe aplicar explícitamente el icono cuadrado incluido en el bundle y
reaplicarlo cuando se crea desde la bandeja.

El indicador circular de carga se ve al abrir equipos remotos, pero no da la
misma señal al cargar las colecciones locales. Además, durante el arranque se
expone una pantalla intermedia con el texto técnico «Conectando con el
backend» en lugar de una bienvenida breve y consistente.

## What Changes

- Quitar `Development` del menú de puntos suspensivos y abrir el modal solo
  mediante un atajo local y poco frecuente: `⌃⌥⌘⇧D` en macOS y
  `Ctrl+Alt+Shift+D` en Linux. La combinación queda reservada, no se registra
  como hotkey global ni se añade a la lista de atajos editables.
- Quitar de Configuración general el botón duplicado de Atajos de teclado y
  conservar el elemento independiente ya existente en el menú, su etiqueta y
  callback, además del modal y su atajo actual.
- Reemplazar los iconos de aplicación con un símbolo propio de ClipVault que
  sea reconocible como clip y bóveda y no parezca un icono genérico de
  documentos; usar el símbolo en el splash, un icono de bandeja transparente y
  una variante cuadrada legible para la ventana y el lanzador.
- Establecer explícitamente la variante cuadrada en la ventana principal Linux
  al iniciar y al recrearla desde la bandeja, para que la ventana y el
  lanzador usen el mismo símbolo visible.
- Hacer que el ID de la ventana Wayland (`com.clipvault.desktop`) tenga una
  entrada `.desktop` visible e instalada con nombre base coincidente. Ocultar
  el lanzador generado cuyo nombre no coincide para evitar una entrada de
  menú duplicada. `StartupWMClass` queda como compatibilidad para X11.
  Conservar la ventana Quick Paste sin entrada propia en esas superficies.
- Reutilizar el indicador circular de carga en el contenido de todas las
  colecciones, locales y remotas, con estado accesible y localizado.
- Mantener oculta la ventana principal durante el arranque y mostrar un
  splash con el logo local de ClipVault y una frase breve localizada. El
  splash dura al menos tres segundos y también espera a que termine la carga
  inicial; después se cierra y aparece el desktop. Mostrar el fondo del
  splash desde el primer frame y cargar solo el módulo pequeño del splash
  durante ese intervalo.
- Hacer más perceptible la animación del indicador circular en movimiento
  normal y mantener una señal estática accesible cuando el sistema solicita
  movimiento reducido; el estado busy debe seguir vinculado a operaciones
  asíncronas reales.

## Capabilities

### Modified Capabilities

- `desktop-shell-layout`: accesos del menú, entrada al modal Development y
  feedback de carga de colecciones.
- `keyboard-shortcuts`: acceso independiente al modal sin duplicado en
  Configuración general; se conserva el elemento de menú actualmente
  etiquetado `Atajo de pegado rápido`.
- `desktop-platform-integration`: identidad e icono de la ventana Linux en
  las superficies de administración de ventanas KDE.
- `desktop-foundation`: splash y secuencia de presentación al iniciar.

## Impact

- Frontend: menú de desbordamiento, `GeneralSettingsModal`, matcher de
  teclado, indicador de carga y bienvenida de inicio.
- Configuración de Tauri, variantes de icono y empaquetado Linux: identidad de
  aplicación, asociación Wayland con el archivo `.desktop` y ventanas de
  inicio.
- Localización: nuevas etiquetas y frase del splash en en/es/pt/de/fr.
- Verificación: pruebas frontend, formatos y metadatos del nuevo logo,
  asociación de bundle y sesión real de KDE Plasma.
