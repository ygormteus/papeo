<script lang="ts">
  import {
    Archive,
    ArrowLeft,
    Ban,
    CheckCheck,
    CircleAlert,
    Download,
    FileText,
    Flag,
    Folder,
    FolderInput,
    Forward,
    Globe,
    Image,
    Inbox,
    LoaderCircle,
    Mail,
    MailOpen,
    MailX,
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
  import ContextMenu from "./ContextMenu.svelte";
  import { loadUnsent, outbox, releaseReturned, startOutbox, takeReturned, undoSend } from "./outbox.svelte";
  import {
    api,
    errorMessage,
    type MailAddr,
    type MailCache,
    type MailFilter,
    type MailFolder,
    type MailMessage,
    type MailRole,
    type MailSummary,
    type ServiceView,
    type MenuItem,
  } from "./api";

  let {
    service,
    reveal,
    undoSecs,
    onsite,
    onconfigure,
    ontoast,
    focused = true,
  }: {
    service: ServiceView;
    /** Assunto a procurar (clique num aviso); muda a cada pedido. */
    reveal: { query: string; n: number; uid?: number | null } | null;
    /** Espera para desfazer o envio (spec 16); 0 envia na hora. */
    undoSecs: number;
    onsite: () => void;
    onconfigure: () => void;
    ontoast: (text: string, action?: { label: string; run: () => void }) => void;
    /** Tela dividida: só a metade em foco atende os atalhos de teclado. */
    focused?: boolean;
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
  /** Buscando a próxima leva da rolagem infinita (spec 27). */
  let more = $state(false);
  let moreError = $state("");
  let query = $state("");
  let searched = $state("");
  /** Filtro rápido (spec 28). */
  let filter = $state<MailFilter>("all");
  /** Seleção com Cmd/Ctrl ou Shift (spec 28): várias de uma vez. */
  let picked = $state<number[]>([]);
  let moveMenu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let searchInput: HTMLInputElement | undefined = $state();
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

  // Mais novas de cada pasta já vista (do disco ao abrir, spec 27, e de
  // cada resposta do servidor): trocar de pasta mostra na hora e atualiza.
  let recent = new Map<string, MailCache["lists"][number]>();

  /** Mostra o guardado de `path`, se houver, enquanto o servidor responde. */
  function showRecent(path: string) {
    const kept = filter === "all" ? recent.get(path) : undefined;
    messages = kept ? kept.messages : [];
    total = kept ? kept.total : 0;
  }

  /** `true` se esta resposta ainda é a atual (não trocaram de pasta). */
  async function loadList(quiet = false): Promise<boolean> {
    const seq = ++listSeq;
    if (!quiet) loading = true;
    error = "";
    moreError = "";
    try {
      const result = await api.mailList(service.id, folder, 0, searched, filter);
      if (seq !== listSeq) return false;
      const fresh = result.messages;
      // Atualização sozinha com mais levas na tela: troca só as mais novas e
      // mantém as antigas já carregadas (a lista vem por UID decrescente).
      const oldest = fresh.length ? fresh[fresh.length - 1].uid : Infinity;
      const tail = quiet && fresh.length >= PAGE ? messages.filter((m) => m.uid < oldest) : [];
      messages = [...fresh, ...tail];
      total = result.total;
      if (!searched && filter === "all") recent.set(folder, { path: folder, total: result.total, messages: fresh });
      return true;
    } catch (err) {
      if (seq === listSeq && !quiet) error = errorMessage(err);
    } finally {
      if (seq === listSeq) loading = false;
    }
    return false;
  }

  /** Próxima leva ao rolar até perto do fim (spec 27). */
  async function loadMore() {
    if (more || loading || error || messages.length >= total) return;
    const seq = listSeq;
    more = true;
    moreError = "";
    try {
      const result = await api.mailList(service.id, folder, messages.length, searched, filter);
      if (seq !== listSeq) return;
      // Mensagem nova no meio empurra as outras: a repetida não entra.
      const have = new Set(messages.map((m) => m.uid));
      const added = result.messages.filter((m) => !have.has(m.uid));
      messages = [...messages, ...added];
      // Nada novo: o total mudou no servidor; para de pedir.
      total = added.length ? result.total : messages.length;
    } catch (err) {
      if (seq === listSeq) moreError = errorMessage(err);
    } finally {
      if (seq === listSeq) more = false;
    }
  }

  /** Avisa quando o fim da lista chega perto da área visível. */
  function nearEnd(node: HTMLElement) {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) void loadMore();
      },
      { root: node.closest(".items"), rootMargin: "0px 0px 600px 0px" },
    );
    observer.observe(node);
    return { destroy: () => observer.disconnect() };
  }

  // Conta trocada: começa de novo, do guardado no disco se houver.
  let loadedFor = "";
  $effect(() => {
    const id = service.id;
    if (unset || noPassword || id === loadedFor) return;
    loadedFor = id;
    folders = [];
    folder = "INBOX";
    messages = [];
    total = 0;
    recent = new Map();
    query = searched = "";
    filter = "all";
    picked = [];
    closeMessage();
    void (async () => {
      const stored = await api.mailCached(id).catch(() => null);
      if (loadedFor !== id) return;
      if (stored?.folders.length && !folders.length) {
        folders = stored.folders;
        folder = stored.folders.find((f) => f.role === "inbox")?.path ?? stored.folders[0].path;
      }
      for (const l of stored?.lists ?? []) recent.set(l.path, l);
      if (!messages.length && !searched) showRecent(folder);
      await loadFolders();
      if (loadedFor === id) await loadList();
    })();
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
      bodyHtml: d.bodyHtml ?? null,
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
    if (lastUnread !== null && unread !== lastUnread && !searched) {
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
    const subject = reveal.query;
    const uid = reveal.uid;
    folder = inbox;
    filter = "all";
    picked = [];
    composing = null;
    const bySubject = () => {
      query = searched = subject;
      void loadList().then((current) => {
        if (current && messages[0]) void openMessage(messages[0].uid);
      });
    };
    // Aviso de e-mail novo: abre a mensagem certa direto (spec 28); se ela
    // já saiu da caixa, procura pelo assunto.
    if (uid) {
      query = searched = "";
      showRecent(inbox);
      void loadList();
      void openMessage(uid).then(() => {
        if (openError && selected === uid) bySubject();
      });
    } else bySubject();
  });

  function chooseFolder(path: string) {
    if (path === folder && !searched) return void loadList();
    folder = path;
    query = searched = "";
    filter = "all";
    picked = [];
    closeMessage();
    showRecent(path);
    void loadList();
  }

  function search(e: SubmitEvent) {
    e.preventDefault();
    searched = query.trim();
    picked = [];
    closeMessage();
    void loadList();
  }

  function clearSearch() {
    query = searched = "";
    picked = [];
    showRecent(folder);
    void loadList();
  }

  function chooseFilter(next: MailFilter) {
    if (next === filter) return;
    filter = next;
    picked = [];
    closeMessage();
    showRecent(folder);
    void loadList();
  }

  async function markAllRead() {
    const path = folder;
    try {
      await api.mailMarkAllRead(service.id, path);
      if (path === folder) messages = messages.map((m) => ({ ...m, seen: true }));
      ontoast(t("Todas marcadas como lidas."));
    } catch (err) {
      ontoast(errorMessage(err));
    }
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
    const request = api.mailOpen(service.id, path, next.uid, true, next.size);
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
        bodyHtml: d.bodyHtml,
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
    confirmDelete = false;
    const summary = messages.find((m) => m.uid === uid);
    // Imagens de fora: decide pela mensagem aberta (o mesmo remetente que o
    // aviso mostra), não pelo resumo da lista.
    showImages = false;
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
      showImages = trusted(cached.from[0]?.email);
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
      const view = await api.mailOpen(service.id, folder, uid, false, summary?.size);
      remember(view);
      if (seq !== openSeq) return;
      message = view;
      showImages = trusted(view.from[0]?.email);
      messages = messages.map((m) => (m.uid === uid ? { ...m, seen: true } : m));
      void prefetchAfter(uid);
    } catch (err) {
      if (seq === openSeq) openError = errorMessage(err);
    } finally {
      if (seq === openSeq) opening = false;
    }
  }

  /** Caminho da pasta de um papel, como o backend escolhe. */
  function rolePath(target: string): string | undefined {
    const by = (r: MailRole) => folders.find((f) => f.role === r)?.path;
    if (target === "role:archive") return by("archive") ?? by("all");
    if (target === "role:junk") return by("junk");
    if (target === "role:inbox") return by("inbox");
    if (target === "role:trash") return by("trash");
    return target;
  }

  // Ação em mensagens: tira da lista na hora e abre a seguinte. Se o
  // servidor diz os UIDs novos no destino, o aviso oferece desfazer.
  async function removeFromList(uids: number[], action: () => Promise<number[]>, done: string, target?: string) {
    const gone = new Set(uids);
    uids.forEach((u) => cache.delete(key(u)));
    const path = folder;
    const before = messages;
    const index = messages.findIndex((m) => gone.has(m.uid));
    messages = messages.filter((m) => !gone.has(m.uid));
    total = Math.max(0, total - (before.length - messages.length));
    picked = [];
    if (selected !== null && gone.has(selected)) {
      const next = messages[Math.min(index, messages.length - 1)];
      if (next && uids.length === 1) void openMessage(next.uid);
      else closeMessage();
    }
    try {
      const moved = await action();
      const back = target && rolePath(target);
      if (back && moved.length === uids.length) {
        ontoast(done, {
          label: t("Desfazer"),
          run: () => {
            api
              .mailMove(service.id, back, moved, path)
              .then(() => {
                if (folder === path) void loadList(true);
              })
              .catch((err) => ontoast(errorMessage(err)));
          },
        });
      } else ontoast(done);
    } catch (err) {
      // Volta só as que saíram (outras mudanças no meio tempo ficam).
      if (folder === path) {
        const removed = before.filter((m) => gone.has(m.uid) && !messages.some((x) => x.uid === m.uid));
        messages = [...messages, ...removed].sort((a, b) => b.uid - a.uid);
        total += removed.length;
      }
      ontoast(errorMessage(err));
    }
  }

  /** Alvo das ações: as escolhidas, senão a aberta. */
  function targets(uid?: number): number[] {
    if (uid === undefined && picked.length) return [...picked];
    if (uid !== undefined) return [uid];
    return selected !== null ? [selected] : [];
  }

  function remove(uid?: number) {
    const uids = targets(uid);
    if (!uids.length) return;
    if (permanent && !confirmDelete) {
      confirmDelete = true;
      return;
    }
    confirmDelete = false;
    const n = uids.length;
    void removeFromList(
      uids,
      () => api.mailDelete(service.id, folder, uids),
      permanent
        ? n > 1 ? t("{n} excluídas de vez.", { n }) : t("Excluída de vez.")
        : n > 1 ? t("{n} movidas para a Lixeira.", { n }) : t("Movida para a Lixeira."),
      permanent ? undefined : "role:trash",
    );
  }

  function moveTo(uid: number | undefined, target: string, done: string) {
    const uids = targets(uid);
    if (!uids.length) return;
    void removeFromList(uids, () => api.mailMove(service.id, folder, uids, target), done, target);
  }

  /** "Mover para…": menu com as outras pastas. */
  function chooseMove(e: MouseEvent, uid?: number) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    moveMenu = {
      x: r.left,
      y: r.bottom + 4,
      items: folders
        .filter((f) => f.path !== folder)
        .map((f) => ({
          label: "\u2003".repeat(Math.min(f.depth, 4)) + folderName(f),
          icon: f.role ? ROLE_ICON[f.role] : Folder,
          run: () => moveTo(uid, f.path, t("Movida para {pasta}.", { pasta: folderName(f) })),
        })),
    };
  }

  // ---- Seleção de várias (spec 28)

  /** Ponto de partida do Shift (a última clicada sem Shift). */
  let anchor: number | null = null;
  /** Ponta móvel do Shift com as setas. */
  let cursor: number | null = null;

  // Exclusão definitiva armada vale só para o que estava escolhido.
  $effect(() => {
    void picked;
    void selected;
    confirmDelete = false;
  });

  function pick(e: MouseEvent, uid: number): boolean {
    if (e.shiftKey) {
      const from = anchor ?? selected;
      const a = messages.findIndex((m) => m.uid === from);
      const b = messages.findIndex((m) => m.uid === uid);
      if (a < 0 || b < 0) return false;
      picked = messages.slice(Math.min(a, b), Math.max(a, b) + 1).map((m) => m.uid);
      cursor = uid;
      return true;
    }
    if (e.metaKey || e.ctrlKey) {
      const base = picked.length ? picked : selected !== null ? [selected] : [];
      picked = base.includes(uid) ? base.filter((u) => u !== uid) : [...base, uid];
      anchor = cursor = uid;
      return true;
    }
    anchor = cursor = uid;
    return false;
  }

  /** Shift+seta: estende a seleção sem abrir (abrir marcaria como lida). */
  function extend(step: 1 | -1) {
    const from = cursor ?? selected;
    const at = messages.findIndex((m) => m.uid === from);
    const next = messages[at + step];
    if (!next) return;
    if (anchor === null) anchor = from;
    pick(new MouseEvent("click", { shiftKey: true }), next.uid);
  }

  function clickItem(e: MouseEvent, uid: number) {
    if (pick(e, uid)) return;
    picked = [];
    void openMessage(uid);
  }

  async function markPicked(flag: "seen" | "flagged", on: boolean) {
    const uids = [...picked];
    const set = new Set(uids);
    const before = messages;
    messages = messages.map((m) => (set.has(m.uid) ? { ...m, [flag]: on } : m));
    try {
      await api.mailSetFlag(service.id, folder, uids, flag, on);
    } catch (err) {
      messages = before;
      ontoast(errorMessage(err));
    }
  }

  // ---- Arrastar para uma pasta (spec 28)

  const DRAG_TYPE = "application/x-papeo-mail";
  let dropOn = $state<string | null>(null);

  function dragStart(e: DragEvent, uid: number) {
    const uids = picked.includes(uid) ? [...picked] : [uid];
    e.dataTransfer?.setData(DRAG_TYPE, JSON.stringify({ id: service.id, folder, uids }));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  }

  function dragOver(e: DragEvent, path: string) {
    if (path === folder || !e.dataTransfer?.types.includes(DRAG_TYPE)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    dropOn = path;
  }

  function drop(e: DragEvent, f: MailFolder) {
    dropOn = null;
    const raw = e.dataTransfer?.getData(DRAG_TYPE);
    if (!raw) return;
    e.preventDefault();
    try {
      const data = JSON.parse(raw) as { id: string; folder: string; uids: number[] };
      if (data.id !== service.id || data.folder !== folder || f.path === folder) return;
      const uids = data.uids.filter((u) => messages.some((m) => m.uid === u));
      if (!uids.length) return;
      void removeFromList(
        uids,
        () => api.mailMove(service.id, folder, uids, f.path),
        t("Movida para {pasta}.", { pasta: folderName(f) }),
        f.path,
      );
    } catch {
      // Arrasto de outra coisa: ignora.
    }
  }

  // ---- Imagens de remetentes confiáveis (spec 28), só neste computador

  const TRUST_KEY = "papeo.mail.trustedImages";
  const TRUST_MAX = 500;
  function trustList(): string[] {
    try {
      const all = JSON.parse(localStorage.getItem(TRUST_KEY) ?? "{}") as Record<string, string[]>;
      return Array.isArray(all[service.id]) ? all[service.id] : [];
    } catch {
      return [];
    }
  }
  // svelte-ignore state_referenced_locally
  let trustedList = $state<string[]>(trustList());
  $effect(() => {
    void service.id;
    trustedList = trustList();
  });
  function trusted(email: string | undefined): boolean {
    return !!email && trustedList.includes(email.toLowerCase());
  }
  function setTrust(email: string, on: boolean) {
    const e = email.toLowerCase();
    const next = trustedList.filter((x) => x !== e);
    if (on) next.unshift(e);
    trustedList = next.slice(0, TRUST_MAX);
    try {
      const all = JSON.parse(localStorage.getItem(TRUST_KEY) ?? "{}") as Record<string, string[]>;
      all[service.id] = trustedList;
      localStorage.setItem(TRUST_KEY, JSON.stringify(all));
    } catch {
      // Sem armazenamento: vale só nesta sessão.
    }
    showImages = on;
  }

  // ---- Descadastrar (List-Unsubscribe, spec 28)

  function unsubscribe(target: string) {
    if (/^https?:/i.test(target)) {
      void api.openUrl(target);
      return;
    }
    const [to = "", qs = ""] = target.slice(7).split("?");
    const decode = (v: string) => {
      try {
        return decodeURIComponent(v);
      } catch {
        return v;
      }
    };
    // `mailto:` não usa `+` como espaço (URLSearchParams usaria).
    const params = new Map(
      qs
        .split("&")
        .map((p) => p.split("="))
        .map(([k = "", v = ""]) => [decode(k).toLowerCase(), decode(v)] as const),
    );
    composing = {
      to: decode(to),
      subject: params.get("subject") || "unsubscribe",
      body: params.get("body") ?? "",
    };
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
    if (picked.length) return void markPicked("seen", false);
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
    // Atalhos valem com o foco no corpo do e-mail também.
    doc.addEventListener("keydown", (e) => shortcut(e));
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
      if (e.shiftKey) return extend(1);
      const next = messages[index + 1] ?? messages[0];
      if (next) {
        anchor = cursor = next.uid;
        picked = [];
        void openMessage(next.uid);
      }
    } else if (e.key === "ArrowUp" || e.key === "k") {
      e.preventDefault();
      if (e.shiftKey) return extend(-1);
      const prev = messages[index - 1];
      if (prev) {
        anchor = cursor = prev.uid;
        picked = [];
        void openMessage(prev.uid);
      }
    } else if ((e.key === "Delete" || e.key === "Backspace") && (selected !== null || picked.length)) {
      e.preventDefault();
      remove();
    } else if (e.key === "a" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      picked = messages.map((m) => m.uid);
    }
  }

  /** Atalhos de teclado do leitor (spec 28). */
  function shortcut(e: KeyboardEvent) {
    if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || e.repeat) return;
    if (composing || unset || noPassword || moveMenu) return;
    const el = e.target as HTMLElement | null;
    if (el?.closest?.("input, textarea, select, [contenteditable]")) return;
    if (document.querySelector("[role='dialog'], dialog[open]")) return;
    const act = (run: () => void) => {
      e.preventDefault();
      run();
    };
    const any = picked.length > 0 || selected !== null;
    switch (e.key) {
      case "/":
        return act(() => searchInput?.focus());
      case "e":
        if (any && hasArchive && current?.role !== "archive" && current?.role !== "all")
          act(() => moveTo(undefined, "role:archive", t("Arquivada.")));
        return;
      case "#":
        if (any) act(() => remove());
        return;
      case "r":
        if (message && !picked.length) act(() => reply(false));
        return;
      case "a":
        if (message && !picked.length) act(() => reply(true));
        return;
      case "f":
        if (message && !picked.length) act(forward);
        return;
      case "s":
        if (picked.length) act(() => markPicked("flagged", true));
        else if (message) {
          const m = messages.find((x) => x.uid === message!.uid);
          if (m) act(() => toggle(m, "flagged"));
        }
        return;
      case "u":
        if (any) act(markUnread);
        return;
      case "Escape":
        if (picked.length) act(() => (picked = []));
        return;
    }
  }

  function windowKey(e: KeyboardEvent) {
    if (!focused) return;
    spaceKey(e);
    shortcut(e);
  }
</script>

<svelte:window onkeydown={windowKey} />

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
              class:drop={dropOn === f.path}
              style:padding-left="{10 + Math.min(f.depth, 4) * 12}px"
              onclick={() => chooseFolder(f.path)}
              ondragover={(e) => dragOver(e, f.path)}
              ondragleave={() => {
                if (dropOn === f.path) dropOn = null;
              }}
              ondrop={(e) => drop(e, f)}
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
        <input bind:this={searchInput} bind:value={query} placeholder={t("Buscar em {nome}", { nome: current ? folderName(current) : t("e-mails") })} aria-label={t("Buscar e-mails")} />
        {#if searched}
          <button type="button" class="icon-btn ghost" onclick={clearSearch} aria-label={t("Limpar a busca")}><X size={14} /></button>
        {/if}
        {#if !searched && filter === "all"}
          <button type="button" class="icon-btn ghost" onclick={markAllRead} aria-label={t("Marcar todas como lidas")} title={t("Marcar todas como lidas")}>
            <CheckCheck size={15} />
          </button>
        {/if}
        <button type="button" class="icon-btn ghost" onclick={() => loadFolders(true).then(() => loadList())} aria-label={t("Atualizar")} title={t("Atualizar")}>
          <RefreshCw size={15} class={loading ? "spin" : ""} />
        </button>
      </form>
      <div class="filters" role="group" aria-label={t("Filtrar")}>
        {#each [["all", tk("Todas")], ["unread", tk("Não lidas")], ["flagged", tk("Com estrela")], ["attachments", tk("Com anexo")]] as [value, label] (value)}
          <button class="chip" class:on={filter === value} aria-pressed={filter === value} onclick={() => chooseFilter(value as MailFilter)}>{t(label)}</button>
        {/each}
      </div>
      {#if picked.length}
        <div class="bulk" role="toolbar" aria-label={t("Selecionadas")}>
          <button class="icon-btn ghost" onclick={() => (picked = [])} aria-label={t("Cancelar a seleção")} title={t("Cancelar a seleção")}><X size={15} /></button>
          <span class="count">{t("{n} selecionadas", { n: picked.length })}</span>
          {#if hasArchive && current?.role !== "archive" && current?.role !== "all"}
            <button class="icon-btn" onclick={() => moveTo(undefined, "role:archive", t("{n} arquivadas.", { n: picked.length }))} title={t("Arquivar")} aria-label={t("Arquivar")}><Archive size={15} /></button>
          {/if}
          {#if current?.role === "junk"}
            <button class="icon-btn" onclick={() => moveTo(undefined, "role:inbox", t("Movidas para a Caixa de Entrada."))} title={t("Não é spam")} aria-label={t("Não é spam")}><Inbox size={15} /></button>
          {:else if hasJunk}
            <button class="icon-btn" onclick={() => moveTo(undefined, "role:junk", t("Movidas para o Spam."))} title={t("Spam")} aria-label={t("Marcar como spam")}><Ban size={15} /></button>
          {/if}
          <button class="icon-btn" class:danger={confirmDelete} onclick={() => remove()} title={permanent ? t("Excluir de vez") : t("Excluir")} aria-label={permanent ? t("Excluir de vez") : t("Excluir")}><Trash2 size={15} /></button>
          <button class="icon-btn" onclick={(e) => chooseMove(e)} title={t("Mover para…")} aria-label={t("Mover para…")}><FolderInput size={15} /></button>
          <button class="icon-btn" onclick={() => markPicked("seen", true)} title={t("Marcar como lidas")} aria-label={t("Marcar como lidas")}><MailOpen size={15} /></button>
          <button class="icon-btn" onclick={() => markPicked("seen", false)} title={t("Marcar como não lidas")} aria-label={t("Marcar como não lidas")}><Mail size={15} /></button>
          <button class="icon-btn" onclick={() => markPicked("flagged", true)} title={t("Marcar com estrela")} aria-label={t("Marcar com estrela")}><Star size={15} /></button>
          {#if confirmDelete}<span class="confirm">{t("Clique de novo para excluir de vez")}</span>{/if}
        </div>
      {/if}
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
            <button
              class="item"
              class:unread={!m.seen}
              class:on={m.uid === selected || picked.includes(m.uid)}
              class:picked={picked.includes(m.uid)}
              draggable="true"
              ondragstart={(e) => dragStart(e, m.uid)}
              ondragend={() => (dropOn = null)}
              onclick={(e) => clickItem(e, m.uid)}
            >
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
        {#if messages.length && messages.length < total && !error}
          {#if moreError}
            <li class="more error" role="alert">
              {moreError}
              <button class="btn" onclick={() => loadMore()}>{t("Tentar de novo")}</button>
            </li>
          {:else}
            <!-- Nova a cada leva: o observador avisa de novo se o fim continuar à vista. -->
            {#key `${messages.length}-${loading}`}
              <li class="more" use:nearEnd aria-hidden={!more}>
                {#if more}<LoaderCircle size={15} class="spin" /> {t("Carregando mais…")}{/if}
              </li>
            {/key}
          {/if}
        {/if}
      </ul>
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
          {#if confirmDelete && !picked.length}<span class="confirm">{t("Clique de novo para excluir de vez")}</span>{/if}
          <button class="icon-btn" onclick={(e) => chooseMove(e, m.uid)} title={t("Mover para…")} aria-label={t("Mover para…")}><FolderInput size={16} /></button>
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
            {#if m.unsubscribe}
              <button class="link-btn unsubscribe" onclick={() => unsubscribe(m.unsubscribe!)} title={m.unsubscribe}>
                <MailX size={13} /> {t("Descadastrar desta lista")}
              </button>
            {/if}
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
            {@const sender = m.from[0]?.email}
            {#if m.hasRemote && !showImages}
              <div class="images">
                <Image size={15} /> {t("Imagens de fora bloqueadas (protegem você de rastreamento).")}
                <button class="link-btn" onclick={() => (showImages = true)}>{t("Mostrar imagens")}</button>
                {#if sender}
                  <button class="link-btn" onclick={() => setTrust(sender, true)}>{t("Sempre mostrar de {endereco}", { endereco: sender })}</button>
                {/if}
              </div>
            {:else if m.hasRemote && sender && trusted(sender)}
              <div class="images">
                <Image size={15} /> {t("Imagens de {endereco} sempre aparecem.", { endereco: sender })}
                <button class="link-btn" onclick={() => setTrust(sender, false)}>{t("Parar de mostrar")}</button>
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


{#if moveMenu}
  <ContextMenu x={moveMenu.x} y={moveMenu.y} items={moveMenu.items} maxX={window.innerWidth} onclose={() => (moveMenu = null)} />
{/if}
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
  .filters {
    display: flex;
    gap: 4px;
    padding: 0 10px 8px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .chip {
    flex-shrink: 0;
    padding: 3px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: transparent;
    font: inherit;
    font-size: 12px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .chip:hover {
    background: var(--bg-hover);
  }
  .chip.on {
    border-color: transparent;
    background: var(--primary);
    color: var(--primary-text);
  }
  .bulk {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    padding: 6px 10px;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    background: var(--bg-hover);
    font-size: 12.5px;
  }
  .bulk .count {
    margin-right: auto;
    font-weight: 600;
  }
  .bulk .confirm {
    width: 100%;
    color: var(--danger);
  }
  .item.picked {
    box-shadow: inset 3px 0 0 var(--accent, var(--text));
  }
  .folder.drop {
    outline: 2px dashed var(--accent, var(--text));
    outline-offset: -2px;
  }
  .unsubscribe {
    margin-top: 4px;
  }
  .more {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    min-height: 40px;
    padding: 8px;
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .more:not(.error) {
    flex-direction: row;
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
