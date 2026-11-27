<script lang="ts">
  import { Bold, Eraser, Image as ImageIcon, Italic, Link, Underline, Unlink } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { t, tk } from "./i18n.svelte";
  import { cleanPasted, prepareImage, signatureText } from "./signatureHtml";

  // Editor da assinatura formatada (spec 22): o texto como vai sair, com
  // negrito, itálico, sublinhado, cor, tamanho, link e imagem.
  let {
    html = $bindable(""),
    onerror,
    onsave,
  }: {
    /** HTML da assinatura (já limpo pelo backend ao abrir). */
    html: string;
    onerror: (message: string) => void;
    /** ⌘/Ctrl+Enter. */
    onsave?: () => void;
  } = $props();

  let box: HTMLDivElement | undefined = $state();
  let picker: HTMLInputElement | undefined = $state();
  let saved: Range | null = null;
  let linking = $state(false);
  let linkUrl = $state("");
  let image = $state<HTMLImageElement | null>(null);
  let busy = $state(false);

  const COLORS = ["#1f2937", "#6b7280", "#b91c1c", "#c2410c", "#a16207", "#15803d", "#0e7490", "#1d4ed8", "#6d28d9", "#be185d"];
  const SIZES = [
    { value: "2", label: tk("Pequeno") },
    { value: "3", label: tk("Normal") },
    { value: "5", label: tk("Grande") },
  ];
  const WIDTHS = [
    { width: 120, label: "P" },
    { width: 200, label: "M" },
    { width: 320, label: "G" },
  ];

  onMount(() => {
    if (box) box.innerHTML = html;
    document.execCommand("styleWithCSS", false, "false");
  });

  /** Texto da assinatura (a versão que vai no editor da mensagem). */
  export function text(): string {
    return box ? signatureText(box, t("Logo")) : "";
  }

  /** Apaga tudo. */
  export function clear() {
    if (box) box.innerHTML = "";
    image = null;
    sync();
  }

  function sync() {
    if (box) html = box.innerHTML;
  }

  function remember() {
    const sel = getSelection();
    if (sel && sel.rangeCount && box?.contains(sel.anchorNode)) saved = sel.getRangeAt(0).cloneRange();
  }

  function restore() {
    box?.focus();
    if (saved) {
      const sel = getSelection();
      sel?.removeAllRanges();
      sel?.addRange(saved);
    }
  }

  function exec(command: string, value?: string) {
    restore();
    document.execCommand(command, false, value);
    remember();
    sync();
  }

  function startLink() {
    remember();
    const a = (saved?.startContainer.parentElement ?? null)?.closest("a");
    linkUrl = a?.getAttribute("href") ?? "https://";
    linking = true;
  }

  function applyLink() {
    let url = linkUrl.trim();
    if (url && !/^(https?:|mailto:|tel:)/i.test(url)) url = url.includes("@") ? `mailto:${url}` : `https://${url}`;
    linking = false;
    if (!url || url === "https://") return;
    restore();
    if (saved?.collapsed) {
      document.execCommand("insertHTML", false, `<a href="${url.replace(/"/g, "&quot;")}">${escapeText(url.replace(/^mailto:/i, ""))}</a>`);
    } else {
      document.execCommand("createLink", false, url);
    }
    remember();
    sync();
  }

  function escapeText(s: string) {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  async function insertFiles(files: File[]) {
    const images = files.filter((f) => f.type.startsWith("image/"));
    if (!images.length) return;
    busy = true;
    try {
      for (const file of images) {
        const url = await prepareImage(file);
        if (!url) {
          onerror(t("Não deu para usar {nome}: use PNG, JPG, GIF ou WebP de até 200 KB.", { nome: file.name }));
          continue;
        }
        restore();
        document.execCommand("insertHTML", false, `<img src="${url}" alt="${escapeText(t("Logo"))}">`);
        remember();
      }
      sync();
    } catch {
      onerror(t("Não deu para abrir a imagem."));
    } finally {
      busy = false;
    }
  }

  function paste(e: ClipboardEvent) {
    const data = e.clipboardData;
    if (!data) return;
    e.preventDefault();
    const files = [...data.files];
    if (files.length) {
      remember();
      void insertFiles(files);
      return;
    }
    const pasted = data.getData("text/html");
    if (pasted) document.execCommand("insertHTML", false, cleanPasted(pasted));
    else document.execCommand("insertText", false, data.getData("text/plain"));
    sync();
  }

  function drop(e: DragEvent) {
    const data = e.dataTransfer;
    if (!data) return;
    e.preventDefault();
    const files = [...data.files];
    if (files.length) {
      remember();
      void insertFiles(files);
      return;
    }
    // Texto arrastado passa pelo mesmo limpador do colar.
    const range = document.caretRangeFromPoint?.(e.clientX, e.clientY);
    if (range) {
      const sel = getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    }
    const dropped = data.getData("text/html");
    if (dropped) document.execCommand("insertHTML", false, cleanPasted(dropped));
    else document.execCommand("insertText", false, data.getData("text/plain"));
    remember();
    sync();
  }

  function click(e: MouseEvent) {
    image = e.target instanceof HTMLImageElement ? e.target : null;
  }

  function resize(width: number | null) {
    if (!image) return;
    if (width) image.setAttribute("width", String(width));
    else image.removeAttribute("width");
    image.removeAttribute("height");
    image.style.removeProperty("width");
    image.style.removeProperty("height");
    sync();
  }

  function removeImage() {
    image?.remove();
    image = null;
    sync();
  }
</script>

<div class="editor">
  <div class="tools" role="toolbar" aria-label={t("Formatação")}>
    <button type="button" onmousedown={(e) => e.preventDefault()} onclick={() => exec("bold")} title={t("Negrito")} aria-label={t("Negrito")}><Bold size={15} /></button>
    <button type="button" onmousedown={(e) => e.preventDefault()} onclick={() => exec("italic")} title={t("Itálico")} aria-label={t("Itálico")}><Italic size={15} /></button>
    <button type="button" onmousedown={(e) => e.preventDefault()} onclick={() => exec("underline")} title={t("Sublinhado")} aria-label={t("Sublinhado")}><Underline size={15} /></button>
    <select
      aria-label={t("Tamanho do texto")}
      title={t("Tamanho do texto")}
      onmousedown={remember}
      onchange={(e) => {
        exec("fontSize", e.currentTarget.value);
        e.currentTarget.value = "";
      }}
    >
      <option value="" disabled selected>{t("Tamanho")}</option>
      {#each SIZES as size (size.value)}<option value={size.value}>{t(size.label)}</option>{/each}
    </select>
    <span class="colors" role="group" aria-label={t("Cor do texto")}>
      {#each COLORS as color (color)}
        <button
          type="button"
          class="swatch"
          style:background={color}
          onmousedown={(e) => e.preventDefault()}
          onclick={() => exec("foreColor", color)}
          title={t("Cor do texto")}
          aria-label={t("Cor {cor}", { cor: color })}
        ></button>
      {/each}
    </span>
    <button type="button" onmousedown={(e) => e.preventDefault()} onclick={startLink} title={t("Link")} aria-label={t("Link")}><Link size={15} /></button>
    <button type="button" onmousedown={(e) => e.preventDefault()} onclick={() => exec("unlink")} title={t("Tirar link")} aria-label={t("Tirar link")}><Unlink size={15} /></button>
    <button
      type="button"
      onmousedown={(e) => {
        e.preventDefault();
        remember();
      }}
      onclick={() => picker?.click()}
      disabled={busy}
      title={t("Imagem ou logo")}
      aria-label={t("Imagem ou logo")}
    >
      <ImageIcon size={15} />
    </button>
    <button
      type="button"
      onmousedown={(e) => e.preventDefault()}
      onclick={() => {
        exec("removeFormat");
        exec("unlink");
      }}
      title={t("Limpar formatação")}
      aria-label={t("Limpar formatação")}
    >
      <Eraser size={15} />
    </button>
  </div>

  {#if linking}
    <form
      class="link"
      onsubmit={(e) => {
        e.preventDefault();
        applyLink();
      }}
    >
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" bind:value={linkUrl} aria-label={t("Endereço do link")} placeholder="https://" autofocus />
      <button class="btn btn-primary" type="submit">{t("Aplicar")}</button>
      <button class="btn" type="button" onclick={() => (linking = false)}>{t("Cancelar")}</button>
    </form>
  {/if}

  {#if image}
    <div class="image-tools" role="group" aria-label={t("Imagem selecionada")}>
      <span>{t("Imagem:")}</span>
      {#each WIDTHS as w (w.width)}
        <button type="button" class="btn" onclick={() => resize(w.width)} title={t("{n} px de largura", { n: w.width })}>{w.label}</button>
      {/each}
      <button type="button" class="btn" onclick={() => resize(null)}>{t("Original")}</button>
      <button type="button" class="btn" onclick={removeImage}>{t("Remover")}</button>
    </div>
  {/if}

  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div
    bind:this={box}
    class="content"
    contenteditable="true"
    role="textbox"
    aria-multiline="true"
    aria-label={t("Assinatura")}
    tabindex="0"
    oninput={sync}
    onkeyup={remember}
    onkeydown={(e) => {
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        sync();
        onsave?.();
      }
    }}
    onmouseup={remember}
    onblur={remember}
    onpaste={paste}
    ondrop={drop}
    onclick={click}
    data-placeholder={t("Ana Souza\nLoja Exemplo · (11) 0000-0000")}
  ></div>

  <input
    bind:this={picker}
    type="file"
    accept="image/png,image/jpeg,image/gif,image/webp"
    hidden
    onchange={(e) => {
      const files = [...(e.currentTarget.files ?? [])];
      e.currentTarget.value = "";
      void insertFiles(files);
    }}
  />
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    overflow: hidden;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
    padding: 4px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  .tools > button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text);
    cursor: pointer;
  }
  .tools > button:hover {
    background: var(--bg-hover);
  }
  .tools select {
    height: 28px;
    font-size: 12.5px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
  }
  .colors {
    display: inline-flex;
    gap: 3px;
    margin: 0 4px;
  }
  .swatch {
    width: 16px;
    height: 16px;
    padding: 0;
    border: 1px solid rgba(0, 0, 0, 0.15);
    border-radius: 50%;
    cursor: pointer;
  }
  .link,
  .image-tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 6px;
    border-bottom: 1px solid var(--border);
    font-size: 13px;
  }
  .link .input {
    flex: 1;
    min-width: 160px;
  }
  .content {
    min-height: 120px;
    max-height: 320px;
    overflow: auto;
    padding: 10px 12px;
    outline: none;
    line-height: 1.4;
    /* A assinatura sai num e-mail: fundo claro como na maioria dos programas. */
    background: #fff;
    color: #1f2937;
  }
  .content:empty::before {
    content: attr(data-placeholder);
    white-space: pre-line;
    color: #9ca3af;
  }
  .content :global(img) {
    max-width: 100%;
    cursor: pointer;
  }
  .content :global(a) {
    color: #1d4ed8;
  }
</style>
