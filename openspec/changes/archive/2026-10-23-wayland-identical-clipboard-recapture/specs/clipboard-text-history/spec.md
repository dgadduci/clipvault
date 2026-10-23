## MODIFIED Requirements

### Requirement: Prevent duplicate history entries

ClipVault SHALL use a metadata-only clipboard revision counter combined with
a content hash to prevent redundant history rows. Linux Wayland SHALL select
the revision source that corresponds to the clipboard transport actually
used: Wayland data-control when available, or X11/XWayland ownership events
when `arboard` falls back to X11. When no matching reliable source is
available, ClipVault SHALL preserve the `UNKNOWN` fallback and SHALL NOT
manufacture a revision from payload changes. The shared watcher treats a poll
as a fresh observation only when the platform revision differs from the
previous observation. Persistence deduplicates identical text against live
history rows only; once a row is removed, the same text at a new revision is
stored with a fresh opaque identifier.

#### Scenario: Deleted text is recaptured after an identical clipboard copy

- **GIVEN** a text capture with hash `H` existed at revision `R1` and was
  deleted
- **AND** the watcher baselined against `R1` while the clipboard still held
  `H`
- **WHEN** the user copies the same text again and the active clipboard
  transport reports revision `R2` (R2 > R1)
- **THEN** the watcher observes `R2` and persistence creates a new text row
  with a new opaque identifier
- **AND** a poll that still sees `H` at `R1` does not recreate the deleted row

#### Scenario: Identical content is copied again

- **WHEN** text with a hash already present in history is copied again and
  the clipboard reports a new revision
- **THEN** ClipVault does not create a second identical row and updates the
  existing entry's latest-seen metadata according to the duplicate policy

#### Scenario: Different content is copied

- **WHEN** the clipboard content differs and produces a different hash at a
  new revision
- **THEN** ClipVault creates a new history entry without modifying the prior
  entry's content

#### Scenario: Clipboard remains unchanged between polls

- **WHEN** the clipboard reports the same revision across consecutive polls
- **THEN** the watcher returns `Unchanged` without persisting another row

#### Scenario: Wayland X11 fallback uses its ownership stream

- **GIVEN** the application runs in a Wayland session without a supported
  data-control protocol and `arboard` uses X11/XWayland
- **WHEN** a new clipboard selection is published, including one with the
  same text as the deleted entry
- **THEN** ClipVault uses the XFixes ownership revision from that X11/XWayland
  transport to evaluate the observation
- **AND** it does not leave the revision as `UNKNOWN` merely because the
  desktop session is Wayland

#### Scenario: Wayland revision source cannot be verified

- **WHEN** the data-control monitor fails unexpectedly and the active
  transport cannot be identified reliably
- **THEN** ClipVault reports `UNKNOWN` and preserves the watcher's state
- **AND** it does not manufacture a new revision from the text or its hash
