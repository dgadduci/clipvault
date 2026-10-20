# Tareas de implementación

## 1. Contrato del filtro

- [x] 1.1 Añadir un único control de tipo al toolbar y cargar sus opciones con
  una consulta metadata-only de los tipos distintos presentes en todo el
  alcance activo, sin depender del límite del rail.
- [x] 1.2 Reutilizar los iconos y las claves de traducción canónicas de las
  cards; mantener «Todas» como primera opción y la navegación accesible del
  combobox.
- [x] 1.3 Colocar el filtro después de los filtros de colección y antes del
  botón «Texto»; conservar disponibilidad, modal y retorno del foco de ese
  botón.

## 2. Estado y consultas

- [x] 2.1 Mantener la selección en `App.svelte`, reiniciarla al cambiar la
  colección o el alcance de colección adicional de Historial y limpiar una
  selección cuyo tipo ya no esté disponible.
- [x] 2.2 Enviar el tipo seleccionado en las solicitudes de recents y búsqueda;
  actualizar las opciones tras cambiar el alcance o el historial.
- [x] 2.3 Añadir el filtro opcional a la consulta SQLite sin cambiar los
  resultados cuando se selecciona «Todas».
- [x] 2.4 Verificar que el filtrado se aplica antes del límite y del ranking.

## 3. Regresiones

- [x] 3.1 Cubrir las opciones completas por alcance, iconos/etiquetas, orden
  del toolbar, reinicio de estado y composición con recents y búsqueda.
- [x] 3.2 Ejecutar regresiones de drag-and-drop protegidas y checks/builds
  afectados.
- [x] 3.3 Validar el cambio con OpenSpec y revisar el diff.
