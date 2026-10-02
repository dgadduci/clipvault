# Propuesta: configuración de atajos de teclado

## Why

ClipVault tiene atajos repartidos entre listeners de Svelte, el registro de
hotkeys nativos y las integraciones de GNOME y KWin. La mayoría usa
combinaciones fijas; QuickVault ya tiene una vista informativa del atajo
global, pero el usuario no puede cambiarlo desde ClipVault. En KDE Wayland la
integración actual tampoco registra el atajo global para abrir QuickVault.

Una preferencia aislada no alcanza: la combinación visible, el matcher de la
ventana activa, el registro global del sistema y la preferencia persistida
deben actualizarse juntos para que el cambio funcione al instante.

## What Changes

- Agregar a Configuración general una opción **Atajos de teclado** que abra un
  modal con las acciones de teclado propias de ClipVault y su combinación
  efectiva.
- Permitir seleccionar una acción, capturar una combinación nueva, validarla,
  aplicarla de inmediato y persistirla localmente.
- Reemplazar los bindings fijos de los matchers de frontend y de los hotkeys
  globales por una configuración común identificada por acción.
- Mantener visibles y sincronizadas las etiquetas, tooltips y atributos
  accesibles que muestran un atajo, incluida la fila del modal que se acaba
  de modificar.
- Reemplazar la acción configurable **Guardar texto** por abrir o editar la
  nota de la captura seleccionada en la ventana principal. Si no hay una
  captura seleccionada, el atajo no hace nada.
- Añadir un atajo para seleccionar Historial en la barra de colecciones y
  actualizar la lista con el contenido de Historial. La fila de Historial no
  muestra el binding junto a su nombre.
- Añadir un atajo para abrir el diálogo existente de captura de texto, sin
  quitar el botón de la barra superior.
- Permitir la reconfiguración de los atajos globales en macOS, Linux/X11,
  GNOME Wayland y KDE Plasma Wayland sin reiniciar ClipVault ni pedir que se
  refresque una ventana.
- Conservar los defaults actuales cuando no hay una preferencia guardada y
  preservar la compatibilidad con `quick_paste_hotkey`.
- Añadir las cadenas nuevas a todos los catálogos existentes: en, es, pt, de
  y fr.

## Inventario de atajos actuales

La configuración expone una fila por cada una de las diez acciones. Las
acciones compartidas entre ventanas usan una sola preferencia. Las acciones
de nota seleccionada, navegación a Historial y creación de captura de texto
pertenecen a la ventana principal.

| Acción | macOS | Linux |
| --- | --- | --- |
| Abrir QuickVault | `⌘⇧V` | `Ctrl+Shift+V` |
| Alternar captura del portapapeles | `⌘⌥⇧B` | `Ctrl+Alt+Shift+B` |
| Enfocar búsqueda de la ventana principal | `⌘F` | `Ctrl+F` |
| Enfocar búsqueda de QuickVault | `⌘K` | `Ctrl+K` |
| Previsualizar la captura seleccionada | `⌘Enter` | `Ctrl+Enter` |
| Editar la captura de texto seleccionada | `⌘E` | `Ctrl+E` |
| Abrir o editar la nota de la captura seleccionada | `⌘⇧N` | `Ctrl+Shift+N` |
| Ir a Historial y mostrar sus capturas | `⌘⇧H` | `Ctrl+Shift+H` |
| Crear una captura de texto | `⌘N` | `Ctrl+N` |
| Copiar como texto sin formato en QuickVault | `Shift+Enter` | `Shift+Enter` |

Las teclas de navegación y activación que no forman parte de una combinación
configurable —por ejemplo, flechas, `Home`, `End`, `Enter`, `Escape`, `F2` y
`Tab`— conservan su comportamiento estándar. El inventario de estas teclas
queda documentado; no se convierten en atajos globales. `Shift+Enter` es una
acción explícita distinta de `Enter` y sí forma parte de la configuración.

## Alcance por plataforma

- **macOS:** registrar y reemplazar las acciones globales mediante el
  adaptador nativo de hotkeys.
- **Linux/X11:** usar el adaptador X11 existente, ampliando su mapa de teclas
  para las combinaciones admitidas.
- **GNOME Wayland (incluido Ubuntu):** reconfigurar desde la integración local
  los dos aceleradores globales que registra la extensión de GNOME.
- **KDE Plasma Wayland:** agregar a la integración KWin el registro de abrir
  QuickVault y permitir reemplazar en caliente ambos atajos globales.
- **Atajos dentro de ventanas y diálogos:** aplicar el mismo binding
  persistido en las ventanas principal y QuickVault, independientemente del
  compositor Linux.
- **Nota de captura:** abrir el mismo diálogo de agregar/editar nota que usa
  la tarjeta seleccionada en la ventana principal; si no hay selección activa,
  no cambiar el foco ni abrir un diálogo.
- **Historial y captura de texto:** seleccionar la colección de sistema
  Historial y reutilizar los flujos de selección de colección y creación de
  texto ya existentes en la ventana principal.

## Fuera de alcance

- Reasignar teclas estándar de edición de texto, navegación, activación de
  controles o accesibilidad.
- Grabar secuencias de varias pulsaciones o combinaciones sin modificadores.
- Configurar atajos de otras aplicaciones o cambiar atajos globales del
  escritorio que no pertenezcan a ClipVault.
- Sincronizar preferencias entre equipos o guardarlas en servicios remotos.
- Añadir dependencias de red o de configuración externa.
