import { mount } from "svelte";

import "./app.css";
import App from "./App.svelte";
import { installErrorLog } from "./lib/kernel/log";

// First, so errors while mounting and booting reach the terminal too.
installErrorLog();

const target = document.getElementById("app");

if (!target) {
  throw new Error("Manang: #app mount target is missing from index.html");
}

const app = mount(App, { target });

export default app;
