<script lang="ts">
  import { BellOff, CirclePlus, House, MemoryStick, Settings } from "@lucide/svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import { isMac, isMuted, modKey, untilLabel, type Snapshot, type ServiceView } from "./api";

  let {
    snap,
    collapsed,
    page,
    onselect,
    onadd,
    onsettings,
    onmenu,
    onmove,
  }: {
    snap: Snapshot;
    collapsed: boolean;
    page: "home" | "settings";
    onselect: (id: string | null) => void;
    onadd: () => void;
    onsettings: () => void;
    onmenu: (service: ServiceView, x: number, y: number) => void;
    onmove: (id: string, to: number) => void;
  } = $props();

  // Arrastar para reordenar (drag and drop do HTML; o Tauri não intercepta na `ui`).
  let dragId = $state<string | null>(null);
  let overIndex = $state<number | null>(null);

  function dragStart(e: DragEvent, service: ServiceView) {
    dragId = service.id;
    e.dataTransfer?.setData("text/plain", service.id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  }

  function dragOver(e: DragEvent, index: number) {
    if (!dragId) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    overIndex = index;
  }

  function drop(e: DragEvent, index: number) {
    e.preventDefault();
    const id = dragId;
    const from = snap.services.findIndex((s) => s.id === id);
    dragEnd();
    if (id && from !== -1 && from !== index) onmove(id, index);
  }

  function dragEnd() {
    dragId = null;
    overIndex = null;
  }

  const loadedCount = $derived(snap.services.filter((s) => s.loaded).length);

  function contextMenu(e: MouseEvent, service: ServiceView) {
    e.preventDefault();
    onmenu(service, e.clientX, e.clientY);
  }

  function menuKey(e: KeyboardEvent, service: ServiceView) {
    if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
      e.preventDefault();
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      onmenu(service, r.left + 24, r.bottom);
    }
  }
</script>

<aside class="sidebar" class:collapsed class:mac={isMac}>
  <div class="brand" data-tauri-drag-region>
    <span class="logo" aria-hidden="true"></span>
    {#if !collapsed}<span class="brand-name" data-tauri-drag-region>Papeo</span>{/if}
  </div>

  <div class="create">
    <button class="btn btn-primary create-btn" onclick={onadd} title="Adicionar serviço ({modKey}N)">
      <CirclePlus size={17} />
      {#if !collapsed}<span>Adicionar serviço</span>{/if}
    </button>
  </div>

  <nav class="nav" aria-label="Serviços">
    <button
      class="item"
      class:active={snap.active === null && page === "home"}
      aria-current={snap.active === null && page === "home" ? "page" : undefined}
      onclick={() => onselect(null)}
      title="Início ({modKey}0)"
    >
      <House size={18} />
      {#if !collapsed}<span class="label">Início</span>{/if}
    </button>

    {#if !collapsed}<div class="section">Mensageiros</div>{:else}<div class="sep"></div>{/if}

    {#each snap.services as service, i (service.id)}
      <button
        class="item"
        class:active={snap.active === service.id}
        class:asleep={!service.loaded}
        class:dragging={dragId === service.id}
        class:drop-target={overIndex === i && dragId !== service.id}
        draggable="true"
        ondragstart={(e) => dragStart(e, service)}
        ondragover={(e) => dragOver(e, i)}
        ondrop={(e) => drop(e, i)}
        ondragend={dragEnd}
        aria-current={snap.active === service.id ? "page" : undefined}
        onclick={() => onselect(service.id)}
        oncontextmenu={(e) => contextMenu(e, service)}
        onkeydown={(e) => menuKey(e, service)}
        title={`${service.name}${i < 9 ? ` (${modKey}${i + 1})` : ""}${service.loaded ? "" : " — hibernado"}`}
      >
        <span class="avatar-wrap">
          <ServiceAvatar {service} dimmed={!service.loaded} />
          {#if collapsed && (service.status === "unresponsive" || service.status === "loggedOut")}
            <span class="dot warn" aria-hidden="true"></span>
          {:else if collapsed && service.unread > 0}
            <span class="dot" aria-hidden="true"></span>
          {/if}
        </span>
        {#if !collapsed}
          <span class="label">{service.name}</span>
          {#if isMuted(service)}
            <BellOff size={14} class="muted-icon" aria-label="Notificações silenciadas" />
          {/if}
          {#if service.status === "unresponsive"}
            <span class="pill pill-danger">Não responde</span>
          {:else if service.status === "loggedOut"}
            <span class="pill pill-warn">Desconectado</span>
          {:else if service.unread > 0}
            <span class="pill pill-success" aria-label="{service.unread} não lidas">
              {service.unread > 99 ? "99+" : service.unread}
            </span>
          {:else if !service.loaded}
            <span class="pill">Dormindo</span>
          {:else if i < 9}
            <kbd class="kbd">{modKey}{i + 1}</kbd>
          {/if}
        {/if}
      </button>
    {/each}

    {#if snap.services.length === 0 && !collapsed}
      <p class="empty">Nenhum serviço ainda.</p>
    {/if}
  </nav>

  <div class="footer-nav">
    <button
      class="item"
      class:active={snap.active === null && page === "settings"}
      aria-current={snap.active === null && page === "settings" ? "page" : undefined}
      onclick={onsettings}
      title="Configurações ({modKey},)"
    >
      <Settings size={18} />
      {#if !collapsed}<span class="label">Configurações</span>{/if}
      {#if snap.dndActive}
        <BellOff size={14} class="muted-icon" aria-label="Não perturbe ativo" />
      {/if}
    </button>
  </div>

  {#if !collapsed}
    <div class="tip card">
      {#if snap.dndActive}
        <div class="tip-title"><BellOff size={15} /> Não perturbe</div>
        <p>Notificações silenciadas {untilLabel(snap.settings.dndUntil) || "pelo horário silencioso"}.</p>
      {:else}
        <div class="tip-title"><MemoryStick size={15} /> Memória</div>
        <p>
          {loadedCount} de {snap.services.length}
          {snap.services.length === 1 ? "serviço" : "serviços"} em memória. Clique com o botão direito num
          serviço para hibernar e liberar RAM.
        </p>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: var(--sidebar-w);
    height: 100%;
    background: var(--bg-sidebar);
    border-right: 1px solid var(--border);
    transition: width 0.18s ease;
    overflow: hidden;
  }

  .collapsed {
    width: 68px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    height: var(--topbar-h);
    padding: 0 20px;
    flex: none;
  }

  .mac .brand {
    padding-left: 84px;
  }

  .mac.collapsed .brand {
    height: calc(var(--topbar-h) + 22px);
    padding: 30px 0 0;
    justify-content: center;
  }

  .logo {
    width: 20px;
    height: 20px;
    border-radius: 6px;
    border: 3.5px solid var(--text);
    flex: none;
    pointer-events: none;
  }

  .brand-name {
    font-size: 17px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .create {
    padding: 4px 12px 8px;
  }

  .create-btn {
    width: 100%;
    height: 40px;
    justify-content: flex-start;
  }

  .collapsed .create-btn {
    justify-content: center;
    padding: 0;
  }

  .nav {
    flex: 1;
    overflow-y: auto;
    padding: 4px 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .section {
    padding: 18px 12px 6px;
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-muted);
  }

  .sep {
    height: 1px;
    margin: 10px 12px;
    background: var(--border);
  }

  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    height: 40px;
    padding: 0 10px 0 12px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    text-align: left;
    font-size: 15px;
  }

  .collapsed .item {
    justify-content: center;
    padding: 0;
  }

  .item:hover {
    background: var(--bg-hover);
  }

  .item.active {
    background: var(--bg-hover);
    font-weight: 550;
  }

  .item.asleep .label {
    color: var(--text-muted);
  }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .avatar-wrap {
    position: relative;
    display: inline-flex;
  }

  .dot {
    position: absolute;
    top: -3px;
    right: -3px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--success-text);
    border: 2px solid var(--bg-sidebar);
  }

  .kbd {
    font-family: inherit;
    font-size: 11.5px;
    color: var(--text-faint);
    opacity: 0;
    transition: opacity 0.12s;
  }

  .item:hover .kbd {
    opacity: 1;
  }

  .item :global(.muted-icon) {
    flex: none;
    color: var(--text-faint);
  }

  .item.dragging {
    opacity: 0.45;
  }

  .item.drop-target {
    box-shadow: inset 0 2px 0 var(--accent);
  }

  .dot.warn {
    background: var(--warn);
  }

  .footer-nav {
    padding: 0 8px 10px;
  }

  .empty {
    padding: 4px 12px;
    color: var(--text-muted);
  }

  .tip {
    margin: 0 12px 14px;
    padding: 14px 16px;
  }

  .tip-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 550;
    margin-bottom: 4px;
  }

  .tip p {
    margin: 0;
    color: var(--text-muted);
    font-size: 13px;
  }
</style>
