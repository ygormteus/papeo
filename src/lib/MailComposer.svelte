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
    /** Rascunho no servidor (spec 18): token das versões salvas e, para um
     * rascunho de outro programa, a mensagem que ele substitui. */
    draftToken?: string | null;
    replaces?: { folder: string; uid: number } | null;
    /** Veio de fora do servidor (desfeito, falha, sobra): conta como não
     * salvo, para fechar guardar nos Rascunhos. */
    unsaved?: boolean;
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
  import AddressInput from "./AddressInput.svelte";
  import { LoaderCircle, Paperclip, Send, Trash2, X } from "@lucide/svelte";
  import { locale, t, tk } from "./i18n.svelte";
  import { api, errorMessage, type MailDraft, type ServiceView } from "./api";
  import { queueSend } from "./outbox.svelte";

  let {
    service,
    start,
    undoSecs,
    onclose,
    onsent,
    ondraftsaved,
  }: {
    service: ServiceView;
    start: ComposeStart;
    /** Espera para desfazer; 0 envia na hora. */
    undoSecs: number;
    onclose: () => void;
    /** `queued`: entrou na espera (ainda não saiu). */
    onsent: (queued: boolean) => void;
    /** Rascunho salvo no servidor (para atualizar a pasta Rascunhos). */
    ondraftsaved?: () => void;
  } = $props();

  const MAX_TOTAL = 25 * 1024 * 1024;
  /** A assinatura formatada só sai se a de texto estiver intacta (spec 22). */
  const richIntact = $derived.by(() => {
    const sig = service.mail?.signature;
    if (!sig) return false;
    const text = body.replace(/\r\n/g, "\n");
    const marker = `-- \n${sig}`;
    let at = text.indexOf(marker);
    while (at !== -1) {
      const end = at + marker.length;
      if ((at === 0 || text[at - 1] === "\n") && (end === text.length || text[end] === "\n")) return true;
      at = text.indexOf(marker, at + 1);
    }
    return false;
  });

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

  // ---- Rascunho no servidor (spec 18)

  /** Atraso do salvamento automático depois da última mudança. */
  const AUTOSAVE_MS = 10_000;
  /** Erro do backend para conta sem pasta Rascunhos (`mailbox::NO_DRAFTS`). */
  const NO_DRAFTS = tk("Esta conta não tem pasta Rascunhos no servidor.");
  /** Token nascido aqui: um salvamento que falha no meio não cria cópias
   * com tokens diferentes. */
  function newToken() {
    const bytes = crypto.getRandomValues(new Uint8Array(12));
    return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  }
  // svelte-ignore state_referenced_locally
  let token = $state<string>(start.draftToken ?? newToken());
  // Conta fixa do editor (o leitor troca de conta desmontando o editor).
  // svelte-ignore state_referenced_locally
  const accountId = service.id;
  // svelte-ignore state_referenced_locally
  let replaces = $state(start.replaces ?? null);
  /** Estado do editor como foi salvo (ou como abriu): igual = nada a salvar. */
  const contentKey = () => JSON.stringify([to, cc, bcc, subject, body, files.map((f) => [f.name, f.size])]);
  // svelte-ignore state_referenced_locally
  let savedKey = $state(start.unsaved ? "" : contentKey());
  let savedAt = $state<Date | null>(null);
  let draftError = $state("");
  /** Já existe no servidor (aberto de lá ou salvo aqui): descartar apaga. */
  // svelte-ignore state_referenced_locally
  let onServer = !!start.draftToken || !!start.replaces;
  /** Conta sem pasta Rascunhos: para de salvar sozinho (R8). */
  let noDrafts = $state(false);
  /** Salvamento em curso (o envio e o fechamento esperam por ele). */
  let saving: Promise<boolean> | null = null;
  /** Enviado ou descartado: nada mais a salvar. */
  let finished = false;
  const dirty = $derived(contentKey() !== savedKey);
  const hasContent = $derived(
    !!(to.trim() || cc.trim() || bcc.trim() || subject.trim() || body.trim() || files.length),
  );

  function draftOf(): MailDraft {
    return {
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
      draftToken: token,
      replaces,
    };
  }

  /** Salva no servidor; `false` com erro (fica em `draftError`). */
  function saveDraft(): Promise<boolean> {
    if (finished) return Promise.resolve(true);
    if (saving) return saving.then(() => (dirty && !finished && !sending && !closing ? saveDraft() : true));
    const key = contentKey();
    const draft = draftOf();
    saving = (async () => {
      try {
        token = await api.mailSaveDraft(accountId, draft);
        replaces = null;
        onServer = true;
        savedKey = key;
        savedAt = new Date();
        draftError = "";
        ondraftsaved?.();
        return true;
      } catch (err) {
        const message = errorMessage(err);
        if (message === NO_DRAFTS) noDrafts = true;
        // O backend manda o texto em português (é a chave da comparação).
        draftError = t("Rascunho não salvo: {erro}", { erro: message === NO_DRAFTS ? t(NO_DRAFTS) : message });
        return false;
      } finally {
        saving = null;
      }
    })();
    return saving;
  }

  // Salva sozinho 10 s depois da última mudança. Depois de um erro, só
  // tenta de novo quando o texto mudar outra vez.
  $effect(() => {
    if (!dirty || !hasContent || sending || noDrafts) return;
    contentKey();
    const timer = setTimeout(() => {
      if (!finished && !draftError && !closing) void saveDraft();
    }, AUTOSAVE_MS);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    contentKey();
    draftError = "";
  });

  // Saiu da tela sem fechar (outra mensagem, outra conta): salva o que
  // faltava sem esperar.
  // Na fila dos outros salvamentos (a versão mais nova fica por último) e
  // nunca durante um envio (ele apaga o rascunho).
  $effect(() => () => {
    if (finished || sending || noDrafts || contentKey() === savedKey || !hasContent) return;
    const draft = draftOf();
    void (saving ?? Promise.resolve(true))
      .then(() => api.mailSaveDraft(accountId, draft))
      .catch(() => {});
  });

  /** Fechar: salva antes (se mudou); com erro, o editor fica aberto. */
  async function close() {
    if (sending || closing) return;
    if (dirty && hasContent) {
      closing = true;
      const ok = await saveDraft();
      closing = false;
      if (!ok) return;
    }
    finished = true;
    onclose();
  }
  let closing = $state(false);

  /** Descartar: fecha e apaga o rascunho salvo. */
  async function discard() {
    if (sending || closing) return;
    closing = true;
    // Nenhum salvamento depois deste ponto (recriaria o rascunho).
    finished = true;
    await saving?.catch(() => {});
    try {
      if (onServer || replaces) await api.mailDiscardDraft(accountId, token, replaces);
      ondraftsaved?.();
      onclose();
    } catch (err) {
      finished = false;
      draftError = t("Não deu para apagar o rascunho: {erro}", { erro: errorMessage(err) });
    } finally {
      closing = false;
    }
  }

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
        error = t("Os anexos passam de 25 MB.");
        break;
      }
      try {
        const data = await readFile(file);
        files = [...files, { name: file.name, mime: file.type || "application/octet-stream", size: file.size, data }];
      } catch {
        error = t("Não deu para ler {nome}.", { nome: file.name });
      }
    }
    if (picker) picker.value = "";
  }

  async function send(e: SubmitEvent) {
    e.preventDefault();
    if (sending || empty) return;
    sending = true;
    error = "";
    // Um salvamento em curso termina antes: o envio apaga o rascunho, e um
    // salvamento atrasado o recriaria.
    await saving?.catch(() => {});
    const draft = draftOf();
    try {
      if (undoSecs > 0) {
        await queueSend(service.id, draft);
        finished = true;
        onsent(true);
      } else {
        await api.mailSend(service.id, draft);
        finished = true;
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
      void close();
    }
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      (e.currentTarget as HTMLFormElement).requestSubmit();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form class="composer" onsubmit={send} onkeydown={keydown} aria-label={t("Escrever e-mail")}>
  <header>
    <h2>{start.inReplyTo ? t("Responder") : t("Nova mensagem")}</h2>
    <span class="from">{t("De {conta}", { conta: service.mail?.user ?? "" })}</span>
    <button
      type="button"
      class="icon-btn ghost"
      onclick={close}
      aria-label={t("Fechar")}
      title={t("Fechar e guardar nos Rascunhos (Esc)")}
      disabled={sending || closing}
    >
      {#if closing}<LoaderCircle size={18} class="spin" />{:else}<X size={18} />{/if}
    </button>
  </header>
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  <div class="fields">
    <label class="row">
      <span>{t("Para")}</span>
      <AddressInput bind:field={toField} bind:value={to} {accountId} label={t("Para")} placeholder={t("nome@exemplo.com")} />
      {#if !showCc}
        <button type="button" class="link-btn" onclick={() => (showCc = true)}>{t("Cc/Cco")}</button>
      {/if}
    </label>
    {#if showCc}
      <label class="row"><span>Cc</span><AddressInput bind:value={cc} {accountId} label="Cc" /></label>
      <label class="row"><span>{t("Cco")}</span><AddressInput bind:value={bcc} {accountId} label={t("Cco")} /></label>
    {/if}
    <label class="row"><span>{t("Assunto")}</span><input bind:value={subject} /></label>
  </div>
  <textarea bind:this={bodyField} bind:value={body} aria-label={t("Texto do e-mail")}></textarea>
  {#if service.signatureRich && service.mail?.signature}
    <p class="rich-note">
      {#if richIntact}{t("A assinatura sai com a formatação (logo, cores e links).")}{:else}{t("A assinatura foi mudada ou apagada: o e-mail sai só em texto.")}{/if}
    </p>
  {/if}
  {#if files.length}
    <ul class="files">
      {#each files as file, i (i)}
        <li>
          <Paperclip size={13} />
          <span class="name">{file.name}</span>
          <span class="size">{size(file.size)}</span>
          <button type="button" class="icon-btn ghost" aria-label={t("Tirar {nome}", { nome: file.name })} onclick={() => (files = files.filter((_, j) => j !== i))}>
            <X size={14} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if draftError}<p class="error" role="alert">{draftError}</p>{/if}
  <footer>
    <button type="submit" class="btn btn-primary" disabled={sending || empty}>
      {#if sending}<LoaderCircle size={15} class="spin" /> {t("Enviando…")}{:else}<Send size={15} /> {t("Enviar")}{/if}
    </button>
    <button type="button" class="btn" onclick={() => picker?.click()} disabled={sending}>
      <Paperclip size={15} /> {t("Anexar")}
    </button>
    <input bind:this={picker} type="file" multiple hidden onchange={(e) => addFiles(e.currentTarget.files)} />
    <button type="button" class="btn ghost-danger" onclick={discard} disabled={sending || closing}>
      <Trash2 size={15} /> {t("Descartar")}
    </button>
    <span class="hint">
      {#if savedAt && !dirty}{t("Rascunho salvo às {hora}", { hora: savedAt.toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" }) })} ·
      {/if}{t("{tecla} Enter envia", { tecla: navigator.platform.startsWith("Mac") ? "⌘" : "Ctrl" })}
    </span>
  </footer>
</form>

<style>
  .rich-note {
    margin: 0;
    padding: 4px 16px;
    font-size: 12px;
    color: var(--text-muted);
  }
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
  .ghost-danger {
    color: var(--danger);
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
