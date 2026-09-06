# Tasks: quick-paste-compact-ui

## 1. Baseline and contract

- [x] 1.1 Read the existing `quick-paste` spec, `QuickPaste.svelte`, bridge,
  controller, Tauri window configuration and current tests.
- [x] 1.2 Confirm that the implementation extends the existing Quick Paste
  window and does not create a second hotkey, listener or paste flow.
- [x] 1.3 Record the current frontend and Rust test baselines before editing.

## 2. Fixed transient window

- [x] 2.1 Change the existing quick-paste window to fixed `720 × 520` logical
  pixels, non-resizable and hidden at startup.
- [x] 2.2 Preserve decorations, always-on-top and taskbar/dock behavior from
  the existing contract.
- [x] 2.3 Center the window on activation using the existing Tauri window API,
  with a safe primary-display fallback.
- [x] 2.4 Preserve the activation order `captureActiveApp → show → focus →
  emitOpened`.

## 3. Compact result surface

- [x] 3.1 Remove the large desktop-style heading or reduce it to a minimal
  non-layout-shifting identity.
- [x] 3.2 Make the search field the primary header and autofocus it after the
  opened signal.
- [x] 3.3 Implement one vertical internally scrollable result container with
  no horizontal overflow.
- [x] 3.4 Implement a fixed-height item with stable two-line columns and
  fixed-size type/source-app areas.
- [x] 3.5 Apply compact typography, truncation and stable hover/selection/focus
  states.
- [x] 3.6 Render text previews and image thumbnails without changing item
  dimensions during asynchronous asset loading.
- [x] 3.7 Preserve accessible names, focus indicators, empty states and
  loading/error states.

## 4. Regression coverage

- [x] 4.1 Add tests for fixed window dimensions, non-resizable behavior and
  centered activation.
- [x] 4.2 Add tests for vertical internal scrolling and no horizontal overflow.
- [x] 4.3 Add tests for fixed-height two-line text and image items, truncation
  and stable image placeholders.
- [x] 4.4 Add tests for autofocus, empty history, no results and search errors.
- [x] 4.5 Add regression tests ensuring image asset references and blob URL
  lifecycle remain unchanged.
- [x] 4.6 Add regression tests for keyboard navigation, Escape, paste flow,
  idempotent listeners and privacy.

## 5. Verification

- [x] 5.1 Run `cargo fmt --all -- --check`.
- [x] 5.2 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.3 Run `cargo test --workspace`.
- [x] 5.4 Run `npm run check`, `npm run build` and `npm test` in
  `app/tauri/frontend`.
- [x] 5.5 Run `openspec validate quick-paste-compact-ui --strict --type change`.
- [ ] 5.6 Report automated results and leave the manual macOS/Linux visual
  verification explicitly pending until the user performs it.
- [ ] 5.7 Do not archive this change automatically.
