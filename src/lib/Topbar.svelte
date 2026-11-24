<script lang="ts">
  import {
    Circle,
    Lock,
    LockOpen,
    Monitor,
    Moon,
    PanelLeft,
    RotateCw,
    Search,
    Snowflake,
    Sun,
  } from "@lucide/svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import { THEME_OPTIONS, isInsecure, modKey, type Appearance, type ServiceView } from "./api";

  let {
    active,
    appearance,
    ontogglesidebar,
    onsearch,
    ontheme,
    onreload,
    onhibernate,
    onzoomreset,
    lockable,
    onlock,
  }: {
    active: ServiceView | null;
    appearance: Appearance;
    ontogglesidebar: () => void;
    onsearch: () => void;
    ontheme: () => void;
    onreload: () => void;
    onhibernate: () => void;
    onzoomreset: () => void;
    lockable: boolean;
    onlock: () => void;
  } = $props();

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
  <button class="search" onclick={onsearch} aria-label="Buscar conta ou contato">
    <Search size={17} />
    <span>Buscar</span>
    <kbd>{modKey} K</kbd>
  </button>

  {#if active}
    <span class="current" data-tauri-drag-region>
      <ServiceAvatar service={active} size={20} />
      <span data-tauri-drag-region>{active.name}</span>
    </span>
  {/if}

  {#if active && isInsecure(active)}
    <span
      class="alert"
      role="status"
      title="Este site usa http: o que você envia e recebe pode ser lido por quem estiver na mesma rede."
    >
      <LockOpen size={15} /> Conexão não criptografada
    </span>
  {/if}

  {#if active?.status === "unresponsive" || active?.status === "loggedOut"}
    <span class="alert" class:danger={active.status === "unresponsive"} role="status">
      {active.status === "unresponsive"
        ? `${active.name} não responde.`
        : `${active.name} está desconectado — faça login de novo.`}
      {#if active.status === "unresponsive"}
        <button class="link" onclick={onreload}>Recarregar</button>
      {/if}
    </span>
  {/if}

  <div class="spacer" data-tauri-drag-region></div>

  {#if active && Math.abs(active.zoom - 1) > 0.001}
    <button class="zoom" onclick={onzoomreset} title="Voltar ao tamanho real ({modKey}⇧0)">
      {Math.round(active.zoom * 100)}%
    </button>
  {/if}

  {#if active}
    <button class="icon-btn" onclick={onreload} title="Recarregar ({modKey}R)" aria-label="Recarregar {active.name}">
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
    class="icon-btn solid"
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

  .link {
    border: 0;
    background: none;
    padding: 0;
    color: inherit;
    font-weight: 600;
    text-decoration: underline;
  }

  .zoom {
    height: 30px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg-subtle);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .spacer {
    flex: 1;
    align-self: stretch;
  }

  .solid {
    background: var(--primary);
    color: var(--primary-text);
    border-color: var(--primary);
  }

  .solid:hover {
    background: var(--primary);
    opacity: 0.88;
  }
</style>
