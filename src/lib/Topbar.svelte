<script lang="ts">
  import {
    Circle,
    Globe,
    Lock,
    LockOpen,
    Mail,
    Minus,
    Monitor,
    NotebookPen,
    Moon,
    PanelLeft,
    Plus,
    RotateCw,
    Search,
    Snowflake,
    Sun,
  } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import {
    MODE_LABEL,
    THEME_OPTIONS,
    isInsecure,
    modKey,
    serviceColor,
    type Appearance,
    type ServiceView,
  } from "./api";

  let {
    active,
    appearance,
    ontogglesidebar,
    onsearch,
    ontheme,
    onreload,
    onhibernate,
    onsite,
    onzoom,
    panelOpen,
    onpanel,
    lockable,
    onlock,
    extra,
    twin = false,
  }: {
    active: ServiceView | null;
    appearance: Appearance;
    ontogglesidebar: () => void;
    onsearch: () => void;
    ontheme: () => void;
    onreload: () => void;
    onhibernate: () => void;
    /** Conta com o leitor de e-mail: mostra o site (`true`) ou volta ao leitor. */
    onsite: (site: boolean) => void;
    /** Zoom absoluto (`zoom`) ou relativo (`delta`). */
    onzoom: (change: { zoom?: number; delta?: number }) => void;
    /** Painel de tarefas e anotações do contato. */
    panelOpen: boolean;
    onpanel: () => void;
    lockable: boolean;
    onlock: () => void;
    /** Conteúdo ao lado da busca (o resumo, no Início). */
    extra?: Snippet;
    /** Há outra conta do mesmo serviço: destaca a troca de conta. */
    twin?: boolean;
  } = $props();

  const ZOOM_MIN = 0.5;
  const ZOOM_MAX = 2;
  const THEME_ICON = { system: Monitor, light: Sun, dark: Moon, oled: Circle };
  const ThemeIcon = $derived(THEME_ICON[appearance]);
  const themeLabel = $derived(THEME_OPTIONS.find((o) => o.value === appearance)?.label ?? "");
  const nextLabel = $derived(
    THEME_OPTIONS[(THEME_OPTIONS.findIndex((o) => o.value === appearance) + 1) % THEME_OPTIONS.length].label,
  );
</script>

<header class="topbar" data-tauri-drag-region>
  <button class="icon-btn ghost" onclick={ontogglesidebar} aria-label="Recolher ou expandir a barra lateral">
    <PanelLeft size={18} />
  </button>
  <span class="divider" aria-hidden="true"></span>
  <button class="search" onclick={onsearch} aria-label="Buscar contas, contatos e configurações" title="Buscar ({modKey}K)">
    <Search size={17} />
    <span>Buscar</span>
    <kbd>{modKey} K</kbd>
  </button>

  {#if extra}
    <span class="divider" aria-hidden="true"></span>
    {@render extra()}
  {/if}

  {#if active}
    {#key active.id}
      <span
        class="current"
        class:flash={twin || !!active.context}
        style:--acc={serviceColor(active)}
        data-tauri-drag-region
      >
        <ServiceAvatar service={active} size={20} />
        <span class="name" data-tauri-drag-region>{active.name}</span>
        {#if active.context && MODE_LABEL[active.context].toLowerCase() !== active.name.trim().toLowerCase()}
          <span class="ctx" data-tauri-drag-region>{MODE_LABEL[active.context]}</span>
        {/if}
      </span>
    {/key}
  {/if}

  {#if active && isInsecure(active)}
    <span
      class="alert"
      role="status"
      title="Este site usa http: o que você envia e recebe pode ser lido por quem estiver na mesma rede."
    >
      <LockOpen size={15} /> <span class="alert-text">Conexão não criptografada</span>
    </span>
  {/if}

  {#if active?.status === "unresponsive" || active?.status === "loggedOut"}
    {@const message =
      active.status === "unresponsive"
        ? `${active.name} não responde — recarregue a conta.`
        : `${active.name} está desconectado — faça login de novo.`}
    <span class="alert" class:danger={active.status === "unresponsive"} role="status" title={message}>
      <span class="alert-text">{message}</span>
    </span>
  {/if}

  <div class="spacer" data-tauri-drag-region></div>

  {#if active?.mail}
    <!-- E-mail por IMAP: leitor do Papeo ou o site do provedor. -->
    <button
      class="btn view-toggle"
      onclick={() => onsite(!!active.reader)}
      title={active.reader ? "Abrir o site do e-mail (carrega a página)" : "Voltar ao leitor do Papeo (libera a memória do site)"}
    >
      {#if active.reader}<Globe size={15} /> Abrir no site{:else}<Mail size={15} /> Voltar ao leitor{/if}
    </button>
  {/if}
  {#if active && !active.reader}
    <div class="zoom" role="group" aria-label="Zoom de {active.name}">
      <button
        onclick={() => onzoom({ delta: -0.1 })}
        disabled={active.zoom <= ZOOM_MIN + 0.001}
        title="Diminuir ({modKey}−)"
        aria-label="Diminuir o zoom"
      >
        <Minus size={15} />
      </button>
      <button
        class="level"
        class:changed={Math.abs(active.zoom - 1) > 0.001}
        onclick={() => onzoom({ zoom: 1 })}
        title="Voltar ao tamanho real ({modKey}⇧0)"
        aria-label="Zoom em {Math.round(active.zoom * 100)}%. Voltar ao tamanho real"
      >
        {Math.round(active.zoom * 100)}%
      </button>
      <button
        onclick={() => onzoom({ delta: 0.1 })}
        disabled={active.zoom >= ZOOM_MAX - 0.001}
        title="Aumentar ({modKey}+)"
        aria-label="Aumentar o zoom"
      >
        <Plus size={15} />
      </button>
    </div>
    <button
      class="icon-btn"
      class:pressed={panelOpen}
      onclick={onpanel}
      title="Tarefas e anotações ({modKey}⇧N)"
      aria-label="Tarefas e anotações"
      aria-pressed={panelOpen}
    >
      <NotebookPen size={17} />
    </button>
    <!-- Conta travada: o próprio botão de recarregar vira o destaque (sem um segundo "Recarregar" no alerta). -->
    <button
      class="icon-btn"
      class:urgent={active.status === "unresponsive"}
      onclick={onreload}
      title="Recarregar ({modKey}R)"
      aria-label="Recarregar {active.name}"
    >
      <RotateCw size={17} />
    </button>
    <button class="icon-btn" onclick={onhibernate} title="Hibernar e liberar memória" aria-label="Hibernar {active.name}">
      <Snowflake size={17} />
    </button>
  {/if}
  {#if lockable}
    <button class="icon-btn" onclick={onlock} title="Bloquear agora ({modKey}L)" aria-label="Bloquear o Papeo">
      <Lock size={17} />
    </button>
  {/if}
  <button
    class="icon-btn ghost"
    onclick={ontheme}
    title="Tema: {themeLabel} (clique para {nextLabel})"
    aria-label="Tema: {themeLabel}. Mudar para {nextLabel}"
  >
    <ThemeIcon size={17} fill={appearance === "oled" ? "currentColor" : "none"} />
  </button>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: var(--topbar-h);
    padding: 0 16px 0 20px;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
    flex: none;
  }

  .divider {
    width: 1px;
    height: 20px;
    margin: 0 6px;
    background: var(--border);
  }

  .search {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 8px 0 10px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    font-size: 15px;
  }

  .search:hover {
    background: var(--bg-hover);
  }

  kbd {
    font-family: inherit;
    font-size: 11.5px;
    padding: 1px 6px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg-subtle);
    color: var(--text-muted);
  }

  .current {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-left: 12px;
    padding-left: 16px;
    border-left: 1px solid var(--border);
    font-weight: 550;
  }

  .ctx {
    padding: 1px 7px;
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--acc) 55%, transparent);
    background: color-mix(in srgb, var(--acc) 12%, transparent);
    font-size: 11.5px;
    font-weight: 550;
  }

  /* Ao trocar entre contas parecidas, o nome da conta pisca na cor dela. */
  .current.flash {
    border-radius: 0 8px 8px 0;
    animation: flash 1.4s ease-out;
  }

  @keyframes flash {
    0%,
    35% {
      background: color-mix(in srgb, var(--acc) 22%, transparent);
    }
    100% {
      background: transparent;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .current.flash {
      animation: none;
    }
  }

  .alert {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-left: 12px;
    padding: 4px 10px;
    border-radius: 8px;
    font-size: 13px;
    color: var(--warn-text);
    background: var(--warn-bg);
    border: 1px solid var(--warn-border);
  }

  .alert.danger {
    color: var(--danger);
    background: var(--danger-bg);
    border-color: color-mix(in srgb, var(--danger) 35%, transparent);
  }

  .urgent {
    color: var(--danger);
    background: var(--danger-bg);
    border-color: color-mix(in srgb, var(--danger) 45%, transparent);
  }

  .urgent:hover {
    background: color-mix(in srgb, var(--danger) 18%, var(--bg));
  }

  .zoom {
    display: inline-flex;
    align-items: center;
    height: 30px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg-subtle);
    overflow: hidden;
  }

  .zoom button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-width: 28px;
    padding: 0 6px;
    border: 0;
    background: transparent;
    color: var(--text-muted);
  }

  .zoom button:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text);
  }

  .zoom button:disabled {
    opacity: 0.4;
  }

  .zoom .level {
    min-width: 46px;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .zoom .level.changed {
    color: var(--text);
    font-weight: 600;
  }

  .pressed {
    background: var(--bg-hover);
    border-color: var(--border-strong);
  }

  .view-toggle {
    gap: 6px;
    white-space: nowrap;
  }

  .spacer {
    flex: 1;
    align-self: stretch;
  }

  /* Controles e textos não encolhem nem quebram: quem cede espaço é o nome da
     conta e o texto do alerta (com reticências e o texto completo no title). */
  .topbar > :global(*) {
    flex: none;
  }

  /* O alerta cede espaço antes do nome da conta (que tem teto próprio). */
  .topbar > .alert,
  .topbar > .spacer {
    flex: 0 1 auto;
    min-width: 0;
  }

  .topbar > .current {
    flex: 0 1 auto;
    min-width: 72px;
    max-width: 240px;
  }

  .topbar > .alert {
    flex-shrink: 1000;
  }

  .topbar > .spacer {
    flex-grow: 1;
  }

  .search,
  kbd,
  .current,
  .alert {
    white-space: nowrap;
  }

  .name,
  .alert-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .current :global(*),
  .alert > :global(svg) {
    flex: none;
  }

  .current .name {
    flex: 0 1 auto;
  }

  /* Janela estreita: a busca vira só ícone (o atalho continua no title). */
  @media (max-width: 1180px) {
    .search > span,
    .search > kbd {
      display: none;
    }
  }
</style>
