## ADDED Requirements

### Requirement: Image preview zoom changes only the image

The native image preview MUST apply touchpad zoom gestures and its zoom
controls only to the displayed image. The window size, image-preview toolbar,
rotation controls and their layout MUST remain unchanged and visible while the
image is zoomed. The webview MUST stay at its default zoom level, and the
touchpad events MUST be canceled before native webview zoom is applied. Desktop
and QuickPaste webviews MUST retain their default zoom scale and MUST ignore
browser-level zoom gestures. The
toolbar MUST show accessible zoom-in and zoom-out buttons
on the same line as the rotation controls. The initial image scale MUST be
`1×` relative to the current fit-to-window size; zoom MUST be bounded between
`0.25×` and `4×`. The zoom state MUST reset when another capture is opened and
MUST NOT persist after closing the preview.

The zoom buttons MUST become enabled when the image has loaded and no rotation
refresh is in progress. The desktop capture rail MUST remain horizontal, and
zooming an image MUST NOT change typography sizing in the collections list,
collection actions menu, linked teams list or toolbar filters. Collection
count badge numbers MUST use white text.

#### Scenario: Touchpad gesture zooms only the image

- **WHEN** the user pinches to zoom in or out in the native image preview
- **THEN** the image scales within the preview content area
- **AND** the native window and toolbar do not scale or change size
- **AND** both rotation controls remain visible and usable
- **AND** the desktop collection and capture layouts keep their default zoom

#### Scenario: Toolbar offers image zoom controls

- **WHEN** the native image preview displays an image
- **THEN** accessible zoom-in and zoom-out icon buttons appear on the same
  toolbar line as clockwise and counterclockwise rotation
- **AND** each zoom button changes only the image scale
- **AND** the relevant button is disabled at its minimum or maximum zoom bound
- **AND** both buttons are enabled after the image finishes loading

#### Scenario: Desktop visual scale remains unchanged

- **WHEN** an image is zoomed or the native image preview is closed
- **THEN** the desktop capture rail remains horizontal
- **AND** collection names, collection actions, linked teams and toolbar filters
  retain their established font sizes
- **AND** collection count badge numbers remain white

#### Scenario: Zoom begins at fit size and resets for another capture

- **WHEN** an image preview opens or is replaced with another capture
- **THEN** the image starts at its current fit-to-window size
- **AND** the zoom state is not restored from a prior capture or session

#### Scenario: Other preview surfaces keep their behavior

- **WHEN** the user previews text or rich text, or scrolls without a zoom
  gesture
- **THEN** this image-only zoom behavior does not intercept that interaction
