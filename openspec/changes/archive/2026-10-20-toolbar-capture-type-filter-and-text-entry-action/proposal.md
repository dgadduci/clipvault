# Propuesta: filtro de tipo de captura y creación rápida de texto

## Por qué

El backend ya admite filtrar capturas por tipo, pero `DesktopToolbar` no monta
un control conectado a esa capacidad. La barra necesita un único filtro de
tipo para Historial y las colecciones locales, con opciones que correspondan
al alcance seleccionado. Además, crear una captura de texto requiere abrir el
menú de acciones secundarias, aunque es una acción frecuente que corresponde
a la barra junto a los filtros.

## Qué cambia

- Incorporar un único filtro de tipo de captura para que muestre los tipos
  presentes en el alcance activo y conserve los iconos y etiquetas usados por
  las cards.
- Aplicar el tipo seleccionado junto con la colección activa, los filtros de
  aplicación y etiqueta, tanto al rail como a la búsqueda, usando el control
  sin renderizar selectores duplicados.
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
