import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import Popover from "./lib/Popover.svelte";
import PreviewBar from "./lib/PreviewBar.svelte";
import AttachmentBar from "./lib/AttachmentBar.svelte";
import DetachedReader from "./lib/DetachedReader.svelte";
import { setLang } from "./lib/i18n.svelte";

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
// E a barra da visualização de anexos (`src-tauri/src/attachment.rs`); `#attachment`.
const attachment = view === "attachment" || location.hash === "#attachment";
// Barras de prévia e de anexo não recebem o estado: o backend troca o idioma
// delas por aqui (`lang::set_script`, spec 20).
(window as { __papeoSetLang?: typeof setLang }).__papeoSetLang = setLang;

// E o leitor de uma conta em janela separada (`src-tauri/src/detach.rs`, spec 32).
const reader = view === "reader";

const app = mount(popover ? Popover : preview ? PreviewBar : attachment ? AttachmentBar : reader ? DetachedReader : App, {
  target: document.getElementById("app")!,
});

export default app;
