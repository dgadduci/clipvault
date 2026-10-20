# Proposal: toolbar filter visual states

## Why

The source-application filter takes more horizontal space than needed, and the
toolbar does not make selected filters easy to distinguish from their neutral
state. Clear, category-specific active colors will make the applied filters
visible at a glance while preserving the existing filtering behavior.

## What Changes

- Reduce the source-application filter width from 220 px to 187 px (15%) and
  its minimum width from 11rem to 9.35rem.
- Give active source-application, tag, collection and capture-type filters
  distinct blue, violet, teal and amber backgrounds with white text, all using
  the same 0.18 alpha as collection-row tints.
- Keep unselected filters and the «Todas» state neutral.

## Non-goals

- Change filter options, query behavior, selection/reset behavior or toolbar
  order.
- Change other filter widths, add dependencies or modify the card rail.

## Capabilities

- `desktop-shell-layout`: source-application filter width.
- `desktop-filtering`: active visual states for the four filters.
