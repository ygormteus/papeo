import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import Popover from "./lib/Popover.svelte";

if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  const { installDevMock } = await import("./lib/devMock");
  installDevMock();
}

// A mini janela da barra de menus/bandeja usa a mesma interface
// (`src-tauri/src/popover.rs`); `#popover` abre ela no navegador (pnpm dev).
const view = (window as { __PAPEO_VIEW?: string }).__PAPEO_VIEW;
const popover = view === "popover" || location.hash === "#popover";
const app = mount(popover ? Popover : App, { target: document.getElementById("app")! });

export default app;
