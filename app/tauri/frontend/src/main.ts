import App from "./App.svelte";
import { mount } from "svelte";

const target = document.getElementById("app");
if (!target) {
  throw new Error("ClipVault frontend: missing #app mount point");
}

const app = mount(App, { target });

export default app;
