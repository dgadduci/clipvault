# Propuesta: Repartir el alto entre colecciones y equipos vinculados

## Why

La lista de colecciones y la sección de equipos vinculados comparten actualmente
el alto disponible de la barra lateral en partes similares. La lista de
colecciones necesita más espacio para mostrar y organizar sus elementos.

## What Changes

- Reservar un contenedor compartido para el viewport de colecciones y la sección
  de equipos vinculados.
- Asignar el 70% de la altura disponible de ese contenedor a colecciones y el
  30% a equipos vinculados.
- Conservar encabezados y controles superiores fuera del reparto, y mantener el
  scroll vertical independiente de ambas listas.
- Preservar la altura del workspace y los comportamientos de selección y drag
  and drop.

## Non-Goals

- Cambiar la altura de la ventana o del workspace.
- Modificar el ancho de la barra lateral, los estilos tipográficos o las filas.
- Cambiar el comportamiento o los datos de colecciones y equipos vinculados.
