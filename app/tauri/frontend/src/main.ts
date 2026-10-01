import App from "./App.svelte";
import { initializeLocalization } from "./lib/localization.ts";
import { mount } from "svelte";

const target = document.getElementById("app");
if (!target) {
  throw new Error("ClipVault frontend: missing #app mount point");
}

const app = initializeLocalization().then(() => mount(App, { target }));

export default app;
