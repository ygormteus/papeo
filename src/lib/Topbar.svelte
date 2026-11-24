<script lang="ts">
  import { Moon, PanelLeft, RotateCw, Search, Snowflake, Sun } from "@lucide/svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import { modKey, type ServiceView } from "./api";

  let {
    active,
    dark,
    ontogglesidebar,
    onsearch,
    ontheme,
    onreload,
    onhibernate,
  }: {
    active: ServiceView | null;
    dark: boolean;
    ontogglesidebar: () => void;
    onsearch: () => void;
    ontheme: () => void;
    onreload: () => void;
    onhibernate: () => void;
  } = $props();
</script>

<header class="topbar" data-tauri-drag-region>
  <button class="icon-btn ghost" onclick={ontogglesidebar} aria-label="Recolher ou expandir a barra lateral">
    <PanelLeft size={18} />
  </button>
  <span class="divider" aria-hidden="true"></span>
  <button class="search" onclick={onsearch} aria-label="Buscar serviço">
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

  <div class="spacer" data-tauri-drag-region></div>

  {#if active}
    <button class="icon-btn" onclick={onreload} title="Recarregar ({modKey}R)" aria-label="Recarregar {active.name}">
      <RotateCw size={17} />
    </button>
    <button class="icon-btn" onclick={onhibernate} title="Hibernar e liberar memória" aria-label="Hibernar {active.name}">
      <Snowflake size={17} />
    </button>
  {/if}
  <button class="icon-btn solid" onclick={ontheme} aria-label={dark ? "Usar tema claro" : "Usar tema escuro"}>
    {#if dark}<Sun size={17} />{:else}<Moon size={17} />{/if}
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
