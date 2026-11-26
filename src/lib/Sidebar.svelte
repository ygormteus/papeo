<script lang="ts">
  import {
    BellOff,
    Briefcase,
    CirclePlus,
    Download,
    House,
    Layers,
    ListTodo,
    MemoryStick,
    Settings,
    Sofa,
  } from "@lucide/svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import {
    MODE_LABEL,
    isMac,
    isMuted,
    modKey,
    nextMode,
    untilLabel,
    updateLabel,
    usesModes,
    type Mode,
    type Snapshot,
    type ServiceView,
  } from "./api";

  let {
    snap,
    collapsed,
    page,
    onselect,
    onadd,
    onsettings,
    onmenu,
    onmove,
    onupdate,
    onnotes,
    ontasks,
    onmode,
    onautomode,
  }: {
    snap: Snapshot;
    collapsed: boolean;
    page: "home" | "settings" | "tasks";
    onselect: (id: string | null) => void;
    onadd: () => void;
    onsettings: () => void;
    onmenu: (service: ServiceView, x: number, y: number) => void;
    onmove: (id: string, to: number) => void;
    onupdate: () => void;
    onnotes: () => void;
    ontasks: () => void;
    /** Troca rápida de modo (trabalho, pessoal, todas). */
    onmode: (mode: Mode) => void;
    /** Desfaz a troca manual e volta ao expediente. */
    onautomode: () => void;
  } = $props();

  const MODE_ICON = { work: Briefcase, personal: Sofa, all: Layers };
  const modes = $derived(usesModes(snap));
  // Só as contas do modo atual (e a aberta, se veio da busca).
  const shown = $derived(snap.services.filter((s) => s.inMode || s.id === snap.active));
  const hiddenCount = $derived(snap.services.length - shown.length);
  /** Número do atalho ({modKey}1–9): só as contas do modo, como no menu. */
  const numberOf = $derived(
    new Map(snap.services.filter((s) => s.inMode).map((s, i) => [s.id, i + 1] as const)),
  );
  const ModeIcon = $derived(MODE_ICON[snap.mode]);

  const due = $derived(snap.tasks.overdue + snap.tasks.today);

  const updating = $derived(
    snap.update.state === "available" || snap.update.state === "installing",
  );

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
    // A lista mostrada pode ser só a do modo: converte para a posição real.
    const to = shown[index] ? snap.services.findIndex((s) => s.id === shown[index].id) : -1;
    dragEnd();
    if (id && from !== -1 && to !== -1 && from !== to) onmove(id, to);
  }

  function dragEnd() {
    dragId = null;
    overIndex = null;
  }

  const loadedCount = $derived(snap.services.filter((s) => s.loaded).length);

  /** Estado da conta no title: na barra recolhida é o único texto que sobra. */
  function stateNote(service: ServiceView): string {
    if (service.status === "unresponsive") return " — não responde";
    if (service.status === "loggedOut") return " — desconectado";
    if (!service.loaded) return " — hibernado";
    if (service.unread > 0) return ` — ${service.unread === 1 ? "1 não lida" : `${service.unread} não lidas`}`;
    return "";
  }

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
    <svg class="logo" viewBox="0 0 1024 1024" aria-hidden="true">
      <rect width="1024" height="1024" rx="230" class="logo-tile" />
      <circle cx="512" cy="422" r="236" class="logo-mark" />
      <path d="M276 422h170v300L276 838Z" class="logo-mark" />
      <circle cx="512" cy="422" r="92" fill="#22c55e" />
    </svg>
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
    <button
      class="item"
      class:active={snap.active === null && page === "tasks"}
      aria-current={snap.active === null && page === "tasks" ? "page" : undefined}
      onclick={ontasks}
      title={collapsed && snap.tasks.overdue > 0
        ? `Tarefas — ${snap.tasks.overdue === 1 ? "1 atrasada" : `${snap.tasks.overdue} atrasadas`}`
        : collapsed && snap.tasks.today > 0
          ? `Tarefas — ${snap.tasks.today} para hoje`
          : "Tarefas e lembretes"}
    >
      <span class="avatar-wrap">
        <ListTodo size={18} />
        {#if collapsed && due > 0}
          <span class="dot" class:danger={snap.tasks.overdue > 0} aria-hidden="true"></span>
        {/if}
      </span>
      {#if !collapsed}
        <span class="label">Tarefas</span>
        {#if snap.tasks.overdue > 0}
          <span class="pill pill-danger" aria-label={snap.tasks.overdue === 1 ? "1 atrasada" : `${snap.tasks.overdue} atrasadas`}>{snap.tasks.overdue}</span>
        {:else if snap.tasks.today > 0}
          <span class="pill" aria-label="{snap.tasks.today} para hoje">{snap.tasks.today}</span>
        {/if}
      {/if}
    </button>

    {#if modes && !collapsed}
      <div class="modes" role="group" aria-label="Modo">
        {#each ["work", "personal", "all"] as const as m (m)}
          {@const Icon = MODE_ICON[m]}
          <button
            class="mode"
            class:on={snap.mode === m}
            aria-pressed={snap.mode === m}
            onclick={() => snap.mode !== m && onmode(m)}
            title="{MODE_LABEL[m]} ({modKey}⇧M alterna)"
          >
            <Icon size={14} />
            {MODE_LABEL[m]}
          </button>
        {/each}
      </div>
      {#if snap.settings.mode === "auto"}
        <p class="mode-note">
          {#if snap.modeManual}
            Até o expediente mudar ·
            <button class="tip-link inline" onclick={onautomode}>voltar ao automático</button>
          {:else}
            Automático pelo expediente
          {/if}
        </p>
      {/if}
    {:else if modes}
      <button
        class="item"
        onclick={() => onmode(nextMode(snap.mode))}
        title="Modo: {MODE_LABEL[snap.mode]} — clique para alternar ({modKey}⇧M)"
        aria-label="Modo {MODE_LABEL[snap.mode]}. Alternar"
      >
        <ModeIcon size={18} />
      </button>
    {/if}

    {#if !collapsed}<div class="section">Mensageiros</div>{:else}<div class="sep"></div>{/if}

    {#each shown as service, i (service.id)}
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
        title={`${service.name}${(numberOf.get(service.id) ?? 99) <= 9 ? ` (${modKey}${numberOf.get(service.id)})` : ""}${stateNote(service)}`}
      >
        <span class="avatar-wrap">
          <ServiceAvatar {service} dimmed={!service.loaded} />
          {#if collapsed && service.status === "unresponsive"}
            <span class="dot danger" aria-hidden="true"></span>
          {:else if collapsed && service.status === "loggedOut"}
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
            <span class="pill pill-success" aria-label={service.unread === 1 ? "1 não lida" : `${service.unread} não lidas`}>
              {service.unread > 99 ? "99+" : service.unread}
            </span>
          {:else if !service.loaded}
            <span class="pill">Dormindo</span>
          {:else if (numberOf.get(service.id) ?? 99) <= 9}
            <kbd class="kbd">{modKey}{numberOf.get(service.id)}</kbd>
          {/if}
        {/if}
      </button>
    {/each}

    {#if snap.services.length === 0 && !collapsed}
      <p class="empty">Nenhum serviço ainda.</p>
    {:else if hiddenCount > 0 && !collapsed}
      <p class="empty hidden-note">
        {hiddenCount === 1 ? "1 conta fica" : `${hiddenCount} contas ficam`} fora do modo {MODE_LABEL[snap.mode]}.
      </p>
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
      <span class="avatar-wrap">
        <Settings size={18} />
        {#if collapsed && snap.update.state === "available"}
          <span class="dot" aria-label="Atualização disponível"></span>
        {/if}
      </span>
      {#if !collapsed}<span class="label">Configurações</span>{/if}
      {#if snap.dndActive}
        <BellOff size={14} class="muted-icon" aria-label="Não perturbe ativo" />
      {/if}
    </button>
  </div>

  {#if !collapsed}
    <div class="tip card">
      {#if updating}
        <div class="tip-title"><Download size={15} /> Atualização</div>
        <p role="status">{updateLabel(snap.update)}</p>
        {#if snap.update.state === "available"}
          <button class="btn btn-primary tip-action" onclick={onupdate}>Atualizar e reiniciar</button>
          {#if snap.update.notes}
            <button class="tip-link" onclick={onnotes}>Ver o que mudou</button>
          {/if}
        {/if}
      {:else if snap.dndActive}
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
    width: 24px;
    height: 24px;
    flex: none;
    pointer-events: none;
  }

  .logo-tile {
    fill: var(--text);
  }

  .logo-mark {
    fill: var(--bg);
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
    color: var(--text-muted);
    opacity: 0;
    transition: opacity 0.12s;
  }

  .item:hover .kbd {
    opacity: 1;
  }

  .item :global(.muted-icon) {
    flex: none;
    color: var(--text-muted);
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

  .dot.danger {
    background: var(--danger);
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

  .tip-action {
    margin-top: 10px;
    width: 100%;
  }

  .tip-link {
    display: block;
    width: 100%;
    margin-top: 8px;
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font-size: 12.5px;
    text-align: center;
  }

  .tip-link:hover {
    text-decoration: underline;
  }

  .modes {
    display: flex;
    gap: 2px;
    margin: 10px 4px 0;
    padding: 3px;
    border-radius: 10px;
    background: var(--bg-hover);
  }

  .mode {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    min-width: 0;
    height: 28px;
    padding: 0 4px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
    font-size: 12.5px;
    white-space: nowrap;
  }

  .mode.on {
    background: var(--bg);
    color: var(--text);
    font-weight: 550;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.12);
  }

  .mode-note {
    margin: 6px 12px 0;
    color: var(--text-muted);
    font-size: 12px;
  }

  .tip-link.inline {
    display: inline;
    width: auto;
    margin: 0;
    font-size: inherit;
  }

  .hidden-note {
    font-size: 12.5px;
  }

  .tip p {
    margin: 0;
    color: var(--text-muted);
    font-size: 13px;
  }
</style>
