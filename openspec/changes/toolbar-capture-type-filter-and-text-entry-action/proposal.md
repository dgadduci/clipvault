# Propuesta: filtro de tipo de captura y creación rápida de texto

## Por qué

El filtro de tipo de captura existente no presenta opciones distintas de
«Todas» en las colecciones. La corrección debe conservar su iconografía actual
y reparar sus opciones, sin agregar un segundo selector. Además, crear una
captura de texto requiere abrir el menú de acciones secundarias, aunque es una
acción frecuente que corresponde a la barra junto a los filtros.

## Qué cambia

- Reparar el filtro de tipo de captura existente para que muestre los tipos
  admitidos en cada colección y conserve los iconos usados por las cards.
- Aplicar el tipo seleccionado junto con la colección activa, los filtros de
  aplicación y etiqueta, tanto al rail como a la búsqueda, usando el control
  existente sin duplicarlo.
- Mover «Nueva captura de texto» fuera del menú de tres puntos y ubicar un
  botón compacto con icono de documento y etiqueta «Texto» entre el filtro de
  tipo y el menú.
- Conservar la misma disponibilidad de creación, modal y persistencia actuales.

## Fuera de alcance

- Cambiar la clasificación o persistencia de capturas.
- Alterar los filtros de aplicación, etiqueta, búsqueda u organización.
- Cambiar acciones secundarias del menú, colecciones, cards o drag-and-drop.
- Agregar dependencias o recursos externos.

## Capacidades afectadas

- `desktop-filtering`: incorporar el filtro aditivo por tipo de captura.
- `desktop-toolbar-layout`: ubicar el filtro y la acción de texto en la fila.

## Impacto esperado

- Frontend: `DesktopToolbar.svelte`, `App.svelte`, un control de filtro de tipo
  y el puente Tauri.
- Rust: adaptar el argumento de tipo en los comandos, servicio de historial,
  búsqueda y consulta SQLite.
- Verificación: pruebas de composición de filtros, opciones del control,
  ubicación del botón y regresiones de drag-and-drop protegidas.
