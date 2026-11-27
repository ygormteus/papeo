<script lang="ts">
  import {
    Archive,
    ArrowLeft,
    Ban,
    ChevronLeft,
    ChevronRight,
    CircleAlert,
    Download,
    FileText,
    Flag,
    Folder,
    Forward,
    Globe,
    Image,
    Inbox,
    LoaderCircle,
    Mail,
    MailOpen,
    Paperclip,
    PenLine,
    RefreshCw,
    Reply,
    ReplyAll,
    Search,
    Send,
    ShieldAlert,
    Star,
    Signature,
    Trash2,
    X,
  } from "@lucide/svelte";
  import { locale, t, tk } from "./i18n.svelte";
  import MailComposer, { type ComposeStart } from "./MailComposer.svelte";
  import SignatureDialog from "./SignatureDialog.svelte";
  import AddressList from "./AddressList.svelte";
  import { loadUnsent, outbox, releaseReturned, startOutbox, takeReturned, undoSend } from "./outbox.svelte";
  import {
    api,
    errorMessage,
    type MailAddr,
    type MailFolder,
    type MailMessage,
    type MailRole,
    type MailSummary,
    type ServiceView,
  } from "./api";

  let {
    service,
    reveal,
    undoSecs,
    onsite,
    onconfigure,
    ontoast,
  }: {
    service: ServiceView;
    /** Assunto a procurar (clique num aviso); muda a cada pedido. */
    reveal: { query: string; n: number } | null;
    /** Espera para desfazer o envio (spec 16); 0 envia na hora. */
    undoSecs: number;
    onsite: () => void;
    onconfigure: () => void;
    ontoast: (text: string, action?: { label: string; run: () => void }) => void;
  } = $props();

  const PAGE = 50;
  /** Editor da assinatura aberto (spec 16). */
  let signing = $state(false);
  const ROLE_ICON: Record<MailRole, typeof Inbox> = {
    inbox: Inbox,
    drafts: FileText,
    sent: Send,
    archive: Archive,
    all: Mail,
    flagged: Star,
    junk: ShieldAlert,
    trash: Trash2,
  };

  /** Pastas especiais com o nome no idioma do Papeo, não o do servidor
   * ("Sent Mail", "Drafts"…). */
  const ROLE_NAME: Record<MailRole, string> = {
    inbox: tk("Caixa de Entrada"),
    drafts: tk("Rascunhos"),
    sent: tk("Enviados"),
    archive: tk("Arquivo"),
    all: tk("Todos os e-mails"),
    flagged: tk("Com estrela"),
    junk: tk("Spam"),
    trash: tk("Lixeira"),
  };
  /** Outras pastas comuns que o servidor não marca. */
  const KNOWN_NAME: Record<string, string> = {
    important: tk("Importante"),
    outbox: tk("Caixa de Saída"),
    notes: tk("Notas"),
    scheduled: tk("Agendados"),
    snoozed: tk("Adiados"),
    chats: tk("Conversas"),
    "conversation history": tk("Histórico de conversas"),
  };
  function folderName(f: MailFolder): string {
    if (f.role) return t(ROLE_NAME[f.role]);
    const known = KNOWN_NAME[f.name.trim().toLowerCase()];
    return known ? t(known) : f.name;
  }

  let folders = $state<MailFolder[]>([]);
  let folder = $state("INBOX");
  let messages = $state<MailSummary[]>([]);
  let total = $state(0);
  let page = $state(0);
  let query = $state("");
  let searched = $state("");
  let loading = $state(false);
  let error = $state("");
  let selected = $state<number | null>(null);
  let message = $state<MailMessage | null>(null);
  let opening = $state(false);
  let openError = $state("");
  let showImages = $state(false);
  let composing = $state<ComposeStart | null>(null);
  let confirmDelete = $state(false);
  let frame: HTMLIFrameElement | undefined = $state();

  const current = $derived(folders.find((f) => f.path === folder));
  const inTrash = $derived(current?.role === "trash");
  // Sem Lixeira (ou já nela), excluir é de vez: pede confirmação.
  const permanent = $derived(inTrash || !folders.some((f) => f.role === "trash"));
  const status = $derived(service.mailStatus?.state);
  const noPassword = $derived(status === "noPassword");
  // Conta nova ainda sem o IMAP: só a tela de configuração.
  const unset = $derived(!service.mail);
  const pages = $derived(Math.max(1, Math.ceil(total / PAGE)));
  const hasArchive = $derived(folders.some((f) => f.role === "archive" || f.role === "all"));
  const hasJunk = $derived(folders.some((f) => f.role === "junk"));

  // Sequência de pedidos: resposta atrasada de uma pasta antiga não vale.
  let listSeq = 0;
  let openSeq = 0;

  async function loadFolders(refresh = false) {
    try {
      const list = await api.mailFolders(service.id, refresh);
      folders = list;
      if (!list.some((f) => f.path === folder)) {
        folder = list.find((f) => f.role === "inbox")?.path ?? list[0]?.path ?? "INBOX";
      }
    } catch (err) {
      error = errorMessage(err);
    }
  }

  /** `true` se esta resposta ainda é a atual (não trocaram de pasta). */
  async function loadList(quiet = false): Promise<boolean> {
    const seq = ++listSeq;
    if (!quiet) loading = true;
    error = "";
    try {
      const result = await api.mailList(service.id, folder, page, searched);
      if (seq !== listSeq) return false;
      messages = result.messages;
      total = result.total;
      return true;
    } catch (err) {
      if (seq === listSeq && !quiet) error = errorMessage(err);
    } finally {
      if (seq === listSeq) loading = false;
    }
    return false;
  }

  // Conta trocada: começa de novo.
  let loadedFor = "";
  $effect(() => {
    const id = service.id;
    if (unset || noPassword || id === loadedFor) return;
    loadedFor = id;
    folders = [];
    folder = "INBOX";
    page = 0;
    query = searched = "";
    closeMessage();
    void loadFolders().then(() => loadList());
  });

  // Envios em espera (spec 16): e-mails devolvidos voltam ao editor. Ao abrir
  // o leitor da conta, busca os guardados no disco (falhas fora da vista,
  // sobras de uma sessão anterior); repetidos são ignorados pelo id.
  let unsentFor = "";
  $effect(() => {
    const id = service.id;
    if (unset || id === unsentFor) return;
    unsentFor = id;
    void startOutbox();
    loadUnsent(id).catch(() => {});
  });
  $effect(() => {
    if (composing || !outbox.returned.some((r) => r.id === service.id)) return;
    const r = takeReturned(service.id);
    if (!r) return;
    const d = r.draft;
    composing = {
      to: d.to.join(", "),
      cc: d.cc.join(", "),
      bcc: d.bcc.join(", "),
      subject: d.subject,
      body: d.body,
      inReplyTo: d.inReplyTo ?? null,
      references: d.references ?? [],
      replyFolder: d.replyFolder ?? null,
      replyUid: d.replyUid ?? null,
      attachments: d.attachments,
      draftToken: d.draftToken ?? null,
      replaces: d.replaces ?? null,
      unsaved: true,
      notice: r.notice,
      noticeError: r.error,
    };
    releaseReturned(r);
  });
  let sentSeen = outbox.sent?.n ?? 0;
  $effect(() => {
    const done = outbox.sent;
    if (!done || done.n === sentSeen) return;
    sentSeen = done.n;
    if (done.id !== service.id) return;
    ontoast(t("E-mail enviado."));
    if (current?.role === "sent") void loadList(true);
  });
  const waiting = $derived(outbox.pending.filter((p) => p.id === service.id));
  let now = $state(Date.now());
  $effect(() => {
    if (!waiting.length) return;
    now = Date.now();
    const t = setInterval(() => (now = Date.now()), 250);
    return () => clearInterval(t);
  });
  let undoing = $state("");
  async function undo(holdId: string) {
    if (undoing) return;
    undoing = holdId;
    try {
      if (!(await undoSend(holdId))) ontoast(t("O e-mail já foi enviado."));
    } catch (err) {
      ontoast(errorMessage(err));
    } finally {
      undoing = "";
    }
  }

  // O servidor avisou de mudança (contador mudou): atualiza a lista sem
  // piscar, se o usuário está vendo a primeira página da caixa.
  let lastUnread: number | null = null;
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const unread = service.unread;
    if (lastUnread !== null && unread !== lastUnread && page === 0 && !searched) {
      clearTimeout(refreshTimer);
      refreshTimer = setTimeout(() => loadList(true), 800);
    }
    lastUnread = unread;
  });
  $effect(() => () => clearTimeout(refreshTimer));

  // Clique num aviso: procura o assunto na Caixa de Entrada e abre se achar.
  let revealed = 0;
  $effect(() => {
    if (!reveal || reveal.n === revealed || noPassword || unset) return;
    revealed = reveal.n;
    const inbox = folders.find((f) => f.role === "inbox")?.path ?? "INBOX";
    folder = inbox;
    page = 0;
    query = searched = reveal.query;
    composing = null;
    void loadList().then((current) => {
      if (current && messages[0]) void openMessage(messages[0].uid);
    });
  });

  function chooseFolder(path: string) {
    if (path === folder && !searched) return void loadList();
    folder = path;
    page = 0;
    query = searched = "";
    closeMessage();
    void loadList();
  }

  function search(e: SubmitEvent) {
    e.preventDefault();
    searched = query.trim();
    page = 0;
    closeMessage();
    void loadList();
  }

  function clearSearch() {
    query = searched = "";
    page = 0;
    void loadList();
  }

  function goPage(next: number) {
    page = Math.min(Math.max(0, next), pages - 1);
    void loadList();
  }

  function closeMessage() {
    openSeq++;
    selected = null;
    message = null;
    openError = "";
    confirmDelete = false;
  }

  // Mensagens já baixadas (abrir de novo ou a próxima da lista é na hora).
  const CACHE_MAX = 40;
  const PREFETCH_MAX = 400 * 1024;
  const cache = new Map<string, MailMessage>();
  const key = (uid: number) => `${folder}\u0000${uid}`;
  function remember(view: MailMessage, path = folder) {
    cache.delete(`${path}\u0000${view.uid}`);
    cache.set(`${path}\u0000${view.uid}`, view);
    while (cache.size > CACHE_MAX) cache.delete(cache.keys().next().value!);
  }
  let prefetching = false;
  // Adiantamento em curso: abrir a mesma mensagem espera por ele em vez de
  // pedir de novo (a conexão faz um pedido por vez).
  const inflight = new Map<string, Promise<MailMessage>>();

  /** Baixa a próxima da lista sem marcar como lida, enquanto o usuário lê. */
  async function prefetchAfter(uid: number) {
    if (prefetching) return;
    const index = messages.findIndex((m) => m.uid === uid);
    const next = messages[index + 1];
    if (!next || next.size > PREFETCH_MAX || cache.has(key(next.uid))) return;
    const path = folder;
    prefetching = true;
    const k = key(next.uid);
    const request = api.mailOpen(service.id, path, next.uid, true);
    inflight.set(k, request);
    try {
      remember(await request, path);
    } catch {
      // Só adiantamento: abre normalmente depois.
    } finally {
      inflight.delete(k);
      prefetching = false;
    }
  }

  /** Rascunho da pasta Rascunhos: abre no editor para continuar (spec 18). */
  async function openDraft(uid: number) {
    const seq = ++openSeq;
    selected = uid;
    openError = "";
    message = null;
    composing = null;
    opening = true;
    try {
      const d = await api.mailOpenDraft(service.id, folder, uid);
      if (seq !== openSeq) return;
      composing = {
        to: d.to.join(", "),
        cc: d.cc.join(", "),
        bcc: d.bcc.join(", "),
        subject: d.subject,
        body: d.body,
        inReplyTo: d.inReplyTo,
        references: d.references,
        attachments: d.attachments,
        draftToken: d.draftToken,
        replaces: d.replaces,
      };
    } catch (err) {
      if (seq === openSeq) openError = errorMessage(err);
    } finally {
      if (seq === openSeq) opening = false;
    }
  }

  async function openMessage(uid: number) {
    if (current?.role === "drafts") return openDraft(uid);
    const seq = ++openSeq;
    composing = null;
    selected = uid;
    openError = "";
    showImages = false;
    confirmDelete = false;
    const summary = messages.find((m) => m.uid === uid);
    const pending = inflight.get(key(uid));
    if (pending) {
      message = null;
      opening = true;
      await pending.catch(() => {});
      if (seq !== openSeq) return;
    }
    const cached = cache.get(key(uid));
    if (cached) {
      message = { ...cached, flagged: summary?.flagged ?? cached.flagged, seen: true };
      opening = false;
      if (summary && !summary.seen) {
        messages = messages.map((m) => (m.uid === uid ? { ...m, seen: true } : m));
        api.mailSetFlag(service.id, folder, [uid], "seen", true).catch(() => {});
      }
      void prefetchAfter(uid);
      return;
    }
    message = null;
    opening = true;
    try {
      const view = await api.mailOpen(service.id, folder, uid);
      remember(view);
      if (seq !== openSeq) return;
      message = view;
      messages = messages.map((m) => (m.uid === uid ? { ...m, seen: true } : m));
      void prefetchAfter(uid);
    } catch (err) {
      if (seq === openSeq) openError = errorMessage(err);
    } finally {
      if (seq === openSeq) opening = false;
    }
  }

  // Ação numa mensagem: tira da lista na hora e abre a seguinte.
  async function removeFromList(uid: number, action: () => Promise<void>, done: string) {
    cache.delete(key(uid));
    const index = messages.findIndex((m) => m.uid === uid);
    const before = messages;
    messages = messages.filter((m) => m.uid !== uid);
    total = Math.max(0, total - 1);
    const next = messages[Math.min(index, messages.length - 1)];
    if (next) void openMessage(next.uid);
    else closeMessage();
    try {
      await action();
      ontoast(done);
    } catch (err) {
      messages = before;
      total += 1;
      ontoast(errorMessage(err));
    }
  }

  function remove(uid: number) {
    if (permanent && !confirmDelete) {
      confirmDelete = true;
      return;
    }
    void removeFromList(
      uid,
      () => api.mailDelete(service.id, folder, [uid]),
      permanent ? t("Excluída de vez.") : t("Movida para a Lixeira."),
    );
  }

  function moveTo(uid: number, target: string, done: string) {
    void removeFromList(uid, () => api.mailMove(service.id, folder, [uid], target), done);
  }

  async function toggle(m: MailSummary, flag: "seen" | "flagged") {
    const on = !m[flag];
    messages = messages.map((x) => (x.uid === m.uid ? { ...x, [flag]: on } : x));
    if (message?.uid === m.uid && flag === "flagged") message = { ...message, flagged: on };
    try {
      await api.mailSetFlag(service.id, folder, [m.uid], flag, on);
    } catch (err) {
      messages = messages.map((x) => (x.uid === m.uid ? { ...x, [flag]: !on } : x));
      ontoast(errorMessage(err));
    }
  }

  function markUnread() {
    const m = messages.find((x) => x.uid === selected);
    if (!m) return;
    void toggle({ ...m, seen: true }, "seen");
    closeMessage();
  }

  // Tipos que a janela de anexos mostra (`attachment_view` no núcleo): só
  // para o atalho de espaço escolher qual abrir; quem decide é o backend.
  const VIEWABLE = /\.(pdf|png|jpe?g|gif|webp|bmp|mp4|mp3|m4a|wav|txt|docx?|xls[xm]?|pptx?|od[tsp]|rtf|csv|pages|numbers|key|heic|heif|tiff?|mov)$/i;
  let viewing = $state<number | null>(null);

  /** Abre o anexo na janela de visualização; tipo sem visualização é salvo. */
  async function openAttachment(index: number) {
    if (!message || viewing !== null) return;
    viewing = index;
    try {
      const shown = await api.mailPreviewAttachment(service.id, folder, message.uid, index);
      if (!shown) await saveAttachment(index);
    } catch (err) {
      ontoast(errorMessage(err));
    } finally {
      viewing = null;
    }
  }

  /** Espaço com a mensagem aberta: o primeiro anexo que dá para ver. */
  function spaceKey(e: KeyboardEvent) {
    if (e.key !== " " || e.repeat || e.metaKey || e.ctrlKey || e.altKey || e.defaultPrevented) return;
    if (!message || composing) return;
    const el = e.target as HTMLElement | null;
    if (el?.closest("input, textarea, select, button, a, [contenteditable], [role='dialog'], dialog")) return;
    if (document.querySelector("[role='dialog'], dialog[open]")) return;
    const first = message.attachments.find((a) => VIEWABLE.test(a.name));
    if (!first) return;
    e.preventDefault();
    void openAttachment(first.index);
  }

  async function saveAttachment(index: number) {
    if (!message) return;
    try {
      const path = await api.mailSaveAttachment(service.id, folder, message.uid, index);
      const name = path.split(/[\\/]/).pop() ?? path;
      ontoast(t("Anexo salvo: {nome}", { nome: name }), { label: t("Mostrar na pasta"), run: () => void api.mailRevealFile(path) });
    } catch (err) {
      ontoast(errorMessage(err));
    }
  }

  let savingAll = $state(false);

  /** Todos os anexos numa pasta em Downloads (spec 24). */
  async function saveAll() {
    if (!message || savingAll) return;
    savingAll = true;
    try {
      const m = message;
      const dir = await api.mailSaveAttachments(service.id, folder, m.uid, m.attachments.map((a) => a.index), m.subject);
      const name = dir.split(/[\\/]/).pop() ?? dir;
      ontoast(t("{n} anexos salvos em {pasta}", { n: m.attachments.length, pasta: name }), {
        label: t("Mostrar na pasta"),
        run: () => void api.mailRevealFile(dir),
      });
    } catch (err) {
      ontoast(errorMessage(err));
    } finally {
      savingAll = false;
    }
  }

  function copied(email: string) {
    ontoast(t("Endereço copiado: {endereco}", { endereco: email }));
  }

  // ---- Escrever

  const me = $derived((service.mail?.user ?? "").toLowerCase());
  // Assinatura acima do texto citado (spec 16, R10).
  const signature = $derived(service.mail?.signature ? `\n\n-- \n${service.mail.signature}` : "");

  function addr(a: MailAddr) {
    if (!a.name) return a.email;
    const name = /[",;<>@]/.test(a.name) ? `"${a.name.replace(/["\\]/g, "")}"` : a.name;
    return a.email ? `${name} <${a.email}>` : name;
  }

  function quote(m: MailMessage) {
    const who = m.from[0] ? addr(m.from[0]) : t("alguém");
    const when = m.date ? new Date(m.date * 1000).toLocaleString(locale(), { dateStyle: "long", timeStyle: "short" }) : "";
    const text = (m.text || "").replace(/\r\n/g, "\n").trimEnd();
    const quoted = text
      .split("\n")
      .map((l) => (l.startsWith(">") ? `>${l}` : `> ${l}`))
      .join("\n");
    const intro = when ? t("Em {data}, {nome} escreveu:", { data: when, nome: who }) : t("{nome} escreveu:", { nome: who });
    return `\n\n${intro}\n${quoted}\n`;
  }

  function prefixed(subject: string, prefix: string) {
    const clean = subject.replace(/^((re|res|fw|fwd|enc|encaminhado)\s*:\s*)+/i, "");
    return `${prefix}: ${clean}`;
  }

  function reply(all: boolean) {
    if (!message) return;
    const m = message;
    const target = m.replyTo.length ? m.replyTo : m.from;
    const toList = target.map(addr);
    const ccList = all
      ? [...m.to, ...m.cc]
          .filter((a) => a.email.toLowerCase() !== me && !target.some((t) => t.email === a.email))
          .map(addr)
      : [];
    composing = {
      to: toList.join(", "),
      cc: ccList.join(", "),
      subject: prefixed(m.subject, "Re"),
      body: signature + quote(m),
      inReplyTo: m.messageId || null,
      references: m.references,
      replyFolder: folder,
      replyUid: m.uid,
    };
  }

  function forward() {
    if (!message) return;
    const m = message;
    const head = [
      t("---------- Mensagem encaminhada ----------"),
      t("De: {valor}", { valor: m.from.map(addr).join(", ") }),
      m.date ? t("Data: {valor}", { valor: new Date(m.date * 1000).toLocaleString(locale()) }) : "",
      t("Assunto: {valor}", { valor: m.subject }),
      t("Para: {valor}", { valor: m.to.map(addr).join(", ") }),
    ]
      .filter(Boolean)
      .join("\n");
    composing = { subject: prefixed(m.subject, t("Enc")), body: `${signature}\n\n${head}\n\n${m.text}` };
  }

  function compose(to = "") {
    composing = { to, body: signature };
  }

  function sent(queued: boolean) {
    composing = null;
    if (queued) return; // A barra de espera mostra; o aviso vem no fim.
    ontoast(t("E-mail enviado."));
    if (current?.role === "sent") void loadList(true);
  }

  // ---- Corpo do e-mail (HTML isolado num quadro sem scripts)

  const srcdoc = $derived.by(() => {
    if (!message?.html) return "";
    const img = showImages ? "data: https: http:" : "data:";
    const csp = `default-src 'none'; img-src ${img}; style-src 'unsafe-inline'${showImages ? " https:" : ""}; font-src data:${showImages ? " https:" : ""}`;
    return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${csp}"><base target="_blank"><style>html,body{margin:0;background:#fff;color:#1f2328;font:14px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;overflow-wrap:anywhere}body{padding:16px}img{max-width:100%;height:auto}table{max-width:100%}pre{white-space:pre-wrap}</style></head><body>${message.html}</body></html>`;
  });

  function link(href: string | null) {
    if (!href) return;
    if (/^mailto:/i.test(href)) {
      const to = decodeURIComponent(href.slice(7).split("?")[0] ?? "");
      compose(to);
    } else if (/^https?:/i.test(href)) {
      void api.openUrl(href);
    }
  }

  function frameLoaded() {
    const doc = frame?.contentDocument;
    if (!doc) return;
    // Links abrem no navegador (ou no escritor, se mailto:); nada navega aqui.
    doc.addEventListener("click", (e) => {
      const a = (e.target as Element | null)?.closest?.("a");
      if (!a) return;
      e.preventDefault();
      link(a.getAttribute("href"));
    });
    doc.addEventListener("submit", (e) => e.preventDefault());
  }

  // Texto puro com endereços clicáveis.
  function pieces(text: string) {
    const out: { text: string; url?: string }[] = [];
    const re = /(https?:\/\/[^\s<>"')\]]+)/g;
    let last = 0;
    for (const m of text.matchAll(re)) {
      if (m.index! > last) out.push({ text: text.slice(last, m.index) });
      out.push({ text: m[0], url: m[0] });
      last = m.index! + m[0].length;
    }
    if (last < text.length) out.push({ text: text.slice(last) });
    return out;
  }

  // ---- Formatação

  function when(secs: number) {
    if (!secs) return "";
    const d = new Date(secs * 1000);
    const now = new Date();
    if (d.toDateString() === now.toDateString()) {
      return d.toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
    }
    if (d.getFullYear() === now.getFullYear()) {
      return d.toLocaleDateString(locale(), { day: "numeric", month: "short" });
    }
    return d.toLocaleDateString(locale(), { day: "2-digit", month: "2-digit", year: "2-digit" });
  }

  function who(a: MailAddr | undefined) {
    return a?.name || a?.email || t("(sem remetente)");
  }

  function size(n: number) {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  function listKey(e: KeyboardEvent) {
    if (composing || (e.target as HTMLElement).closest("input, textarea")) return;
    const index = messages.findIndex((m) => m.uid === selected);
    if (e.key === "ArrowDown" || e.key === "j") {
      e.preventDefault();
      const next = messages[index + 1] ?? messages[0];
      if (next) void openMessage(next.uid);
    } else if (e.key === "ArrowUp" || e.key === "k") {
      e.preventDefault();
      const prev = messages[index - 1];
      if (prev) void openMessage(prev.uid);
    } else if ((e.key === "Delete" || e.key === "Backspace") && selected !== null) {
      e.preventDefault();
      remove(selected);
    }
  }
</script>

<svelte:window onkeydown={spaceKey} />

<div class="reader">
  {#if unset}
    <div class="notice">
      <Mail size={26} />
      <p>
        <strong>{service.name}</strong>
        {t("pode abrir direto no leitor do Papeo, pelo IMAP: mais leve que o site, que só carrega quando você pedir.")}
      </p>
      <div class="actions">
        <button class="btn btn-primary" onclick={onconfigure}>
          {service.kind === "outlook" ? t("Entrar com a Microsoft") : t("Configurar e-mail")}
        </button>
        <button class="btn" onclick={onsite}><Globe size={15} /> {t("Usar o site")}</button>
      </div>
    </div>
  {:else if noPassword}
    <div class="notice">
      <CircleAlert size={22} />
      <p>{t("A senha do e-mail de")} <strong>{service.name}</strong> {t("não está salva neste computador.")}</p>
      <div class="actions">
        <button class="btn btn-primary" onclick={onconfigure}>{t("Configurar e-mail")}</button>
        <button class="btn" onclick={onsite}><Globe size={15} /> {t("Abrir no site")}</button>
      </div>
    </div>
  {:else}
    <nav class="folders" aria-label={t("Pastas")}>
      <button class="btn btn-primary compose" onclick={() => compose()}>
        <PenLine size={15} /> {t("Escrever")}
      </button>
      <ul>
        {#each folders as f (f.path)}
          {@const Icon = f.role ? ROLE_ICON[f.role] : Folder}
          <li>
            <button
              class="folder"
              class:on={f.path === folder}
              style:padding-left="{10 + Math.min(f.depth, 4) * 12}px"
              onclick={() => chooseFolder(f.path)}
              title={folderName(f)}
            >
              <Icon size={15} />
              <span>{folderName(f)}</span>
              {#if f.role === "inbox" && service.unread}<span class="count">{service.unread}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
      <div class="foot">
        {#if status === "offline" || status === "auth"}
          <p class="status" title={service.mailStatus && "detail" in service.mailStatus ? String(service.mailStatus.detail ?? "") : ""}>
            <CircleAlert size={13} /> {status === "auth" ? t("Login recusado") : t("Sem conexão")}
          </p>
        {/if}
        <button class="link-btn" onclick={() => (signing = true)}><Signature size={13} /> {t("Assinatura")}</button>
        <button class="link-btn" onclick={onsite}><Globe size={13} /> {t("Abrir no site")}</button>
      </div>
    </nav>

    <section class="list" aria-label={t("Mensagens")}>
      <div class="narrow-bar">
        <select value={folder} onchange={(e) => chooseFolder(e.currentTarget.value)} aria-label={t("Pasta")}>
          {#each folders as f (f.path)}<option value={f.path}>{"  ".repeat(Math.min(f.depth, 4))}{folderName(f)}</option>{/each}
        </select>
        <button class="icon-btn" onclick={() => compose()} aria-label={t("Escrever")} title={t("Escrever")}><PenLine size={15} /></button>
        <button class="icon-btn" onclick={() => (signing = true)} aria-label={t("Assinatura")} title={t("Assinatura")}><Signature size={15} /></button>
        <button class="icon-btn" onclick={onsite} aria-label={t("Abrir no site")} title={t("Abrir no site")}><Globe size={15} /></button>
      </div>
      <form class="search" onsubmit={search} role="search">
        <Search size={15} />
        <input bind:value={query} placeholder={t("Buscar em {nome}", { nome: current ? folderName(current) : t("e-mails") })} aria-label={t("Buscar e-mails")} />
        {#if searched}
          <button type="button" class="icon-btn ghost" onclick={clearSearch} aria-label={t("Limpar a busca")}><X size={14} /></button>
        {/if}
        <button type="button" class="icon-btn ghost" onclick={() => loadFolders(true).then(() => loadList())} aria-label={t("Atualizar")} title={t("Atualizar")}>
          <RefreshCw size={15} class={loading ? "spin" : ""} />
        </button>
      </form>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul class="items" tabindex="0" onkeydown={listKey} aria-busy={loading}>
        {#if error}
          <li class="empty error" role="alert">
            {error}
            <button class="btn" onclick={() => loadList()}>{t("Tentar de novo")}</button>
          </li>
        {:else if !messages.length && !loading}
          <li class="empty">{searched ? t("Nada encontrado.") : t("Nenhuma mensagem.")}</li>
        {/if}
        {#each messages as m (m.uid)}
          <li>
            <button class="item" class:unread={!m.seen} class:on={m.uid === selected} onclick={() => openMessage(m.uid)}>
              <span class="line">
                <span class="who">{current?.role === "sent" || current?.role === "drafts" ? t("Para: {valor}", { valor: who(m.to[0]) }) : who(m.from)}</span>
                {#if m.hasAttachments}<Paperclip size={12} />{/if}
                {#if m.answered}<Reply size={12} />{/if}
                <span class="date">{when(m.date)}</span>
              </span>
              <span class="subject">{m.subject || t("(sem assunto)")}</span>
              <span class="preview">{m.preview}</span>
            </button>
            <button
              class="star"
              class:on={m.flagged}
              onclick={() => toggle(m, "flagged")}
              aria-label={m.flagged ? t("Tirar a estrela") : t("Marcar com estrela")}
              aria-pressed={m.flagged}
            >
              <Star size={14} />
            </button>
          </li>
        {/each}
      </ul>
      {#if total > PAGE}
        <div class="pager">
          <button class="icon-btn ghost" disabled={page === 0 || loading} onclick={() => goPage(page - 1)} aria-label={t("Mais novas")}><ChevronLeft size={16} /></button>
          <span>{t("{de}–{ate} de {total}", { de: page * PAGE + 1, ate: Math.min(total, (page + 1) * PAGE), total })}</span>
          <button class="icon-btn ghost" disabled={page >= pages - 1 || loading} onclick={() => goPage(page + 1)} aria-label={t("Mais antigas")}><ChevronRight size={16} /></button>
        </div>
      {/if}
    </section>

    <section class="view" aria-label={t("Mensagem")}>
      {#if composing}
        <MailComposer
          {service}
          start={composing}
          {undoSecs}
          onclose={() => (composing = null)}
          onsent={sent}
          ondraftsaved={() => {
            if (current?.role === "drafts") void loadList(true);
          }}
        />
      {:else if opening}
        {@const s = messages.find((x) => x.uid === selected)}
        {#if s}
          <div class="scroll">
            <header class="head">
              <h2>{s.subject || t("(sem assunto)")}</h2>
              <div class="meta">
                <span class="from"><strong>{who(s.from)}</strong></span>
                <span class="date">{new Date(s.date * 1000).toLocaleString(locale(), { dateStyle: "medium", timeStyle: "short" })}</span>
              </div>
            </header>
            <div class="placeholder"><LoaderCircle size={18} class="spin" /></div>
          </div>
        {:else}
          <div class="placeholder"><LoaderCircle size={20} class="spin" /></div>
        {/if}
      {:else if openError}
        <div class="placeholder error" role="alert">{openError}</div>
      {:else if message}
        {@const m = message}
        <div class="toolbar">
          <button class="icon-btn ghost back" onclick={closeMessage} aria-label={t("Voltar à lista")}><ArrowLeft size={17} /></button>
          <button class="btn" onclick={() => reply(false)}><Reply size={15} /> {t("Responder")}</button>
          <button class="icon-btn" onclick={() => reply(true)} title={t("Responder a todos")} aria-label={t("Responder a todos")}><ReplyAll size={16} /></button>
          <button class="icon-btn" onclick={forward} title={t("Encaminhar")} aria-label={t("Encaminhar")}><Forward size={16} /></button>
          <span class="sep"></span>
          {#if hasArchive && current?.role !== "archive" && current?.role !== "all"}
            <button class="icon-btn" onclick={() => moveTo(m.uid, "role:archive", t("Arquivada."))} title={t("Arquivar")} aria-label={t("Arquivar")}><Archive size={16} /></button>
          {/if}
          {#if current?.role === "junk"}
            <button class="icon-btn" onclick={() => moveTo(m.uid, "role:inbox", t("Movida para a Caixa de Entrada."))} title={t("Não é spam")} aria-label={t("Não é spam")}><Inbox size={16} /></button>
          {:else if hasJunk}
            <button class="icon-btn" onclick={() => moveTo(m.uid, "role:junk", t("Movida para o Spam."))} title={t("Spam")} aria-label={t("Marcar como spam")}><Ban size={16} /></button>
          {/if}
          <button
            class="icon-btn"
            class:danger={confirmDelete}
            onclick={() => remove(m.uid)}
            title={permanent ? (confirmDelete ? t("Clique de novo para excluir de vez") : t("Excluir de vez")) : t("Excluir")}
            aria-label={permanent ? t("Excluir de vez") : t("Excluir")}
          >
            <Trash2 size={16} />
          </button>
          {#if confirmDelete}<span class="confirm">{t("Clique de novo para excluir de vez")}</span>{/if}
          <button class="icon-btn" onclick={markUnread} title={t("Marcar como não lida")} aria-label={t("Marcar como não lida")}><MailOpen size={16} /></button>
          <button
            class="icon-btn"
            class:pressed={m.flagged}
            onclick={() => {
              const s = messages.find((x) => x.uid === m.uid);
              if (s) void toggle(s, "flagged");
            }}
            title={t("Estrela")}
            aria-label={t("Estrela")}
            aria-pressed={m.flagged}
          >
            <Flag size={16} />
          </button>
        </div>
        <div class="scroll">
          <header class="head">
            <h2>{m.subject || t("(sem assunto)")}</h2>
            <div class="meta">
              <span class="from">
                {#if m.from.length}
                  <AddressList list={m.from} full oncompose={(a) => compose(addr(a))} oncopied={copied} />
                {:else}<strong>{who(m.from[0])}</strong>{/if}
              </span>
              <span class="date">{m.date ? new Date(m.date * 1000).toLocaleString(locale(), { dateStyle: "medium", timeStyle: "short" }) : ""}</span>
            </div>
            {#key m.uid}
              {#if m.to.length}<AddressList label={t("Para:")} list={m.to} oncompose={(a) => compose(addr(a))} oncopied={copied} />{/if}
              {#if m.cc.length}<AddressList label={t("Cc:")} list={m.cc} oncompose={(a) => compose(addr(a))} oncopied={copied} />{/if}
            {/key}
          </header>
          {#if m.attachments.length > 1}
            <div class="all-attachments">
              <span>{t("{n} anexos", { n: m.attachments.length })}</span>
              <button class="link-btn" onclick={saveAll} disabled={savingAll}>
                <Download size={13} />
                {t("Baixar todos")}
              </button>
            </div>
          {/if}
          {#if m.attachments.length}
            <ul class="attachments">
              {#each m.attachments as a (a.index)}
                <li class="attachment" class:busy={viewing === a.index}>
                  <button class="open" onclick={() => openAttachment(a.index)} title={t("Abrir {nome}", { nome: a.name })}>
                    <Paperclip size={14} />
                    <span class="name">{a.name}</span>
                    <span class="size">{size(a.size)}</span>
                  </button>
                  <button
                    class="save"
                    onclick={() => saveAttachment(a.index)}
                    title={t("Salvar em Downloads")}
                    aria-label={t("Salvar {nome} em Downloads", { nome: a.name })}
                  >
                    <Download size={14} />
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
          {#if m.html}
            {#if m.hasRemote && !showImages}
              <div class="images">
                <Image size={15} /> {t("Imagens de fora bloqueadas (protegem você de rastreamento).")}
                <button class="link-btn" onclick={() => (showImages = true)}>{t("Mostrar imagens")}</button>
              </div>
            {/if}
            {#key srcdoc}
              <iframe
                bind:this={frame}
                class="body-frame"
                title={t("Conteúdo do e-mail")}
                sandbox="allow-same-origin"
                {srcdoc}
                onload={frameLoaded}
              ></iframe>
            {/key}
          {:else}
            <div class="text">
              {#each pieces(m.text) as p, i (i)}{#if p.url}<a href={p.url} onclick={(e) => { e.preventDefault(); link(p.url!); }}>{p.text}</a>{:else}{p.text}{/if}{/each}
            </div>
          {/if}
        </div>
      {:else}
        <div class="placeholder">
          <Mail size={28} />
          <span>{total ? (total === 1 ? t("{n} mensagem", { n: total }) : t("{n} mensagens", { n: total })) : ""}</span>
        </div>
      {/if}
    </section>
  {/if}
  {#if waiting.length}
    <ul class="sending" aria-label={t("Envios em espera")}>
      {#each waiting as p (p.holdId)}
        <li role="status">
          <Send size={14} />
          <span class="what">{t("Enviando")} <strong>{p.subject || t("(sem assunto)")}</strong></span>
          <span class="secs">{t("em {n} s", { n: Math.max(0, Math.ceil((p.until - now) / 1000)) })}</span>
          <button class="btn" onclick={() => undo(p.holdId)} disabled={undoing === p.holdId}>{t("Desfazer")}</button>
        </li>
      {/each}
    </ul>
  {/if}
</div>


{#if signing}<SignatureDialog {service} onclose={() => (signing = false)} />{/if}
<style>
  .sending {
    position: absolute;
    left: 50%;
    bottom: 16px;
    transform: translateX(-50%);
    z-index: 5;
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    max-width: calc(100% - 32px);
  }
  .sending li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 8px 8px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    box-shadow: 0 6px 24px rgb(0 0 0 / 0.16);
    font-size: 13px;
  }
  .sending .what {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sending .secs {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .reader {
    position: absolute;
    inset: 0;
    display: grid;
    grid-template-columns: 200px minmax(260px, 360px) 1fr;
    background: var(--bg);
    color: var(--text);
    min-height: 0;
  }
  .notice {
    grid-column: 1 / -1;
    align-self: center;
    justify-self: center;
    max-width: 420px;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    color: var(--text-muted);
  }
  .notice .actions {
    display: flex;
    gap: 8px;
  }
  .folders {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    background: var(--bg-subtle);
    min-height: 0;
  }
  .compose {
    margin: 12px;
    justify-content: center;
  }
  .folders ul {
    list-style: none;
    margin: 0;
    padding: 0 6px;
    overflow-y: auto;
    flex: 1;
  }
  .folder {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    text-align: left;
    font-size: 13.5px;
    cursor: pointer;
  }
  .folder span:first-of-type {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folder:hover {
    background: var(--bg-hover);
  }
  .folder.on {
    background: var(--bg-hover);
    font-weight: 600;
  }
  .count {
    font-size: 12px;
    font-weight: 600;
    color: var(--primary);
  }
  .foot {
    padding: 10px 12px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .foot .link-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
  }
  .status {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--warn-text);
  }
  .list {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    min-height: 0;
    min-width: 0;
  }
  .narrow-bar {
    display: none;
    gap: 6px;
    padding: 8px 8px 0;
  }
  .narrow-bar select {
    flex: 1;
    min-width: 0;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border);
    color: var(--text-muted);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    padding: 4px 0;
  }
  .items {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    flex: 1;
    outline: none;
  }
  .items li {
    position: relative;
    border-bottom: 1px solid var(--border);
  }
  .item {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 9px 34px 9px 14px;
    border: 0;
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .item:hover {
    background: var(--bg-hover);
  }
  .item.on {
    background: var(--bg-hover);
    box-shadow: inset 3px 0 0 var(--primary);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
  }
  .who {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13.5px;
    color: var(--text);
  }
  .date {
    font-size: 12px;
    color: var(--text-faint);
    white-space: nowrap;
  }
  .subject,
  .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }
  .preview {
    color: var(--text-faint);
    font-size: 12.5px;
  }
  .unread .who,
  .unread .subject {
    font-weight: 650;
  }
  .unread .who::before {
    content: "";
    display: inline-block;
    width: 7px;
    height: 7px;
    margin-right: 6px;
    border-radius: 50%;
    background: var(--primary);
    vertical-align: 1px;
  }
  .star {
    position: absolute;
    right: 8px;
    bottom: 8px;
    border: 0;
    background: transparent;
    color: var(--text-faint);
    padding: 3px;
    cursor: pointer;
    opacity: 0;
  }
  .items li:hover .star,
  .star:focus-visible,
  .star.on {
    opacity: 1;
  }
  .star.on {
    color: #e3a008;
  }
  .star.on :global(svg) {
    fill: currentColor;
  }
  .empty {
    padding: 24px 16px;
    color: var(--text-muted);
    font-size: 13px;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }
  .error {
    color: var(--danger);
  }
  .pager {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 6px;
    border-top: 1px solid var(--border);
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .view {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--text-faint);
    font-size: 13px;
    padding: 20px;
    text-align: center;
  }
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  .toolbar .back {
    display: none;
  }
  .sep {
    flex: 1;
  }
  .confirm {
    font-size: 12px;
    color: var(--danger);
  }
  .icon-btn.danger {
    color: var(--danger);
    border-color: var(--danger);
  }
  .icon-btn.pressed {
    color: #e3a008;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
  .head {
    padding: 16px 20px 10px;
  }
  .head h2 {
    margin: 0 0 10px;
    font-size: 18px;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .meta {
    display: flex;
    gap: 10px;
    justify-content: space-between;
    align-items: baseline;
    flex-wrap: wrap;
    font-size: 13.5px;
  }
  .head .from {
    min-width: 0;
  }
  .all-attachments {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 20px 6px;
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .all-attachments .link-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12.5px;
  }
  .attachments {
    list-style: none;
    margin: 0;
    padding: 0 20px 10px;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .attachment {
    display: flex;
    align-items: stretch;
    max-width: 320px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-subtle);
    overflow: hidden;
  }
  .attachment.busy {
    opacity: 0.6;
  }
  .attachment button {
    display: flex;
    align-items: center;
    gap: 6px;
    border: 0;
    background: none;
    color: var(--text);
    font-size: 12.5px;
    cursor: pointer;
  }
  .attachment .open {
    min-width: 0;
    padding: 5px 9px;
  }
  .attachment .save {
    padding: 5px 8px;
    border-left: 1px solid var(--border);
    color: var(--text-muted);
  }
  .attachment button:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .attachment .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .attachment .size {
    color: var(--text-faint);
  }
  .images {
    margin: 0 20px 10px;
    padding: 8px 12px;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    border-radius: var(--radius-sm);
    background: var(--bg-subtle);
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .body-frame {
    flex: 1;
    min-height: 360px;
    width: 100%;
    border: 0;
    border-top: 1px solid var(--border);
    background: #fff;
  }
  .text {
    padding: 4px 20px 24px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: 14px;
    line-height: 1.55;
  }
  .reader :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .reader :global(.spin) {
      animation: none;
    }
  }
  /* Janela estreita: pastas somem; lista ou mensagem ocupam a tela. */
  @media (max-width: 980px) {
    .reader {
      grid-template-columns: minmax(240px, 320px) 1fr;
    }
    .folders {
      display: none;
    }
    .narrow-bar {
      display: flex;
    }
  }
  @media (max-width: 700px) {
    .reader {
      grid-template-columns: 1fr;
    }
    .reader:has(.view .toolbar) .list,
    .reader:has(.view :global(.composer)) .list {
      display: none;
    }
    .reader:not(:has(.view .toolbar)):not(:has(.view :global(.composer))) .view {
      display: none;
    }
    .toolbar .back {
      display: inline-flex;
    }
  }
</style>
