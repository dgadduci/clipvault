# Diseño: proporción de alto para las listas de la barra lateral

## Contenedor compartido

Agregar un contenedor explícito en `OrganizationSidebar.svelte` que agrupe el
viewport `<ul>` de colecciones y el componente `LinkedPeers`. El encabezado de
colecciones y su formulario de creación permanecen encima de este contenedor,
fuera del cálculo de la proporción.

El contenedor usa una grilla vertical con filas `minmax(0, 7fr)` y
`minmax(0, 3fr)`, sin espacio entre filas. De ese modo, sus dos hijos reciben
respectivamente el 70% y el 30% de su altura disponible. La proporción se mide
contra el alto interior del contenedor compartido, no contra la ventana ni el
workspace completo.

## Scroll y compatibilidad

- El viewport de colecciones conserva `min-height: 0` y `overflow-y: auto`.
- La sección de equipos vinculados ocupa la segunda fila; su lista conserva su
  propio `min-height: 0` y `overflow-y: auto`.
- Los encabezados internos de ambas secciones quedan visibles mientras sus
  respectivas listas hacen scroll.
- El wrapper no cambia el elemento `<ul>` que recibe los eventos de drag and
  drop ni el payload opaco de las capturas.
- El workspace y el alto de la ventana conservan sus límites actuales.

## Verificación

Agregar una regresión frontend que compruebe la jerarquía del contenedor
compartido y las filas 7fr/3fr. Ejecutar los tests afectados de layout y las
regresiones protegidas de drag and drop, además de los checks/build frontend,
validación OpenSpec estricta y `git diff --check`.
