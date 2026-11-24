<script lang="ts">
  import { onMount, tick } from "svelte";
  import { BellOff, Bell, ExternalLink, Lock, LockOpen, SendHorizontal } from "@lucide/svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import {
    api,
    EMPTY_SNAPSHOT,
    MAX_REPLY_CHARS,
    modKey,
    onState,
    type RecentChat,
    type Snapshot,
  } from "./api";

  // Mini janela da barra de menus (macOS) / bandeja (Windows): conversas
  // recentes com resposta rápida. Some ao perder o foco (backend).

  let snap = $state<Snapshot>(EMPTY_SNAPSHOT);
  let open = $state<string | null>(null);
  let drafts = $state<Record<string, string>>({});
  let sending = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string>>({});
  let sent = $state<string | null>(null);
  let sentTimer: ReturnType<typeof setTimeout> | undefined;
  let input = $state<HTMLTextAreaElement>();
  let thread = $state<HTMLElement>();

  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  let systemIsDark = $state(systemDark.matches);
  const theme = $derived(
    snap.settings.appearance === "system"
      ? systemIsDark
        ? "dark"
        : "light"
      : snap.settings.appearance,
  );
  $effect(() => {
    document.documentElement.dataset.theme = theme;
  });

  const byId = $derived(new Map(snap.services.map((s) => [s.id, s])));
  const chats = $derived(snap.recent.filter((c) => byId.has(c.serviceId)));
  const withUnread = $derived(snap.services.filter((s) => s.unread > 0));
  const hidden = $derived(snap.settings.notificationPreview === "hidden");
  const keyOf = (c: RecentChat) => `${c.serviceId}\u0000${c.chat}`;

  onMount(() => {
    api.getState().then((s) => {
      snap = s;
      // Abre já a conversa mais recente que dá para responder.
      const first = s.recent.find((c) => c.canReply);
      open ??= first ? keyOf(first) : null;
      tick().then(() => input?.focus());
    });
    const off = onState((s) => (snap = s));
    const onScheme = (e: MediaQueryListEvent) => (systemIsDark = e.matches);
    systemDark.addEventListener("change", onScheme);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") api.hidePopover().catch(() => {});
    };
    window.addEventListener("keydown", onKey);
    // Reaberta pela barra: foca a resposta da conversa aberta.
    const onFocus = () => input?.focus();
    window.addEventListener("focus", onFocus);
    return () => {
      off.then((f) => f());
      systemDark.removeEventListener("change", onScheme);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("focus", onFocus);
    };
  });

  // A conversa aberta sumiu (prévia mudou, conta removida): fecha.
  $effect(() => {
    if (open && !chats.some((c) => keyOf(c) === open)) open = null;
  });

  // Mostra sempre a mensagem mais nova.
  $effect(() => {
    void chats;
    void open;
    tick().then(() => thread?.scrollTo({ top: thread.scrollHeight }));
  });

  function toggle(c: RecentChat) {
    const key = keyOf(c);
    open = open === key ? null : key;
    if (open) tick().then(() => input?.focus());
  }

  async function send(c: RecentChat) {
    const key = keyOf(c);
    const text = (drafts[key] ?? "").trim();
    if (!text || sending[key]) return;
    sending[key] = true;
    errors[key] = "";
    try {
      snap = await api.sendReply(c.serviceId, c.chat, text);
      drafts[key] = "";
      sent = key;
      clearTimeout(sentTimer);
      sentTimer = setTimeout(() => (sent = null), 2500);
    } catch (e) {
      errors[key] = String(e);
    } finally {
      sending[key] = false;
      // Mesmo depois de enviar, o próximo texto já pode ser digitado.
      tick().then(() => input?.focus());
    }
  }

  function onInputKey(e: KeyboardEvent, c: RecentChat) {
    // Enter envia; Shift+Enter quebra a linha.
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send(c);
    }
  }

  function grow(el: HTMLTextAreaElement) {
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 120)}px`;
  }

  const go = (id: string | null = null, chat: string | null = null) =>
    api.showApp(id, chat).catch(() => {});

  async function toggleDnd() {
    snap = await api.setDnd(snap.dndActive ? null : 60).catch(() => snap);
  }

  async function lockNow() {
    snap = await api.lockNow().catch(() => snap);
  }

  function ago(at: number): string {
    const s = Math.max(0, Date.now() / 1000 - at);
    if (s < 60) return "agora";
    if (s < 3600) return `${Math.floor(s / 60)} min`;
    const d = new Date(at * 1000);
    const today = new Date().toDateString() === d.toDateString();
    return today
      ? d.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })
      : d.toLocaleDateString("pt-BR", { day: "2-digit", month: "2-digit" });
  }

  // Tempo relativo atualizado sem depender de mensagens novas.
  let now = $state(Date.now());
  onMount(() => {
    const t = setInterval(() => (now = Date.now()), 30000);
    return () => clearInterval(t);
  });
</script>

<div class="pop">
  <header>
    <div class="brand">
      <img src="/favicon.svg" alt="" width="20" height="20" />
      <strong>Papeo</strong>
      {#if snap.totalUnread > 0 && !snap.locked}
        <span class="count" aria-label="{snap.totalUnread} não lidas">{snap.totalUnread}</span>
      {/if}
    </div>
    <button class="icon" title="Abrir o Papeo" aria-label="Abrir o Papeo" onclick={() => go()}>
      <ExternalLink size={16} />
    </button>
  </header>

  {#if snap.dndActive && !snap.locked}
    <p class="banner">Não perturbe ligado: as notificações estão em silêncio.</p>
  {/if}

  <main>
    {#if snap.locked}
      <div class="empty">
        <Lock size={28} strokeWidth={1.8} />
        <p><strong>O Papeo está bloqueado</strong></p>
        <p class="muted">As conversas aparecem depois de desbloquear.</p>
        <button class="btn btn-primary" onclick={() => go()}>
          <LockOpen size={15} /> Desbloquear
        </button>
      </div>
    {:else if chats.length === 0}
      <div class="empty">
        <p><strong>{hidden ? "Prévia desligada" : "Nenhuma mensagem nova"}</strong></p>
        <p class="muted">
          {hidden
            ? "Em Configurações → Notificações, escolha mostrar a mensagem para ler e responder por aqui."
            : "As conversas que chegarem como notificação aparecem aqui para ler e responder."}
        </p>
      </div>
    {:else}
      <ul class="chats" data-now={now}>
        {#each chats as c (keyOf(c))}
          {@const service = byId.get(c.serviceId)!}
          {@const key = keyOf(c)}
          {@const expanded = open === key}
          {@const last = c.messages[c.messages.length - 1]}
          <li class:expanded>
            <button class="row" aria-expanded={expanded} onclick={() => toggle(c)}>
              <ServiceAvatar {service} size={30} />
              <span class="text">
                <span class="line">
                  <span class="title">{c.title}</span>
                  <span class="time">{ago(c.at)}</span>
                </span>
                <span class="preview">
                  {#if last?.mine}
                    <span class="you">Você:</span> {last.body}
                  {:else}
                    {c.body || service.name}
                  {/if}
                </span>
              </span>
            </button>

            {#if expanded}
              <div class="detail">
                {#if c.messages.length}
                  <div class="thread" bind:this={thread}>
                    {#each c.messages as m, i (i)}
                      <p class="bubble" class:mine={m.mine}>
                        {m.body}<time>{ago(m.at)}</time>
                      </p>
                    {/each}
                  </div>
                {/if}
                <p class="account">{service.name}</p>

                {#if c.canReply}
                  <form
                    class="reply"
                    onsubmit={(e) => {
                      e.preventDefault();
                      send(c);
                    }}
                  >
                    <textarea
                      bind:this={input}
                      bind:value={drafts[key]}
                      oninput={(e) => grow(e.currentTarget)}
                      onkeydown={(e) => onInputKey(e, c)}
                      rows="1"
                      maxlength={MAX_REPLY_CHARS}
                      placeholder="Responder a {c.title}…"
                      aria-label="Responder a {c.title}"
                      disabled={sending[key]}
                      autocomplete="off"
                      spellcheck="true"
                    ></textarea>
                    <button
                      class="send"
                      type="submit"
                      title="Enviar (Enter)"
                      aria-label="Enviar"
                      disabled={sending[key] || !(drafts[key] ?? "").trim()}
                    >
                      {#if sending[key]}
                        <span class="spinner" aria-hidden="true"></span>
                      {:else}
                        <SendHorizontal size={16} />
                      {/if}
                    </button>
                  </form>
                {/if}
                <div class="status" aria-live="polite">
                  {#if sending[key]}
                    <span class="muted">Enviando pelo {service.name}…</span>
                  {:else if errors[key]}
                    <span class="error">{errors[key]}</span>
                  {:else if sent === key}
                    <span class="ok">Enviado ✓</span>
                  {:else if c.canReply}
                    <span class="muted">Enter envia · Shift+Enter quebra a linha</span>
                  {/if}
                  <button class="link" onclick={() => go(c.serviceId, c.chat)}>Abrir conversa</button>
                </div>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    {#if !snap.locked && withUnread.length}
      <section class="accounts" aria-label="Contas com não lidas">
        {#each withUnread as s (s.id)}
          <button class="chip" onclick={() => go(s.id)} title="Abrir {s.name}">
            <ServiceAvatar service={s} size={16} />
            {s.name}
            <span class="n">{s.unread}</span>
          </button>
        {/each}
      </section>
    {/if}
  </main>

  {#if !snap.locked}
    <footer>
      <button class="foot" onclick={toggleDnd} aria-pressed={snap.dndActive}>
        {#if snap.dndActive}
          <Bell size={15} /> Reativar avisos
        {:else}
          <BellOff size={15} /> Silenciar 1 h
        {/if}
      </button>
      {#if snap.lockAvailable && snap.settings.lock.enabled}
        <button class="foot" onclick={lockNow} title="Bloquear agora ({modKey} L)">
          <Lock size={15} /> Bloquear
        </button>
      {/if}
      <button class="foot right" onclick={() => go()}>Abrir Papeo</button>
    </footer>
  {/if}
</div>

<style>
  .pop {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 12px 10px 16px;
    border-bottom: 1px solid var(--border);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
  }

  .count {
    min-width: 20px;
    height: 20px;
    padding: 0 6px;
    border-radius: 10px;
    background: var(--danger);
    color: #fff;
    font-size: 12px;
    font-weight: 650;
    display: inline-grid;
    place-items: center;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
  }

  .icon:hover,
  .foot:hover,
  .row:hover {
    background: var(--bg-hover);
  }

  .banner {
    margin: 0;
    padding: 7px 16px;
    background: var(--warn-bg);
    color: var(--warn-text);
    border-bottom: 1px solid var(--warn-border);
    font-size: 12.5px;
  }

  main {
    flex: 1;
    overflow-y: auto;
    padding: 6px 0;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 2px;
    padding: 48px 32px;
    color: var(--text-muted);
  }

  .empty p {
    margin: 0;
  }

  .empty strong {
    color: var(--text);
  }

  .empty .btn {
    margin-top: 14px;
  }

  .muted {
    color: var(--text-muted);
  }

  .chats {
    list-style: none;
    margin: 0;
    padding: 0 6px;
  }

  li {
    border-radius: 12px;
  }

  li.expanded {
    background: var(--bg-subtle);
    margin: 4px 0;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 9px 10px;
    border: none;
    border-radius: 12px;
    background: transparent;
    text-align: left;
  }

  li.expanded .row:hover {
    background: transparent;
  }

  .text {
    flex: 1;
    min-width: 0;
  }

  .line {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }

  .title {
    flex: 1;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .time {
    flex: none;
    font-size: 11.5px;
    color: var(--text-faint);
  }

  .preview {
    display: block;
    color: var(--text-muted);
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  li.expanded .preview {
    display: none;
  }

  .you {
    color: var(--text-faint);
  }

  .detail {
    padding: 0 10px 10px;
  }

  .thread {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 220px;
    overflow-y: auto;
    padding: 2px 0 6px;
    user-select: text;
    -webkit-user-select: text;
    cursor: text;
  }

  .bubble {
    align-self: flex-start;
    max-width: 85%;
    margin: 0;
    padding: 6px 10px;
    border-radius: 12px 12px 12px 4px;
    background: var(--bg);
    border: 1px solid var(--border);
    font-size: 13.5px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .bubble.mine {
    align-self: flex-end;
    border-radius: 12px 12px 4px 12px;
    background: var(--primary);
    color: var(--primary-text);
    border-color: transparent;
  }

  .bubble time {
    margin-left: 8px;
    font-size: 10.5px;
    opacity: 0.6;
  }

  .account {
    margin: 2px 0 6px;
    font-size: 11.5px;
    color: var(--text-faint);
  }

  .reply {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    padding: 4px 4px 4px 10px;
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    background: var(--bg);
  }

  .reply:focus-within {
    border-color: var(--accent);
  }

  textarea {
    flex: 1;
    min-height: 28px;
    max-height: 120px;
    padding: 5px 0;
    border: none;
    outline: none;
    resize: none;
    background: transparent;
    font: inherit;
    font-size: 13.5px;
    color: var(--text);
    user-select: text;
    -webkit-user-select: text;
  }

  .send {
    display: grid;
    place-items: center;
    flex: none;
    width: 30px;
    height: 30px;
    border: none;
    border-radius: 9px;
    background: var(--primary);
    color: var(--primary-text);
  }

  .send:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation-duration: 2.4s;
    }
  }

  .status {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 10px;
    margin-top: 6px;
    font-size: 12px;
  }

  .error {
    color: var(--danger);
  }

  .ok {
    color: var(--success-text);
  }

  .link {
    flex: none;
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font-size: 12px;
  }

  .link:hover {
    text-decoration: underline;
  }

  .accounts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 10px 14px 4px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 8px 0 6px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--bg);
    font-size: 12.5px;
  }

  .chip:hover {
    background: var(--bg-hover);
  }

  .chip .n {
    font-weight: 650;
    color: var(--danger);
  }

  footer {
    display: flex;
    gap: 2px;
    padding: 6px 8px;
    border-top: 1px solid var(--border);
  }

  .foot {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
    font-size: 12.5px;
  }

  .foot.right {
    margin-left: auto;
    color: var(--text);
    font-weight: 550;
  }
</style>
