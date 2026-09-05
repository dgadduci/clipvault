# Diseño: desktop-toolbar-layout

## Composición del desktop

La estructura visual debe quedar conceptualmente así:

    main
    └── desktop-workspace
        ├── OrganizationSidebar
        └── desktop-content-column
            ├── DesktopToolbar
            ├── search-status
            └── HistoryCardRail

Debe existir una sola instancia de DesktopToolbar. No debe quedar otra barra
global por encima del workspace ni una segunda búsqueda en el rail.

desktop-workspace usa una grilla de dos columnas con min-width: 0 en la
columna derecha y align-items: stretch. La altura natural de la columna
derecha —barra de búsqueda, estado mínimo y rail— determina la altura del
workspace. OrganizationSidebar debe ocupar esa misma altura mediante
stretch/height: 100%, no mediante un segundo valor fijo que pueda divergir.

El panel de colecciones mantiene un layout flex de columna. Su encabezado
permanece visible y su lista usa flex: 1, min-height: 0 y overflow-y: auto.
Agregar colecciones no puede aumentar la altura del desktop ni crear una
segunda barra de scroll para el panel.

El rail conserva su altura y el tamaño cuadrado fijo de sus cards. La
columna derecha debe tener min-width: 0 y el viewport horizontal del rail
debe seguir siendo el único responsable del overflow de cards. El body y la
grilla no deben crecer horizontalmente por el contenido.

## Barra de búsqueda

DesktopToolbar se renderiza dentro de desktop-content-column, antes de
HistoryCardRail. El input conserva:

- búsqueda local y callback onSearchInput;
- placeholder y aria-label;
- estado aria-busy;
- foco y atajo Cmd-F en macOS / Ctrl-F en Linux;
- hint visible y accesible calculado por el padre.

El search-status actual puede permanecer debajo de la barra, pero debe formar
parte de la misma columna y no crear un alto independiente en el panel de
colecciones. Los estados de error, carga, sin coincidencias y cantidad de
resultados se conservan.

## Menú de acciones globales

El componente debe reemplazar los cuatro botones textuales por un único
control:

- botón con icono local de puntos suspensivos;
- aria-label y title descriptivos;
- aria-haspopup=menu y aria-expanded;
- foco visible y navegación por teclado.

El menú contiene cuatro menuitem que llaman exactamente los callbacks actuales:
Development, Privacidad, Retención y Atajo de pegado rápido. Activar un item
debe cerrar el menú y abrir el modal correspondiente a través del estado
único que ya posee App.svelte. No se deben registrar comandos nuevos ni
duplicar listeners.

El menú se cierra con Escape, click fuera, selección de un item o destroy.
Al cerrarse sin seleccionar un item, el foco vuelve al botón de puntos
suspensivos. Si se abre repetidamente sólo debe existir una instancia.

El basurero queda como botón hermano del menú, inmediatamente a su derecha.
Conserva aria-label, title, color de peligro, estado busy y el flujo de
confirmación existente. No se debe colocar dentro del menú.

## Responsive

En un ancho reducido, el input y los controles pueden ocupar filas distintas
o permitir que el menú se despliegue hacia abajo, pero:

- el input debe seguir siendo usable;
- el botón de menú y el basurero deben permanecer visibles;
- el menú no debe quedar recortado por overflow del toolbar;
- el rail debe conservar su scroll horizontal;
- el panel de colecciones puede pasar a la fila superior según el breakpoint
  existente, sin introducir una altura mínima vacía.

## Compatibilidad y regresiones

Este cambio es de presentación. No reconstruir EntryRecord ni modificar
visibleEntries para resolver la geometría. Deben conservarse asset_ref,
mime_type, payload_width, payload_height, content_size, created_at, tags,
collections e is_pinned.

La ruta de imagen debe seguir siendo:

    SQLite → recent_entries → HistoryCardRail → clipboard_asset → Blob URL →
    thumbnail

después de reiniciar y también después de cambiar de colección, buscar,
hidratar tags, hacer pin/unpin o hacer drop. El menú sólo dispara callbacks
existentes y no accede a SQLite.

El drag-and-drop debe continuar resolviendo sus destinos con el hit-testing
actual. El menú, el basurero y el input no deben convertirse accidentalmente
en fuentes o destinos de drop.

## Tests requeridos

- Test de que DesktopToolbar se renderiza dentro de la columna de contenido y
  antes del rail.
- Test de que no existe una segunda barra de búsqueda o toolbar global.
- Tests de que los cuatro botones textuales ya no se muestran y sus cuatro
  menuitem conservan callbacks y labels accesibles.
- Tests de apertura, cierre, Escape, click fuera, selección y retorno de foco
  del menú, con instalación idempotente de listeners.
- Test de que el basurero está fuera del menú y conserva el flujo de
  confirmación.
- Tests de grilla con min-width: 0, altura compartida, lista de colecciones
  scrolleable y ausencia de crecimiento del body.
- Tests responsive para input, menú, basurero y rail.
- Regresiones de búsqueda, colección activa, imágenes tras reinicio,
  favoritos, tags y drag-and-drop.

Las pruebas visuales finales en macOS y Linux deben confirmar la alineación
real, el despliegue del menú y la ausencia de overflow horizontal.
