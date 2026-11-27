<script lang="ts" module>
  export interface ComposeStart {
    to?: string;
    cc?: string;
    subject?: string;
    body?: string;
    inReplyTo?: string | null;
    references?: string[];
    replyFolder?: string | null;
    replyUid?: number | null;
    /** E-mail que voltou ao editor (desfeito, falhou ou sobrou). */
    bcc?: string;
    attachments?: { name: string; mime: string; data: string }[];
    /** Aviso no topo do editor; `noticeError` o mostra como erro. */
    notice?: string;
    noticeError?: boolean;
  }

  /** Tamanho aproximado do conteúdo em base64. */
  export function base64Size(data: string): number {
    const pad = data.endsWith("==") ? 2 : data.endsWith("=") ? 1 : 0;
    return Math.max(0, Math.floor((data.length * 3) / 4) - pad);
  }

  /** Separa endereços por vírgula ou ponto e vírgula (fora de aspas e `<>`). */
  export function splitAddresses(text: string): string[] {
    const out: string[] = [];
    let cur = "";
    let quoted = false;
    let angle = false;
    for (const ch of text) {
      if (ch === '"') quoted = !quoted;
      else if (ch === "<" && !quoted) angle = true;
      else if (ch === ">" && !quoted) angle = false;
      if ((ch === "," || ch === ";") && !quoted && !angle) {
        if (cur.trim()) out.push(cur.trim());
        cur = "";
      } else cur += ch;
    }
    if (cur.trim()) out.push(cur.trim());
    return out;
  }
</script>

<script lang="ts">
  import { LoaderCircle, Paperclip, Send, X } from "@lucide/svelte";
  import { api, errorMessage, type MailDraft, type ServiceView } from "./api";
  import { queueSend } from "./outbox.svelte";

  let {
    service,
    start,
    undoSecs,
    onclose,
    onsent,
  }: {
    service: ServiceView;
    start: ComposeStart;
    /** Espera para desfazer; 0 envia na hora. */
    undoSecs: number;
    onclose: () => void;
    /** `queued`: entrou na espera (ainda não saiu). */
    onsent: (queued: boolean) => void;
  } = $props();

  const MAX_TOTAL = 25 * 1024 * 1024;

  // svelte-ignore state_referenced_locally
  let to = $state(start.to ?? "");
  // svelte-ignore state_referenced_locally
  let cc = $state(start.cc ?? "");
  // svelte-ignore state_referenced_locally
  let bcc = $state(start.bcc ?? "");
  // svelte-ignore state_referenced_locally
  let showCc = $state(!!start.cc || !!start.bcc);
  // svelte-ignore state_referenced_locally
  let subject = $state(start.subject ?? "");
  // svelte-ignore state_referenced_locally
  let body = $state(start.body ?? "");
  // svelte-ignore state_referenced_locally
  let files = $state<{ name: string; mime: string; size: number; data: string }[]>(
    (start.attachments ?? []).map((a) => ({ ...a, size: base64Size(a.data) })),
  );
  let sending = $state(false);
  // svelte-ignore state_referenced_locally
  let error = $state(start.noticeError ? (start.notice ?? "") : "");
  // svelte-ignore state_referenced_locally
  const notice = start.noticeError ? "" : (start.notice ?? "");
  let picker: HTMLInputElement | undefined = $state();
  let bodyField: HTMLTextAreaElement | undefined = $state();
  let toField: HTMLInputElement | undefined = $state();

  const total = $derived(files.reduce((n, f) => n + f.size, 0));
  const empty = $derived(!to.trim() && !cc.trim() && !bcc.trim());

  $effect(() => {
    // Resposta: começa no texto; mensagem nova, no destinatário.
    if (start.to) {
      bodyField?.focus();
      bodyField?.setSelectionRange(0, 0);
    } else toField?.focus();
  });

  function size(n: number) {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  function readFile(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const url = String(reader.result);
        resolve(url.slice(url.indexOf(",") + 1));
      };
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });
  }

  async function addFiles(list: FileList | null) {
    if (!list) return;
    error = "";
    for (const file of Array.from(list)) {
      if (total + file.size > MAX_TOTAL) {
        error = "Os anexos passam de 25 MB.";
        break;
      }
      try {
        const data = await readFile(file);
        files = [...files, { name: file.name, mime: file.type || "application/octet-stream", size: file.size, data }];
      } catch {
        error = `Não deu para ler ${file.name}.`;
      }
    }
    if (picker) picker.value = "";
  }

  async function send(e: SubmitEvent) {
    e.preventDefault();
    if (sending || empty) return;
    sending = true;
    error = "";
    const draft: MailDraft = {
      to: splitAddresses(to),
      cc: splitAddresses(cc),
      bcc: splitAddresses(bcc),
      subject,
      body,
      inReplyTo: start.inReplyTo ?? null,
      references: start.references ?? [],
      replyFolder: start.replyFolder ?? null,
      replyUid: start.replyUid ?? null,
      attachments: files.map(({ name, mime, data }) => ({ name, mime, data })),
    };
    try {
      if (undoSecs > 0) {
        await queueSend(service.id, draft);
        onsent(true);
      } else {
        await api.mailSend(service.id, draft);
        onsent(false);
      }
    } catch (err) {
      error = errorMessage(err);
    } finally {
      sending = false;
    }
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !sending) {
      e.stopPropagation();
      onclose();
    }
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      (e.currentTarget as HTMLFormElement).requestSubmit();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form class="composer" onsubmit={send} onkeydown={keydown} aria-label="Escrever e-mail">
  <header>
    <h2>{start.inReplyTo ? "Responder" : "Nova mensagem"}</h2>
    <span class="from">De {service.mail?.user}</span>
    <button type="button" class="icon-btn ghost" onclick={onclose} aria-label="Descartar" title="Descartar (Esc)" disabled={sending}>
      <X size={18} />
    </button>
  </header>
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  <div class="fields">
    <label class="row">
      <span>Para</span>
      <input bind:this={toField} bind:value={to} autocomplete="off" spellcheck="false" placeholder="nome@exemplo.com" />
      {#if !showCc}
        <button type="button" class="link-btn" onclick={() => (showCc = true)}>Cc/Cco</button>
      {/if}
    </label>
    {#if showCc}
      <label class="row"><span>Cc</span><input bind:value={cc} autocomplete="off" spellcheck="false" /></label>
      <label class="row"><span>Cco</span><input bind:value={bcc} autocomplete="off" spellcheck="false" /></label>
    {/if}
    <label class="row"><span>Assunto</span><input bind:value={subject} /></label>
  </div>
  <textarea bind:this={bodyField} bind:value={body} aria-label="Texto do e-mail"></textarea>
  {#if files.length}
    <ul class="files">
      {#each files as file, i (i)}
        <li>
          <Paperclip size={13} />
          <span class="name">{file.name}</span>
          <span class="size">{size(file.size)}</span>
          <button type="button" class="icon-btn ghost" aria-label="Tirar {file.name}" onclick={() => (files = files.filter((_, j) => j !== i))}>
            <X size={14} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <footer>
    <button type="submit" class="btn btn-primary" disabled={sending || empty}>
      {#if sending}<LoaderCircle size={15} class="spin" /> Enviando…{:else}<Send size={15} /> Enviar{/if}
    </button>
    <button type="button" class="btn" onclick={() => picker?.click()} disabled={sending}>
      <Paperclip size={15} /> Anexar
    </button>
    <input bind:this={picker} type="file" multiple hidden onchange={(e) => addFiles(e.currentTarget.files)} />
    <span class="hint">{navigator.platform.startsWith("Mac") ? "⌘" : "Ctrl"} Enter envia</span>
  </footer>
</form>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg);
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px 8px;
  }
  h2 {
    font-size: 15px;
    margin: 0;
  }
  .from {
    flex: 1;
    color: var(--text-muted);
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fields {
    border-top: 1px solid var(--border);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
  }
  .row span {
    width: 58px;
    color: var(--text-muted);
    font-size: 13px;
  }
  .row input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    padding: 9px 0;
    font: inherit;
    outline: none;
  }
  textarea {
    flex: 1;
    min-height: 120px;
    resize: none;
    border: 0;
    outline: none;
    padding: 14px 16px;
    background: transparent;
    color: var(--text);
    font: inherit;
    line-height: 1.5;
  }
  .files {
    list-style: none;
    margin: 0;
    padding: 6px 16px;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    border-top: 1px solid var(--border);
  }
  .files li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 4px 2px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-size: 12.5px;
    max-width: 260px;
  }
  .files .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .files .size {
    color: var(--text-faint);
  }
  .error {
    margin: 0;
    padding: 8px 16px;
    color: var(--danger);
    font-size: 13px;
  }
  .notice {
    margin: 0;
    padding: 8px 16px;
    background: var(--bg-subtle);
    color: var(--text-muted);
    font-size: 13px;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border-top: 1px solid var(--border);
  }
  .hint {
    margin-left: auto;
    color: var(--text-faint);
    font-size: 12px;
  }
  .composer :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .composer :global(.spin) {
      animation: none;
    }
  }
</style>
