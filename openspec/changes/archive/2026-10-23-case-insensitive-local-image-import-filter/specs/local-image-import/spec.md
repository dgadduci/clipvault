## ADDED Requirements

### Requirement: Image picker accepts uppercase and lowercase extensions

The native image import picker SHALL display files whose extension uses either
lowercase or uppercase spelling for every supported format: PNG, JPG, JPEG,
WebP, GIF, BMP, TIF and TIFF. Supporting an uppercase extension in the picker
SHALL NOT expand the set of image formats accepted by the decoder.

#### Scenario: Select an image with an uppercase extension

- **WHEN** the user opens the image import picker in a directory containing a
  supported image whose extension is uppercase, such as `.PNG` or `.TIFF`
- **THEN** the file is visible and can be selected
- **AND** it follows the existing local image decoding and import flow

#### Scenario: Lowercase extensions remain available

- **WHEN** the user opens the image import picker in a directory containing a
  supported image whose extension is lowercase
- **THEN** the file remains visible and selectable

#### Scenario: Unsupported image formats remain filtered out

- **WHEN** the user opens the image import picker in a directory containing a
  format outside the existing supported set
- **THEN** adding uppercase variants does not make that format selectable
