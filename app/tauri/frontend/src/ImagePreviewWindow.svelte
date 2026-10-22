<svelte:head>
  {@html `<style data-clipvault-visual-tokens>:root{${VISUAL_TOKEN_ROOT_CSS}}</style>`}
</svelte:head>

<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
  import type { EntryRecord } from "./types.ts";
  import ClipboardPreview from "./ClipboardPreview.svelte";
  import CircularLoadingIndicator from "./CircularLoadingIndicator.svelte";
  import { entryByIdCommand } from "./lib/tauri.ts";
  import { visualTokenCss } from "./lib/visualTokens.ts";
  import { isImageEntry } from "./lib/clipboardAsset.ts";
  import { t } from "./lib/localization.ts";

  const VISUAL_TOKEN_ROOT_CSS = visualTokenCss();
  void VISUAL_TOKEN_ROOT_CSS;
  const params = new URLSearchParams(window.location.search);
  const parsedEntryId = Number(params.get("entry_id"));
  const entryId = Number.isSafeInteger(parsedEntryId) && parsedEntryId > 0
    ? parsedEntryId
    : null;
  const originLabel = params.get("origin");
  let entry: EntryRecord | null = null;
  let loading = true;

  async function restoreOriginFocus(): Promise<void> {
    if (originLabel !== "main" && originLabel !== "quick-paste") return;
    try {
      const originWindow = await WebviewWindow.getByLabel(originLabel);
      if (!originWindow) return;
      await originWindow.show();
      await originWindow.setFocus();
    } catch {
      // The originating window may have been closed while the preview was open.
    }
  }

  async function closePreviewWindow(): Promise<void> {
    await restoreOriginFocus();
    await getCurrentWindow().destroy();
  }

  function onPreviewKeydown(event: KeyboardEvent): void {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    void closePreviewWindow().catch(() => undefined);
  }

  onMount(() => {
    window.addEventListener("keydown", onPreviewKeydown);

    void (async () => {
      try {
        await getCurrentWebview().setZoom(1);
      } catch {
        // Browser previews do not expose the Tauri webview API.
      }
      if (entryId === null) {
        loading = false;
        return;
      }
      try {
        const record = await entryByIdCommand({ entryId });
        if (record && isImageEntry(record)) entry = record;
      } catch {
        // A missing or deleted capture uses the localized unavailable state.
      } finally {
        loading = false;
      }
    })();

    return () => {
      window.removeEventListener("keydown", onPreviewKeydown);
    };
  });
</script>

{#if entry}
  <ClipboardPreview
    entry={entry}
    testIdPrefix="image-preview"
    accessibleLabel={$t("preview.accessible", { title: entry.title || $t("content_type.image") })}
    standaloneImage={true}
    onClose={() => void closePreviewWindow().catch(() => undefined)}
  />
{:else}
  <div
    class="image-preview-fallback"
    role="main"
    aria-label={$t("preview.window_title")}
    tabindex="-1"
  >
    {#if loading}
      <CircularLoadingIndicator label={$t("common.loading")} />
    {:else}
      <p>{$t("preview.unavailable")}</p>
    {/if}
  </div>
{/if}

<style>
  :global(html),
  :global(body),
  :global(#app) {
    width: 100%;
    height: 100%;
    margin: 0;
    background: #0e1116;
  }

  .image-preview-fallback {
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    padding: 0;
    box-sizing: border-box;
    background: #0e1116;
    color: #f0f4f8;
  }

  .image-preview-fallback p {
    margin: 0;
    text-align: center;
  }

</style>
