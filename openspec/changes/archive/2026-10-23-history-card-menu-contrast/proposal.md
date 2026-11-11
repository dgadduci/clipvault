# Proposal: Restore readable colors in the history card menu

## Why

The ellipsis trigger and menu labels on capture cards can inherit the WebView's
default black text color. That makes the controls hard to read against the
dark card and popover surfaces.

## What Changes

- Set an explicit light foreground color on each capture card's ellipsis
  trigger and dark menu surface.
- Keep the existing visual colors for disabled and destructive actions.

## Non-Goals

- Change menu actions, behavior, positioning, keyboard navigation, or card
  layout.
- Change colors for the desktop toolbar or collection menus.
