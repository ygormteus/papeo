import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import Popover from "./lib/Popover.svelte";
import PreviewBar from "./lib/PreviewBar.svelte";

if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  const { installDevMock } = await import("./lib/devMock");
  installDevMock();
}

// A mini janela da barra de menus/bandeja usa a mesma interface
// (`src-tauri/src/popover.rs`); `#popover` abre ela no navegador (pnpm dev).
const view = (window as { __PAPEO_VIEW?: string }).__PAPEO_VIEW;
// A barra da prévia de links também (`src-tauri/src/preview.rs`); `#preview` no navegador.
const popover = view === "popover" || location.hash === "#popover";
const preview = view === "preview" || location.hash === "#preview";
const app = mount(popover ? Popover : preview ? PreviewBar : App, {
  target: document.getElementById("app")!,
});

export default app;
