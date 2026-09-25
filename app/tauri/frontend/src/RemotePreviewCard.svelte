<script lang="ts">
  /**
   * Read-only metadata-only preview card the
   * `peer-text-history-browser` change renders for the host's
   * transferable text rows. The component shares the visual
   * skeleton (typography, density, hierarchy) of the local
   * `HistoryCard` but uses a dedicated read-only surface and
   * state; it never registers a drag handler, never starts a
   * pointer drag, never copies content to the clipboard and
   * never opens the local edit / pin / menu flows the local
   * card controls. The card carries a single menu entry —
   * `Importar` — that triggers the explicit `peer-text-import`
   * (or `peer-image-import`) flow: the runtime dials the
   * productive mTLS transport, the importer commits the import
   * transaction through the shared SQLite handle and the
   * discriminated union the bridge returns identifies the
   * local snapshot without leaking the imported body or bytes.
   *
   * The payload is metadata-only by construction. The card
   * NEVER receives the entry body, image bytes or any field
   * the spec / design forbid (tags, collections, favourites,
   * source-app metadata, content hash, asset references). The
   * component simply does not have a slot for those fields;
   * the bridge refuses to forward them.
   */
  import { onDestroy, onMount } from "svelte";
  import type {
    PeerHistoryRow,
    PeerImportResponse,
    PeerImageImportResponse,
    PeerImageThumbnailResponse,
  } from "./types";
  import {
    peerImportFetchCommand,
    peerImageFetchCommand,
    peerImageThumbnailFetchCommand,
  } from "./lib/tauri";
  import { formatElapsedTime, type ElapsedTime } from "./lib/elapsedTime";
  import {
    isCurrentThumbnailRequest,
    remoteImageThumbnailCardIdentity,
    shouldRequestRemoteImageThumbnail,
    supportsRemoteImageThumbnails,
    type RemoteImageThumbnailPhase,
  } from "./lib/remoteImageThumbnailState";

  export let row: PeerHistoryRow;
  /**
   * Stable row id the test suite / debug overlay can use to
   * attribute the card without inspecting the opaque
   * `remote_entry_id` value. The component never exposes the
   * remote id outside the dedicated `data-remote-entry-id`
   * attribute so the test surface stays consistent across
   * hosts that mint the id with a different prefix.
   */
  export let rowTestId: string | null = null;
  /**
   * `peer_id` the parent passes through so the import bridge
   * can dial the productive mTLS transport. The component
   * never inspects the value beyond forwarding it verbatim.
   */
  export let peerId: string | null = null;
  /**
   * Display name the parent renders for the active peer. The
   * bridge forwards the value to the importer so the
   * peer-bound collection uses the same visible name the UI
   * already shows.
   */
  export let displayName: string | null = null;
  /**
   * Whether the row represents an image capture. When `true`,
   * the menu action delegates to the `peer-image-import`
   * bridge; otherwise it delegates to the `peer-text-import`
   * bridge. The bridge never receives bytes for either path.
   */
  export let isImageRow: boolean = false;
  /**
   * Comma-separated capability tokens the parent observed for
   * the active peer (the same `capability` field the
   * `Equipos` view reads from `known_peers`). The card uses
   * the value to gate the image Import action so a peer that
   * did not advertise `image_import` never offers a successful
   * import path. The host / client core additionally re-validates
   * the gate so a regression in the UI cannot bypass the
   * security check.
   */
  export let peerCapability: string | null = null;
  /** True only after the parent synchronized this peer's runtime state. */
  export let peerStateReady: boolean = false;

  /**
   * Stable predicate the template and the action handlers use
   * to disable the Import button when the peer lacks the
   * `image_import` capability. Empty / missing values mean
   * "unknown" and the predicate defaults to allow; the bridge
   * surfaces the typed `peer_unavailable { reason:
   * "not_available" }` outcome so a stale UI never reaches a
   * successful import.
   */
  $: peerSupportsImageImport = (() => {
    if (!isImageRow) return true;
    if (peerCapability === null) return true;
    return peerCapability
      .split(",")
      .map((token) => token.trim())
      .some((token) => token === "image_import");
  })();

  /**
   * Stable predicate the
   * `peer-image-preview-thumbnails` change ships. The helper
   * mirrors [`peerSupportsImageImport`] but reads the additive
   * `image_preview_thumbnail` capability the host advertises
   * through the `caps_extra` TXT field. The thumbnail route
   * additionally requires `image_import`; both predicates are
   * evaluated together so a peer that ships only one of the
   * two capabilities never opens the thumbnail route.
   */
  $: peerSupportsImagePreviewThumbnail = (() => {
    return isImageRow && supportsRemoteImageThumbnails(peerCapability);
  })();

  /**
   * Localised content-type label. The remote rows arrive with
   * the canonical snake_case string the local SQLite layer
   * persists; the helper mirrors the local card so the
   * typography / label stay aligned. The component intentionally
   * does NOT inspect the `entry` payload for the type — the
   * label is the only metadata the renderer needs to render the
   * badge.
   */
  function describeContentType(value: string): string {
    switch (value) {
      case "text":
        return "Texto";
      case "url":
        return "URL";
      case "email":
        return "Email";
      case "json":
        return "JSON";
      case "jwt":
        return "JWT";
      case "uuid":
        return "UUID";
      case "ipv4":
        return "IPv4";
      case "ipv6":
        return "IPv6";
      case "hex_color":
        return "Color";
      case "html":
        return "HTML";
      case "file_path":
        return "Ruta";
      case "shell_command":
        return "Shell";
      case "sql":
        return "SQL";
      case "code":
        return "Código";
      case "image":
        return "Imagen";
      default:
        return value;
    }
  }

  function describeImageOutcome(outcome: PeerImageImportResponse): string {
    switch (outcome.kind) {
      case "imported":
        return outcome.deduplicated ? "Importado (ya existía)" : "Importado";
      case "peer_unavailable":
        // The host surfaces `reason = not_available` when the
        // peer did not advertise `image_import`. The card
        // renders a dedicated copy so the user understands the
        // gate is a peer-version mismatch, not a transient
        // transport failure.
        if (outcome.reason === "not_available") {
          return "Este equipo no admite la importación de imágenes.";
        }
        return `No disponible (${outcome.reason})`;
      case "transport_unavailable":
        return `No disponible (${outcome.reason})`;
      case "body_too_large":
        return "Imagen demasiado grande";
      case "invalid_image":
        return "Imagen no válida";
      case "not_transferable":
        return "No transferible";
      case "title_invalid":
        return "Título no válido";
      case "persistence_error":
        return `Error al guardar (${outcome.reason})`;
    }
  }

  let menuOpen = false;
  let busy = false;
  /**
   * Metadata-only feedback the card surfaces after the
   * importer resolves the request. The shape is the
   * discriminated union the bridge returns — the renderer
   * never has to inspect free-form strings or content bytes.
   */
  let lastResult: { kind: "ok" | "error"; summary: string } | null = null;

  /**
   * Thumbnail request state the `peer-image-preview-thumbnails`
   * change ships. The card only requests a thumbnail when
   * the row represents an image AND the peer advertises both
   * `image_import` and `image_preview_thumbnail`. The state
   * machine has four discrete phases the template branches
   * on without inspecting free-form strings or content bytes.
   *
   * `loading` is also the phase the static placeholder shows
   * while the bridge call is in flight; `error` is reserved
   * for malformed / invalid / busy outcomes the renderer
   * surfaces as a still-placeholder (so a stale / drifted /
   * unauthorised caller cannot leak the original image bytes
   * through an error envelope).
   */
  let thumbnailPhase: RemoteImageThumbnailPhase = "idle";
  let thumbnailUrl: string | null = null;
  /**
   * Stable token the thumbnail fetcher increments on every
   * entry that may invalidate an in-flight request (peer
   * change, page change, card change, unmount, busy /
   * revoke / block). The fetcher compares the captured token
   * with the live token before applying the response so a
   * late response can never replace the placeholder of
   * another card.
   */
  let thumbnailRequestToken = 0;
  let thumbnailCardIdentity: string | null = null;
  /**
   * Container the `IntersectionObserver` watches. The card
   * only fires the thumbnail request when the visible
   * element intersects the root scroller; the observer is
   * detached the moment the card unmounts or the peer /
   * page / row changes so the bridge never sees a request
   * for a stale identifier.
   */
  let cardElement: HTMLElement | null = null;
  let thumbnailObserver: IntersectionObserver | null = null;
  let thumbnailRequestInFlight = false;
  /**
   * Live elapsed-time label the card renders in the same
   * shape every local card uses (`formatElapsedTime`). The
   * anchor is refreshed by a local timer so a long-lived
   * rail keeps the metadata accurate without driving a new
   * bridge round-trip; the formatter itself is pure and
   * shared with the local cards so the visual + accessible
   * strings stay consistent.
   */
  const METADATA_REFRESH_MS = 30_000;
  let nowAnchor = Date.now();
  let metadataTimer: ReturnType<typeof setInterval> | null = null;
  $: elapsed = formatElapsedTime(row.created_at, new Date(nowAnchor));
  $: elapsedLabel = (() => {
    const value: ElapsedTime = elapsed;
    return { aria: value.accessible, visual: value.visual };
  })();
  function toggleMenu(): void {
    menuOpen = !menuOpen;
  }
  function describeOutcome(outcome: PeerImportResponse): string {
    switch (outcome.kind) {
      case "imported":
        return outcome.deduplicated
          ? "Importado (ya existía)"
          : "Importado";
      case "peer_unavailable":
        return `No disponible (${outcome.reason})`;
      case "transport_unavailable":
        return `No disponible (${outcome.reason})`;
      case "body_too_large":
        return "Cuerpo demasiado grande";
      case "invalid_utf8":
        return "Cuerpo no válido";
      case "not_transferable":
        return "No transferible";
      case "empty_content":
        return "Contenido vacío";
      case "title_invalid":
        return "Título no válido";
      case "persistence_error":
        return "Error al guardar";
    }
  }
  async function importEntry(): Promise<void> {
    if (peerId === null || busy) return;
    // Defence in depth: refuse the import locally when the
    // peer did not advertise the `image_import` capability. The
    // host enforces the same gate so the bridge never carries a
    // request the listener would reject; the UI guard avoids
    // a wasted round-trip and surfaces the typed reason before
    // the user sees a generic error copy.
    if (isImageRow && !peerSupportsImageImport) {
      lastResult = {
        kind: "error",
        summary: "Este equipo no admite la importación de imágenes.",
      };
      return;
    }
    busy = true;
    lastResult = null;
    menuOpen = false;
    try {
      if (isImageRow) {
        const outcome = await peerImageFetchCommand({
          peer_id: peerId,
          remote_entry_id: row.remote_entry_id,
          display_name: displayName ?? "",
        });
        lastResult = {
          kind: outcome.kind === "imported" ? "ok" : "error",
          summary: describeImageOutcome(outcome),
        };
      } else {
        const outcome = await peerImportFetchCommand({
          peer_id: peerId,
          remote_entry_id: row.remote_entry_id,
          display_name: displayName ?? "",
        });
        lastResult = {
          kind: outcome.kind === "imported" ? "ok" : "error",
          summary: describeOutcome(outcome),
        };
      }
    } catch (err) {
      lastResult = {
        kind: "error",
        summary: err instanceof Error ? err.message : String(err),
      };
    } finally {
      busy = false;
    }
  }
  function refreshNowAnchor(): void {
    nowAnchor = Date.now();
  }

  /**
   * Decode a base64 PNG payload into a transient
   * `URL.createObjectURL` handle. The renderer MUST release
   * the URL with `URL.revokeObjectURL` on replacement /
   * unmount / peer change so the bytes never linger outside
   * the lifetime of the rendered card.
   */
  function bytesB64ToObjectUrl(bytesB64: string): string | null {
    if (typeof atob !== "function") return null;
    try {
      const binary = atob(bytesB64);
      const bytes = new Uint8Array(binary.length);
      for (let index = 0; index < binary.length; index += 1) {
        bytes[index] = binary.charCodeAt(index);
      }
      const blob = new Blob([bytes], { type: "image/png" });
      return URL.createObjectURL(blob);
    } catch (err) {
      // Defence in depth: a malformed base64 string collapses
      // to a still-placeholder without surfacing the bytes.
      console.warn("remote thumbnail decode failed", err);
      return null;
    }
  }

  /**
   * Apply a thumbnail response to the live card. The helper
   * is the single place where the renderer builds the Object
   * URL; every entry point that could receive a response
   * MUST go through this function so the stale-token guard
   * stays correct.
   */
  function applyThumbnail(
    token: number,
    response: PeerImageThumbnailResponse,
  ): void {
    if (!isCurrentThumbnailRequest(token, thumbnailRequestToken)) {
      // The peer / page / card changed since the request
      // was issued. The renderer MUST discard the response so
      // a stale thumbnail can never replace the placeholder of
      // another row.
      return;
    }
    thumbnailRequestInFlight = false;
    if (response.kind === "ok") {
      const next = bytesB64ToObjectUrl(response.bytes_b64);
      if (next === null) {
        thumbnailPhase = "error";
        if (thumbnailUrl !== null) {
          URL.revokeObjectURL(thumbnailUrl);
          thumbnailUrl = null;
        }
        return;
      }
      if (thumbnailUrl !== null) {
        URL.revokeObjectURL(thumbnailUrl);
      }
      thumbnailUrl = next;
      thumbnailPhase = "ready";
      return;
    }
    // Every failure variant collapses to the static
    // placeholder. The renderer never surfaces a global rail
    // error; the original-image bytes never cross the bridge
    // on a failure path.
    thumbnailPhase = "error";
  }

  /**
   * Fire a thumbnail request for the current row. The helper
   * is the only entry point the IntersectionObserver calls;
   * it short-circuits when the gate is not satisfied, when a
   * request is already in flight, or when the row no longer
   * matches the live card.
   */
  async function requestThumbnail(): Promise<void> {
    if (peerId === null) return;
    if (
      !shouldRequestRemoteImageThumbnail({
        isImageRow,
        isIntersecting: true,
        peerId,
        peerStateReady,
        peerCapability,
        requestInFlight: thumbnailRequestInFlight,
        phase: thumbnailPhase,
      })
    ) {
      return;
    }
    const token = thumbnailRequestToken;
    thumbnailRequestInFlight = true;
    thumbnailPhase = "loading";
    try {
      const outcome = await peerImageThumbnailFetchCommand({
        peer_id: peerId,
        remote_entry_id: row.remote_entry_id,
      });
      applyThumbnail(token, outcome);
    } catch (err) {
      applyThumbnail(token, {
        kind: "transport_unavailable",
        reason: "unavailable",
      });
      // The bridge surfaced an exception; log the metadata
      // only — the renderer never persists the bytes nor the
      // message string.
      console.warn("peer thumbnail fetch failed", err);
    }
  }

  /**
   * Detach the IntersectionObserver and release every Object
   * URL the card allocated. The helper runs on unmount and
   * on every transition that may invalidate the live card
   * (peer change, page change, card change) so the bytes
   * never linger outside the lifetime of the rendered card.
   */
  function releaseThumbnail(): void {
    if (thumbnailObserver !== null) {
      thumbnailObserver.disconnect();
      thumbnailObserver = null;
    }
    if (thumbnailUrl !== null) {
      URL.revokeObjectURL(thumbnailUrl);
      thumbnailUrl = null;
    }
    thumbnailRequestToken += 1;
    thumbnailRequestInFlight = false;
    thumbnailPhase = "idle";
  }

  $: {
    // Reactive guard: every time the parent swaps the live
    // card (different row, different peer, different page)
    // the renderer invalidates the in-flight request and
    // releases the Object URL the previous card allocated.
    // `bind:this` and the card identity are reactive dependencies,
    // so the observer is attached to the live node after Svelte
    // applies a peer / row replacement.
    const nextCardIdentity = remoteImageThumbnailCardIdentity(
      peerId,
      row.remote_entry_id,
      row.created_at,
      row.title,
      row.preview,
      isImageRow,
      peerCapability,
    );
    if (
      thumbnailCardIdentity !== null &&
      thumbnailCardIdentity !== nextCardIdentity
    ) {
      releaseThumbnail();
    }
    thumbnailCardIdentity = nextCardIdentity;
    if (
      isImageRow &&
      peerStateReady &&
      peerSupportsImagePreviewThumbnail &&
      typeof IntersectionObserver !== "undefined" &&
      cardElement !== null &&
      thumbnailObserver === null
    ) {
      thumbnailObserver = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            if (
              shouldRequestRemoteImageThumbnail({
                isImageRow,
                isIntersecting: entry.isIntersecting,
                peerId,
                peerStateReady,
                peerCapability,
                requestInFlight: thumbnailRequestInFlight,
                phase: thumbnailPhase,
              })
            ) {
              void requestThumbnail();
              return;
            }
          }
        },
        { root: null, threshold: 0.05 },
      );
      thumbnailObserver.observe(cardElement);
    } else if (
      (!isImageRow || !peerStateReady || !peerSupportsImagePreviewThumbnail) &&
      thumbnailObserver !== null
    ) {
      thumbnailObserver.disconnect();
      thumbnailObserver = null;
    }
  }

  onMount(() => {
    refreshNowAnchor();
    metadataTimer = setInterval(refreshNowAnchor, METADATA_REFRESH_MS);
  });
  onDestroy(() => {
    if (metadataTimer !== null) {
      clearInterval(metadataTimer);
      metadataTimer = null;
    }
    releaseThumbnail();
  });
</script>

<article
  class="remote-preview-card"
  data-testid="remote-preview-card"
  data-remote-entry-id={row.remote_entry_id}
  data-row-test-id={rowTestId}
  draggable="false"
  bind:this={cardElement}
  aria-label={row.title ?? "Vista previa remota"}
>
  <header class="remote-preview-card-header">
    {#if row.title}
      <h3 class="remote-preview-card-title" data-testid="remote-preview-card-title">
        {row.title}
      </h3>
    {:else}
      <span class="remote-preview-card-title muted" data-testid="remote-preview-card-title">
        {describeContentType(row.content_type)}
      </span>
    {/if}
    <span
      class="remote-preview-card-type"
      data-testid="remote-preview-card-type"
      data-content-type={row.content_type}
    >
      {describeContentType(row.content_type)}
    </span>
  </header>
  <p
    class="remote-preview-card-preview"
    data-testid="remote-preview-card-preview"
  >
    {row.preview}
  </p>
  {#if isImageRow}
    <div
      class="remote-preview-card-image-placeholder"
      data-testid="remote-preview-card-image-placeholder"
      data-placeholder-kind={thumbnailPhase === "ready" ? "thumbnail" : "static"}
      aria-hidden={thumbnailPhase === "ready" ? "false" : "true"}
    >
      {#if thumbnailPhase === "ready" && thumbnailUrl !== null}
        <img
          class="remote-preview-card-image-thumbnail"
          data-testid="remote-preview-card-image-thumbnail"
          src={thumbnailUrl}
          alt={row.title ?? "Miniatura remota"}
          draggable="false"
        />
      {:else}
        <svg
          class="remote-preview-card-image-placeholder-svg"
          viewBox="0 0 64 64"
          xmlns="http://www.w3.org/2000/svg"
          role="img"
          aria-label="Marcador estático de imagen remota"
        >
          <rect
            x="2"
            y="2"
            width="60"
            height="60"
            rx="8"
            ry="8"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          />
          <circle cx="22" cy="22" r="5" fill="currentColor" opacity="0.55" />
          <path
            d="M6 50 L24 32 L36 44 L46 34 L58 50 Z"
            fill="currentColor"
            opacity="0.45"
          />
        </svg>
      {/if}
    </div>
  {/if}
  <footer class="remote-preview-card-footer">
    <time
      class="remote-preview-card-date"
      data-testid="remote-preview-card-date"
      datetime={row.created_at}
      aria-label={elapsedLabel.aria}
      title={row.created_at}
    >
      {elapsedLabel.visual}
    </time>
    <div class="remote-preview-card-menu">
      <button
        type="button"
        class="remote-preview-card-menu-button"
        data-testid="remote-preview-card-menu-button"
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        on:click={toggleMenu}
      >
        ⋯
      </button>
      {#if menuOpen}
        <ul
          class="remote-preview-card-menu-list"
          data-testid="remote-preview-card-menu-list"
          role="menu"
        >
          <li role="none">
            <button
              type="button"
              role="menuitem"
              class="remote-preview-card-menu-item"
              data-testid="remote-preview-card-import"
              disabled={busy || peerId === null || !peerSupportsImageImport}
              aria-disabled={busy || peerId === null || !peerSupportsImageImport}
              aria-busy={busy}
              on:click={importEntry}
            >
              {busy
                ? "Importando…"
                : isImageRow && !peerSupportsImageImport
                  ? "No disponible"
                  : "Importar"}
            </button>
          </li>
        </ul>
      {/if}
      {#if lastResult}
        <p
          class="remote-preview-card-result"
          data-testid="remote-preview-card-result"
          data-result-kind={lastResult.kind}
          role={lastResult.kind === "error" ? "alert" : "status"}
        >
          {lastResult.summary}
        </p>
      {/if}
    </div>
  </footer>
</article>

<style>
  .remote-preview-card {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    width: 18rem;
    flex: 0 0 auto;
    padding: 0.65rem 0.8rem;
    border-radius: 8px;
    border: 1px solid var(--cv-border, #30363d);
    background: var(--cv-bg-elev, rgba(255, 255, 255, 0.02));
    color: inherit;
    user-select: none;
    pointer-events: auto;
  }
  .remote-preview-card-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .remote-preview-card-title {
    margin: 0;
    font-size: 0.85rem;
    font-weight: 600;
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .remote-preview-card-title.muted {
    color: var(--cv-fg-muted, #94a3b8);
    font-weight: 500;
  }
  .remote-preview-card-type {
    flex: 0 0 auto;
    font-size: 0.7rem;
    text-transform: lowercase;
    color: var(--cv-fg-muted, #94a3b8);
    padding: 0.05rem 0.45rem;
    border-radius: 999px;
    border: 1px solid var(--cv-border, #30363d);
  }
  .remote-preview-card-preview {
    margin: 0;
    font-size: 0.8rem;
    line-height: 1.4;
    color: var(--cv-fg, #f0f4f8);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-word;
  }
  .remote-preview-card-image-placeholder {
    margin: 0;
    align-self: center;
    width: 100%;
    aspect-ratio: 4 / 3;
    border-radius: 6px;
    border: 1px dashed var(--cv-border, #30363d);
    background: rgba(148, 163, 184, 0.06);
    color: var(--cv-fg-muted, #94a3b8);
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    overflow: hidden;
  }
  .remote-preview-card-image-placeholder-svg {
    width: 56%;
    height: 56%;
  }
  .remote-preview-card-image-thumbnail {
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
    user-select: none;
  }
  .remote-preview-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .remote-preview-card-date {
    font-size: 0.7rem;
    color: var(--cv-fg-muted, #94a3b8);
    flex: 1 1 auto;
  }
  .remote-preview-card-menu {
    position: relative;
    flex: 0 0 auto;
  }
  .remote-preview-card-menu-button {
    background: transparent;
    color: inherit;
    border: 1px solid transparent;
    border-radius: 6px;
    width: 1.65rem;
    height: 1.65rem;
    cursor: pointer;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .remote-preview-card-menu-button:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .remote-preview-card-menu-button:focus-visible {
    outline: 2px solid var(--cv-focus-ring, rgba(37, 99, 235, 0.5));
    outline-offset: 1px;
  }
  .remote-preview-card-menu-list {
    list-style: none;
    margin: 0;
    padding: 0.25rem;
    position: absolute;
    right: 0;
    bottom: calc(100% + 0.25rem);
    background: var(--cv-bg-elev, #1f2937);
    border: 1px solid var(--cv-border, #30363d);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
    z-index: 10;
    min-width: 12rem;
  }
  .remote-preview-card-menu-item {
    width: 100%;
    background: transparent;
    border: 0;
    color: inherit;
    padding: 0.45rem 0.65rem;
    text-align: left;
    border-radius: 4px;
    cursor: pointer;
    font: inherit;
  }
  .remote-preview-card-menu-item:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .remote-preview-card-result {
    margin: 0.35rem 0 0;
    padding: 0.35rem 0.55rem;
    font-size: 0.7rem;
    line-height: 1.3;
    border-radius: 6px;
    border: 1px solid var(--cv-border, #30363d);
    background: rgba(255, 255, 255, 0.04);
    color: inherit;
    min-width: 12rem;
    max-width: 18rem;
  }
  .remote-preview-card-result[data-result-kind="error"] {
    border-color: rgba(248, 113, 113, 0.55);
    background: rgba(248, 113, 113, 0.12);
    color: #fee2e2;
  }
</style>
