<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { emitTo, listen } from "@tauri-apps/api/event";
  import { getCurrentWindow, Window } from "@tauri-apps/api/window";
  import logo from "../../src-tauri/icons/clipvault-mark-distinct.png";
  import { setLocale, t } from "./lib/localization.ts";
  import { startupStatusCommand } from "./lib/tauri.ts";
  import {
    canRevealStartupDesktop,
    scheduleMinimumSplashDuration,
    waitForStartupStatus,
  } from "./lib/startup.ts";

  const STARTUP_READY_EVENT = "clipvault://startup-ready";
  const SPLASH_READY_EVENT = "clipvault://splash-ready";

  let minimumElapsed = false;
  let desktopReady = false;
  let revealingDesktop = false;
  let cancelMinimumDurationTimer: (() => void) | null = null;
  let handshakeTimer: ReturnType<typeof setInterval> | null = null;
  let stopListening: (() => void) | null = null;

  async function revealDesktop(): Promise<void> {
    if (!minimumElapsed || !desktopReady || revealingDesktop) return;
    revealingDesktop = true;
    try {
      const mainWindow = await Window.getByLabel("main");
      if (!mainWindow) throw new Error("main window is unavailable");
      await mainWindow.show();
      await mainWindow.setFocus();
      await getCurrentWindow().close();
    } catch {
      revealingDesktop = false;
    }
  }

  $: if (canRevealStartupDesktop(minimumElapsed, desktopReady)) {
    void revealDesktop();
  }

  onMount(() => {
    cancelMinimumDurationTimer = scheduleMinimumSplashDuration(() => {
      minimumElapsed = true;
    });

    void listen(STARTUP_READY_EVENT, () => {
      desktopReady = true;
      if (handshakeTimer !== null) {
        clearInterval(handshakeTimer);
        handshakeTimer = null;
      }
    }).then((unlisten) => {
      stopListening = unlisten;
      // Repeat until the desktop confirms readiness so the two webviews may
      // finish loading in either order without losing the handshake event.
      handshakeTimer = setInterval(() => {
        void emitTo("main", SPLASH_READY_EVENT).catch(() => undefined);
      }, 150);
      void emitTo("main", SPLASH_READY_EVENT).catch(() => undefined);
    }).catch(() => {
      // The splash runs only in Tauri; the status poll still provides a
      // recoverable path if event registration is temporarily unavailable.
    });

    void waitForStartupStatus(startupStatusCommand)
      .then((status) => setLocale(status.language))
      .catch(() => setLocale("en"));
  });

  onDestroy(() => {
    cancelMinimumDurationTimer?.();
    if (handshakeTimer !== null) clearInterval(handshakeTimer);
    stopListening?.();
  });
</script>

<svelte:head>
  <title>ClipVault</title>
</svelte:head>

<main class="startup-splash" data-testid="startup-splash">
  <img class="startup-splash-logo" src={logo} alt="" />
  <p class="startup-splash-tagline">{$t("app.splash.tagline")}</p>
  <span class="startup-splash-spinner" role="status" aria-label={$t("app.startup.loading")}></span>
</main>

<style>
  :global(html, body, #app) {
    width: 100%;
    height: 100%;
    margin: 0;
    background: #0e1116;
  }

  .startup-splash {
    box-sizing: border-box;
    display: flex;
    width: 100%;
    height: 100%;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    padding: 2rem;
    color: #f0f4f8;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    text-align: center;
    user-select: none;
  }

  .startup-splash-logo {
    width: 112px;
    height: 112px;
    object-fit: contain;
  }

  .startup-splash-tagline {
    margin: 0;
    color: #cbd5e1;
    font-size: 1rem;
  }

  .startup-splash-spinner {
    width: 1.25rem;
    height: 1.25rem;
    border: 2px solid #30363d;
    border-top-color: #60a5fa;
    border-right-color: #60a5fa;
    border-radius: 50%;
    animation: clipvault-splash-spin 0.72s linear infinite;
  }

  @keyframes clipvault-splash-spin {
    to { transform: rotate(360deg); }
  }

  @media (prefers-reduced-motion: reduce) {
    .startup-splash-spinner {
      animation: none;
    }
  }
</style>
