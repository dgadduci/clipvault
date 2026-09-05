import QuickPaste from "./QuickPaste.svelte";
import { mount } from "svelte";

const target = document.getElementById("app");
if (!target) {
  throw new Error("ClipVault quick-paste: missing #app mount point");
}

const app = mount(QuickPaste, { target });

export default app;
