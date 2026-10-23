# Design: Explicit foreground colors for the card menu

The menu is a fixed-position descendant of `HistoryCard` and currently lets its
text color inherit from surrounding application styles. Set the ellipsis
trigger's foreground explicitly to the card's existing light text color, and
set the menu surface's foreground to that same light color. Menu items already
inherit their foreground from the menu, so their normal labels become readable
without changing menu structure or behavior.

Keep existing overrides for disabled items, shortcut hints, and the destructive
delete action. This confines the correction to default readable text and the
trigger glyph.

Add a focused frontend source regression for the trigger and popover colors.
Run the frontend checks, the card drag-and-drop regressions required for
changes to `HistoryCard`, strict OpenSpec validation, and `git diff --check`.
