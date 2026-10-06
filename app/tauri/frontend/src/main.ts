import App from "./App.svelte";
import StartupSplash from "./StartupSplash.svelte";
import { initializeLocalization } from "./lib/localization.ts";
import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";

const target = document.getElementById("app");
if (!target) {
  throw new Error("ClipVault frontend: missing #app mount point");
}

let Root = App;
let isStartupSplash = false;
try {
  isStartupSplash = getCurrentWindow().label === "startup-splash";
  if (isStartupSplash) Root = StartupSplash;
} catch {
  // Browser previews render the main desktop without a Tauri window.
}

const app = initializeLocalization({ waitForBackend: !isStartupSplash })
  .then(() => mount(Root, { target }));

export default app;
