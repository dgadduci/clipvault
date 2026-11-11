## 1. OpenSpec and dependency

- [x] 1.1 Record import formats, local-only handling, active collection
  membership, PNG persistence, limits, duplicate behavior and unsupported-file
  handling in the change artifacts.
- [x] 1.2 Add the pinned image decoder dependency with only the required codecs.

## 2. Core import pipeline

- [x] 2.1 Decode supported file formats by signature, apply image orientation,
  enforce the existing image dimension and allocation limits, and convert to
  the platform-neutral RGBA type.
- [x] 2.2 Reuse the current image persistence and deduplication path, and add
  imported outcomes to the active local collection when present.

## 3. Desktop and Tauri integration

- [x] 3.1 Add a native multi-file picker command that keeps file paths and image
  bytes inside Rust and continues after per-file failures.
- [x] 3.2 Add the photo icon button between PDF and delete, plus localized
  labels, pending state and aggregate result for `en`, `es`, `pt`, `de`, `fr`.
- [x] 3.3 Refresh Desktop entries and organization counts after successful or
  duplicate imports.

## 4. Confirmable multi-file order

- [x] 4.1 Update the native picker flow to retain selected paths in transient
  Rust state and return only an opaque session, per-file identifiers and
  basename labels.
- [x] 4.2 Add an accessible localized order-review modal with move controls,
  confirm and cancel actions for all five languages.
- [x] 4.3 Validate that the submitted order is an exact permutation, consume or
  discard pending paths, and persist the batch so new captures appear in the
  confirmed order in History.
- [x] 4.4 Refresh the history and organization after the confirmed batch and
  retain the aggregate import summary.

## 5. Review and verification

- [x] 5.1 Review supported formats, active-collection assignment, privacy and
  baseline toolbar ordering against this spec.
- [x] 5.2 Run applicable static checks, strict OpenSpec validation and
  `git diff --check`.
- [x] 5.3 Review the diff and confirm no source paths, image bytes or generated
  assets are exposed or committed.

## 6. Compact manual text-capture action

- [x] 6.1 Remove the visible text label and shortcut badge from the existing
  toolbar button while retaining its accessible label, tooltip, shortcut
  metadata and action.
- [x] 6.2 Update the existing toolbar layout regression expectation to require
  icon-only visible content.
- [x] 6.3 Record the user's manual approval of the multi-image import flow and
  rerun the frontend static check and strict OpenSpec validation.
