<script lang="ts">
  import {
    ArrowLeft,
    AtSign,
    CircleAlert,
    Download,
    FileText,
    Globe,
    LoaderCircle,
    LogOut,
    MessagesSquare,
    Paperclip,
    Phone,
    RefreshCw,
    Reply,
    Search,
    Send,
    SmilePlus,
    Star,
    X,
  } from "@lucide/svelte";
  import { open as pickFile } from "@tauri-apps/plugin-dialog";
  import { locale, t } from "./i18n.svelte";
  import {
    api,
    errorMessage,
    onTalkLogin,
    onTalkRooms,
    type ServiceView,
    type TalkFile,
    type TalkMention,
    type TalkMessage,
    type TalkRoom,
  } from "./api";

  // Leitor do Nextcloud Talk pela API (spec 29): conversas, chat e envio,
  // sem a página do Nextcloud. Chamadas abrem no navegador.
  let {
    service,
    reveal,
    onsite,
    ontoast,
  }: {
    service: ServiceView;
    /** Conversa a abrir (clique num aviso); muda a cada pedido. */
    reveal: { token: string; n: number } | null;
    onsite: () => void;
    ontoast: (text: string, action?: { label: string; run: () => void }) => void;
  } = $props();

  const QUICK = ["👍", "❤️", "😂", "😮", "😢", "🙏"];

  const status = $derived(service.mailStatus?.state);
  const connected = $derived(!!service.talk && status !== "noPassword" && status !== "auth");

  // ---- Conectar (Login Flow v2)

  let connecting = $state(false);
  let connectError = $state("");

  async function connect() {
    connectError = "";
    connecting = true;
    try {
      await api.talkLogin(service.id);
    } catch (err) {
      connecting = false;
      connectError = errorMessage(err);
    }
  }

  function cancelConnect() {
    connecting = false;
    void api.talkLoginCancel(service.id);
  }

  $effect(() => {
    const id = service.id;
    const off = onTalkLogin((r) => {
      if (r.id !== id) return;
      connecting = false;
      if (r.ok) {
        ontoast(t("Talk conectado."));
        void loadRooms();
      } else if (r.error) connectError = r.error;
    });
    return () => {
      void off.then((f) => f());
    };
  });

  async function disconnect() {
    try {
      await api.talkLogout(service.id);
      rooms = [];
      closeRoom();
    } catch (err) {
      ontoast(errorMessage(err));
    }
  }

  // ---- Conversas

  let rooms = $state<TalkRoom[]>([]);
  let roomsError = $state("");
  let loadingRooms = $state(false);
  let filter = $state("");
  const shown = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return q ? rooms.filter((r) => r.name.toLowerCase().includes(q)) : rooms;
  });

  function applyRooms(list: TalkRoom[]) {
    // A aberta foi lida aqui: o servidor pode ainda não ter visto.
    rooms = list.map((r) => (r.token === token && document.hasFocus() ? { ...r, unread: 0, mention: false } : r));
    roomsError = "";
  }

  async function loadRooms(quiet = false) {
    if (!connected) return;
    if (!quiet) loadingRooms = true;
    try {
      applyRooms(await api.talkRooms(service.id));
    } catch (err) {
      if (!quiet) roomsError = errorMessage(err);
    } finally {
      loadingRooms = false;
    }
  }

  // Conta trocada ou conectada: carrega; depois, a lista que o vigia traz
  // (uma consulta só a cada 30 s, não duas).
  let loadedFor = "";
  $effect(() => {
    const key = `${service.id}\u0000${connected}`;
    if (key === loadedFor) return;
    loadedFor = key;
    rooms = [];
    closeRoom();
    if (connected) void loadRooms();
  });
  $effect(() => {
    if (!connected) return;
    const id = service.id;
    let stop: (() => void) | undefined;
    let gone = false;
    void onTalkRooms((r) => {
      if (r.id === id) applyRooms(r.rooms);
    }).then((un) => (gone ? un() : (stop = un)));
    return () => {
      gone = true;
      stop?.();
    };
  });

  // Fotos das conversas (pedidas quando aparecem na tela).
  let avatars = $state<Record<string, string>>({});
  const asked = new Set<string>();
  function avatarOf(node: HTMLElement, room: TalkRoom) {
    const key = `${room.token}\u0000${room.avatarVersion}`;
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting) || asked.has(key)) return;
      asked.add(key);
      observer.disconnect();
      api
        .talkAvatar(service.id, room.token, room.avatarVersion)
        .then((data) => {
          if (data) avatars = { ...avatars, [room.token]: data };
        })
        .catch(() => {});
    });
    observer.observe(node);
    return { destroy: () => observer.disconnect() };
  }

  function initials(name: string) {
    const parts = name.trim().split(/\s+/).filter(Boolean);
    return ((parts[0]?.[0] ?? "") + (parts.length > 1 ? (parts.at(-1)?.[0] ?? "") : "")).toUpperCase() || "?";
  }

  // ---- Conversa aberta

  let token = $state<string | null>(null);
  const room = $derived(rooms.find((r) => r.token === token) ?? null);
  let messages = $state<TalkMessage[]>([]);
  let loadingChat = $state(false);
  let chatError = $state("");
  let more = $state(false);
  let first = 0;
  let last = 0;
  let olderBusy = $state(false);
  let chatSeq = 0;
  let scroller: HTMLDivElement | undefined = $state();

  function closeRoom() {
    clearTimeout(readTimer);
    chatSeq++;
    token = null;
    messages = [];
    chatError = "";
    replyTo = null;
    draft = "";
  }

  function atBottom() {
    if (!scroller) return true;
    return scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;
  }

  function toBottom() {
    requestAnimationFrame(() => {
      if (scroller) scroller.scrollTop = scroller.scrollHeight;
    });
  }

  async function openRoom(next: string) {
    if (next === token) return;
    clearTimeout(readTimer);
    const seq = ++chatSeq;
    token = next;
    messages = [];
    chatError = "";
    replyTo = null;
    draft = "";
    loadingChat = true;
    try {
      const chunk = await api.talkMessages(service.id, next);
      if (seq !== chatSeq) return;
      messages = chunk.messages;
      first = chunk.first;
      last = chunk.last;
      more = chunk.more;
      toBottom();
      markRead();
      void listen(seq, next);
    } catch (err) {
      if (seq === chatSeq) chatError = errorMessage(err);
    } finally {
      if (seq === chatSeq) loadingChat = false;
    }
  }

  /** Espera longa por mensagens novas enquanto a conversa está aberta. */
  async function listen(seq: number, room: string) {
    let pause = 2000;
    while (seq === chatSeq && token === room) {
      try {
        const chunk = await api.talkWait(service.id, room, last);
        if (seq !== chatSeq) return;
        pause = 2000;
        last = Math.max(last, chunk.last);
        if (chunk.messages.length) {
          const stick = atBottom();
          const have = new Set(messages.map((m) => m.id));
          messages = [...messages, ...chunk.messages.filter((m) => !have.has(m.id))];
          if (stick) toBottom();
          markRead();
          void loadRooms(true);
        }
      } catch {
        if (seq !== chatSeq) return;
        await new Promise((r) => setTimeout(r, pause));
        pause = Math.min(pause * 2, 60_000);
      }
    }
  }

  /** Anteriores ao rolar até o topo (mantém a posição). */
  async function loadOlder() {
    if (!token || !more || olderBusy || !first) return;
    const seq = chatSeq;
    olderBusy = true;
    try {
      const chunk = await api.talkMessages(service.id, token, first);
      if (seq !== chatSeq) return;
      const height = scroller?.scrollHeight ?? 0;
      const have = new Set(messages.map((m) => m.id));
      messages = [...chunk.messages.filter((m) => !have.has(m.id)), ...messages];
      first = chunk.first || first;
      more = chunk.more;
      requestAnimationFrame(() => {
        if (scroller) scroller.scrollTop += scroller.scrollHeight - height;
      });
    } catch (err) {
      ontoast(errorMessage(err));
    } finally {
      olderBusy = false;
    }
  }

  function nearTop(node: HTMLElement) {
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) void loadOlder();
    }, { root: node.closest(".messages"), rootMargin: "300px 0px 0px 0px" });
    observer.observe(node);
    return { destroy: () => observer.disconnect() };
  }

  let readTimer: ReturnType<typeof setTimeout> | undefined;
  /** Marca como lida até a última vista (só com a janela em foco). */
  function markRead() {
    clearTimeout(readTimer);
    // Conversa e mensagem de agora: trocar de conversa cancela o pedido.
    const current = token;
    const upTo = last;
    if (!current || !upTo) return;
    readTimer = setTimeout(() => {
      if (current !== token || !document.hasFocus()) return;
      rooms = rooms.map((r) => (r.token === current ? { ...r, unread: 0, mention: false } : r));
      api.talkRead(service.id, current, upTo).catch(() => {});
    }, 600);
  }
  $effect(() => () => {
    clearTimeout(readTimer);
    chatSeq++;
  });

  // Clique num aviso: abre a conversa.
  let revealed = 0;
  $effect(() => {
    if (!reveal || reveal.n === revealed || !connected) return;
    revealed = reveal.n;
    void openRoom(reveal.token);
  });

  // ---- Escrever

  let draft = $state("");
  let replyTo = $state<TalkMessage | null>(null);
  let sending = $state(false);
  let uploading = $state(false);
  let field: HTMLTextAreaElement | undefined = $state();

  async function sendDraft() {
    if (!token || sending || !draft.trim()) return;
    const text = draft;
    const reply = replyTo;
    const room = token;
    sending = true;
    try {
      const sent = await api.talkSend(service.id, room, text, reply?.id);
      if (room !== token) return;
      draft = "";
      replyTo = null;
      if (sent && !messages.some((m) => m.id === sent.id)) {
        messages = [...messages, sent];
        last = Math.max(last, sent.id);
      }
      toBottom();
      void loadRooms(true);
    } catch (err) {
      ontoast(errorMessage(err));
    } finally {
      sending = false;
      field?.focus();
    }
  }

  async function attach() {
    if (!token || uploading) return;
    const picked = await pickFile({ multiple: false, directory: false }).catch(() => null);
    if (typeof picked !== "string") return;
    const room = token;
    uploading = true;
    try {
      await api.talkUpload(service.id, room, picked);
      ontoast(t("Arquivo enviado."));
    } catch (err) {
      ontoast(errorMessage(err));
    } finally {
      uploading = false;
    }
  }

  // Menções: "@algo" antes do cursor sugere participantes.
  let suggestions = $state<TalkMention[]>([]);
  let suggestAt = -1;
  let suggestIndex = $state(0);
  let mentionTimer: ReturnType<typeof setTimeout> | undefined;

  function typed() {
    autosize();
    clearTimeout(mentionTimer);
    if (!field || !token) return;
    const before = draft.slice(0, field.selectionStart ?? draft.length);
    const m = /(^|\s)@([^\s@]{0,30})$/.exec(before);
    if (!m) {
      suggestions = [];
      return;
    }
    suggestAt = before.length - m[2].length - 1;
    const search = m[2];
    const room = token;
    mentionTimer = setTimeout(() => {
      api
        .talkMentions(service.id, room, search)
        .then((list) => {
          if (room === token) {
            suggestions = list;
            suggestIndex = 0;
          }
        })
        .catch(() => (suggestions = []));
    }, 200);
  }

  function chooseMention(m: TalkMention) {
    if (!field || suggestAt < 0) return;
    const caret = field.selectionStart ?? draft.length;
    draft = `${draft.slice(0, suggestAt)}@${m.insert} ${draft.slice(caret)}`;
    suggestions = [];
    const at = suggestAt + m.insert.length + 2;
    requestAnimationFrame(() => {
      field?.focus();
      field?.setSelectionRange(at, at);
    });
  }

  function keydown(e: KeyboardEvent) {
    if (suggestions.length) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const n = suggestions.length;
        suggestIndex = (suggestIndex + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        chooseMention(suggestions[suggestIndex]);
        return;
      }
      if (e.key === "Escape") {
        suggestions = [];
        return;
      }
    }
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void sendDraft();
    } else if (e.key === "Escape" && replyTo) {
      replyTo = null;
    }
  }

  function autosize() {
    if (!field) return;
    field.style.height = "auto";
    field.style.height = `${Math.min(field.scrollHeight, 160)}px`;
  }

  function startReply(m: TalkMessage) {
    replyTo = m;
    field?.focus();
  }

  // ---- Reações

  let picking = $state<number | null>(null);

  async function react(m: TalkMessage, emoji: string) {
    if (!token) return;
    picking = null;
    const existing = m.reactions.find((r) => r.emoji === emoji);
    const on = !existing?.mine;
    const update = (list: TalkMessage[]) =>
      list.map((x) => {
        if (x.id !== m.id) return x;
        let reactions = x.reactions.map((r) =>
          r.emoji === emoji ? { ...r, mine: on, count: r.count + (on ? 1 : -1) } : r,
        );
        if (!existing && on) reactions = [...reactions, { emoji, count: 1, mine: true }];
        return { ...x, reactions: reactions.filter((r) => r.count > 0) };
      });
    const before = messages;
    messages = update(messages);
    try {
      await api.talkReact(service.id, token, m.id, emoji, on);
    } catch (err) {
      messages = before;
      ontoast(errorMessage(err));
    }
  }

  // ---- Arquivos

  let previews = $state<Record<string, string>>({});
  const previewAsked = new Set<string>();
  function previewOf(node: HTMLElement, f: TalkFile) {
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting) || previewAsked.has(f.id)) return;
      previewAsked.add(f.id);
      observer.disconnect();
      api
        .talkPreview(service.id, f.id)
        .then((data) => {
          if (data) previews = { ...previews, [f.id]: data };
        })
        .catch(() => {});
    });
    observer.observe(node);
    return { destroy: () => observer.disconnect() };
  }

  let fileBusy = $state<string | null>(null);
  async function openFile(f: TalkFile, open: boolean) {
    if (fileBusy) return;
    fileBusy = f.id;
    try {
      const path = await api.talkFile(service.id, f.path, f.name, open);
      if (path) {
        const name = path.split(/[\\/]/).pop() ?? path;
        ontoast(t("Anexo salvo: {nome}", { nome: name }), { label: t("Mostrar na pasta"), run: () => void api.mailRevealFile(path) });
      }
    } catch (err) {
      ontoast(errorMessage(err));
    } finally {
      fileBusy = null;
    }
  }

  function call() {
    if (!token || !service.talk) return;
    void api.openUrl(`${service.talk.server}/index.php/call/${token}`);
  }

  // ---- Formatação

  function time(secs: number) {
    return new Date(secs * 1000).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
  }

  function day(secs: number) {
    const d = new Date(secs * 1000);
    const now = new Date();
    if (d.toDateString() === now.toDateString()) return t("Hoje");
    const y = new Date(now);
    y.setDate(now.getDate() - 1);
    if (d.toDateString() === y.toDateString()) return t("Ontem");
    return d.toLocaleDateString(locale(), { day: "numeric", month: "long", year: d.getFullYear() === now.getFullYear() ? undefined : "numeric" });
  }

  function when(secs: number) {
    if (!secs) return "";
    const d = new Date(secs * 1000);
    const now = new Date();
    if (d.toDateString() === now.toDateString()) return time(secs);
    return d.toLocaleDateString(locale(), { day: "numeric", month: "short" });
  }

  function size(n: number) {
    if (!n) return "";
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  function pieces(text: string) {
    const out: { text: string; url?: string; mention?: boolean }[] = [];
    const re = /(https?:\/\/[^\s<>"')\]]+)|(@[^\s@]+(?: [A-ZÀ-Ý][^\s@]*)?)/g;
    let at = 0;
    for (const m of text.matchAll(re)) {
      if (m.index! > at) out.push({ text: text.slice(at, m.index) });
      if (m[1]) out.push({ text: m[1], url: m[1] });
      else out.push({ text: m[0], mention: true });
      at = m.index! + m[0].length;
    }
    if (at < text.length) out.push({ text: text.slice(at) });
    return out;
  }

  /** Mostra autor e data só quando mudam. */
  function head(i: number) {
    const m = messages[i];
    const prev = messages[i - 1];
    return {
      day: !prev || day(prev.time) !== day(m.time),
      author: !m.system && !m.own && (!prev || prev.system || prev.authorId !== m.authorId || m.time - prev.time > 300),
    };
  }
</script>

<div class="talk">
  {#if !service.talk || status === "noPassword" || status === "auth"}
    <div class="notice">
      <MessagesSquare size={28} />
      {#if status === "auth"}
        <p>{t("O Nextcloud recusou o acesso do Papeo (o acesso pode ter sido revogado). Conecte de novo.")}</p>
      {:else if service.talk}
        <p>{t("O acesso ao Talk não está salvo neste computador.")}</p>
      {:else}
        <p>
          <strong>{service.name}</strong>
          {t("pode abrir direto no leitor do Papeo: só as conversas, mais leve que o site.")}
        </p>
      {/if}
      {#if connecting}
        <p class="muted"><LoaderCircle size={14} class="spin" /> {t("Aprove o acesso do Papeo na página que abriu no navegador…")}</p>
        <div class="actions"><button class="btn" onclick={cancelConnect}>{t("Cancelar")}</button></div>
      {:else}
        <div class="actions">
          <button class="btn btn-primary" onclick={connect}>{t("Conectar ao Talk")}</button>
          <button class="btn" onclick={onsite}><Globe size={15} /> {t("Usar o site")}</button>
        </div>
        <p class="muted small">{t("O login acontece no navegador (com a verificação em duas etapas, se houver). O Papeo recebe uma senha de app que pode ser revogada nas configurações do Nextcloud.")}</p>
      {/if}
      {#if connectError}<p class="error" role="alert">{connectError}</p>{/if}
    </div>
  {:else}
    <section class="rooms" class:hide-narrow={!!token} aria-label={t("Conversas")}>
      <div class="rooms-bar">
        <div class="search">
          <Search size={14} />
          <input bind:value={filter} placeholder={t("Buscar conversa")} aria-label={t("Buscar conversa")} />
          {#if filter}<button class="icon-btn ghost" onclick={() => (filter = "")} aria-label={t("Limpar a busca")}><X size={13} /></button>{/if}
        </div>
        <button class="icon-btn ghost" onclick={() => loadRooms()} aria-label={t("Atualizar")} title={t("Atualizar")}>
          <RefreshCw size={15} class={loadingRooms ? "spin" : ""} />
        </button>
      </div>
      <ul class="room-list">
        {#if roomsError}
          <li class="empty error" role="alert">{roomsError} <button class="btn" onclick={() => loadRooms()}>{t("Tentar de novo")}</button></li>
        {:else if !shown.length && !loadingRooms}
          <li class="empty">{filter ? t("Nada encontrado.") : t("Nenhuma conversa.")}</li>
        {/if}
        {#each shown as r (r.token)}
          <li>
            <button class="room" class:on={r.token === token} class:unread={r.unread > 0} onclick={() => openRoom(r.token)}>
              <span class="avatar" use:avatarOf={r}>
                {#if avatars[r.token]}<img src={avatars[r.token]} alt="" />{:else}{initials(r.name)}{/if}
              </span>
              <span class="room-text">
                <span class="line">
                  <span class="name">{r.name}</span>
                  {#if r.favorite}<Star size={11} class="fav" />{/if}
                  <span class="date">{when(r.activity)}</span>
                </span>
                <span class="line">
                  <span class="last">
                    {#if r.last}{r.last.own ? t("Você: ") : r.kind !== 1 && r.last.author ? `${r.last.author}: ` : ""}{r.last.text}{/if}
                  </span>
                  {#if r.mention}<span class="badge mention" title={t("Mencionaram você")}><AtSign size={11} /></span>{/if}
                  {#if r.unread}<span class="badge">{r.unread > 99 ? "99+" : r.unread}</span>{/if}
                </span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
      <div class="foot">
        {#if status === "offline"}
          <p class="status"><CircleAlert size={13} /> {t("Sem conexão")}</p>
        {/if}
        <button class="link-btn" onclick={onsite}><Globe size={13} /> {t("Abrir no site")}</button>
        <button class="link-btn" onclick={disconnect} title={t("Revoga o acesso do Papeo no Nextcloud")}><LogOut size={13} /> {t("Desconectar")}</button>
      </div>
    </section>

    <section class="chat" class:hide-narrow={!token} aria-label={t("Conversa")}>
      {#if !token}
        <div class="placeholder"><MessagesSquare size={28} /><span>{t("Escolha uma conversa.")}</span></div>
      {:else}
        <header class="chat-head">
          <button class="icon-btn ghost back" onclick={closeRoom} aria-label={t("Voltar às conversas")}><ArrowLeft size={17} /></button>
          <span class="avatar small">
            {#if avatars[token]}<img src={avatars[token]} alt="" />{:else}{initials(room?.name ?? "")}{/if}
          </span>
          <h2>{room?.name ?? ""}</h2>
          {#if room?.hasCall}<span class="live">{t("Chamada em andamento")}</span>{/if}
          <button class="icon-btn" onclick={call} title={t("Chamada (abre no navegador)")} aria-label={t("Chamada (abre no navegador)")}><Phone size={16} /></button>
        </header>
        <div class="messages" bind:this={scroller}>
          {#if loadingChat}
            <div class="placeholder"><LoaderCircle size={20} class="spin" /></div>
          {:else if chatError}
            <div class="placeholder error" role="alert">{chatError}</div>
          {:else}
            {#if more}
              <div class="older" use:nearTop>{#if olderBusy}<LoaderCircle size={15} class="spin" />{/if}</div>
            {/if}
            {#each messages as m, i (m.id)}
              {@const h = head(i)}
              {#if h.day}<div class="day"><span>{day(m.time)}</span></div>{/if}
              {#if m.system}
                <div class="system">{m.text} · {time(m.time)}</div>
              {:else}
                <div class="msg" class:own={m.own} class:first={h.author}>
                  {#if h.author}<span class="author">{m.author}</span>{/if}
                  <div class="bubble">
                    {#if m.parent}
                      <div class="quote"><strong>{m.parent.author}</strong> {m.parent.text}</div>
                    {/if}
                    {#each m.files as f (f.id)}
                      {#if f.preview && f.mime.startsWith("image/")}
                        <button class="image" use:previewOf={f} onclick={() => openFile(f, true)} title={f.name}>
                          {#if previews[f.id]}<img src={previews[f.id]} alt={f.name} />{:else}<span class="image-wait"><LoaderCircle size={16} class="spin" /></span>{/if}
                        </button>
                      {:else}
                        <span class="file" class:busy={fileBusy === f.id}>
                          <button class="file-open" onclick={() => openFile(f, true)} title={t("Abrir {nome}", { nome: f.name })}>
                            <FileText size={15} /> <span class="file-name">{f.name}</span> <span class="file-size">{size(f.size)}</span>
                          </button>
                          <button class="file-save" onclick={() => openFile(f, false)} aria-label={t("Salvar {nome} em Downloads", { nome: f.name })} title={t("Salvar em Downloads")}><Download size={14} /></button>
                        </span>
                      {/if}
                    {/each}
                    {#if !(m.files.length && m.files.some((f) => f.name === m.text.trim()))}
                      <span class="text">{#each pieces(m.text) as p, k (k)}{#if p.url}<a href={p.url} onclick={(e) => { e.preventDefault(); void api.openUrl(p.url!); }}>{p.text}</a>{:else if p.mention}<span class="mention">{p.text}</span>{:else}{p.text}{/if}{/each}</span>
                    {/if}
                    <span class="time">{time(m.time)}</span>
                  </div>
                  {#if m.reactions.length}
                    <div class="reactions">
                      {#each m.reactions as r (r.emoji)}
                        <button class="reaction" class:mine={r.mine} onclick={() => react(m, r.emoji)}>{r.emoji} {r.count}</button>
                      {/each}
                    </div>
                  {/if}
                  <div class="actions" class:open={picking === m.id}>
                    {#if picking === m.id}
                      {#each QUICK as e (e)}<button class="emoji" onclick={() => react(m, e)}>{e}</button>{/each}
                    {:else}
                      <button class="icon-btn ghost" onclick={() => (picking = m.id)} aria-label={t("Reagir")} title={t("Reagir")}><SmilePlus size={14} /></button>
                      {#if m.replyable && !room?.readOnly}
                        <button class="icon-btn ghost" onclick={() => startReply(m)} aria-label={t("Responder")} title={t("Responder")}><Reply size={14} /></button>
                      {/if}
                    {/if}
                  </div>
                </div>
              {/if}
            {/each}
          {/if}
        </div>
        {#if room?.readOnly}
          <p class="readonly">{t("Conversa só para leitura.")}</p>
        {:else}
          <div class="composer">
            {#if replyTo}
              <div class="replying">
                <Reply size={13} />
                <span><strong>{replyTo.author}</strong> {replyTo.text}</span>
                <button class="icon-btn ghost" onclick={() => (replyTo = null)} aria-label={t("Cancelar a resposta")}><X size={13} /></button>
              </div>
            {/if}
            {#if suggestions.length}
              <ul class="mentions" role="listbox" aria-label={t("Mencionar")}>
                {#each suggestions as s, k (s.insert)}
                  <li>
                    <button role="option" aria-selected={k === suggestIndex} class:on={k === suggestIndex} onmousedown={(e) => { e.preventDefault(); chooseMention(s); }}>
                      <AtSign size={12} /> {s.label}
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
            <div class="write">
              <button class="icon-btn ghost" onclick={attach} disabled={uploading} aria-label={t("Enviar arquivo")} title={t("Enviar arquivo")}>
                {#if uploading}<LoaderCircle size={16} class="spin" />{:else}<Paperclip size={16} />{/if}
              </button>
              <textarea
                bind:this={field}
                bind:value={draft}
                rows="1"
                placeholder={t("Escreva uma mensagem")}
                aria-label={t("Mensagem")}
                oninput={typed}
                onkeydown={keydown}
                onblur={() => setTimeout(() => (suggestions = []), 150)}
              ></textarea>
              <button class="icon-btn send" onclick={sendDraft} disabled={sending || !draft.trim()} aria-label={t("Enviar")} title={t("Enviar")}>
                {#if sending}<LoaderCircle size={16} class="spin" />{:else}<Send size={16} />{/if}
              </button>
            </div>
          </div>
        {/if}
      {/if}
    </section>
  {/if}
</div>

<style>
  .talk {
    position: absolute;
    inset: 0;
    display: grid;
    grid-template-columns: minmax(240px, 320px) 1fr;
    background: var(--bg);
    color: var(--text);
    min-height: 0;
  }
  .notice {
    grid-column: 1 / -1;
    align-self: center;
    justify-self: center;
    max-width: 440px;
    padding: 0 16px;
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
  .muted {
    color: var(--text-muted);
  }
  .small {
    font-size: 12px;
  }
  .error {
    color: var(--danger);
  }
  .rooms {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
    background: var(--bg-subtle, var(--bg));
  }
  .rooms-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 10px;
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: var(--text-muted);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    padding: 6px 0;
    background: transparent;
    font: inherit;
    color: var(--text);
    outline: none;
  }
  .room-list {
    flex: 1;
    overflow-y: auto;
    margin: 0;
    padding: 0 6px;
    list-style: none;
  }
  .empty {
    padding: 24px 12px;
    text-align: center;
    color: var(--text-muted);
  }
  .room {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    font: inherit;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .room:hover {
    background: var(--bg-hover);
  }
  .room.on {
    background: var(--bg-active, var(--bg-hover));
  }
  .avatar {
    flex-shrink: 0;
    width: 38px;
    height: 38px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    overflow: hidden;
    background: var(--bg-hover);
    font-size: 13px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .avatar.small {
    width: 28px;
    height: 28px;
    font-size: 11px;
  }
  .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .room-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .name,
  .last {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    font-weight: 500;
  }
  .room.unread .name {
    font-weight: 700;
  }
  .last {
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .room.unread .last {
    color: var(--text);
  }
  .date {
    flex-shrink: 0;
    font-size: 11.5px;
    color: var(--text-muted);
  }
  .room :global(.fav) {
    flex-shrink: 0;
    color: var(--text-muted);
  }
  .badge {
    flex-shrink: 0;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 9px;
    display: inline-grid;
    place-items: center;
    background: var(--primary);
    color: var(--primary-text);
    font-size: 11px;
    font-weight: 600;
  }
  .badge.mention {
    padding: 0;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
  }
  .status {
    width: 100%;
    margin: 0;
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--danger);
  }
  .chat {
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
    color: var(--text-muted);
  }
  .chat-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
  }
  .chat-head h2 {
    flex: 1;
    min-width: 0;
    margin: 0;
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .live {
    font-size: 12px;
    color: var(--primary);
  }
  .back {
    display: none;
  }
  .messages {
    flex: 1;
    overflow-y: auto;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .older {
    min-height: 24px;
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }
  .day {
    align-self: center;
    margin: 10px 0 6px;
    font-size: 11.5px;
    color: var(--text-muted);
  }
  .day span {
    padding: 2px 10px;
    border-radius: 10px;
    background: var(--bg-hover);
  }
  .system {
    align-self: center;
    max-width: 80%;
    margin: 4px 0;
    font-size: 12px;
    color: var(--text-muted);
    text-align: center;
  }
  .msg {
    position: relative;
    align-self: flex-start;
    max-width: min(70ch, 78%);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
  }
  .msg.first {
    margin-top: 8px;
  }
  .msg.own {
    align-self: flex-end;
    align-items: flex-end;
  }
  .author {
    margin: 0 4px 2px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .bubble {
    position: relative;
    padding: 6px 10px 6px;
    border-radius: 14px;
    background: var(--bg-hover);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .msg.own .bubble {
    background: var(--primary);
    color: var(--primary-text);
  }
  .msg.own .bubble a,
  .msg.own .mention {
    color: inherit;
  }
  .text a {
    color: var(--accent);
  }
  .mention {
    font-weight: 600;
    color: var(--accent);
  }
  .time {
    align-self: flex-end;
    font-size: 10.5px;
    opacity: 0.65;
  }
  .quote {
    padding: 4px 8px;
    border-left: 3px solid currentColor;
    border-radius: 4px;
    font-size: 12.5px;
    opacity: 0.8;
    white-space: normal;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
  }
  .image {
    padding: 0;
    border: 0;
    background: none;
    cursor: zoom-in;
    border-radius: 10px;
    overflow: hidden;
  }
  .image img {
    display: block;
    max-width: min(320px, 100%);
    max-height: 320px;
  }
  .image-wait {
    width: 200px;
    height: 140px;
    display: grid;
    place-items: center;
    background: var(--bg);
    opacity: 0.6;
  }
  .file {
    display: inline-flex;
    align-items: center;
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    overflow: hidden;
  }
  .file.busy {
    opacity: 0.6;
  }
  .file button {
    border: 0;
    background: transparent;
    font: inherit;
    color: inherit;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
  }
  .file button:hover {
    background: var(--bg-hover);
  }
  .file-name {
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file-size {
    font-size: 11.5px;
    color: var(--text-muted);
  }
  .reactions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 3px;
  }
  .reaction {
    padding: 1px 7px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    font: inherit;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
  }
  .reaction.mine {
    border-color: var(--primary);
  }
  .actions {
    position: absolute;
    top: -6px;
    right: -8px;
    display: none;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    box-shadow: var(--shadow-pop);
    z-index: 1;
  }
  .msg.own .actions {
    right: auto;
    left: -8px;
  }
  .msg:hover .actions,
  .msg:focus-within .actions,
  .actions.open {
    display: flex;
  }
  .emoji {
    border: 0;
    background: transparent;
    font-size: 17px;
    padding: 2px 4px;
    cursor: pointer;
    border-radius: 6px;
  }
  .emoji:hover {
    background: var(--bg-hover);
  }
  .readonly {
    margin: 0;
    padding: 12px;
    text-align: center;
    color: var(--text-muted);
    border-top: 1px solid var(--border);
  }
  .composer {
    position: relative;
    border-top: 1px solid var(--border);
    padding: 8px 12px;
  }
  .replying {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
    padding: 4px 8px;
    border-left: 3px solid var(--primary);
    border-radius: 4px;
    background: var(--bg-hover);
    font-size: 12.5px;
  }
  .replying span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mentions {
    position: absolute;
    bottom: 100%;
    left: 52px;
    margin: 0 0 4px;
    padding: 4px;
    list-style: none;
    min-width: 200px;
    max-height: 240px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    box-shadow: var(--shadow-pop);
  }
  .mentions button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    font: inherit;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .mentions button.on,
  .mentions button:hover {
    background: var(--bg-hover);
  }
  .write {
    display: flex;
    align-items: flex-end;
    gap: 6px;
  }
  .write textarea {
    flex: 1;
    min-height: 36px;
    max-height: 160px;
    resize: none;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    font: inherit;
    color: var(--text);
  }
  .send:not(:disabled) {
    background: var(--primary);
    color: var(--primary-text);
  }
  .talk :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .talk :global(.spin) {
      animation: none;
    }
  }
  @media (max-width: 700px) {
    .talk {
      grid-template-columns: 1fr;
    }
    .hide-narrow {
      display: none;
    }
    .back {
      display: inline-flex;
    }
  }
</style>
