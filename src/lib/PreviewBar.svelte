<script lang="ts">
  import { onMount } from "svelte";
  import { ExternalLink, Globe, X } from "@lucide/svelte";
  import { api, errorMessage, isMac, onPreview, type PreviewInfo } from "./api";

  // Barra da prévia de links (`src-tauri/src/preview.rs`): o site fica na
  // webview de baixo, sem acesso a nada do Papeo.

  let info = $state<PreviewInfo>({ url: "", title: "" });
  let error = $state("");

  const host = $derived.by(() => {
    try {
      return new URL(info.url).host;
    } catch {
      return "";
    }
  });
  const title = $derived(info.title.trim() || host || "Prévia");

  // O tema da janela segue a aparência escolhida no app.
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  let dark = $state(systemDark.matches);
  $effect(() => {
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    document.title = title;
  });

  async function act(action: Promise<void>) {
    try {
      error = "";
      await action;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  const openBrowser = () => act(api.previewOpenBrowser());
  const close = () => act(api.previewClose());

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") close();
    else if (e.key === "Enter" && (isMac ? e.metaKey : e.ctrlKey)) openBrowser();
  }

  onMount(() => {
    const onScheme = (e: MediaQueryListEvent) => (dark = e.matches);
    systemDark.addEventListener("change", onScheme);
    api.previewState().then((i) => (info = i)).catch(() => {});
    const off = onPreview((i) => (info = i));
    return () => {
      systemDark.removeEventListener("change", onScheme);
      off.then((f) => f());
    };
  });
</script>

<svelte:window {onkeydown} />

<header class="bar" class:mac={isMac} data-tauri-drag-region>
  <Globe size={15} class="globe" aria-hidden="true" />
  <div class="where" data-tauri-drag-region>
    <span class="title" data-tauri-drag-region {title}>{title}</span>
    {#if host && host !== title}<span class="host" data-tauri-drag-region>{host}</span>{/if}
  </div>
  {#if error}<span class="error" role="alert">{error}</span>{/if}
  <button class="btn small" onclick={openBrowser} title="Abrir no navegador ({isMac ? '⌘' : 'Ctrl+'}Enter)">
    <ExternalLink size={15} /> Abrir no navegador
  </button>
  <button class="icon-btn ghost" onclick={close} title="Fechar (Esc)" aria-label="Fechar a prévia">
    <X size={17} />
  </button>
</header>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    overflow: hidden;
    background: var(--bg-sidebar);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    box-sizing: border-box;
    padding: 0 8px 0 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-sidebar);
    color: var(--text);
    user-select: none;
    -webkit-user-select: none;
  }

  /* Espaço para os botões de janela do macOS. */
  .bar.mac {
    padding-left: 80px;
  }

  .bar :global(.globe) {
    flex: none;
    color: var(--text-faint);
  }

  .where {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    font-weight: 600;
  }

  .host {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--text-muted);
  }

  .error {
    flex: none;
    max-width: 30%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--danger);
  }

  .btn.small {
    flex: none;
    height: 30px;
    padding: 0 10px;
    font-size: 13px;
  }

  .icon-btn {
    flex: none;
    width: 30px;
    height: 30px;
  }
</style>
