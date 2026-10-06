# Propuesta: accesos, carga y bienvenida del desktop e identidad de ventana en KDE

## Why

El menú principal expone `Development` como una acción cotidiana, aunque es
una vista de diagnóstico. Además, la entrada a Atajos de teclado está
duplicada dentro de Configuración general y como elemento independiente del
menú.

En KDE Plasma, el icono de ClipVault aparece en Estado y notificaciones, pero
no identifica la ventana en el Gestor de tareas ni al alternar ventanas con
Alt+Tab. El icono de bandeja y la identidad de la ventana son superficies
distintas y ambas deben tener el comportamiento correcto.

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
- Alinear la identidad e icono de la ventana principal de Linux con el
  lanzador `.desktop` para que KDE Plasma la muestre en el Gestor de tareas y
  en Alt+Tab. Conservar el icono de bandeja y la ventana Quick Paste sin
  entrada propia en esas superficies.
- Reutilizar el indicador circular de carga en el contenido de todas las
  colecciones, locales y remotas, con estado accesible y localizado.
- Mantener oculta la ventana principal durante el arranque y mostrar un
  splash con el logo local de ClipVault y una frase breve localizada. El
  splash dura al menos tres segundos y también espera a que termine la carga
  inicial; después se cierra y aparece el desktop.

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
- Configuración de Tauri y empaquetado Linux: identidad de aplicación,
  asociación con el archivo `.desktop`, icono de ventana y ventanas de inicio.
- Localización: nuevas etiquetas y frase del splash en en/es/pt/de/fr.
- Verificación: pruebas frontend, metadatos de bundle, secuencias de inicio y
  sesión real de KDE Plasma; se preserva el asset de icono existente.
