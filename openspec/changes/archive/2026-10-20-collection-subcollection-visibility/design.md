# Design: collection-subcollection-visibility

## Control en el sidebar

- Mostrar el botón sólo en filas de colecciones principales de usuario que
  tengan al menos una subcolección directa. Historial y las filas hijas no
  muestran el control porque no pueden tener subcolecciones.
- Insertar el botón inmediatamente después del botón con el nombre de la
  colección y antes de todos sus badges; el icono de nota y el menú permanecen
  después de los badges.
- Mantener los badges fuera del botón de selección para evitar anidar botones.
  Los clicks sobre badges siguen seleccionando la colección, y los botones de
  ojo, nota, disclosure y menú conservan sus acciones independientes.
- El icono representa ojo abierto cuando se incluyen las capturas compartidas
  y ojo tachado cuando se ocultan. `aria-pressed`, `aria-label` y `title`
  reflejan el estado y la acción siguiente con texto localizado.
- El sidebar emite el identificador de la colección principal y el nuevo
  estado. `App.svelte` mantiene una colección de IDs cuya visibilidad está
  desactivada; ausencia de ID equivale a visible.

## Proyección de capturas

- La consulta existente continúa seleccionando membresías directas de la
  colección principal. No se añade una unión implícita con las capturas que
  sólo pertenecen a una subcolección.
- Cuando la opción está activa, conservar la lista existente sin filtrar.
- Cuando está desactivada, excluir las capturas cuya lista de membresías
  contiene cualquier ID de una subcolección directa de la principal activa.
- Aplicar la misma proyección a los resultados recientes y a los resultados
  de búsqueda. La búsqueda, los tags y la aplicación de origen siguen siendo
  filtros del backend y su salida recibe esta proyección adicional.
- Reutilizar la hidratación actual de metadatos de organización por captura.
  Para búsquedas en modo oculto, esperar la hidratación de los resultados
  antes de exponerlos. Si una membresía no se pudo cargar, ocultar esa captura
  mientras el modo esté desactivado; así el filtro no la presenta por error
  como una captura exclusiva de la principal.
- Mantener el estado de cada principal en memoria durante la sesión. Cada una
  empieza en modo visible y no se agrega persistencia.
- No cambiar entradas, membresías, clipboard, payloads de drag-and-drop,
  conteos persistidos ni acciones de borrar/vaciar colecciones.

## Ancho del panel lateral

- El panel que agrupa la lista de colecciones y los equipos vinculados usa la
  primera columna del grid de escritorio. Aumentar en 10% sus anchos mínimo y
  máximo actuales, de `180px–220px` a `198px–242px`.
- Mantener el breakpoint móvil y su grid de una columna sin cambios.

## Localización y verificación

- Agregar las claves para mostrar u ocultar capturas de subcolecciones en los
  cinco catálogos, con el mismo placeholder `{name}`.
- Agregar pruebas del helper para conservar capturas compartidas al mostrar,
  excluirlas al ocultar, considerar todas las hijas y no mezclar otras ramas.
- Actualizar la regresión de filas para confirmar el orden nombre, ojo,
  badges, nota, menú y que el control sólo aparece en una principal con hijas.
- Verificar que la primera columna de escritorio del panel lateral mida
  `198px–242px`, un 10% más que el ancho anterior.
- Ejecutar pruebas frontend relevantes, regresiones de drag-and-drop,
  validación de locales, check, build, validación OpenSpec estricta y
  `git diff --check`.
