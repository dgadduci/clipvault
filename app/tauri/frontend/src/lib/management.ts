// UI helpers for the `clipboard-management` capability. Each helper
// stays pure so it can be unit-tested without Tauri or the DOM.
//
// The helpers encapsulate the contract documented in
// `openspec/changes/clipboard-management/specs/clipboard-management/spec.md`:
//   * destructive actions require `confirm: true`;
//   * `confirmation_required` responses must surface a dialog and
//     never re-invoke the mutation;
//   * no clipboard content ever travels through the helper.

import type {
  ClearResponse,
  DeleteResponse,
  RetentionPreview,
  RetentionResponse,
  SetFavoriteResponse,
} from "../types.ts";

export interface FavoriteAction {
  id: number;
  pinned: boolean;
}

export interface DeleteAction {
  id: number;
}

export interface ClearAction {
  confirm: boolean;
}

export interface ManagementCommands {
  setFavorite: (action: FavoriteAction) => Promise<SetFavoriteResponse>;
  deleteEntry: (action: DeleteAction & { confirm: boolean }) => Promise<DeleteResponse>;
  clearHistory: (action: ClearAction) => Promise<ClearResponse>;
  applyRetention: () => Promise<RetentionResponse>;
  retentionPreview: () => Promise<RetentionPreview>;
}

export type ManagementOutcome<T> =
  | { kind: "applied"; value: T }
  | { kind: "confirmation_required"; next: { confirm: true } };

/**
 * Apply a destructive action with the right confirmation flag. The
 * helper centralises the rule that the UI must ask for confirmation
 * before mutating and must surface a `confirmation_required` outcome
 * when the backend rejects the call.
 */
export async function applyDestructive<Response, Action extends { confirm: boolean }>(
  request: (action: Action) => Promise<Response>,
  isConfirmationRequired: (response: Response) => boolean,
  action: Action,
): Promise<ManagementOutcome<Response>> {
  if (!action.confirm) {
    return {
      kind: "confirmation_required",
      next: { confirm: true },
    };
  }
  const response = await request(action);
  if (isConfirmationRequired(response)) {
    return {
      kind: "confirmation_required",
      next: { confirm: true },
    };
  }
  return { kind: "applied", value: response };
}

export function isDeleteConfirmationRequired(response: DeleteResponse): boolean {
  return response.kind === "confirmation_required";
}

export function isClearConfirmationRequired(response: ClearResponse): boolean {
  return response.kind === "confirmation_required";
}

/**
 * Format a retention preview for the UI without exposing the
 * underlying policy identifier: we keep the wording stable and
 * conservative so the frontend never has to map between the database
 * identifier and a user-visible string on its own.
 */
export function describeRetentionPreview(preview: RetentionPreview): string {
  switch (preview.policy) {
    case "forever":
      return "Retention is set to forever: nothing will be purged automatically.";
    case "days_7":
    case "days_30":
    case "days_90":
      return preview.would_remove === 0
        ? `No non-favorite entries are older than the configured retention period.`
        : `${preview.would_remove} non-favorite entr${preview.would_remove === 1 ? "y" : "ies"} would be removed by the next retention pass.`;
  }
}

export function describeRetentionResult(result: RetentionResponse): string {
  if (result.removed === 0) {
    return "No non-favorite entries were removed by the retention pass.";
  }
  return `${result.removed} non-favorite entr${result.removed === 1 ? "y" : "ies"} removed by the retention pass.`;
}
