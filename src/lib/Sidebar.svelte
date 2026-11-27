<script lang="ts">
  import { t } from "./i18n.svelte";
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
  import { untrack } from "svelte";
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

  // Contador que acabou de subir (mensagem ou e-mail novo): destaque curto.
  // Só compara números que o snapshot já traz; nada é consultado a mais.
  let fresh = $state<Record<string, number>>({});
  const lastUnread = new Map<string, number>();
  let freshSeq = 0;
  $effect(() => {
    for (const s of snap.services) {
      const before = lastUnread.get(s.id);
      lastUnread.set(s.id, s.unread);
      if (before === undefined || s.unread <= before) continue;
      const seq = ++freshSeq;
      const id = s.id;
      untrack(() => (fresh = { ...fresh, [id]: seq }));
      setTimeout(() => {
        if (untrack(() => fresh[id]) !== seq) return;
        const { [id]: _, ...rest } = untrack(() => fresh);
        fresh = rest;
      }, 2400);
    }
  });

  const loadedCount = $derived(snap.services.filter((s) => s.loaded).length);

  /** Estado da conta no title: na barra recolhida é o único texto que sobra. */
  function stateNote(service: ServiceView): string {
    if (service.status === "unresponsive") return ` — ${t("não responde")}`;
    if (service.status === "loggedOut") return ` — ${t("desconectado")}`;
    if (!service.loaded && service.mailStatus?.state === "ok") {
      const n = service.unread;
      const note =
        n === 1
          ? t("hibernado, avisos por IMAP, 1 não lida")
          : n > 1
            ? t("hibernado, avisos por IMAP, {n} não lidas", { n })
            : t("hibernado, avisos por IMAP");
      return ` — ${note}`;
    }
    if (!service.loaded) return ` — ${t("hibernado")}`;
    if (service.unread > 0) return ` — ${service.unread === 1 ? t("1 não lida") : t("{n} não lidas", { n: service.unread })}`;
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
    <button class="btn btn-primary create-btn" onclick={onadd} title={t("Adicionar serviço ({atalho}N)", { atalho: modKey })}>
      <CirclePlus size={17} />
      {#if !collapsed}<span>{t("Adicionar serviço")}</span>{/if}
    </button>
  </div>

  <nav class="nav" aria-label={t("Serviços")}>
    <button
      class="item"
      class:active={snap.active === null && page === "home"}
      aria-current={snap.active === null && page === "home" ? "page" : undefined}
      onclick={() => onselect(null)}
      title={t("Início ({atalho}0)", { atalho: modKey })}
    >
      <House size={18} />
      {#if !collapsed}<span class="label">{t("Início")}</span>{/if}
    </button>
    <button
      class="item"
      class:active={snap.active === null && page === "tasks"}
      aria-current={snap.active === null && page === "tasks" ? "page" : undefined}
      onclick={ontasks}
      title={collapsed && snap.tasks.overdue > 0
        ? snap.tasks.overdue === 1
          ? t("Tarefas — 1 atrasada")
          : t("Tarefas — {n} atrasadas", { n: snap.tasks.overdue })
        : collapsed && snap.tasks.today > 0
          ? t("Tarefas — {n} para hoje", { n: snap.tasks.today })
          : t("Tarefas e lembretes")}
    >
      <span class="avatar-wrap">
        <ListTodo size={18} />
        {#if collapsed && due > 0}
          <span class="dot" class:danger={snap.tasks.overdue > 0} aria-hidden="true"></span>
        {/if}
      </span>
      {#if !collapsed}
        <span class="label">{t("Tarefas")}</span>
        {#if snap.tasks.overdue > 0}
          <span class="count count-danger" aria-label={snap.tasks.overdue === 1 ? t("1 atrasada") : t("{n} atrasadas", { n: snap.tasks.overdue })}>{snap.tasks.overdue}</span>
        {:else if snap.tasks.today > 0}
          <span class="count count-neutral" aria-label={t("{n} para hoje", { n: snap.tasks.today })}>{snap.tasks.today}</span>
        {/if}
      {/if}
    </button>

    {#if modes && !collapsed}
      <div class="modes" role="group" aria-label={t("Modo")}>
        {#each ["work", "personal", "all"] as const as m (m)}
          {@const Icon = MODE_ICON[m]}
          <button
            class="mode"
            class:on={snap.mode === m}
            aria-pressed={snap.mode === m}
            onclick={() => snap.mode !== m && onmode(m)}
            title={t("{modo} ({atalho}⇧M alterna)", { modo: t(MODE_LABEL[m]), atalho: modKey })}
          >
            <Icon size={14} />
            {t(MODE_LABEL[m])}
          </button>
        {/each}
      </div>
      {#if snap.settings.mode === "auto"}
        <p class="mode-note">
          {#if snap.modeManual}
            {t("Até o expediente mudar")} ·
            <button class="tip-link inline" onclick={onautomode}>{t("voltar ao automático")}</button>
          {:else}
            {t("Automático pelo expediente")}
          {/if}
        </p>
      {/if}
    {:else if modes}
      <button
        class="item"
        onclick={() => onmode(nextMode(snap.mode))}
        title={t("Modo: {modo} — clique para alternar ({atalho}⇧M)", { modo: t(MODE_LABEL[snap.mode]), atalho: modKey })}
        aria-label={t("Modo {modo}. Alternar", { modo: t(MODE_LABEL[snap.mode]) })}
      >
        <ModeIcon size={18} />
      </button>
    {/if}

    {#if !collapsed}<div class="section">{t("Mensageiros")}</div>{:else}<div class="sep"></div>{/if}

    {#each shown as service, i (service.id)}
      <button
        class="item account"
        class:active={snap.active === service.id}
        class:beside={snap.split !== null && snap.active !== service.id && (snap.split.left === service.id || snap.split.right === service.id)}
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
          <ServiceAvatar {service} size={24} dimmed={!service.loaded} />
          {#if service.status === "unresponsive"}
            <span class="dot danger" aria-hidden="true"></span>
          {:else if service.status === "loggedOut"}
            <span class="dot warn" aria-hidden="true"></span>
          {:else if collapsed && service.unread > 0}
            <span class="badge" class:fresh={service.id in fresh} aria-hidden="true">
              {service.unread > 99 ? "99+" : service.unread}
            </span>
          {/if}
        </span>
        {#if !collapsed}
          <span class="text">
            <span class="label">{service.name}</span>
            {#if service.status === "unresponsive"}
              <span class="sub sub-danger">{t("Não responde")}</span>
            {:else if service.status === "loggedOut"}
              <span class="sub sub-warn">{t("Desconectado")}</span>
            {:else if !service.loaded}
              <!-- Site hibernado, mas o IMAP continua avisando. -->
              <span class="sub">{service.mailStatus?.state === "ok" ? t("Avisos por IMAP") : t("Dormindo")}</span>
            {/if}
          </span>
          {#if isMuted(service)}
            <BellOff size={14} class="muted-icon" aria-label={t("Notificações silenciadas")} />
          {/if}
          {#if service.unread > 0}
            <span class="count" class:fresh={service.id in fresh} aria-label={service.unread === 1 ? t("1 não lida") : t("{n} não lidas", { n: service.unread })}>
              {service.unread > 99 ? "99+" : service.unread}
            </span>
          {:else if service.loaded && (numberOf.get(service.id) ?? 99) <= 9}
            <kbd class="kbd">{modKey}{numberOf.get(service.id)}</kbd>
          {/if}
        {/if}
      </button>
    {/each}

    {#if snap.services.length === 0 && !collapsed}
      <p class="empty">{t("Nenhum serviço ainda.")}</p>
    {:else if hiddenCount > 0 && !collapsed}
      <p class="empty hidden-note">
        {hiddenCount === 1
          ? t("1 conta fica fora do modo {modo}.", { modo: t(MODE_LABEL[snap.mode]) })
          : t("{n} contas ficam fora do modo {modo}.", { n: hiddenCount, modo: t(MODE_LABEL[snap.mode]) })}
      </p>
    {/if}
  </nav>

  <div class="footer-nav">
    <button
      class="item"
      class:active={snap.active === null && page === "settings"}
      aria-current={snap.active === null && page === "settings" ? "page" : undefined}
      onclick={onsettings}
      title={t("Configurações ({atalho},)", { atalho: modKey })}
    >
      <span class="avatar-wrap">
        <Settings size={18} />
        {#if collapsed && snap.update.state === "available"}
          <span class="dot" aria-label={t("Atualização disponível")}></span>
        {/if}
      </span>
      {#if !collapsed}<span class="label">{t("Configurações")}</span>{/if}
      {#if snap.dndActive}
        <BellOff size={14} class="muted-icon" aria-label={t("Não perturbe ativo")} />
      {/if}
    </button>
  </div>

  {#if !collapsed}
    <div class="tip card">
      {#if updating}
        <div class="tip-title"><Download size={15} /> {t("Atualização")}</div>
        <p role="status">{updateLabel(snap.update)}</p>
        {#if snap.update.state === "available"}
          <button class="btn btn-primary tip-action" onclick={onupdate}>{t("Atualizar e reiniciar")}</button>
          {#if snap.update.notes}
            <button class="tip-link" onclick={onnotes}>{t("Ver o que mudou")}</button>
          {/if}
        {/if}
      {:else if snap.dndActive}
        <div class="tip-title"><BellOff size={15} /> {t("Não perturbe")}</div>
        <p>{t("Notificações silenciadas {ate}.", { ate: untilLabel(snap.settings.dndUntil) || t("pelo horário silencioso") })}</p>
      {:else}
        <div class="tip-title"><MemoryStick size={15} /> {t("Memória")}</div>
        <p>
          {snap.services.length === 1
            ? t("{n} de {total} serviço em memória. Clique com o botão direito num serviço para hibernar e liberar RAM.", { n: loadedCount, total: snap.services.length })
            : t("{n} de {total} serviços em memória. Clique com o botão direito num serviço para hibernar e liberar RAM.", { n: loadedCount, total: snap.services.length })}
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
    padding: 20px 12px 6px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .sep {
    height: 1px;
    margin: 10px 12px;
    background: var(--border);
  }

  .item {
    position: relative;
    /* Altura fixa: em janelas baixas a lista rola em vez de espremer. */
    flex: none;
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
    transition: background-color 0.12s ease;
  }

  .collapsed .item {
    justify-content: center;
    padding: 0;
  }

  .item:hover {
    background: var(--bg-hover);
  }

  .item.active {
    background: color-mix(in srgb, var(--accent) 12%, var(--bg-hover));
    font-weight: 550;
  }

  /* Tela dividida: a conta da outra metade, mais suave que a em foco. */
  .item.beside {
    background: color-mix(in srgb, var(--accent) 5%, transparent);
  }

  .item.beside::before {
    content: "";
    position: absolute;
    left: 0;
    top: 14px;
    bottom: 14px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }

  .collapsed .item.beside::before {
    left: -8px;
  }

  /* Marca discreta do item aberto (além da cor, para quem não distingue o fundo). */
  .item.active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }

  .collapsed .item.active::before {
    left: -8px;
  }

  /* Contador de não lidas: cápsula sólida e compacta, números alinhados. */
  .count {
    flex: none;
    min-width: 20px;
    height: 20px;
    padding: 0 6px;
    border-radius: 10px;
    display: inline-grid;
    place-items: center;
    background: var(--success-bg);
    color: var(--success-text);
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .count-danger {
    background: var(--danger-bg);
    color: var(--danger);
  }

  .count-neutral {
    background: var(--bg-hover);
    color: var(--text-muted);
  }

  /* Nome em cima e, só quando há algo a dizer, o estado embaixo. */
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    line-height: 1.2;
  }

  .text .label {
    flex: none;
  }

  .account {
    height: 46px;
  }

  .collapsed .account {
    height: 42px;
  }

  .sub {
    margin-top: 2px;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .sub-danger {
    color: var(--danger);
  }

  .sub-warn {
    color: var(--warn-text);
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

  /* Barra recolhida: o número de não lidas em cima do ícone. */
  .badge {
    position: absolute;
    top: -7px;
    right: -10px;
    min-width: 20px;
    height: 20px;
    padding: 0 4px;
    border-radius: 10px;
    display: inline-grid;
    place-items: center;
    background: var(--success-text);
    color: var(--bg-sidebar);
    border: 2px solid var(--bg-sidebar);
    box-sizing: border-box;
    font-size: 10.5px;
    font-weight: 700;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }

  /* Chegou algo novo: o contador pulsa duas vezes (sem movimento se o
     sistema pede menos animação, pela regra global). */
  .fresh {
    animation: unread-pulse 1.2s ease-out 2;
  }

  @keyframes unread-pulse {
    0% {
      transform: scale(1);
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--success-text) 55%, transparent);
    }
    25% {
      transform: scale(1.25);
    }
    50% {
      transform: scale(1);
    }
    100% {
      box-shadow: 0 0 0 7px transparent;
    }
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
