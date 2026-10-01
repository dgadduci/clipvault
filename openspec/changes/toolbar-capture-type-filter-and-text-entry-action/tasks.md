# Tareas de implementación

## 1. Contrato del filtro

- [ ] 1.1 Localizar y reparar la fuente de opciones del filtro de tipo
  existente en todas las colecciones.
- [ ] 1.2 Conservar los iconos y etiquetas canónicos del control existente, sin
  agregar un select duplicado.
- [x] 1.3 Colocar el filtro y el botón «Texto» en el orden requerido del
  toolbar; conservar la disponibilidad, modal y retorno del foco.

## 2. Estado y consultas

- [ ] 2.1 Mantener la selección del filtro existente en `App.svelte` y
  reiniciarla al
  cambiar la colección.
- [ ] 2.2 Enviar el tipo seleccionado desde el control existente en
  solicitudes de recents y búsqueda.
- [x] 2.3 Añadir el filtro opcional a la consulta SQLite sin cambiar los
  resultados cuando se selecciona «Todas».
- [x] 2.4 Verificar que el filtrado se aplica antes del límite y del ranking.

## 3. Regresiones

- [ ] 3.1 Cubrir las opciones, el orden del toolbar, el control de alcance y
  la composición con historial y búsqueda.
- [x] 3.2 Ejecutar regresiones de drag-and-drop protegidas y checks/builds
  afectados.
- [x] 3.3 Validar el cambio con OpenSpec y revisar el diff.
