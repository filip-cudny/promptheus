import "$lib/utils/platformAttribute";
import { attachConsole } from "@tauri-apps/plugin-log";
import App from "./App.svelte";
import { mount } from "svelte";

await attachConsole();

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
