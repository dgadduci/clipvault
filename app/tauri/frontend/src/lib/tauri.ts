// Thin wrapper around `window.__TAURI__` IPC. Avoids depending on the
// `@tauri-apps/api` invoke helper until we have a working Vite build.

import type {
  ActiveAppDiagnostics,
  ActiveApplicationResponse,
  Capabilities,
  CaptureResponse,
  ClearResponse,
  ClipvaultCommand,
  ClipvaultCommandArg,
  Collection,
  DatabasePath,
  DeleteResponse,
  Diagnostics,
  EntryRecord,
  IgnoredAppEntry,
  MigrationsApplied,
  OrganizationSnapshot,
  PasteResponse,
  PickAndAddResponse,
  PlatformSettingsTarget,
  RetentionPreview,
  RetentionResponse,
  SearchResponse,
  SetFavoriteResponse,
  Settings,
  SettingsOpenResponse,
  SettingsUpdate,
  SourceAppFilter,
  SourceApplicationsSnapshot,
  Tag,
  WatchTickResponse,
} from "../types.ts";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    };
  }
}

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const tauri = window.__TAURI_INTERNALS__;
  if (!tauri) {
    return Promise.reject(
      new Error(`ClipVault frontend invoked outside Tauri: ${cmd}`),
    );
  }
  return tauri.invoke<T>(cmd, args);
}

export const diagnosticsCommand: ClipvaultCommand<Diagnostics> = () =>
  invoke<Diagnostics>("clipvault_diagnostics");

export const databasePathCommand: ClipvaultCommand<DatabasePath> = () =>
  invoke<DatabasePath>("clipvault_database_path");

export const migrationsAppliedCommand: ClipvaultCommand<MigrationsApplied> =
  () => invoke<MigrationsApplied>("clipvault_migrations_applied");

export const platformCapabilitiesCommand: ClipvaultCommand<Capabilities> =
  () => invoke<Capabilities>("clipvault_platform_capabilities");

export const captureTextCommand: ClipvaultCommandArg<
  CaptureResponse,
  { sourceApp?: string | null }
> = (args) =>
  invoke<CaptureResponse>("clipvault_capture_text", {
    source_app: args.sourceApp ?? null,
  });

export const captureTickCommand: ClipvaultCommandArg<
  WatchTickResponse,
  { sourceApp?: string | null }
> = (args) =>
  invoke<WatchTickResponse>("clipvault_capture_tick", {
    source_app: args.sourceApp ?? null,
  });

export const recentEntriesCommand: ClipvaultCommandArg<
  EntryRecord[],
  { limit?: number }
> = (args) =>
  invoke<EntryRecord[]>("clipvault_recent_entries", {
    limit: args.limit ?? 50,
  });

export const recentEntriesFilteredCommand: ClipvaultCommandArg<
  EntryRecord[],
  {
    limit?: number;
    collectionId?: number | null;
    tagIds?: number[];
    sourceApp?: SourceAppFilter | null;
  }
> = (args) =>
  invoke<EntryRecord[]>("clipvault_recent_entries_filtered", {
    limit: args.limit ?? 50,
    collectionId: args.collectionId ?? null,
    tagIds: args.tagIds ?? [],
    sourceApp: args.sourceApp ?? null,
  });

export const searchEntriesCommand: ClipvaultCommandArg<
  SearchResponse,
  {
    query: string;
    limit?: number;
    collectionId?: number | null;
    tagIds?: number[];
    sourceApp?: SourceAppFilter | null;
  }
> = (args) =>
  invoke<SearchResponse>("clipvault_search_entries", {
    query: args.query,
    limit: args.limit ?? 50,
    collectionId: args.collectionId ?? null,
    tagIds: args.tagIds ?? [],
    sourceApp: args.sourceApp ?? null,
  });

/**
 * List the source applications represented in the active collection
 * scope. The result feeds the combobox the desktop toolbar renders
 * between the search input and the configuration menu. `collection_id`
 * follows the same convention the recents/search filters use: `null`
 * matches the system `Historial` collection. `tag_ids` mirrors the
 * AND-combined tag filter so the combobox can reflect whichever
 * secondary facet the user has applied.
 *
 * The response is metadata-only: no clipboard content, hashes,
 * snippets or asset references ever cross the bridge. Stale responses
 * (a collection switch that landed while the query was in flight) can
 * be detected through the embedded `scope` field and dropped before
 * they pollute the combobox.
 */
export const sourceApplicationsCommand: ClipvaultCommandArg<
  SourceApplicationsSnapshot,
  { collectionId?: number | null; tagIds?: number[] }
> = (args) =>
  invoke<SourceApplicationsSnapshot>("clipvault_source_applications", {
    collectionId: args.collectionId ?? null,
    tagIds: args.tagIds ?? [],
  });

export const pasteEntryCommand: ClipvaultCommandArg<
  PasteResponse,
  { id: number; mode?: "plain" | "rich" | null }
> = (args) =>
  invoke<PasteResponse>("clipvault_paste_entry", {
    entryId: args.id,
    mode: args.mode ?? null,
  });

export const activeApplicationCommand: ClipvaultCommand<ActiveApplicationResponse> =
  () => invoke<ActiveApplicationResponse>("clipvault_active_application");

export const activeAppDiagnosticsCommand: ClipvaultCommand<ActiveAppDiagnostics> =
  () => invoke<ActiveAppDiagnostics>("clipvault_active_app_diagnostics");

export const refreshActiveAppDiagnosticsCommand: ClipvaultCommand<ActiveAppDiagnostics> =
  () => invoke<ActiveAppDiagnostics>("clipvault_refresh_active_app_diagnostics");

export const openPlatformSettingsCommand: ClipvaultCommandArg<
  SettingsOpenResponse,
  { target: PlatformSettingsTarget }
> = (args) =>
  invoke<SettingsOpenResponse>("clipvault_open_platform_settings", {
    target: args.target,
  });

export const refreshCapabilitiesCommand: ClipvaultCommand<Capabilities> = () =>
  invoke<Capabilities>("clipvault_refresh_capabilities");

export const setFavoriteCommand: ClipvaultCommandArg<
  SetFavoriteResponse,
  { id: number; pinned: boolean }
> = (args) =>
  invoke<SetFavoriteResponse>("clipvault_set_favorite", {
    entryId: args.id,
    pinned: args.pinned,
  });

export const deleteEntryCommand: ClipvaultCommandArg<
  DeleteResponse,
  { id: number; confirm: boolean }
> = (args) =>
  invoke<DeleteResponse>("clipvault_delete_entry", {
    entryId: args.id,
    confirm: args.confirm,
  });

export const clearHistoryCommand: ClipvaultCommandArg<
  ClearResponse,
  { confirm: boolean }
> = (args) =>
  invoke<ClearResponse>("clipvault_clear_history", {
    confirm: args.confirm,
  });

/**
 * How many non-favorite entries would be removed by
 * [`clearUnorganizedHistoryCommand`]. Mirrors the backend predicate so
 * the confirmation message the toolbar renders quotes the exact
 * number of rows the destructive operation will touch. The call is
 * metadata-only: no clipboard content, no reference, no path travels
 * through the bridge.
 */
export const unorganizedClearableCountCommand: ClipvaultCommand<number> =
  () => invoke<number>("clipvault_unorganized_clearable_count");

/**
 * Clear the "unorganized" history: every non-favorite entry whose
 * only association is the system `Historial` collection. Entries in
 * a user (secondary) collection, favorite entries and their payload
 * assets are preserved. The destructive-action contract documented
 * in `clipboard-management` still applies: the call must pass
 * `confirm: true`; otherwise the backend surfaces a
 * `confirmation_required` response without touching the database.
 */
export const clearUnorganizedHistoryCommand: ClipvaultCommandArg<
  ClearResponse,
  { confirm: boolean }
> = (args) =>
  invoke<ClearResponse>("clipvault_clear_unorganized_history", {
    confirm: args.confirm,
  });

export const applyRetentionCommand: ClipvaultCommand<RetentionResponse> = () =>
  invoke<RetentionResponse>("clipvault_apply_retention");

export const retentionPreviewCommand: ClipvaultCommand<RetentionPreview> = () =>
  invoke<RetentionPreview>("clipvault_retention_preview");

export const settingsGetCommand: ClipvaultCommand<Settings> = () =>
  invoke<Settings>("clipvault_settings_get");

export const settingsSetCommand: ClipvaultCommandArg<
  Settings,
  SettingsUpdate
> = (args) =>
  invoke<Settings>("clipvault_settings_set", { update: args });

export const ignoredAppsListCommand: ClipvaultCommand<string[]> = () =>
  invoke<string[]>("clipvault_ignored_apps_list");

export const ignoredAppsAddCommand: ClipvaultCommandArg<
  Settings,
  { id: string }
> = (args) =>
  invoke<Settings>("clipvault_ignored_apps_add", { id: args.id });

export const ignoredAppsRemoveCommand: ClipvaultCommandArg<
  Settings,
  { id: string }
> = (args) =>
  invoke<Settings>("clipvault_ignored_apps_remove", { id: args.id });

/**
 * Drive the platform application picker. The command returns a
 * discriminated union (added / updated / cancelled / error) so the
 * UI can branch on the `kind` field without parsing free-form
 * messages.
 */
export const ignoredAppPickAndAddCommand: ClipvaultCommand<PickAndAddResponse> =
  () => invoke<PickAndAddResponse>("clipvault_ignored_app_pick_and_add");

/**
 * List blacklisted applications with their presentation metadata.
 * Legacy rows (created before the picker landed) flow through with
 * `display_name` and `icon_ref` set to `null`.
 */
export const ignoredAppsListWithMetadataCommand: ClipvaultCommand<
  IgnoredAppEntry[]
> = () =>
  invoke<IgnoredAppEntry[]>("clipvault_ignored_apps_list_with_metadata");

/**
 * Resolve an `icon_ref` produced by the application picker to the
 * raw PNG bytes the settings panel renders. The command is the only
 * bridge between the relative reference the database stores and the
 * image data the webview can decode — the frontend never sees an
 * absolute filesystem path and the backend rejects every reference
 * that escapes `<data_dir>/assets/ignored-apps/`.
 *
 * The wrapper accepts the response as a `number[]` because Tauri
 * serialises `Vec<u8>` as a JSON array of numbers; the caller is
 * expected to wrap the result into a `Uint8Array` (see
 * `lib/iconResolver.ts`).
 */
export const ignoredAppIconCommand: ClipvaultCommandArg<
  number[],
  { ref: string }
> = (args) =>
  invoke<number[]>("clipvault_ignored_app_icon", { iconRef: args.ref });

/**
 * Resolve a `source_app_icon_ref` produced by the
 * `history-card-layout` capture pipeline to the raw PNG bytes the
 * card rail renders. Mirrors `ignoredAppIconCommand` but resolves
 * references under the `application-icons/` namespace; the
 * validator on the Rust side enforces the same scope / traversal /
 * symlink guarantees as the picker.
 */
export const sourceAppIconCommand: ClipvaultCommandArg<
  number[],
  { ref: string }
> = (args) =>
  invoke<number[]>("clipvault_source_app_icon", { iconRef: args.ref });

/**
 * Resolve an `asset_ref` produced by the `clipboard-rich-content`
 * capture pipeline to the raw PNG bytes of the captured image.
 *
 * This is the ONLY bridge between the relative reference SQLite stores
 * (`clipboard/<sha256>.png`) and the bytes the webview can decode. The
 * frontend never receives an absolute filesystem path and cannot widen
 * the surface: the backend validates the namespace, rejects traversal
 * and symlinks, and refuses anything that is not a PNG within the
 * documented size and dimension caps.
 *
 * As with the icon bridges the response arrives as a `number[]`
 * (Tauri's JSON encoding of `Vec<u8>`); `lib/clipboardAsset.ts` wraps
 * it into a `Blob` and mints the `blob:` URL the card renders.
 */
export const clipboardAssetCommand: ClipvaultCommandArg<
  number[],
  { ref: string }
> = (args) =>
  invoke<number[]>("clipvault_clipboard_asset", { assetRef: args.ref });

/**
 * Resolve a `rich_preview_ref` produced by the `clipboard-rich-text`
 * capture pipeline to the sanitised HTML fragment the card renders.
 *
 * Mirrors the validation guarantees of `clipboardAssetCommand`: the
 * frontend never receives an absolute filesystem path and cannot widen
 * the surface — the backend rejects every reference that escapes
 * `<data_dir>/assets/rich-text/`, refuses traversal and symlinks, and
 * caps the response size to the documented rich-text preview budget.
 *
 * The bytes returned are the *sanitised* preview the core produced
 * when the entry was committed; the original HTML and RTF the source
 * application published never cross this bridge. The frontend MUST
 * treat the response as trusted-but-bounded input (clip / overflow),
 * NOT as raw HTML to be injected verbatim.
 */
export const richTextPreviewCommand: ClipvaultCommandArg<
  number[],
  { ref: string }
> = (args) =>
  invoke<number[]>("clipvault_rich_text_preview", { previewRef: args.ref });

/**
 * Update or restore the card title for a single entry. Passing
 * `null` (or an empty string after trimming) restores the default
 * title (the localised `content_type` label).
 *
 * The discriminated response lets the UI branch on the outcome
 * without parsing free-form messages:
 * - `kind: "updated"` — the title was persisted and the response
 *   carries the refreshed entry so the rail can update its state;
 * - `kind: "not_found"` — the entry disappeared between the rail
 *   read and the title mutation; the rail should silently drop the
 *   row.
 */
export type SetEntryTitleResponse =
  | { kind: "updated"; entry: EntryRecord }
  | { kind: "not_found" };

export const setEntryTitleCommand: ClipvaultCommandArg<
  SetEntryTitleResponse,
  { id: number; title: string | null }
> = (args) =>
  invoke<SetEntryTitleResponse>("clipvault_set_entry_title", {
    entryId: args.id,
    title: args.title,
  });

// ---------------------------------------------------------------------------
// `tags-and-collections` command surface.
// ---------------------------------------------------------------------------

export const organizationSnapshotCommand: ClipvaultCommand<OrganizationSnapshot> = () =>
  invoke<OrganizationSnapshot>("clipvault_organization_snapshot");

export const collectionsCreateCommand: ClipvaultCommandArg<
  Collection,
  { name: string }
> = (args) =>
  invoke<Collection>("clipvault_collections_create", { name: args.name });

export const collectionsRenameCommand: ClipvaultCommandArg<
  Collection,
  { collectionId: number; name: string }
> = (args) =>
  invoke<Collection>("clipvault_collections_rename", {
    collectionId: args.collectionId,
    name: args.name,
  });

export const collectionsDeleteCommand: ClipvaultCommandArg<
  boolean,
  { collectionId: number }
> = (args) =>
  invoke<boolean>("clipvault_collections_delete", {
    collectionId: args.collectionId,
  });

export const tagsCreateCommand: ClipvaultCommandArg<Tag, { name: string }> =
  (args) => invoke<Tag>("clipvault_tags_create", { name: args.name });

export const tagsRenameCommand: ClipvaultCommandArg<
  Tag,
  { tagId: number; name: string }
> = (args) =>
  invoke<Tag>("clipvault_tags_rename", {
    tagId: args.tagId,
    name: args.name,
  });

export const tagsDeleteCommand: ClipvaultCommandArg<
  boolean,
  { tagId: number }
> = (args) => invoke<boolean>("clipvault_tags_delete", { tagId: args.tagId });

export const entryCollectionsCommand: ClipvaultCommandArg<
  number[],
  { entryId: number }
> = (args) =>
  invoke<number[]>("clipvault_entry_collections", {
    entryId: args.entryId,
  });

export const entryTagsCommand: ClipvaultCommandArg<
  number[],
  { entryId: number }
> = (args) => invoke<number[]>("clipvault_entry_tags", { entryId: args.entryId });

export const entryCollectionsSetCommand: ClipvaultCommandArg<
  number[],
  { entryId: number; collectionIds: number[] }
> = (args) =>
  invoke<number[]>("clipvault_entry_collections_set", {
    entryId: args.entryId,
    collectionIds: args.collectionIds,
  });

export const entryTagsSetCommand: ClipvaultCommandArg<
  number[],
  { entryId: number; tagIds: number[] }
> = (args) =>
  invoke<number[]>("clipvault_entry_tags_set", {
    entryId: args.entryId,
    tagIds: args.tagIds,
  });

export const entryRemoveFromCollectionCommand: ClipvaultCommandArg<
  boolean,
  { entryId: number; collectionId: number }
> = (args) =>
  invoke<boolean>("clipvault_entry_remove_from_collection", {
    entryId: args.entryId,
    collectionId: args.collectionId,
  });

/**
 * Atomically create-or-reuse a tag by normalised name and attach
 * it to a single entry. The command is the canonical bridge the card
 * menu uses when the user types a new tag in **Agregar tag**: the
 * core owns the normalisation, the idempotency and the join-table
 * insert in a single transaction so a transient failure cannot
 * leave an orphan tag without its association, nor an association
 * pointing at a tag that has not been committed yet.
 *
 * The response carries the refreshed [`Tag`] row regardless of
 * whether the tag was newly inserted or already existed; the
 * association itself is idempotent, so calling the command twice
 * for the same `(entryId, name)` pair is safe. The command never
 * accepts clipboard content and never returns rich payload bytes.
 */
export const entryUpsertTagCommand: ClipvaultCommandArg<
  Tag,
  { entryId: number; name: string }
> = (args) =>
  invoke<Tag>("clipvault_entry_upsert_tag", {
    entryId: args.entryId,
    name: args.name,
  });

export const historyCollectionIdCommand: ClipvaultCommand<number> = () =>
  invoke<number>("clipvault_history_collection_id");

// Re-export the types so consumers don't need a second import.
export type {
  ActiveAppDiagnostics,
  ActiveAppRefreshOutcome,
  ActiveApplicationResponse,
  Capabilities,
  CaptureResponse,
  ClearResponse,
  Collection,
  DeleteResponse,
  Diagnostics,
  EntryRecord,
  IgnoredAppEntry,
  OrganizationSnapshot,
  PasteResponse,
  PickAndAddResponse,
  PlatformGuidance,
  PlatformSettingsTarget,
  RetentionPreview,
  RetentionResponse,
  SearchResponse,
  SetFavoriteResponse,
  Settings,
  SettingsOpenResponse,
  SettingsUpdate,
  SourceAppFilter,
  SourceApplicationOption,
  SourceApplicationsScope,
  SourceApplicationsSnapshot,
  Tag,
  WatchTickResponse,
} from "../types";
