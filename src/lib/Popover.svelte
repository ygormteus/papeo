<script lang="ts">
  import { onMount } from "svelte";
  import { BellOff, Bell, ExternalLink, Lock, LockOpen } from "@lucide/svelte";
  import RecentChats from "./RecentChats.svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import {
    api,
    EMPTY_SNAPSHOT,
    modKey,
    onState,
    type Snapshot,
  } from "./api";

  // Mini janela da barra de menus (macOS) / bandeja (Windows): conversas
  // recentes com resposta rápida. Some ao perder o foco (backend).

  let snap = $state<Snapshot>(EMPTY_SNAPSHOT);
  let list = $state<ReturnType<typeof RecentChats>>();

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

  onMount(() => {
    api.getState().then((s) => (snap = s));
    const off = onState((s) => (snap = s));
    const onScheme = (e: MediaQueryListEvent) => (systemIsDark = e.matches);
    systemDark.addEventListener("change", onScheme);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") api.hidePopover().catch(() => {});
    };
    window.addEventListener("keydown", onKey);
    // Reaberta pela barra: foca a resposta da conversa aberta.
    const onFocus = () => list?.focus();
    window.addEventListener("focus", onFocus);
    return () => {
      off.then((f) => f());
      systemDark.removeEventListener("change", onScheme);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("focus", onFocus);
    };
  });

  const go = (id: string | null = null, chat: string | null = null) =>
    api.showApp(id, chat).catch(() => {});

  async function toggleDnd() {
    snap = await api.setDnd(snap.dndActive ? null : 60).catch(() => snap);
  }

  async function lockNow() {
    snap = await api.lockNow().catch(() => snap);
  }

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
      <RecentChats
        bind:this={list}
        {chats}
        services={snap.services}
        onsnap={(s) => (snap = s)}
        onopen={(id, chat) => go(id, chat)}
        autoOpen
      />
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
  .foot:hover {
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
