<script lang="ts">
  import { onMount, tick } from "svelte";
  import { SendHorizontal } from "@lucide/svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import { api, MAX_REPLY_CHARS, type RecentChat, type ServiceView, type Snapshot } from "./api";

  // Conversas recentes com resposta rápida: na mini janela e no Início.
  let {
    chats,
    services,
    onsnap,
    onopen,
    autoOpen = false,
  }: {
    chats: RecentChat[];
    services: ServiceView[];
    /** Estado novo depois de enviar. */
    onsnap: (snap: Snapshot) => void;
    /** "Abrir conversa" no app. */
    onopen: (serviceId: string, chat: string) => void;
    /** Já abre a conversa mais recente que dá para responder. */
    autoOpen?: boolean;
  } = $props();

  let open = $state<string | null>(null);
  let drafts = $state<Record<string, string>>({});
  let sending = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string>>({});
  let sent = $state<string | null>(null);
  let sentTimer: ReturnType<typeof setTimeout> | undefined;
  let input = $state<HTMLTextAreaElement>();
  let thread = $state<HTMLElement>();

  const byId = $derived(new Map(services.map((s) => [s.id, s])));
  const visible = $derived(chats.filter((c) => byId.has(c.serviceId)));
  const keyOf = (c: RecentChat) => `${c.serviceId}\u0000${c.chat}`;

  /** Foca a resposta da conversa aberta (mini janela reaberta). */
  export function focus() {
    input?.focus();
  }

  let opened = false;
  $effect(() => {
    if (!autoOpen || opened || visible.length === 0) return;
    opened = true;
    const first = visible.find((c) => c.canReply);
    if (first) {
      open = keyOf(first);
      tick().then(() => input?.focus());
    }
  });

  // A conversa aberta sumiu (prévia mudou, conta removida): fecha.
  $effect(() => {
    if (open && !visible.some((c) => keyOf(c) === open)) open = null;
  });

  // Mostra sempre a mensagem mais nova.
  $effect(() => {
    void visible;
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
      onsnap(await api.sendReply(c.serviceId, c.chat, text));
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

<ul class="chats" data-now={now}>
  {#each visible as c (keyOf(c))}
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
            <button class="link" onclick={() => onopen(c.serviceId, c.chat)}>Abrir conversa</button>
          </div>
        </div>
      {/if}
    </li>
  {/each}
</ul>

<style>
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

  .row:hover {
    background: var(--bg-hover);
  }
</style>
