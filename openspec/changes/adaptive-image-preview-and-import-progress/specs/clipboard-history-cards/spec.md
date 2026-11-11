## ADDED Requirements

### Requirement: Adaptive image preview dimensions

The shared Desktop and Quick Paste image preview SHALL size its card to the
rendered image while preserving the image's aspect ratio. The preview SHALL
scale the image to at most 80% of its intrinsic dimensions and SHALL leave 10
CSS pixels of inner spacing on each side. It SHALL scale the image down
further when necessary to keep the full image and preview card within the
available viewport. The user SHALL be able to resize the image preview card
with a pointer and keyboard control. Resizing SHALL keep the card inside the
available viewport and fit the full image within the resized card without
distortion. The manually selected size SHALL apply only to the current preview.

#### Scenario: Image preview adapts to the image dimensions

- **WHEN** the shared preview displays a persisted image capture
- **THEN** the image is shown at up to 80% of its intrinsic width and height
  with its aspect ratio preserved
- **AND** the image preview card sizes to the displayed image instead of
  filling the available window width
- **AND** the image has 10 CSS pixels of inner spacing on each side
- **AND** the image is scaled down further when needed to fit within the
  available viewport while remaining fully visible
- **AND** resizing or rotating the image recalculates its displayed dimensions
- **AND** Quick Paste's native window geometry remains unchanged

#### Scenario: Image preview keeps the complete image visible

- **WHEN** a portrait, landscape or square image is previewed
- **THEN** its full contents remain visible without cropping or distortion
- **AND** the same sizing behavior is used by Desktop and Quick Paste

#### Scenario: User resizes the image preview

- **WHEN** the user drags the resize control or adjusts it with the keyboard
- **THEN** the preview card changes size within the available viewport
- **AND** the image scales to fit the card while preserving its proportions
- **AND** the selected size resets when another capture is previewed or the
  modal closes
- **AND** Quick Paste's native window remains fixed at its existing geometry

#### Scenario: Preview stays open after outside interaction

- **WHEN** the user clicks or releases a pointer on the overlay outside the
  preview card
- **THEN** the preview remains open
- **AND** the preview closes only when the user presses its close button or
  presses `Escape`
