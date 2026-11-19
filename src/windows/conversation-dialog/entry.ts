import "$lib/utils/platformAttribute";
import { attachConsole } from "@tauri-apps/plugin-log";

await attachConsole();

import "highlight.js/styles/night-owl.min.css";

import App from "./App.svelte";
import { mount } from "svelte";

const app = mount(App, { target: document.getElementById("app")! });
export default app;
