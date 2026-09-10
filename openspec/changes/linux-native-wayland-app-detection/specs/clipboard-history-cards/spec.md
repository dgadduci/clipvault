# Metadata de origen en las capturas

## MODIFIED Requirements

### Requirement: Origen de capturas Wayland nativas

Las capturas obtenidas desde aplicaciones nativas Wayland SHALL conservar el `app_id` como `source_app` cuando el compositor y el protocolo compatible lo permitan, y SHALL reutilizar el flujo existente de enriquecimiento para mostrar nombre e icono sin exponer rutas internas.

#### Scenario: Card de una aplicación Wayland

- GIVEN una fila capturada con `source_app` obtenido del toplevel Wayland
- WHEN se cargan Desktop y Quick Paste
- THEN ambos pueden mostrar el nombre e icono ya resueltos por el provider Linux
- AND la fila conserva contenido, timestamp, título, tags, colecciones y favorito sin alteraciones

#### Scenario: Protocolo no soportado

- GIVEN una fila capturada en una sesión Wayland sin identidad disponible
- WHEN se renderiza la captura
- THEN se mantiene el estado accesible de aplicación desconocida
- AND no se muestra un nombre inventado a partir del título de ventana
