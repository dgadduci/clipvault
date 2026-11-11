# Proposal: capture-into-active-collection

## Por qué

Las capturas automáticas del portapapeles actualmente sólo se agregan a
`Historial`, aunque el usuario tenga seleccionada una colección local o una
subcolección. Esto obliga a organizar manualmente cada captura que se realiza
mientras se trabaja dentro de una colección.

## Qué cambia

- Al guardar una captura automática, agregarla también a la colección local o
  subcolección activa en ese momento.
- Mantener siempre la membresía de `Historial`.
- Aplicar la asignación también cuando la captura reutiliza una entrada
  existente por deduplicación.
- Si está activa la vista de `Historial`, una colección remota/de equipo, o no
  hay una selección local válida, guardar sólo en `Historial`.
- Mantener la selección activa en memoria durante la sesión; al iniciar, la
  vista activa predeterminada sigue siendo `Historial`.

## Capacidades

### Capacidades modificadas

- `tags-and-collections`: asignación automática de capturas a la colección
  local activa.

## Impacto

- Afecta el watcher y la persistencia del core, el adaptador Tauri y el estado
  de selección del frontend.
- No agrega persistencia nueva, migraciones ni dependencias.
- La asignación adicional no reemplaza otras membresías existentes y no cambia
  la captura, el contenido ni la deduplicación de `Historial`.
