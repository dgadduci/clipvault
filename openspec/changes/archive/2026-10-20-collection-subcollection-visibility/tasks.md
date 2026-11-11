# Tareas de implementación

## 1. Diseño y alcance

- [x] 1.1 Confirmar que las subcolecciones son de un nivel y que sus
  membresías son independientes de la colección principal.
- [x] 1.2 Definir el modo visible como predeterminado y el estado temporal por
  colección principal, sin persistencia ni cambios de backend.

## 2. Interfaz y proyección

- [x] 2.1 Añadir el control de ojo accesible sólo en colecciones principales
  que tengan subcolecciones.
- [x] 2.2 Mantener el estado de visibilidad por colección y conectarlo con la
  proyección de capturas de la vista activa.
- [x] 2.3 Aplicar el filtro a las capturas recientes y a la búsqueda, esperando
  metadatos de membresía cuando el modo de ocultar esté activo.
- [x] 2.4 Agregar y mantener en paridad las etiquetas accesibles en los cinco
  catálogos.

## 3. Regresiones y verificación

- [x] 3.1 Probar la proyección para membresías compartidas, colecciones hijas
  múltiples, capturas sólo hijas y colecciones no relacionadas.
- [x] 3.2 Actualizar la regresión de orden de controles y elegibilidad del
  icono de ojo.
- [x] 3.3 Ejecutar pruebas frontend relevantes, regresiones de drag-and-drop,
  `npm run check`, `npm run build`, `npm test`, `openspec validate
  collection-subcollection-visibility --strict` y `git diff --check`.
- [x] 3.4 Revisar el diff y confirmar que no hay cambios de membresías,
  clipboard, payloads de drag-and-drop, dependencias ni archivos generados.

## 4. Ajustes de interfaz

- [x] 4.1 Separar el control de ojo de la acción de selección de colección.
- [x] 4.2 Ampliar 10% los límites de ancho de la columna lateral de escritorio
  que contiene colecciones y equipos vinculados.
- [x] 4.3 Ejecutar regresiones frontend de colección y drag-and-drop, checks,
  build, validación OpenSpec estricta y `git diff --check`.

## 5. Ajuste de posición del ojo

- [x] 5.1 Colocar el ojo entre el nombre y los badges de la colección, y
  actualizar la regresión de orden.
- [x] 5.2 Confirmar que el panel de colecciones y equipos vinculados conserva
  el aumento del 10% respecto del ancho original.
- [x] 5.3 Ejecutar regresiones frontend de colección y drag-and-drop, checks,
  build, validación OpenSpec estricta y `git diff --check`.
