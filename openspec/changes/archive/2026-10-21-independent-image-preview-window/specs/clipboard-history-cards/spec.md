## ADDED Requirements

### Requirement: Independent image preview window

The shared Desktop and Quick Paste image preview SHALL open in its own
resizable top-level native window, independent of the size of either source
window. The image SHALL render directly in the native preview window without a
nested modal, dialog overlay, or inner backdrop. Rotation controls SHALL be
part of the preview window's own content. The preview window SHALL
initially fit the image to the available monitor work area while preserving its
aspect ratio, and resizing the native window SHALL automatically refit the full
image to the available content area without cropping or distortion, including
upscaling beyond the image's intrinsic dimensions when the available area is
larger. The native window content SHALL NOT show an additional close button;
the user SHALL close the preview with the native window close control or
`Escape`. Each close path SHALL close the preview window. Opening another
image SHALL replace the current image preview. Closing the preview SHALL return
focus to the source surface; if Quick Paste opened it, Quick Paste SHALL be
shown again with its query and selection intact. The preview SHALL NOT provide
an inner drag handle for manually resizing the image.
Text and rich-text previews SHALL keep their existing in-window overlay.

#### Scenario: Image preview opens independently from Desktop

- **WHEN** the user opens an image preview from Desktop
- **THEN** ClipVault opens a separate resizable native window for that image
- **AND** the preview window's size does not depend on the Desktop window size
- **AND** resizing the preview window does not resize the Desktop window
- **AND** the image remains fully visible with its proportions preserved

#### Scenario: Image preview opens independently from QuickPaste

- **WHEN** the user opens an image preview from QuickPaste
- **THEN** ClipVault opens a separate resizable native window for that image
- **AND** QuickPaste keeps its configured fixed geometry
- **AND** closing the preview restores QuickPaste with its prior query and
  selection

#### Scenario: Preview size fits the monitor and image orientation

- **WHEN** an image preview opens on a monitor with a known work area
- **THEN** the initial window size is derived from the image dimensions and
  constrained to the available work area
- **AND** horizontal, vertical and square images preserve their aspect ratio
- **AND** resizing the preview window refits the image without cropping

#### Scenario: Image renders directly in the native window

- **WHEN** an image preview opens in its native window
- **THEN** the image is rendered directly in that window's content area
- **AND** no nested modal, dialog overlay, or backdrop is present
- **AND** clockwise and counterclockwise rotation controls are available in
  that window
- **AND** the image automatically fits the available content area with its
  aspect ratio preserved
- **AND** no inner drag handle is available to resize the image

#### Scenario: Native preview can enlarge an image beyond its original dimensions

- **WHEN** the user enlarges the native preview window beyond the image's
  intrinsic dimensions
- **THEN** the image expands to use the available area
- **AND** its original aspect ratio is preserved

#### Scenario: Opening another image replaces the active preview

- **WHEN** an image preview window is already open and the user opens a
  different image preview
- **THEN** the existing preview window is replaced with the newly requested
  capture
- **AND** only one image preview window remains open

#### Scenario: Preview closes through native close or Escape

- **WHEN** the user activates the native close control or presses `Escape`
- **THEN** the preview window closes and focus returns to its source surface
- **AND** no additional close button is shown inside the preview content
- **WHEN** the user clicks outside the preview window
- **THEN** the preview remains open

#### Scenario: Native window close is honored

- **WHEN** the user activates the operating system's native close control
- **THEN** the image-preview window closes
- **AND** focus returns to its source surface

#### Scenario: The requested capture was deleted

- **WHEN** the preview window requests a capture that no longer exists
- **THEN** it shows a localized unavailable message
- **AND** the user can close the window with the native close control or
  `Escape`
