## MODIFIED Requirements

### Requirement: Start every capture on a new page

Every selected capture MUST start on a new PDF page, in selection order.
General Settings MUST NOT expose a preference for changing this behavior.
Content from a single capture MAY continue on additional pages when it exceeds
the available page area.

#### Scenario: Every capture starts on a new page

- **WHEN** the selected captures are exported to PDF on a new or older install
- **THEN** every capture starts on a page after the preceding capture
- **AND** unused space at the end of the preceding capture's last page is not
  used by the next capture

#### Scenario: One capture naturally spans multiple pages

- **WHEN** a selected capture's content exceeds one page
- **THEN** that capture continues onto additional pages as needed
- **AND** the following capture starts on a new page after it

#### Scenario: Ignore an obsolete continuous-flow preference

- **GIVEN** local settings contain an older page-break preference set to false
- **WHEN** the user exports selected captures
- **THEN** each capture still starts on a new page
- **AND** General Settings does not offer a control to restore continuous flow

## ADDED Requirements

### Requirement: Show progress while generating a PDF

When PDF generation starts from Desktop or QuickPaste, the initiating window
MUST show an accessible rotating spinner centered in that window. The spinner
MUST remain visible until PDF generation completes, then disappear before the
native save dialog is presented. The PDF button MUST remain disabled until the
save dialog is completed or cancelled, or the export fails. The completion
signal MUST be targeted to the initiating window and MUST NOT contain capture
content or file information.

#### Scenario: Desktop generation progress

- **WHEN** the user starts PDF export from Desktop
- **THEN** a localized accessible spinner appears centered over the Desktop
  window while the PDF is generated
- **AND** it disappears when generation completes, before the save dialog opens
- **AND** the Desktop PDF button remains disabled until the export action ends

#### Scenario: QuickPaste generation progress

- **WHEN** the user starts PDF export from QuickPaste
- **THEN** a localized accessible spinner appears centered over QuickPaste while
  the PDF is generated
- **AND** it disappears when generation completes, before the save dialog opens
- **AND** the QuickPaste PDF button remains disabled until the export action ends

#### Scenario: Generation fails or saving is cancelled

- **WHEN** generation fails or the user cancels the save dialog
- **THEN** the spinner and in-flight button state are cleared
- **AND** cancellation is not shown as an export error

#### Scenario: Progress event is private to the initiating window

- **WHEN** generation completes for an export started by one application window
- **THEN** only that window receives the payload-free completion signal
- **AND** the other window does not show or dismiss a spinner for that export
