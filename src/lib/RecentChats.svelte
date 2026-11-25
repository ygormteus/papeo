<script lang="ts">
  import { onMount, tick } from "svelte";
  import { AlarmClock, SendHorizontal } from "@lucide/svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import TaskForm from "./TaskForm.svelte";
  import { matchQuickReplies } from "./quickReplies";
  import {
    api,
    MAX_REPLY_CHARS,
    type QuickReply,
    type RecentChat,
    type ServiceView,
    type Snapshot,
    type TaskFields,
  } from "./api";

  // Conversas recentes com resposta rápida: na mini janela e no Início.
  let {
    chats,
    services,
    onsnap,
    onopen,
    autoOpen = false,
    oncreatetask,
    clock,
    quickReplies = [],
  }: {
    chats: RecentChat[];
    services: ServiceView[];
    /** Estado novo depois de enviar. */
    onsnap: (snap: Snapshot) => void;
    /** "Abrir conversa" no app. */
    onopen: (serviceId: string, chat: string) => void;
    /** Já abre a conversa mais recente que dá para responder. */
    autoOpen?: boolean;
    /** "Lembrar disso": cria uma tarefa ligada à conversa (só no Início). */
    oncreatetask?: (fields: TaskFields) => Promise<void>;
    /** Relógio do app, para o formulário de tarefa. */
    clock?: Date;
    /** Respostas prontas, chamadas com "/" na resposta. */
    quickReplies?: QuickReply[];
  } = $props();



  /** Conversa com o formulário "Lembrar disso" aberto. */
  let reminding = $state<string | null>(null);
  /** Última mensagem recebida, para começar o texto da tarefa. */
  const lastIncoming = (c: RecentChat) => {
    const body = [...c.messages].reverse().find((m) => !m.mine)?.body ?? c.body;
    return body.length > 140 ? `${body.slice(0, 139)}…` : body;
  };

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

  // Respostas prontas: lista sob a resposta enquanto o texto começa com "/".
  let pick = $state(0);
  let dismissed = $state<string | null>(null);
  const matches = $derived(
    open && dismissed !== (drafts[open] ?? "") ? matchQuickReplies(quickReplies, drafts[open] ?? "") : [],
  );
  // A prop chega nova a cada estado publicado: só volta ao topo quando a
  // lista de sugestões muda de fato.
  const matchKey = $derived(matches.map((r) => r.shortcut).join("\u0000"));
  $effect(() => {
    void matchKey;
    pick = 0;
  });

  function useReply(key: string, reply: QuickReply) {
    drafts[key] = reply.text;
    tick().then(() => {
      if (input) {
        grow(input);
        input.focus();
        input.setSelectionRange(input.value.length, input.value.length);
      }
    });
  }
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

  // Mostra sempre a mensagem mais nova, só quando a conversa aberta muda ou
  // recebe mensagem (não a cada estado publicado, para não tirar a leitura).
  const openTail = $derived.by(() => {
    const c = open ? visible.find((v) => keyOf(v) === open) : undefined;
    const last = c?.messages[c.messages.length - 1];
    return c ? `${c.messages.length}:${last?.at ?? c.at}:${last?.body.length ?? 0}` : "";
  });
  $effect(() => {
    void openTail;
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
    const key = keyOf(c);
    if (matches.length && !e.isComposing) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const step = e.key === "ArrowDown" ? 1 : -1;
        pick = (pick + step + matches.length) % matches.length;
        return;
      }
      if ((e.key === "Enter" && !e.shiftKey) || e.key === "Tab") {
        e.preventDefault();
        useReply(key, matches[pick]);
        return;
      }
      if (e.key === "Escape") {
        // Fecha só a lista (não a mini janela).
        e.preventDefault();
        e.stopPropagation();
        dismissed = drafts[key] ?? "";
        return;
      }
    }
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

  function ago(at: number, nowMs: number): string {
    const s = Math.max(0, nowMs / 1000 - at);
    if (s < 60) return "agora";
    if (s < 3600) return `${Math.floor(s / 60)} min`;
    const d = new Date(at * 1000);
    const today = new Date(nowMs).toDateString() === d.toDateString();
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

<ul class="chats">
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
            <span class="time">{ago(c.at, now)}</span>
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
                  {m.body}<time>{ago(m.at, now)}</time>
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
                aria-autocomplete="list"
                aria-controls="quick-replies"
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
          {#if c.canReply && matches.length}
            <ul class="quick" id="quick-replies" role="listbox" aria-label="Respostas prontas">
              {#each matches as r, i (i)}
                <li role="option" aria-selected={i === pick}>
                  <button
                    type="button"
                    class:picked={i === pick}
                    onmousedown={(e) => e.preventDefault()}
                    onclick={() => useReply(key, r)}
                  >
                    {#if r.shortcut}<span class="sc">/{r.shortcut}</span>{/if}
                    <span class="qt">{r.text}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {:else if c.canReply && quickReplies.length === 0 && (drafts[key] ?? "") === "/"}
            <p class="quick-empty">Nenhuma resposta pronta ainda. Crie em Configurações → Respostas prontas.</p>
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
            <span class="links">
              {#if oncreatetask && clock && c.chat}
                <button class="link" aria-expanded={reminding === key} onclick={() => (reminding = reminding === key ? null : key)}>
                  <AlarmClock size={12} /> Lembrar disso
                </button>
              {/if}
              <button class="link" onclick={() => onopen(c.serviceId, c.chat)}>Abrir conversa</button>
            </span>
          </div>
          {#if reminding === key && oncreatetask && clock}
            <div class="remind">
              <TaskForm
                link={{ serviceId: c.serviceId, chat: c.chat }}
                linkLabel="{c.title} · {service.name}"
                initialText={lastIncoming(c)}
                placeholder="Tarefa ou lembrete sobre {c.title}…"
                compact
                now={clock}
                onsave={async (fields) => {
                  await oncreatetask(fields);
                  reminding = null;
                }}
                oncancel={() => (reminding = null)}
              />
            </div>
          {/if}
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

  .quick {
    list-style: none;
    margin: 6px 0 0;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    max-height: 200px;
    overflow-y: auto;
  }

  .quick button {
    display: flex;
    align-items: baseline;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }

  .quick button.picked,
  .quick button:hover {
    background: var(--bg-hover);
  }

  .quick .sc {
    flex: none;
    color: var(--accent);
    font-weight: 600;
  }

  .quick .qt {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
  }

  .quick-empty {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--text-muted);
  }

  .links {
    display: inline-flex;
    flex: none;
    gap: 12px;
  }

  .remind {
    margin-top: 8px;
  }

  .link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
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
