# Selector Linux de aplicaciones en Privacidad

## ADDED Requirements

### Requirement: El catálogo Linux se ordena por nombre visible

El catálogo de aplicaciones Linux usado por el selector de Privacidad MUST
devolver sus candidatas en orden alfabético por nombre visible. El nombre
visible es `display_name` cuando existe después de quitar espacios exteriores;
si falta o queda vacío, se debe usar `identifier`. La comparación MUST ser
insensible a mayúsculas, independiente del locale y determinista ante nombres
iguales.

#### Scenario: Ordena por nombre y no por identificador

- **GIVEN** candidatas con nombres visibles `Zed`, `alpha` y `Beta`, descubiertas
  en cualquier orden y con identificadores que no siguen ese orden
- **WHEN** se solicita el catálogo Linux
- **THEN** el resultado aparece como `alpha`, `Beta`, `Zed`
- **AND** el orden es el mismo para X11, XWayland y Wayland cuando las
  candidatas contienen la misma metadata

#### Scenario: Usa el identificador cuando falta el nombre

- **GIVEN** una candidata sin `display_name` o con un nombre compuesto solo por
  espacios
- **WHEN** se solicita el catálogo Linux
- **THEN** se ordena usando su `identifier`
- **AND** la candidata sigue siendo seleccionable y la UI usa el identificador
  como texto de fallback

#### Scenario: Desempata nombres visibles iguales de forma estable

- **GIVEN** dos o más candidatas con el mismo nombre visible ignorando
  mayúsculas y espacios exteriores
- **WHEN** se solicita el catálogo Linux más de una vez
- **THEN** se ordenan por el identificador normalizado y después por el
  identificador original
- **AND** el resultado no depende del orden de descubrimiento ni de la
  estrategia (`WmClass` o `DesktopFileId`)

### Requirement: El selector muestra iconos locales de las candidatas

Cuando una candidata Linux contiene un `icon_ref` válido bajo el namespace
`application-icons/`, el selector de aplicaciones de Privacidad MUST cargarlo
mediante `sourceAppIconCommand` y el resolver de iconos existente, y MUST
mostrar la imagen local resultante en la fila de la candidata.

#### Scenario: Muestra un icono de una candidata X11 o XWayland

- **GIVEN** una candidata descubierta desde X11 o XWayland con un
  `icon_ref` relativo `application-icons/...` que resuelve a un PNG válido
- **WHEN** se abre el selector de aplicaciones
- **THEN** la fila muestra el icono en un elemento de imagen
- **AND** la carga usa el comando de iconos de aplicaciones
- **AND** no se llama al comando restringido a `ignored-apps/...`

#### Scenario: Muestra un icono de una candidata Wayland

- **GIVEN** una candidata Wayland respaldada por Desktop File ID
- **AND** el proveedor local resolvió `Name` e `Icon` mediante metadata XDG,
  incluyendo rasterización SVG local cuando corresponde
- **WHEN** se abre el selector de aplicaciones
- **THEN** la fila muestra el icono referenciado bajo `application-icons/`
- **AND** no se requiere consultar GNOME Shell, títulos de ventana, PID ni
  filesystem desde el frontend

#### Scenario: Conserva un fallback cuando el icono no está disponible

- **GIVEN** una candidata sin icono, con un `icon_ref` no resoluble o cuya
  lectura devuelve un error
- **WHEN** el selector intenta cargar sus iconos
- **THEN** la fila muestra la primera inicial en mayúsculas del nombre visible o
  del identificador
- **AND** no aparece una imagen rota
- **AND** las demás filas continúan cargándose y siendo seleccionables

#### Scenario: Libera los recursos visuales del selector

- **GIVEN** el selector creó Blob URLs para iconos de candidatas
- **WHEN** se cierra o se destruye el modal, o se reemplaza el catálogo
- **THEN** se liberan las referencias del resolver y las Blob URLs que ya no se
  usan
- **AND** una resolución tardía no vuelve a insertar una URL en un catálogo
  descartado

### Requirement: El cambio visual no altera la privacidad

El orden y los iconos del selector MUST ser cambios de presentación. La
selección de una candidata MUST continuar enviando solo su identificador opaco,
y la blacklist MUST conservar su comportamiento de alta, baja, cancelación,
persistencia y bloqueo de capturas.

#### Scenario: Una aplicación blacklisteada sigue sin generar capturas

- **GIVEN** una aplicación seleccionada desde el selector y agregada a la
  blacklist
- **WHEN** esa aplicación vuelve a estar activa y se produce un evento de
  captura
- **THEN** no se agrega una nueva captura
- **AND** ordenar la lista o mostrar su icono no modifica esta decisión

#### Scenario: La selección no transporta contenido de la aplicación

- **GIVEN** una persona selecciona una candidata con nombre e icono visibles
- **WHEN** confirma el alta en la blacklist
- **THEN** el payload contiene únicamente el identificador opaco requerido por
  el contrato existente
- **AND** no contiene nombre, contenido, hash, ruta, bytes de imagen ni
  referencias adicionales
