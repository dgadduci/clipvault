## ADDED Requirements

### Requirement: Capture consecutive clipboard images in Ubuntu Wayland

ClipVault SHALL continue observing supported clipboard image updates for the
whole active session on Ubuntu Wayland. A successful image capture SHALL NOT
disable or strand the observer or capture pipeline. New image content SHALL
pass through the existing image capture and persistence pipeline, while
identical content SHALL continue to follow the existing duplicate policy.
Transient image read or conversion failures SHALL NOT prevent a later
supported image from being captured. Clipboard image content SHALL remain
local and SHALL NOT be included in logs, errors or diagnostics.

#### Scenario: Several distinct images are captured in one session

- **WHEN** ClipVault remains open in an Ubuntu Wayland session
- **AND** the clipboard publishes several supported raster images one after
  another, with no higher-priority non-empty text representation
- **AND** each image has content distinct from the previously captured image
- **THEN** each image is stored as a separate history entry with its own valid
  local asset
- **AND** earlier entries and assets remain unchanged

#### Scenario: The same image is captured again

- **WHEN** the clipboard publishes an image whose persisted content is
  identical to an existing image entry
- **THEN** ClipVault applies the existing image duplicate policy
- **AND** it does not create a second history row or asset for that content

#### Scenario: A transient image read failure is followed by a valid image

- **WHEN** reading or converting one clipboard image fails transiently
- **AND** the clipboard later publishes a supported image while ClipVault is
  still open
- **THEN** the capture observer remains usable
- **AND** the later image is stored as a valid history entry
- **AND** the failure does not expose clipboard image content in logs, errors
  or diagnostics
