# Iconos de aplicación para capturas Wayland

## ADDED Requirements

### Requirement: Cards Wayland enriquecidas sólo con icono local controlado

Las cards de Desktop y Quick Paste SHALL reutilizar la presentación y el bridge de iconos existentes para una captura Wayland cuyo Desktop File ID haya sido resuelto localmente. Si no existe una referencia PNG válida, MUST conservar el fallback accesible actual.

#### Scenario: Icono resoluble para Desktop File ID

- GIVEN una captura Wayland permitida con `source_app_icon_ref` relativo y válido
- WHEN se renderiza en Desktop o Quick Paste
- THEN usa el bridge de iconos existente para mostrar el PNG local
- AND no expone una ruta absoluta ni bytes del icono en el DTO de la card

#### Scenario: Identificador sin asociación de desktop

- GIVEN una captura sin `source_app` porque GNOME informó una app window-backed
- WHEN se renderiza la card
- THEN muestra el fallback de origen/aplicación desconocida existente
- AND no muestra un icono, nombre o título inventado

#### Scenario: Icono local no resoluble

- GIVEN una captura Wayland con nombre de aplicación pero sin `source_app_icon_ref` válido
- WHEN se renderiza la card
- THEN conserva el nombre disponible y el fallback visual
- AND la captura, tags, colecciones, favorito y acciones de la card no cambian
