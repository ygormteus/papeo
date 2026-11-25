<script lang="ts">
  import { onMount } from "svelte";
  import { AppWindow, Check, Download, FileText, FolderOpen, X } from "@lucide/svelte";
  import { api, errorMessage, isMac, onAttachment, type AttachmentInfo } from "./api";

  // Barra da visualização de anexos (`src-tauri/src/attachment.rs`). O
  // arquivo aparece abaixo (Quick Look no Mac, visualizador do Edge no
  // Windows). Sem visualização (Excel/Word no Windows), a barra ocupa a
  // janela e oferece abrir no app.

  let info = $state<AttachmentInfo>({ fileName: "", view: null, saved: false });
  let error = $state("");
  let busy = $state(false);

  const name = $derived(info.fileName || "Anexo");
  const external = $derived(info.view === "external");

  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  let dark = $state(systemDark.matches);
  $effect(() => {
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    document.title = name;
  });

  async function act(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = "";
    try {
      await action();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  const save = () => act(api.attachmentSave);
  const reveal = () => act(api.attachmentReveal);
  const openApp = () => act(api.attachmentOpenApp);
  // Fechar não espera um salvamento em curso (a cópia termina sozinha).
  const close = () => api.attachmentClose().catch(() => {});

  function onkeydown(e: KeyboardEvent) {
    const mod = isMac ? e.metaKey : e.ctrlKey;
    if (e.key === "Escape") close();
    else if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    } else if (mod && e.key.toLowerCase() === "w") {
      e.preventDefault();
      close();
    }
  }

  onMount(() => {
    const onScheme = (e: MediaQueryListEvent) => (dark = e.matches);
    systemDark.addEventListener("change", onScheme);
    api.attachmentState().then((i) => (info = i)).catch(() => {});
    const off = onAttachment((i) => {
      info = i;
      error = "";
    });
    return () => {
      systemDark.removeEventListener("change", onScheme);
      off.then((f) => f());
    };
  });
</script>

<svelte:window {onkeydown} />

<header class="bar" class:mac={isMac} data-tauri-drag-region>
  <FileText size={15} class="file-icon" aria-hidden="true" />
  <span class="title" data-tauri-drag-region title={name}>{name}</span>
  {#if error}<span class="error" role="alert" title={error}>{error}</span>{/if}
  {#if info.saved}
    <span class="saved" role="status"><Check size={14} /> Salvo</span>
    <button class="icon-btn ghost" onclick={reveal} disabled={busy} title="Mostrar na pasta" aria-label="Mostrar na pasta">
      <FolderOpen size={16} />
    </button>
  {:else}
    <button class="btn small" onclick={save} disabled={busy} title="Salvar em Downloads ({isMac ? '⌘' : 'Ctrl+'}S)">
      <Download size={15} /> Salvar
    </button>
  {/if}
  <button
    class="btn small"
    onclick={openApp}
    disabled={busy}
    title="Salva em Downloads e abre no app padrão (Excel, Word, leitor de PDF…) para editar"
  >
    <AppWindow size={15} /> Abrir no app
  </button>
  <button class="icon-btn ghost" onclick={close} title="Fechar (Esc)" aria-label="Fechar o anexo">
    <X size={17} />
  </button>
</header>

{#if external}
  <main class="external">
    <FileText size={40} strokeWidth={1.5} aria-hidden="true" />
    <p class="name">{name}</p>
    <p class="hint">
      O Windows não mostra este tipo de arquivo sem o programa dele. Abra no app para ver e editar: o Papeo salva uma
      cópia em Downloads antes, então nada do que você editar se perde.
    </p>
    <button class="btn btn-primary" onclick={openApp} disabled={busy}><AppWindow size={16} /> Abrir no app</button>
  </main>
{/if}

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

  .bar :global(.file-icon) {
    flex: none;
    color: var(--text-faint);
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    font-weight: 600;
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

  .saved {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12.5px;
    color: var(--text-muted);
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

  .external {
    height: calc(100% - 44px);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 24px;
    box-sizing: border-box;
    text-align: center;
    color: var(--text-muted);
    background: var(--bg);
  }

  .name {
    margin: 0;
    font-weight: 600;
    color: var(--text);
    word-break: break-word;
  }

  .hint {
    margin: 0 0 6px;
    max-width: 420px;
    font-size: 13px;
    line-height: 1.5;
  }

  /* Janela estreita: só os ícones nos botões da barra. */
  @media (max-width: 560px) {
    .saved {
      display: none;
    }
  }
</style>
