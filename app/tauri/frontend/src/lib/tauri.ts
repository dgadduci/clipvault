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
  CopyResponse,
  DatabasePath,
  DeleteResponse,
  Diagnostics,
  EntryRecord,
  GnomeConsentDecision,
  GnomeIntegrationInstallResult,
  GnomeIntegrationPayload,
  GnomeIntegrationStatusResponse,
  IgnoredAppEntry,
  LinuxCatalogResponse,
  LinuxPickAndAddResponse,
  LocalPeerProfileResponse,
  MigrationsApplied,
  OrganizationSnapshot,
  PasteResponse,
  PeerSharingToggleResponse,
  PeerSnapshot,
  PeerPairingHealthResponse,
  PeerPairingOutcomeResponse,
  PeerPairingSessionSnapshot,
  PeerHistoryBrowseResponse,
  PeerTrustOperationResponse,
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
  UpdateTextEntryResponse,
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

/**
 * Copy-only keyboard command used by Quick Paste.
 *
 * Forwards `id` and the optional `mode` argument to
 * `clipvault_copy_entry`. The backend writes the type-appropriate
 * representation (plain, rich, image) to the system clipboard and
 * arms the suppression registry so the next watcher tick does not
 * create a history card as a side effect. The command NEVER
 * invokes any synthetic paste controller and NEVER clears the
 * clipboard after writing it.
 */
export const copyEntryCommand: ClipvaultCommandArg<
  CopyResponse,
  { id: number; mode?: "plain" | "rich" | null }
> = (args) =>
  invoke<CopyResponse>("clipvault_copy_entry", {
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

export const localPeerProfileGetCommand: ClipvaultCommand<LocalPeerProfileResponse> = () =>
  invoke<LocalPeerProfileResponse>("clipvault_local_peer_profile_get");

export const localPeerProfileUpdateCommand: ClipvaultCommandArg<
  LocalPeerProfileResponse,
  { name: string | null }
> = (args) =>
  invoke<LocalPeerProfileResponse>("clipvault_local_peer_profile_update", {
    update: { name: args.name },
  });

/**
 * Read the persisted `Compartir en red local` toggle together
 * with the runtime's current state. The response is the typed
 * union [`PeerSharingToggleResponse`] the Settings panel
 * branches on. The bridge NEVER carries clipboard content,
 * addresses, ports or raw public keys.
 */
export const peerSharingToggleGetCommand: ClipvaultCommand<PeerSharingToggleResponse> =
  () => invoke<PeerSharingToggleResponse>("clipvault_peer_sharing_toggle_get");

/**
 * Persist the opt-in toggle and drive the runtime in lockstep.
 * The toggle is always accepted: turning it off stops the
 * runtime unconditionally; turning it on surfaces
 * `identity_unavailable` when the secure store is unreachable
 * so the user can retry without losing the persisted `true`.
 *
 * The Rust command declares `enabled` as a root argument; the
 * bridge MUST forward the bare boolean (not a nested `update`
 * envelope), otherwise Tauri rejects the call with
 * `missing required key enabled`.
 */
export const peerSharingToggleSetCommand: ClipvaultCommandArg<
  PeerSharingToggleResponse,
  { enabled: boolean }
> = (args) =>
  invoke<PeerSharingToggleResponse>("clipvault_peer_sharing_toggle_set", {
    enabled: args.enabled,
  });

/**
 * Metadata-only snapshot the `Equipos` view renders. The
 * response carries only the persisted `known_peers` rows merged
 * with the in-memory presence state; it NEVER carries IP
 * addresses, ports, raw public keys or clipboard content.
 */
export const peerSnapshotCommand: ClipvaultCommand<PeerSnapshot> = () =>
  invoke<PeerSnapshot>("clipvault_peer_snapshot");

/**
 * Trigger an on-demand re-load of the local identity the
 * runtime uses for self-filtering. The Settings panel calls
 * this from a `Refrescar` button so the user can recover
 * without restarting the app when the keychain is temporarily
 * unavailable. The bridge forwards nothing but the typed
 * outcome.
 */
export const peerSharingRefreshIdentityCommand: ClipvaultCommand<PeerSharingToggleResponse> =
  () =>
    invoke<PeerSharingToggleResponse>("clipvault_peer_sharing_refresh_identity");

/**
 * Bridge the `local-peer-mutual-pairing` change exposes for the
 * pairing modal. Every command is metadata-only: the runtime
 * never accepts an IP, a port, a TLS key, a SAS candidate or a
 * signature from the frontend; the pairing state machine owns
 * every byte that crosses the trust boundary. The `Equipos`
 * view links to [`peerPairingStartCommand`] from each
 * `unverified` row; the modal then drives the bounded session
 * against the typed [`PeerPairingOutcomeResponse`] variants.
 */
export const peerPairingStartCommand: ClipvaultCommandArg<
  PeerPairingOutcomeResponse,
  { peer_id: string; display_name: string }
> = (args) =>
  invoke<PeerPairingOutcomeResponse>("clipvault_peer_pairing_start", {
    peerId: args.peer_id,
    displayName: args.display_name,
  });

export const peerPairingApproveLocalCommand: ClipvaultCommandArg<
  PeerPairingOutcomeResponse,
  { session_id: number }
> = (args) =>
  invoke<PeerPairingOutcomeResponse>("clipvault_peer_pairing_approve_local", {
    sessionId: args.session_id,
  });

export const peerPairingCancelCommand: ClipvaultCommandArg<
  PeerPairingOutcomeResponse,
  { session_id: number }
> = (args) =>
  invoke<PeerPairingOutcomeResponse>("clipvault_peer_pairing_cancel", {
    sessionId: args.session_id,
  });

export const peerPairingSnapshotCommand: ClipvaultCommand<
  PeerPairingSessionSnapshot[]
> = () => invoke<PeerPairingSessionSnapshot[]>("clipvault_peer_pairing_snapshot");

export const peerPairingRevokeCommand: ClipvaultCommandArg<
  PeerTrustOperationResponse,
  { peer_id: string }
> = (args) =>
  invoke<PeerTrustOperationResponse>("clipvault_peer_pairing_revoke", {
    peerId: args.peer_id,
  });

export const peerPairingBlockCommand: ClipvaultCommandArg<
  PeerTrustOperationResponse,
  { peer_id: string }
> = (args) =>
  invoke<PeerTrustOperationResponse>("clipvault_peer_pairing_block", {
    peerId: args.peer_id,
  });

export const peerPairingUnblockCommand: ClipvaultCommandArg<
  PeerTrustOperationResponse,
  { peer_id: string }
> = (args) =>
  invoke<PeerTrustOperationResponse>("clipvault_peer_pairing_unblock", {
    peerId: args.peer_id,
  });

/**
 * Productive metadata-only health probe the runtime runs against
 * a trusted peer. The transport dials the remote listener over
 * mTLS, exchanges the bounded `health` envelope, and returns
 * only the typed response (`ok` / `failed`). The bridge never
 * accepts an IP, a port or a TLS key from the renderer; the
 * runtime reads the pinned cert fingerprint from the persisted
 * row before dialing. History / fetch / import routes are not
 * part of this change and the productive transport refuses
 * them as `not_available`.
 */
export const peerPairingHealthCommand: ClipvaultCommandArg<
  PeerPairingHealthResponse,
  { peer_id: string }
> = (args) =>
  invoke<PeerPairingHealthResponse>("clipvault_peer_pairing_health", {
    peerId: args.peer_id,
  });

/**
 * Bridge for the `peer-text-history-browser` change. The command
 * returns the bounded metadata-only text page the host projects
 * for the supplied `peer_id`; the cursor is the opaque value
 * the renderer submitted verbatim on the previous page request
 * (empty string for the first page). The discriminated union
 * keeps the wire contract stable: the renderer branches on
 * `kind` (`ok` / `invalid_cursor` / `peer_unavailable` /
 * `persistence_unavailable`) without inspecting free-form
 * strings or content bytes. The bridge never returns a typed
 * `CommandError` for the page request — every typed failure
 * collapses into a variant.
 */
export const peerHistoryBrowseCommand: ClipvaultCommandArg<
  PeerHistoryBrowseResponse,
  { peer_id: string; cursor?: string; limit?: number }
> = (args) =>
  invoke<PeerHistoryBrowseResponse>("clipvault_peer_history_browse", {
    peerId: args.peer_id,
    cursor: args.cursor ?? null,
    limit: args.limit ?? null,
  });

/**
 * Best-effort sync hook the shell calls after every peer
 * snapshot / health probe so the in-memory trust / active cache
 * the [`peerHistoryBrowseCommand`] consults cannot outrun the
 * runtime transition that should invalidate it. The hook is
 * metadata-only: it never mutates SQLite, never opens a network
 * call, and never emits a `history-updated` event.
 */
export const peerHistoryRecordStateCommand: ClipvaultCommandArg<
  void,
  { peer_id: string; trusted: boolean; active: boolean }
> = (args) =>
  invoke<void>("clipvault_peer_history_record_state", {
    peerId: args.peer_id,
    trusted: args.trusted,
    active: args.active,
  });

/**
 * Forget the cache entry for `peer_id`. The shell calls this
 * after `Desvincular`, `Bloquear` and `Desbloquear` so a
 * subsequent browse collapses to
 * [`PeerHistoryBrowseResponse.peer_unavailable`] without a
 * network round-trip.
 */
export const peerHistoryForgetCommand: ClipvaultCommandArg<
  void,
  { peer_id: string }
> = (args) =>
  invoke<void>("clipvault_peer_history_forget", {
    peerId: args.peer_id,
  });

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
 * raw PNG bytes the privacy modal renders. The command is the only
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
 * Linux-only: enumerate the installed `.desktop` files whose
 * identifier maps deterministically to the active-app adapter's
 * published value. The wrapper exposes the discriminated
 * `kind: "supported" | "unsupported"` response so the privacy modal
 * can render either the picker modal or the manual-entry fallback
 * without inspecting free-form text.
 */
export const ignoredAppLinuxCatalogCommand: ClipvaultCommand<
  LinuxCatalogResponse
> = () => invoke<LinuxCatalogResponse>("clipvault_ignored_app_linux_catalog");

/**
 * Linux-only: persist the user-selected catalog entry as a
 * blacklisted application. The wrapper forwards **only** the
 * deterministic identifier the catalog returned; the Rust side
 * re-resolves the catalog, ignores any frontend-supplied metadata
 * and updates the privacy gate identically to the macOS picker
 * flow. Keeping the payload identifier-only ensures the IPC
 * bridge never carries application names, icon references or
 * other metadata the picker UI already has in memory.
 */
export const ignoredAppLinuxAddCommand: ClipvaultCommandArg<
  LinuxPickAndAddResponse,
  {
    identifier: string;
  }
> = (args) =>
  invoke<LinuxPickAndAddResponse>("clipvault_ignored_app_linux_add", {
    identifier: args.identifier,
  });

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

/**
 * Replace the textual payload of an existing history entry in
 * place. The bridge only carries the user-entered draft the editor
 * needs to persist; the response and the metadata-only
 * `clipvault://history-updated` event the shell emits after a
 * successful commit are both payload-free.
 */
export const updateTextEntryCommand: ClipvaultCommandArg<
  UpdateTextEntryResponse,
  { id: number; content: string }
> = (args) =>
  invoke<UpdateTextEntryResponse>("clipvault_update_text_entry", {
    entryId: args.id,
    content: args.content,
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

export const collectionsSetColorCommand: ClipvaultCommandArg<
  Collection,
  { collectionId: number; colorHex: string }
> = (args) =>
  invoke<Collection>("clipvault_collections_set_color", {
    collectionId: args.collectionId,
    colorHex: args.colorHex,
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

// ---------------------------------------------------------------------------
// `code-language-detection` command surface.
// ---------------------------------------------------------------------------

/**
 * Response shape of `clipvault_code_language_set`. Mirrors the
 * backend `SetCodeLanguageResponse` discriminated union so the UI can
 * branch on `kind` without parsing free-form messages.
 */
export type SetCodeLanguageResponse =
  | { kind: "updated"; entry: EntryRecord }
  | { kind: "noop"; entry: EntryRecord }
  | { kind: "not_found" };

/**
 * Persist the canonical `code_language` the frontend detector accepted
 * for `entryId`. The bridge carries only metadata — the canonical
 * language identifier and the entry id — and never inspects the
 * clipboard payload, hash or snippet.
 *
 * The command accepts `null` to mean "the detector could not classify
 * this capture"; the backend refuses to overwrite an already-stored
 * classification with `null` and reports the refusal as `noop` so a
 * stale frontend cannot poison the persistence layer.
 */
export const codeLanguageSetCommand: ClipvaultCommandArg<
  SetCodeLanguageResponse,
  { entryId: number; codeLanguage: string | null }
> = (args) =>
  invoke<SetCodeLanguageResponse>("clipvault_code_language_set", {
    entryId: args.entryId,
    codeLanguage: args.codeLanguage,
  });

// ---------------------------------------------------------------------------
// `gnome-wayland-integration` command surface.
// ---------------------------------------------------------------------------

export const gnomeIntegrationStatusCommand: ClipvaultCommand<GnomeIntegrationStatusResponse> =
  () => invoke<GnomeIntegrationStatusResponse>("clipvault_gnome_integration_status");

export const gnomeIntegrationSetConsentCommand: ClipvaultCommandArg<
  GnomeIntegrationPayload,
  { decision: GnomeConsentDecision }
> = (args) =>
  invoke<GnomeIntegrationPayload>("clipvault_gnome_integration_set_consent", {
    update: { decision: args.decision },
  });

export const gnomeIntegrationInstallCommand: ClipvaultCommand<GnomeIntegrationInstallResult> =
  () => invoke<GnomeIntegrationInstallResult>("clipvault_gnome_integration_install");

export const gnomeIntegrationUninstallCommand: ClipvaultCommand<GnomeIntegrationPayload> =
  () => invoke<GnomeIntegrationPayload>("clipvault_gnome_integration_uninstall");

export const gnomeIntegrationRetryCommand: ClipvaultCommand<GnomeIntegrationPayload> =
  () => invoke<GnomeIntegrationPayload>("clipvault_gnome_integration_retry");

// Re-export the types so consumers don't need a second import.
export type {
  ActiveAppDiagnostics,
  ActiveAppRefreshOutcome,
  ActiveApplicationResponse,
  Capabilities,
  CaptureResponse,
  ClearResponse,
  Collection,
  CopyResponse,
  DeleteResponse,
  Diagnostics,
  EntryRecord,
  GnomeConsentDecision,
  GnomeIntegrationInstallResult,
  GnomeIntegrationPayload,
  GnomeIntegrationStatusResponse,
  GnomeTechnicalState,
  IgnoredAppEntry,
  LocalPeerProfile,
  LocalPeerProfileResponse,
  OrganizationSnapshot,
  PasteResponse,
  PeerDisplayNameErrorCode,
  PeerPresence,
  PeerSharingToggleResponse,
  PeerSnapshot,
  PeerSnapshotEntry,
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
  UpdateTextEntryResponse,
  WatchTickResponse,
} from "../types";

export { isEditableTextEntry } from "../types";
