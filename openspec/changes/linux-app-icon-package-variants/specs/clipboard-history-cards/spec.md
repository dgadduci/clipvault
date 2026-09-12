# Cards con iconos de aplicaciones Linux empaquetadas

## ADDED Requirements

### Requirement: Reutilizar el bridge de iconos para aliases locales

Desktop y Quick Paste SHALL reutilizar `source_app_name`,
`source_app_icon_ref` y el bridge PNG existente cuando un alias local o una
raíz de paquete resolvieron metadata. MUST NOT añadir rutas absolutas, bytes,
`Exec=`, estrategias internas ni datos de empaquetado al DTO de las cards.

#### Scenario: Firefox y xterm muestran el asset controlado

- GIVEN una captura permitida con nombre y referencia PNG relativa resuelta
- WHEN se renderiza en Desktop o Quick Paste
- THEN se muestra el PNG mediante el bridge actual
- AND no se modifica el layout, las acciones, el contenido ni los atributos
  protegidos de las cards

#### Scenario: Alias sin icono resoluble

- GIVEN una captura con nombre resuelto pero sin PNG válido
- WHEN se renderiza la card
- THEN conserva el nombre disponible y el fallback accesible actual
- AND no se crea un asset vacío ni se expone la causa interna
