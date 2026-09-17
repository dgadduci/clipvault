// Pure helpers for the inline collection chip row the
// `clipboard-history-cards` change renders under each card's tag
// chips.
//
// The helpers stay pure and framework-free so they can be unit
// tested without mounting a Svelte component. The card surface
// imports them through this module; the test build transpiles the
// file alongside the rest of the helpers so a regression in the
// visible-subset computation surfaces as a failed assertion instead
// of a runtime crash.

/**
 * Pixel reservation the responsive chip row keeps free for the
 * overflow chip when no measurement is available. The constant
 * mirrors the documented worst case the `.collection-overflow`
 * rule declares so the helper still produces a deterministic
 * subset on the first render before the measurement strip has
 * reported its real width.
 */
export const COLLECTION_OVERFLOW_BUTTON_PX = 32;

/**
 * Inter-chip gap the measurement strip and the painted row share.
 */
export const COLLECTION_CHIP_GAP_PX = 4;

export interface CollectionLike {
  id: number;
  name: string;
}

/**
 * Compute the intrinsic width the inline chip row would occupy if
 * the card painted every user collection followed by the `+N`
 * overflow chip without any clipping. The total feeds the
 * overflow predicate so the row can decide whether the full set
 * fits inside the card width before it ever paints a trimmed
 * subset.
 *
 * The computation is purely arithmetic and never inspects the
 * painted DOM, so a paint pass that already trimmed the chip
 * subset cannot bias the result. The intrinsic total compares
 * against the *available* card width (the painted row's
 * `clientWidth`) — a column painted from the full set, the
 * `COLLECTION_CHIP_GAP_PX` gaps between chips, the gap before
 * the overflow chip and the overflow chip's measured width
 * itself.
 *
 * When the measurement strip has not yet reported the overflow
 * chip width (`overflowChipWidth <= 0`) the helper falls back to
 * {@link COLLECTION_OVERFLOW_BUTTON_PX} so the first render keeps
 * a deterministic comparison and the row still converges.
 */
export function intrinsicCollectionRowWidth(
  chipWidths: readonly number[],
  overflowChipWidth: number,
): number {
  if (chipWidths.length === 0) return 0;
  const chipsTotal = chipWidths.reduce((acc, width) => acc + width, 0);
  const gaps = (chipWidths.length - 1) * COLLECTION_CHIP_GAP_PX;
  const chipReservation =
    overflowChipWidth > 0
      ? overflowChipWidth
      : COLLECTION_OVERFLOW_BUTTON_PX;
  // The overflow chip is preceded by a single gap (it never
  // touches the first chip without one) so the helper reserves
  // both the gap and the chip itself.
  return chipsTotal + gaps + COLLECTION_CHIP_GAP_PX + chipReservation;
}

/**
 * Pixel overflow tolerance. The `+1` slack keeps the predicate
 * stable across sub-pixel rounding errors from `getBoundingClientRect`
 * and CSS rounding so a card whose intrinsic width exactly matches
 * the available width keeps the all-fit branch instead of flickering
 * into the overflow branch.
 */
export const COLLECTION_OVERFLOW_TOLERANCE_PX = 1;

/**
 * Compute the visible chip subset. The helper accepts the full
 * user-collection set, the cached per-chip intrinsic widths (in CSS
 * pixels), the cached row `clientWidth`, the current overflow flag
 * and the measured width of the overflow chip itself (in CSS
 * pixels). It returns the subset that the card should paint.
 *
 * The algorithm:
 *
 *   - if no overflow is detected, return every chip and the
 *     chip stays hidden;
 *   - if overflow is detected, iterate the chips in order,
 *     accumulating widths and gaps, until adding the next chip
 *     plus the chip reservation would clip the row;
 *   - the reservation accounts for the gap before the chip and
 *     the chip's measured intrinsic width so the last visible chip
 *     never overlaps the overflow indicator.
 *
 * When the caller does not yet have a measurement for the overflow
 * chip (e.g. the first render before the `ResizeObserver` has run)
 * the helper falls back to `COLLECTION_OVERFLOW_BUTTON_PX` so the
 * computation stays deterministic. When the row is narrower than a
 * single chip plus the chip reservation the helper falls back to
 * the first chip so the user always sees at least one membership
 * through the inline row instead of the overflow indicator alone.
 */
export function computeVisibleCollections<C extends CollectionLike>(
  collections: readonly C[],
  widths: readonly number[],
  rowWidth: number,
  overflow: boolean,
  overflowButtonWidth: number = COLLECTION_OVERFLOW_BUTTON_PX,
): C[] {
  if (collections.length === 0) return [...collections];
  if (!overflow) return [...collections];
  if (rowWidth <= 0) return [collections[0]];
  const reservation =
    overflowButtonWidth > 0
      ? overflowButtonWidth
      : COLLECTION_OVERFLOW_BUTTON_PX;
  const available = Math.max(0, rowWidth - reservation);
  const visible: C[] = [];
  let total = 0;
  for (let i = 0; i < collections.length; i += 1) {
    const width = widths[i] ?? 0;
    const gap = visible.length > 0 ? COLLECTION_CHIP_GAP_PX : 0;
    if (total + width + gap > available) break;
    visible.push(collections[i]);
    total += width + gap;
  }
  if (visible.length === 0) {
    visible.push(collections[0]);
  }
  return visible;
}

/**
 * Number of user collections the overflow chip would hide at the
 * current visible subset. The count is the difference between the
 * input set and the visible subset so the rendered chip text
 * (`+N`) is the canonical contract the membership modal relies on
 * to advertise the hidden memberships.
 *
 * The helper never returns a negative count: a defensive `Math.max`
 * clamps the result to zero so a stale or partial layout cannot
 * leak a phantom overflow value into the chip.
 */
export function computeOverflowCount<C extends CollectionLike>(
  collections: readonly C[],
  visible: readonly C[],
): number {
  return Math.max(0, collections.length - visible.length);
}