# Design: toolbar filter visual states

## Layout

The source-application filter currently uses a 220 px fixed width and an 11rem
minimum. Multiplying each dimension by 0.85 yields a 187 px fixed width and a
9.35rem minimum. The tag, collection and capture-type filter widths and the
toolbar order remain unchanged.

## Active filter colors

Only the selected control receives a tinted background. The palette uses
familiar interface color semantics and separate hues for each filter:

| Filter | Surface | Meaning |
| --- | --- | --- |
| Source application | Blue `rgba(37, 99, 235, 0.18)` | Application/source |
| Tag | Violet `rgba(109, 40, 217, 0.18)` | Label/category |
| Collection | Teal `rgba(15, 118, 110, 0.18)` | Organization/scope |
| Capture type | Amber `rgba(180, 83, 9, 0.18)` | Content type |

Every filter surface SHALL use the same alpha as collection-row tints,
`COLLECTION_ROW_TINT_OPACITY` (currently `0.18`), so both parts of the
organization UI remain visually consistent if the shared opacity changes.

Each active control uses white text and white icon color where the icon follows
the text color. The surfaces remain translucent over the existing dark toolbar
surface. «Todas» is the neutral state, as is the unselected source-app filter.
The menus retain their existing appearance.

## Implementation boundaries

This is a presentation-only frontend change. Existing selected values are the
source of truth for the active-state attributes; no new state, localization
strings, dependencies, persistence or backend behavior is required.
