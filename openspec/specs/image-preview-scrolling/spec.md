# image-preview-scrolling Specification

## Purpose
Define scrolling and zoom behavior for image captures opened in the standalone
native preview window.

## Requirements

### Requirement: Scroll oversized standalone image previews

The standalone image preview SHALL allow horizontal and vertical scrolling
when a zoomed image exceeds the visible image area. It SHALL preserve the
image's aspect ratio, keep scrolling inside the image area, and keep the native
window controls visible.

#### Scenario: Horizontally oversized image can be traversed

- **WHEN** the user zooms an image until its width exceeds the visible image
  area
- **THEN** the user can scroll horizontally to reach both sides of the image
- **AND** the preview window and its rotation and zoom controls remain fixed

#### Scenario: Vertically oversized image can be traversed

- **WHEN** the user zooms an image until its height exceeds the visible image
  area
- **THEN** the user can scroll vertically to reach the top and bottom of the
  image
- **AND** the image retains its aspect ratio

#### Scenario: Both dimensions exceed the visible area

- **WHEN** the zoomed image exceeds the visible area in both dimensions
- **THEN** horizontal and vertical scrolling can reach every part of the image
- **AND** ordinary wheel or touchpad scrolling moves the image while the
  existing modified-wheel and pinch gestures continue to control zoom

#### Scenario: Image fits inside the visible area

- **WHEN** the image fits inside the visible area
- **THEN** the image remains centered and no scroll range is required

### Requirement: Preserve embedded preview behavior

The standalone-window scrolling behavior SHALL NOT change the clipping or
layout of embedded image previews in cards or text previews in Desktop and
QuickPaste.

#### Scenario: Embedded image preview remains bounded

- **WHEN** an image is rendered inside an embedded preview
- **THEN** it retains its existing bounded layout and does not create a
  scrollable preview surface
