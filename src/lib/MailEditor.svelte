<script lang="ts" module>
  /** Marcador do bloco da assinatura no HTML enviado (`signature::SIGNATURE_TOKEN`). */
  export const SIGNATURE_TOKEN = "\u{E000}papeo-signature\u{E000}";

  const KEEP = new Set([
    "B", "STRONG", "I", "EM", "U", "S", "STRIKE", "BR", "P", "DIV", "SPAN", "FONT", "UL", "OL", "LI", "A",
    "BLOCKQUOTE", "H1", "H2", "H3", "H4", "SMALL", "BIG", "SUB", "SUP", "HR", "CENTER",
    "TABLE", "TBODY", "THEAD", "TR", "TD", "TH",
  ]);
  /** Tirados com o conteúdo (o resto que não está em `KEEP` some só a tag). */
  const DROP = new Set(["SCRIPT", "STYLE", "IFRAME", "OBJECT", "EMBED", "IMG", "SVG", "MATH", "TEMPLATE", "HEAD", "TITLE", "META", "LINK", "FORM", "INPUT", "BUTTON", "TEXTAREA", "SELECT", "VIDEO", "AUDIO", "CANVAS"]);
  const STYLES = ["color", "background-color", "font-size", "font-weight", "font-style", "font-family", "text-decoration", "text-align"];
  const ATTRS = ["align", "color", "size", "face"];

  /** HTML que entra no editor (rascunho reaberto, e-mail que voltou):
   * só o que a formatação usa, sem scripts nem eventos. O backend limpa de
   * novo ao enviar. */
  export function cleanHtml(html: string): string {
    const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
    const walk = (node: Element) => {
      for (const child of Array.from(node.children)) {
        if (DROP.has(child.tagName)) {
          child.remove();
          continue;
        }
        walk(child);
        if (!KEEP.has(child.tagName)) {
          child.replaceWith(...Array.from(child.childNodes));
          continue;
        }
        for (const attr of Array.from(child.attributes)) {
          const name = attr.name.toLowerCase();
          if (ATTRS.includes(name)) continue;
          if (name === "href" && child.tagName === "A" && /^(https?:|mailto:)/i.test(attr.value.trim())) continue;
          if (name === "class" && child.tagName === "DIV" && attr.value.split(/\s+/).includes("papeo-signature")) {
            child.setAttribute("class", "papeo-signature");
            continue;
          }
          if (name === "style") {
            const style = (child as HTMLElement).style;
            const kept = STYLES.map((p) => [p, style.getPropertyValue(p)] as const).filter(([, v]) => v && !/url\(|expression/i.test(v));
            child.removeAttribute("style");
            for (const [p, v] of kept) (child as HTMLElement).style.setProperty(p, v);
            continue;
          }
          child.removeAttribute(attr.name);
        }
      }
    };
    walk(doc.body);
    return doc.body.innerHTML;
  }

  function escape(text: string) {
    return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  /** Linhas de texto no formato que o editor usa (um `div` por linha). */
  export function textHtml(text: string): string {
    if (!text) return "";
    return text
      .split("\n")
      .map((line) => (line ? `<div>${escape(line).replace(/ {2}/g, "&nbsp; ")}</div>` : "<div><br></div>"))
      .join("");
  }

  /** Onde a assinatura `-- \nassinatura` está no texto (linha inteira). */
  export function signatureAt(text: string, signature: string): number {
    if (!signature) return -1;
    const marker = `-- \n${signature}`;
    let at = text.indexOf(marker);
    while (at !== -1) {
      const end = at + marker.length;
      if ((at === 0 || text[at - 1] === "\n") && (end === text.length || text[end] === "\n")) return at;
      at = text.indexOf(marker, at + 1);
    }
    return -1;
  }
</script>

<script lang="ts">
  import {
    AlignCenter,
    AlignLeft,
    Bold,
    Italic,
    Link,
    List,
    ListOrdered,
    Palette,
    Quote,
    RemoveFormatting,
    Strikethrough,
    Underline,
  } from "@lucide/svelte";
  import { untrack } from "svelte";
  import { t, tk } from "./i18n.svelte";
  import { modKey } from "./api";

  let {
    initialText,
    initialHtml = null,
    signature,
    text = $bindable(""),
    html = $bindable(""),
    rich = $bindable(false),
    label,
  }: {
    initialText: string;
    /** HTML de um rascunho ou de um e-mail que voltou (limpo aqui). */
    initialHtml?: string | null;
    /** Assinatura de texto da conta (vira um bloco que não se edita). */
    signature: string;
    /** Texto puro (parte de texto do e-mail, rascunho, assinatura intacta). */
    text?: string;
    /** HTML com o marcador no lugar da assinatura. */
    html?: string;
    /** Há formatação (só então o HTML vai junto). */
    rich?: boolean;
    label: string;
  } = $props();

  let editor: HTMLDivElement | undefined = $state();

  const SIG_CLASS = "papeo-signature";
  function signatureBlock() {
    const lines = signature.replace(/\r\n/g, "\n").split("\n").map(escape).join("<br>");
    return `<div class="${SIG_CLASS}" contenteditable="false">--&nbsp;<br>${lines}</div>`;
  }

  /** Conteúdo inicial: o HTML (com a assinatura de volta como bloco) ou o
   * texto, com o bloco no lugar de `-- \nassinatura`. */
  function startHtml(): string {
    if (initialHtml) {
      const clean = cleanHtml(initialHtml);
      const doc = new DOMParser().parseFromString(`<body>${clean}</body>`, "text/html");
      const sig = doc.body.querySelector(`div.${SIG_CLASS}`);
      if (sig && signature) {
        const holder = doc.createElement("div");
        holder.innerHTML = signatureBlock();
        sig.replaceWith(holder.firstChild!);
      } else sig?.removeAttribute("class");
      return doc.body.innerHTML;
    }
    const body = initialText.replace(/\r\n/g, "\n");
    const at = signatureAt(body, signature);
    if (at === -1) return textHtml(body);
    const before = body.slice(0, at).replace(/\n$/, "");
    const after = body.slice(at + `-- \n${signature}`.length).replace(/^\n/, "");
    return `${textHtml(before)}${before ? "" : "<div><br></div>"}${signatureBlock()}${textHtml(after)}`;
  }

  const FORMATTING = "b,strong,i,em,u,s,strike,font,ul,ol,a,blockquote,h1,h2,h3,h4,span[style],div[style],p[style],div[align],p[align]";

  /** Lê o editor: texto puro, HTML (com o marcador) e se há formatação. */
  function sync() {
    if (!editor) return;
    text = editor.innerText.replace(/\u00a0/g, " ").replace(/\n$/, "");
    const copy = editor.cloneNode(true) as HTMLElement;
    for (const sig of Array.from(copy.querySelectorAll(`.${SIG_CLASS}`))) sig.replaceWith(SIGNATURE_TOKEN);
    rich = !!copy.querySelector(FORMATTING);
    html = copy.innerHTML;
  }

  $effect(() => {
    if (!editor) return;
    // Uma vez por editor aberto: a assinatura mudar nas Configurações não
    // apaga o que já foi escrito.
    editor.innerHTML = untrack(startHtml);
    try {
      document.execCommand("defaultParagraphSeparator", false, "div");
      document.execCommand("styleWithCSS", false, "false");
    } catch {
      /* navegador sem esses ajustes: o padrão serve */
    }
    sync();
  });

  /** Foco no começo do texto (resposta) ou onde estava. */
  export function focus(atStart = false) {
    if (!editor) return;
    editor.focus();
    if (!atStart) return;
    const range = document.createRange();
    range.setStart(editor, 0);
    range.collapse(true);
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);
  }

  // Seleção guardada (o campo do link tira o foco do texto).
  let saved: Range | null = null;
  function keepSelection() {
    const sel = window.getSelection();
    if (sel && sel.rangeCount && editor?.contains(sel.anchorNode)) saved = sel.getRangeAt(0).cloneRange();
  }
  function restoreSelection() {
    if (!saved || !editor) return;
    editor.focus();
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(saved);
  }

  function run(command: string, value?: string) {
    if (!editor) return;
    if (document.activeElement !== editor) restoreSelection();
    document.execCommand(command, false, value);
    sync();
    keepSelection();
  }

  /** Botões não roubam o foco do texto (a seleção continua). */
  function hold(e: MouseEvent) {
    e.preventDefault();
  }

  const SIZES = [
    { value: "2", label: tk("Pequeno") },
    { value: "3", label: tk("Normal") },
    { value: "4", label: tk("Grande") },
    { value: "6", label: tk("Muito grande") },
  ];
  const COLORS = ["#1f2328", "#d1242f", "#bc4c00", "#9a6700", "#1a7f37", "#0969da", "#8250df", "#6e7781"];
  let colorsOpen = $state(false);

  let linkOpen = $state(false);
  let linkUrl = $state("");
  let linkField: HTMLInputElement | undefined = $state();
  function openLink() {
    keepSelection();
    linkUrl = "https://";
    linkOpen = true;
    queueMicrotask(() => linkField?.select());
  }
  function applyLink(e: Event) {
    e.preventDefault();
    const url = linkUrl.trim();
    linkOpen = false;
    if (!/^(https?:\/\/\S+|mailto:\S+)$/i.test(url)) {
      restoreSelection();
      return;
    }
    restoreSelection();
    const sel = window.getSelection();
    if (sel && sel.isCollapsed) {
      // Sem texto escolhido: o próprio endereço vira o texto do link.
      document.execCommand("insertHTML", false, `<a href="${escape(url).replace(/"/g, "&quot;")}">${escape(url)}</a>`);
      sync();
    } else run("createLink", url);
  }

  function keydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && !e.shiftKey && e.key.toLowerCase() === "k") {
      e.preventDefault();
      openLink();
    }
  }

  /** Colar e soltar entram como texto (nada de HTML de fora na janela). */
  function paste(e: ClipboardEvent) {
    const plain = e.clipboardData?.getData("text/plain");
    e.preventDefault();
    if (plain) document.execCommand("insertText", false, plain.replace(/\r\n/g, "\n"));
    sync();
  }
  function drop(e: DragEvent) {
    // Arquivos ficam com o editor de e-mail (anexos); texto entra puro.
    if (e.dataTransfer?.files.length) return;
    e.preventDefault();
    const plain = e.dataTransfer?.getData("text/plain");
    if (!plain) return;
    const doc = document as Document & { caretRangeFromPoint?: (x: number, y: number) => Range | null };
    const range = doc.caretRangeFromPoint?.(e.clientX, e.clientY);
    if (range) {
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    }
    document.execCommand("insertText", false, plain);
    sync();
  }
</script>

<div class="mail-editor">
  <div class="toolbar" role="toolbar" aria-label={t("Formatação")}>
    <select
      aria-label={t("Tamanho da letra")}
      title={t("Tamanho da letra")}
      onmousedown={keepSelection}
      onchange={(e) => {
        run("fontSize", (e.currentTarget as HTMLSelectElement).value);
        (e.currentTarget as HTMLSelectElement).value = "";
      }}
    >
      <option value="" selected disabled>{t("Tamanho")}</option>
      {#each SIZES as size (size.value)}
        <option value={size.value}>{t(size.label)}</option>
      {/each}
    </select>
    <span class="sep"></span>
    <button type="button" onmousedown={hold} onclick={() => run("bold")} title={t("Negrito ({atalho}B)", { atalho: modKey })} aria-label={t("Negrito")}><Bold size={15} /></button>
    <button type="button" onmousedown={hold} onclick={() => run("italic")} title={t("Itálico ({atalho}I)", { atalho: modKey })} aria-label={t("Itálico")}><Italic size={15} /></button>
    <button type="button" onmousedown={hold} onclick={() => run("underline")} title={t("Sublinhado ({atalho}U)", { atalho: modKey })} aria-label={t("Sublinhado")}><Underline size={15} /></button>
    <button type="button" onmousedown={hold} onclick={() => run("strikeThrough")} title={t("Tachado")} aria-label={t("Tachado")}><Strikethrough size={15} /></button>
    <span class="colors">
      <button
        type="button"
        onmousedown={(e) => {
          hold(e);
          keepSelection();
        }}
        onclick={() => (colorsOpen = !colorsOpen)}
        title={t("Cor da letra")}
        aria-label={t("Cor da letra")}
        aria-expanded={colorsOpen}
      ><Palette size={15} /></button>
      {#if colorsOpen}
        <span class="swatches" role="group" aria-label={t("Cor da letra")}>
          {#each COLORS as color (color)}
            <button
              type="button"
              class="swatch"
              style:background={color}
              aria-label={color}
              onmousedown={hold}
              onclick={() => {
                colorsOpen = false;
                run("foreColor", color);
              }}
            ></button>
          {/each}
        </span>
      {/if}
    </span>
    <span class="sep"></span>
    <button type="button" onmousedown={hold} onclick={() => run("insertUnorderedList")} title={t("Lista")} aria-label={t("Lista")}><List size={15} /></button>
    <button type="button" onmousedown={hold} onclick={() => run("insertOrderedList")} title={t("Lista numerada")} aria-label={t("Lista numerada")}><ListOrdered size={15} /></button>
    <button type="button" onmousedown={hold} onclick={() => run("formatBlock", "blockquote")} title={t("Citação")} aria-label={t("Citação")}><Quote size={15} /></button>
    <button type="button" onmousedown={hold} onclick={() => run("justifyLeft")} title={t("Alinhar à esquerda")} aria-label={t("Alinhar à esquerda")}><AlignLeft size={15} /></button>
    <button type="button" onmousedown={hold} onclick={() => run("justifyCenter")} title={t("Centralizar")} aria-label={t("Centralizar")}><AlignCenter size={15} /></button>
    <span class="sep"></span>
    <button type="button" onmousedown={hold} onclick={openLink} title={t("Link ({atalho}K)", { atalho: modKey })} aria-label={t("Link")}><Link size={15} /></button>
    <button type="button" onmousedown={hold} onclick={() => run("removeFormat")} title={t("Tirar formatação")} aria-label={t("Tirar formatação")}><RemoveFormatting size={15} /></button>
    {#if linkOpen}
      <form class="link" onsubmit={applyLink}>
        <input
          bind:this={linkField}
          bind:value={linkUrl}
          aria-label={t("Endereço do link")}
          onkeydown={(e) => {
            if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              linkOpen = false;
              restoreSelection();
            }
          }}
        />
        <button type="submit" class="btn">{t("Pôr link")}</button>
      </form>
    {/if}
  </div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    bind:this={editor}
    class="body"
    contenteditable="true"
    role="textbox"
    aria-multiline="true"
    aria-label={label}
    tabindex="0"
    oninput={sync}
    onkeyup={keepSelection}
    onmouseup={keepSelection}
    onkeydown={keydown}
    onpaste={paste}
    ondrop={drop}
  ></div>
</div>

<style>
  .mail-editor {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
    position: relative;
  }

  .toolbar button:not(.btn):not(.swatch) {
    width: 28px;
    height: 28px;
    display: inline-grid;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
  }

  .toolbar button:not(.btn):not(.swatch):hover {
    background: var(--bg-hover);
    color: var(--text);
  }

  .toolbar select {
    height: 28px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
    font-size: 12.5px;
    padding: 0 6px;
  }

  .sep {
    width: 1px;
    height: 18px;
    margin: 0 4px;
    background: var(--border);
  }

  .colors {
    position: relative;
  }

  .swatches {
    position: absolute;
    top: 32px;
    left: 0;
    z-index: 5;
    display: grid;
    grid-template-columns: repeat(4, 22px);
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-elevated, var(--bg));
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.18);
  }

  .swatch {
    width: 22px;
    height: 22px;
    border: 1px solid color-mix(in srgb, var(--text) 20%, transparent);
    border-radius: 50%;
    padding: 0;
  }

  .link {
    display: flex;
    gap: 6px;
    margin-left: 6px;
  }

  .link input {
    height: 28px;
    width: 220px;
    max-width: 40vw;
  }

  .body {
    flex: 1;
    min-height: 120px;
    overflow: auto;
    padding: 12px 16px;
    outline: none;
    font: inherit;
    line-height: 1.5;
    color: var(--text);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .body :global(blockquote) {
    margin: 0 0 0 0.8ex;
    padding-left: 1ex;
    border-left: 2px solid var(--border);
    color: var(--text-muted);
  }

  .body :global(ul),
  .body :global(ol) {
    margin: 0;
    padding-left: 24px;
  }

  .body :global(a) {
    color: var(--primary);
  }

  .body :global(.papeo-signature) {
    color: var(--text-muted);
    user-select: none;
  }
</style>
