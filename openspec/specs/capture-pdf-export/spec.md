# capture-pdf-export Specification

## Purpose
Permitir exportar una selección local de capturas a un único archivo PDF
mediante una operación local explícita.

## Requirements

### Requirement: Generate one PDF from an ordered selection

El sistema MUST generar exactamente un documento PDF por acción de exportación
con una o más capturas seleccionadas, tanto desde Desktop como desde
QuickPaste. MUST respetar el orden de selección definido por la lista ordenada
de IDs y MUST mantener la selección intacta al finalizar o cancelar.

#### Scenario: Export one capture

- **WHEN** el usuario activa PDF con una captura seleccionada
- **THEN** el sistema genera un único PDF que contiene esa captura
- **AND** no crea ni persiste otros PDFs

#### Scenario: Export several captures in selection order

- **WHEN** el usuario activa PDF con varias capturas seleccionadas
- **THEN** el sistema genera un solo PDF que contiene todas las capturas
- **AND** las presenta en el orden de selección del usuario

#### Scenario: Empty selection

- **WHEN** no hay capturas seleccionadas
- **THEN** el botón PDF permanece deshabilitado y el core rechaza cualquier
  solicitud vacía

### Requirement: Include supported capture content

El PDF MUST incluir el texto de capturas textuales y el texto plano de
capturas rich text, conservando sus saltos de línea. MUST incluir capturas de
imagen PNG en su proporción original, ajustadas al área imprimible. Las
capturas rich text originales MUST permanecer sin cambios.

#### Scenario: Export text and rich text

- **WHEN** la selección contiene capturas de texto o rich text
- **THEN** el PDF muestra su contenido como texto legible
- **AND** mantiene los saltos de línea del texto plano de cada captura
- **AND** no modifica ni convierte de forma persistente la captura original

#### Scenario: Export images

- **WHEN** la selección contiene una captura de imagen válida
- **THEN** el PDF incluye esa imagen sin recortarla ni cambiar su proporción

#### Scenario: Invalid or missing selection content

- **WHEN** falta un ID seleccionado o la imagen referenciada no puede
  validarse o leerse
- **THEN** la generación falla antes de mostrar el diálogo de guardado
- **AND** no se escribe un PDF parcial

### Requirement: Configure page breaks between captures

Configuración general MUST permitir elegir si cada captura comienza en una
página nueva. El ajuste MUST persistirse localmente y su valor predeterminado
MUST ser activado. Con el ajuste desactivado, el contenido continúa en el
espacio disponible y las páginas solo se agregan por paginación natural.

#### Scenario: Default one capture per page

- **WHEN** la preferencia no existe en una instalación nueva o antigua
- **THEN** cada captura empieza en una página nueva
- **AND** el contenido de una captura que excede la página puede continuar en
  páginas adicionales

#### Scenario: Continuous flow between captures

- **WHEN** el usuario desactiva la preferencia
- **THEN** la siguiente captura continúa después de la anterior en el espacio
  restante
- **AND** una página nueva solo comienza cuando el contenido ya no cabe

### Requirement: Save the PDF to a user-selected local path

Tras completar el documento, el sistema MUST abrir un único diálogo nativo de
guardado con extensión `.pdf` y un nombre sugerido. Al confirmar, MUST escribir
el PDF completo como un solo archivo en el destino escogido. Al cancelar, MUST
dejar el sistema de archivos sin cambios. ClipVault MUST NOT conservar una
copia del archivo exportado.

#### Scenario: Save one generated PDF

- **WHEN** el usuario elige una ruta y confirma el diálogo
- **THEN** el sistema guarda un único archivo `.pdf` con el documento generado

#### Scenario: Cancel save dialog

- **WHEN** el usuario cancela o cierra el diálogo
- **THEN** no se crea ni se modifica ningún archivo
- **AND** la cancelación no se presenta como error

### Requirement: Expose export on both local selection surfaces

Desktop MUST mostrar el botón PDF entre el menú de acciones y el botón de
borrado. QuickPaste MUST mostrarlo junto al indicador de selección múltiple.
Ambos MUST estar habilitados solo cuando hay una o más capturas seleccionadas,
deshabilitar disparos repetidos durante la generación y exponer etiquetas
accesibles traducidas.

#### Scenario: Export from Desktop

- **WHEN** hay una o más capturas locales seleccionadas en Desktop
- **THEN** el botón PDF está habilitado entre el menú y el botón de borrado
- **AND** al activarlo exporta esa selección sin limpiarla

#### Scenario: Export from QuickPaste

- **WHEN** hay una o más capturas seleccionadas en QuickPaste
- **THEN** el botón PDF aparece junto al indicador de selección
- **AND** activarlo inicia la exportación y no confirma un paste stack

#### Scenario: Export in progress

- **WHEN** una ventana ya está generando o guardando una exportación
- **THEN** su botón PDF no permite iniciar una segunda exportación

### Requirement: Keep export local and private

La generación y el guardado MUST permanecer en el equipo. El sistema MUST NOT
registrar contenido de capturas, bytes de imágenes, rutas elegidas ni nombres
de archivos ni enviar información a servicios remotos. Las capturas ya
importadas al historial local son elegibles; la superficie de historial remoto
de equipos MUST NOT exponer esta acción.

#### Scenario: Local export

- **WHEN** una selección local se exporta
- **THEN** el contenido y el archivo permanecen en el flujo local de la
  aplicación y del sistema de archivos elegido
- **AND** no se envía información a servicios remotos

### Requirement: Preserve image proportions and selection identity

PDF export MUST preserve the original aspect ratio of each image and fit it
inside the printable page area. When captures start on separate pages, each
page MUST show the image belonging to that capture's position in the user's
selection order.

#### Scenario: Fit landscape image to the printable width

- **WHEN** a landscape image is exported
- **THEN** it uses the available printable width when that is the limiting
  dimension
- **AND** its height is scaled by the same factor as its width

#### Scenario: Fit portrait image to the printable height

- **WHEN** a portrait image is exported
- **THEN** it uses the available printable height when that is the limiting
  dimension
- **AND** its width is scaled by the same factor as its height
- **AND** the image remains within the printable width

#### Scenario: Export multiple images on separate pages

- **WHEN** the one-capture-per-page setting is enabled for multiple selected
  images
- **THEN** every page shows the image for its corresponding selected capture
- **AND** the pages follow the user's selection order

### Requirement: Show a red PDF text label for export

The PDF export action on Desktop and QuickPaste MUST show the visible text
`PDF` in red instead of a document icon. It MUST retain its existing
semi-transparent red background, localized accessible action name, placement,
selection gating and export behavior.

#### Scenario: Desktop export action uses a red text label

- **WHEN** the Desktop toolbar shows the PDF export action
- **THEN** its visible content is the text `PDF` in red
- **AND** it retains the full localized accessible name and red-tinted
  semi-transparent background

#### Scenario: QuickPaste export action uses a red text label

- **WHEN** QuickPaste shows the PDF export action for a selection
- **THEN** its visible content is the text `PDF` in red
- **AND** activating it still exports the selection without confirming a paste
