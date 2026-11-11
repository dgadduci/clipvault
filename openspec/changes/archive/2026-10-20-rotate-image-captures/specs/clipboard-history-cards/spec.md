## MODIFIED Requirements

### Requirement: Preview capture from Desktop cards

Every Desktop history card SHALL expose a `Previsualizar` action in its
existing menu and SHALL support the same platform-aware shortcut used by Quick
Paste: `Cmd+Enter` on macOS and `Ctrl+Enter` on Linux when the card is focused.
Both entry points SHALL open the same shared preview implementation used by
Quick Paste. The preview SHALL keep text and rich-text content read-only and
MAY persist an explicit image rotation requested through its image controls.

#### Scenario: Preview text from the card menu

- **WHEN** the user opens a text card menu and chooses `Previsualizar`
- **THEN** the Desktop opens the shared preview overlay for that exact entry,
  closes the card menu once and leaves the clipboard, history and card
  metadata unchanged

#### Scenario: Preview image from the card menu

- **WHEN** the user opens an image card menu and chooses `Previsualizar`
- **THEN** the Desktop opens the shared image preview and displays the complete
  persisted image inside the bounded preview area without changing the
  clipboard
- **AND** the user can explicitly rotate the displayed image in either
  direction and persist that rotation for the same capture

#### Scenario: Preview with the platform shortcut

- **WHEN** a Desktop card has focus and the user presses `Cmd+Enter` on macOS
  or `Ctrl+Enter` on Linux
- **THEN** the shared preview opens for that card without invoking copy, paste,
  pin, delete, title editing or drag-and-drop

#### Scenario: Invalid shortcut context is ignored

- **WHEN** the shortcut is pressed with no focused card or while focus is in a
  text editor, input, menu, button or other interactive control
- **THEN** no preview opens and the active control keeps its normal behavior

### Requirement: Shared preview behavior and presentation

Desktop and Quick Paste SHALL render text, rich text and image previews through
one shared component/helper path. The shared preview SHALL preserve full safe
text, the existing rich-preview policy, validated image assets, loading/error
states, bounded scrolling and consistent type/title/source metadata. Text and
rich-text previews remain read-only. Image previews SHALL expose persistent
rotation controls for exactly 90° clockwise and counterclockwise turns.

#### Scenario: Full text is not truncated

- **WHEN** a captured text entry is longer than the card or row summary and
  the user opens its preview
- **THEN** the preview shows the complete canonical text with internal scroll
  and never uses the truncated card/row summary as its source

#### Scenario: Text preview is safe

- **WHEN** the captured text contains HTML-active characters or markup-like
  content
- **THEN** the preview displays it as inert text according to the existing
  escaping policy and executes no script, handler or navigation

#### Scenario: Image rotation controls are available

- **WHEN** the shared preview displays an image capture in Desktop or Quick
  Paste
- **THEN** it shows separate accessible controls with icons for clockwise and
  counterclockwise rotation
- **AND** activating either control rotates the image by exactly 90° in the
  indicated direction and updates the preview after persistence succeeds
- **AND** text and rich-text previews do not show these controls

#### Scenario: Rotation shows a spinner until the refreshed image is ready

- **WHEN** the user rotates an image capture
- **THEN** the shared preview shows an accessible spinning indicator while the
  rotation is being saved and while the resulting PNG is being loaded
- **AND** it does not show the initial “Loading image…” text during that
  rotation and reload
- **AND** the spinner stops when the refreshed image is displayed or an error
  fallback is shown

#### Scenario: Image rotation persists on the same capture

- **WHEN** a user rotates an image from its preview
- **THEN** ClipVault stores the rotated pixels as a PNG asset and updates that
  entry's asset reference, content hash, byte size and dimensions
- **AND** the entry ID, title, favorite, note, tags, collection memberships,
  capture timestamps and source metadata remain unchanged
- **AND** the rotation remains visible after closing and reopening the preview
  or restarting ClipVault

#### Scenario: Rotation updates dimensions and retains other assets

- **WHEN** an image is rotated by 90° in either direction
- **THEN** its persisted width and height are swapped while the full image and
  its proportions are preserved
- **AND** any previous asset still referenced by another entry remains
  available and unchanged

#### Scenario: Rotation failure leaves the entry unchanged

- **WHEN** the image is missing, invalid, the entry was removed or persistence
  fails while a rotation is requested, including a content-hash collision with
  another capture
- **THEN** the current capture stays unchanged and the preview shows a
  localized error while remaining usable

#### Scenario: Image preview preserves the asset

- **WHEN** the preview loads a persisted image asset
- **THEN** it uses the existing validated asset bridge, shows the full image
  with contain-style layout, preserves intrinsic dimensions and releases its
  Blob URL when the preview closes or changes entry

#### Scenario: Preview asset failure is isolated

- **WHEN** the text or image preview asset is unavailable, invalid or resolves
  after the entry changed
- **THEN** the preview shows the existing safe fallback or loading/error state,
  ignores stale responses and leaves other cards and Quick Paste usable

#### Scenario: Shared implementation is actually reused

- **WHEN** Desktop and Quick Paste open previews
- **THEN** both surfaces call the same preview component/helpers and there is
  no duplicate renderer, sanitizer, rotation path or asset lifecycle

### Requirement: Accessible preview lifecycle

The Desktop preview SHALL behave as a bounded accessible overlay without
changing the fixed geometry of the application. Rotation controls SHALL have
localized accessible names and remain keyboard operable.

#### Scenario: Escape closes the preview first

- **WHEN** the Desktop preview is open and the user presses Escape
- **THEN** the preview closes first, the underlying Desktop remains available,
  and focus returns to the triggering card/control when possible

#### Scenario: Outside click closes the preview

- **WHEN** the user clicks outside the preview surface
- **THEN** the preview closes without performing an additional mutation; a
  click inside the preview does not close it accidentally

#### Scenario: Repeated open and close is leak-free

- **WHEN** the user opens and closes previews repeatedly or changes the active
  collection while one is open
- **THEN** only one preview/listener set remains active, stale entries are not
  rendered, and all owned Blob URLs/listeners are cleaned up
