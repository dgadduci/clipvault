import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";

import { matchesDevelopmentShortcut } from "../src/lib/developmentShortcut.ts";
import {
  canRevealStartupDesktop,
  MINIMUM_SPLASH_DURATION_MS,
  scheduleMinimumSplashDuration,
  waitForStartupStatus,
} from "../src/lib/startup.ts";

const frontendRoot = process.cwd();

function source(...segments: string[]): string {
  return readFileSync(path.join(frontendRoot, ...segments), "utf8");
}

test("Development shortcut is local to the main window and platform specific", () => {
  assert.equal(matchesDevelopmentShortcut({
    key: "D", ctrlKey: true, altKey: true, shiftKey: true, metaKey: true,
  }, true), true);
  assert.equal(matchesDevelopmentShortcut({
    key: "d", ctrlKey: true, altKey: true, shiftKey: true, metaKey: false,
  }, false), true);
  assert.equal(matchesDevelopmentShortcut({
    key: "d", ctrlKey: true, altKey: true, shiftKey: false, metaKey: false,
  }, false), false);
  assert.equal(matchesDevelopmentShortcut({
    key: "d", ctrlKey: true, altKey: true, shiftKey: true, metaKey: true,
  }, false), false);

  const app = source("src/App.svelte");
  const toolbar = source("src/DesktopToolbar.svelte");
  assert.match(app, /matchesDevelopmentShortcut\(event, macos\)/);
  assert.match(app, /openModal !== null[\s\S]*?guidance !== null[\s\S]*?isShortcutBlockedSurface/);
  assert.match(app, /openModalWith\("development", trigger\)/);
  assert.doesNotMatch(toolbar, /open-development|onOpenDevelopment|toolbar\.menu\.development/);
});

test("Keyboard shortcuts stay in the overflow menu and are absent from General Settings", () => {
  const settings = source("src/GeneralSettingsModal.svelte");
  const toolbar = source("src/DesktopToolbar.svelte");
  const app = source("src/App.svelte");
  assert.doesNotMatch(settings, /keyboard-shortcuts-setting|keyboardShortcutsRequested/);
  assert.match(toolbar, /data-testid="open-shortcut"/);
  assert.match(app, /onOpenShortcut=\{onOpenShortcut\}/);
});

test("startup splash gates the hidden desktop and uses the shared minimum duration", async () => {
  const tauriConfig = JSON.parse(source("../src-tauri/tauri.conf.json"));
  const defaultCapability = JSON.parse(source("../src-tauri/capabilities/default.json"));
  const splashCapability = JSON.parse(source("../src-tauri/capabilities/startup-splash.json"));
  const windows = tauriConfig.app.windows;
  const mainEntry = source("src/main.ts");
  const html = source("index.html");
  const localization = source("src/lib/localization.ts");
  assert.equal(windows.find((window: { label: string }) => window.label === "main").visible, false);
  assert.equal(windows.find((window: { label: string }) => window.label === "startup-splash").visible, true);
  assert.equal(windows.find((window: { label: string }) => window.label === "startup-splash").backgroundColor, "#0e1116");
  assert.equal(windows.find((window: { label: string }) => window.label === "startup-splash").skipTaskbar, true);
  assert.equal(splashCapability.windows.length, 1);
  assert.deepEqual(splashCapability.windows, ["startup-splash"]);
  assert.deepEqual(splashCapability.permissions, ["core:window:allow-close"]);
  assert.equal(defaultCapability.permissions.includes("core:window:allow-close"), false);
  assert.match(mainEntry, /isStartupSplash\s*\?\s*import\("\.\/StartupSplash\.svelte"\)[\s\S]*import\("\.\/App\.svelte"\)/);
  assert.match(mainEntry, /initializeLocalization\(\{ waitForBackend: !isStartupSplash \}\)/);
  assert.doesNotMatch(mainEntry, /^import App from/m);
  assert.match(html, /background:\s*#0e1116/);
  assert.match(localization, /waitForStartupStatus\(startupStatusCommand\)/);
  assert.doesNotMatch(localization, /settingsGetCommand/);

  const splash = source("src/StartupSplash.svelte");
  const app = source("src/App.svelte");
  const distinctiveMark = readFileSync(path.join(frontendRoot, "../src-tauri/icons/clipvault-mark-distinct.png"));
  const trayIcon = readFileSync(path.join(frontendRoot, "../src-tauri/icons/tray-icon.png"));
  assert.match(splash, /clipvault-mark-distinct\.png/);
  assert.doesNotMatch(splash, /clipvault-logo\.png/);
  assert.equal(distinctiveMark.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  assert.equal(distinctiveMark[25], 6, "splash logo is an RGBA PNG");
  assert.equal(trayIcon.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  assert.equal(trayIcon[25], 6, "tray logo is an RGBA PNG");
  assert.match(splash, /canRevealStartupDesktop\(minimumElapsed, desktopReady\)/);
  assert.match(splash, /scheduleMinimumSplashDuration/);
  assert.match(splash, /mainWindow\.show\(\)[\s\S]*mainWindow\.setFocus\(\)/);
  assert.match(splash, /startupStatusCommand/);
  assert.match(app, /await waitForStartupStatus\(startupStatusCommand\)/);
  assert.match(app, /initialDesktopLoadSettled = true/);
  assert.match(app, /startup\.status === "failed"[\s\S]*?initialDesktopLoadSettled = true/);

  const states = [
    { status: "pending" as const, language: "en" },
    { status: "ready" as const, language: "es" },
  ];
  const settled = await waitForStartupStatus(async () => states.shift()!, 0);
  assert.deepEqual(settled, { status: "ready", language: "es" });

  let minimumElapsed = false;
  const timerCallbacks: Array<() => void> = [];
  let scheduledDelay = 0;
  let cancelledTimer: unknown;
  const cancelTimer = scheduleMinimumSplashDuration(
    () => { minimumElapsed = true; },
    (callback, delayMs) => {
      timerCallbacks.push(callback);
      scheduledDelay = delayMs;
      return "fake-timer";
    },
    (timer) => { cancelledTimer = timer; },
  );
  assert.equal(scheduledDelay, MINIMUM_SPLASH_DURATION_MS);
  assert.equal(MINIMUM_SPLASH_DURATION_MS, 3000);
  assert.equal(canRevealStartupDesktop(minimumElapsed, true), false);
  timerCallbacks[0]?.();
  assert.equal(canRevealStartupDesktop(minimumElapsed, false), false);
  assert.equal(canRevealStartupDesktop(minimumElapsed, true), true);
  cancelTimer();
  assert.equal(cancelledTimer, "fake-timer");

  minimumElapsed = false;
  const failedTimerCallbacks: Array<() => void> = [];
  const failedStartupIsSettled = true;
  const cancelFailedTimer = scheduleMinimumSplashDuration(
    () => { minimumElapsed = true; },
    (callback) => { failedTimerCallbacks.push(callback); return "failed-timer"; },
    () => undefined,
  );
  assert.equal(canRevealStartupDesktop(minimumElapsed, failedStartupIsSettled), false);
  failedTimerCallbacks[0]?.();
  assert.equal(canRevealStartupDesktop(minimumElapsed, failedStartupIsSettled), true);
  cancelFailedTimer();

  assert.equal(canRevealStartupDesktop(false, true), false);
  assert.equal(canRevealStartupDesktop(true, false), false);
  assert.equal(canRevealStartupDesktop(true, true), true);
});

test("Linux window identity matches the packaged desktop entry and retains its icon variable", () => {
  const linuxConfig = JSON.parse(source("../src-tauri/tauri.linux.conf.json"));
  const appConfig = JSON.parse(source("../src-tauri/tauri.conf.json"));
  const template = source("../src-tauri/desktop-template.desktop");
  const hiddenLauncher = source("../src-tauri/desktop-launcher-hidden.desktop");
  const waylandEntry = source("../src-tauri/desktop-entry-wayland.desktop");
  const devEntryHelper = source("../scripts/install-kde-dev-desktop-entry.sh");
  const tray = source("../src-tauri/src/tray.rs");
  const main = source("../src-tauri/src/main.rs");
  assert.equal(linuxConfig.app.enableGTKAppId, true);
  assert.equal(linuxConfig.bundle.linux.deb.desktopTemplate, "desktop-template.desktop");
  assert.equal(linuxConfig.bundle.linux.deb.files[`/usr/share/applications/${appConfig.productName}.desktop`], "desktop-launcher-hidden.desktop");
  assert.equal(linuxConfig.bundle.linux.appimage.files[`/usr/share/applications/${appConfig.productName}.desktop`], "desktop-launcher-hidden.desktop");
  assert.match(template, new RegExp(`^StartupWMClass=${appConfig.identifier}$`, "m"));
  assert.match(template, /^Icon=\{\{icon\}\}$/m);
  assert.match(hiddenLauncher, /^NoDisplay=true$/m);
  assert.doesNotMatch(hiddenLauncher, /\{\{/);
  assert.match(waylandEntry, new RegExp(`^StartupWMClass=${appConfig.identifier}$`, "m"));
  assert.doesNotMatch(waylandEntry, /^NoDisplay=true$/m);
  assert.doesNotMatch(devEntryHelper, /^NoDisplay=true$/m);
  assert.match(tray, /include_bytes!\([\s\S]*?tray-icon\.png/);
  assert.match(tray, /\.icon_as_template\(cfg!\(target_os = "macos"\)\)/);
  assert.match(tray, /include_bytes!\("\.\.\/icons\/icon\.png"\)/);
  assert.match(tray, /apply_linux_main_window_icon\(&window\)/);
  assert.match(main, /apply_linux_main_window_icon\(&window\)/);
  assert.equal(appConfig.app.windows.find((window: { label: string }) => window.label === "quick-paste").skipTaskbar, true);
  assert.match(main, /GTK_APP_ACTIVATED\.swap\(true, Ordering::AcqRel\)/);
  assert.match(main, /status\.snapshot\(\)\.status == "ready"[\s\S]*?"main"[\s\S]*?"startup-splash"/);
  assert.match(main, /TauriTrayController::install\(/);
});

test("new product status copy has matching entries in all supported locales", () => {
  for (const locale of ["en", "es", "pt", "de", "fr"]) {
    const catalog = JSON.parse(source("src", "locales", `${locale}.json`));
    assert.ok(catalog["app.startup.loading"]);
    assert.ok(catalog["app.splash.tagline"]);
    assert.ok(catalog["collections.loading"]);
    assert.equal(catalog["toolbar.menu.development"], undefined);
    assert.equal(catalog["app.backend.connecting"], undefined);
  }
});
