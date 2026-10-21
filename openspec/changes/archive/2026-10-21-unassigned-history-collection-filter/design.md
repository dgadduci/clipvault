# Design: unassigned history collection filter

## Interfaz

- `CollectionFilter` agrega el valor `unassigned` con la etiqueta localizada
  `Sin colección`, después de `Todas` y antes de las colecciones de usuario.
- El toolbar siempre monta el filtro. Desktop pasa `disabled=true` mientras la
  vista activa no sea Historial; el elemento nativo `<select>` recibe el estado
  disabled y su presentación indica que no se puede usar. Se conserva su
  posición en la barra.
- Al navegar fuera de Historial el estado del filtro sigue restableciéndose a
  `Todas`. El filtro deshabilitado no puede cambiar el ámbito de la colección
  activa.

## Semántica de “Sin colección”

- Todas las capturas pertenecen a la colección de sistema `Historial`; esa
  membresía automática no las excluye de `Sin colección`.
- El filtro incluye una captura cuando no hay ninguna fila de membresía hacia
  una colección con `kind = 'user'`. Tener una membresía en cualquier colección
  de usuario la excluye, aunque también pertenezca a `Historial`.
- Se usa un predicado `NOT EXISTS` en la consulta SQLite. Se aplica a recents,
  búsqueda y tipos disponibles, en el mismo query que los filtros de tag,
  aplicación y tipo de captura. No se filtra una página limitada en el
  frontend.
- Tauri expone el indicador `unassigned_only` como parámetro opcional; los
  callers existentes mantienen el comportamiento actual cuando no lo envían.
  El core y la persistencia ofrecen rutas compatibles con las APIs existentes
  para no cambiar los consumidores actuales.

## Verificación

- Probar que Historial-only queda incluido y que asignar una captura a una
  colección de usuario la excluye de `Sin colección`; quitar esa membresía la
  vuelve a incluir.
- Confirmar que recents y búsqueda aplican el mismo alcance y conservan los
  demás filtros.
- Confirmar que el filtro está montado y deshabilitado en una colección de
  usuario, y activo en Historial; probar que la selección se reinicia al
  cambiar de ámbito.
- Verificar paridad de traducciones, regresiones de drag and drop del sidebar,
  checks/build frontend y Rust, OpenSpec estricto y `git diff --check`.
