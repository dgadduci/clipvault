# Metadata de origen en las capturas

## MODIFIED Requirements

### Requirement: Origen de capturas Wayland nativas

Las capturas obtenidas desde aplicaciones nativas Wayland SHALL
conservar el `app_id` como `source_app` cuando el compositor publica
un protocolo público capaz de resolver foco y el toplevel enfocado
reporta un `app_id` no vacío, y SHALL reutilizar el flujo existente
de enriquecimiento para mostrar nombre e icono sin exponer rutas
internas. Cuando el compositor no publica un protocolo que permita
identificar foco, la captura permanece con origen desconocido y la
sonda Wayland devuelve `Unavailable` sin inventar identidad a partir
del título, PID, orden de handle o `/proc`.

#### Scenario: Card de una aplicación Wayland identificada por zwlr

- GIVEN una fila capturada con `source_app` obtenido del handle
  `zwlr_foreign_toplevel_handle_v1` cuyo `state[activated] == 2` y
  cuyo `app_id` no está vacío
- WHEN se cargan Desktop y Quick Paste
- THEN ambos pueden mostrar el nombre e icono ya resueltos por el
  provider Linux
- AND la fila conserva contenido, timestamp, título, tags,
  colecciones y favorito sin alteraciones

#### Scenario: Protocolo no soportado por el compositor

- GIVEN una fila capturada en una sesión Wayland sin identidad
  disponible (sin zwlr ni ext con un focus-source interno)
- WHEN se renderiza la captura
- THEN se mantiene el estado accesible de aplicación desconocida
- AND no se muestra un nombre inventado a partir del título de
  ventana, del PID, del orden de handle o de `/proc`

#### Scenario: Compositor anuncia ext sin zwlr

- GIVEN un compositor que sólo publica
  `ext-foreign-toplevel-list-v1`
- AND ningún compositor que publique
  `zwlr_foreign_toplevel_management_unstable_v1` está enlazado
- WHEN la sonda Wayland intenta resolver la aplicación activa
- THEN devuelve `Unavailable` con causa
  `registry_without_protocol`
- AND no se infiere foco del orden de handle, del orden de
  creación, del título o del PID
