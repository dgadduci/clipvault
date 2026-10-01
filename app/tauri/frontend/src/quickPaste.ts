import QuickPaste from "./QuickPaste.svelte";
import { initializeLocalization } from "./lib/localization.ts";
import { mount } from "svelte";

const target = document.getElementById("app");
if (!target) {
  throw new Error("ClipVault quick-paste: missing #app mount point");
}

const app = initializeLocalization().then(() => mount(QuickPaste, { target }));

export default app;
