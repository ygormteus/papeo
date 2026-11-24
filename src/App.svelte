<script lang="ts">
  import { onMount } from "svelte";
  import { Bell, BellOff, Pencil, RotateCw, Snowflake, Trash2, UserPlus } from "@lucide/svelte";
  import AddServiceDialog from "./lib/AddServiceDialog.svelte";
  import CommandPalette from "./lib/CommandPalette.svelte";
  import ConfirmDialog from "./lib/ConfirmDialog.svelte";
  import ContextMenu from "./lib/ContextMenu.svelte";
  import Home from "./lib/Home.svelte";
  import Modal from "./lib/Modal.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import Topbar from "./lib/Topbar.svelte";
  import {
    api,
    errorMessage,
    onCommand,
    onState,
    type MenuItem,
    type ServiceKind,
    type ServiceView,
    type Snapshot,
  } from "./lib/api";

  const store = {
    get(key: string) {
      try {
        return localStorage.getItem(key);
      } catch {
        return null;
      }
    },
    set(key: string, value: string) {
      try {
        localStorage.setItem(key, value);
      } catch {
        /* preferência local apenas */
      }
    },
  };

  let snap = $state<Snapshot>({ services: [], active: null, totalUnread: 0 });
  let ready = $state(false);
  let collapsed = $state(store.get("papeo.sidebar") === "collapsed");
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  let themePref = $state(store.get("papeo.theme") ?? "system");
  let systemIsDark = $state(systemDark.matches);
  const dark = $derived(themePref === "dark" || (themePref === "system" && systemIsDark));

  let addOpen = $state(false);
  let paletteOpen = $state(false);
  let confirmRemove = $state<ServiceView | null>(null);
  let renaming = $state<{ service: ServiceView; name: string } | null>(null);
  let menu = $state<{ service: ServiceView; x: number; y: number } | null>(null);
  let toast = $state("");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  let content = $state<HTMLElement>();

  const active = $derived(snap.services.find((s) => s.id === snap.active) ?? null);
  const modalOpen = $derived(addOpen || paletteOpen || !!confirmRemove || !!renaming);

  $effect(() => {
    document.documentElement.dataset.theme = dark ? "dark" : "light";
  });

  $effect(() => {
    // A webview do serviço fica por cima da interface; some enquanto há um modal aberto.
    if (ready) api.setOverlay(modalOpen).catch(() => {});
  });

  function showError(err: unknown) {
    toast = errorMessage(err);
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 5000);
  }

  async function run(action: Promise<Snapshot | void>) {
    try {
      const next = await action;
      if (next) snap = next;
    } catch (err) {
      showError(err);
    }
  }

  const select = (id: string | null) => {
    paletteOpen = false;
    run(api.activate(id));
  };

  function reportArea() {
    if (!content) return;
    const r = content.getBoundingClientRect();
    api.setContentArea({ x: r.left, y: r.top, width: r.width, height: r.height }).catch(() => {});
  }

  function toggleTheme() {
    themePref = dark ? "light" : "dark";
    store.set("papeo.theme", themePref);
  }

  function toggleSidebar() {
    collapsed = !collapsed;
    store.set("papeo.sidebar", collapsed ? "collapsed" : "expanded");
  }

  async function addService(kind: ServiceKind, name: string, url: string) {
    // Erros sobem para o diálogo, que os mostra no próprio formulário.
    snap = await api.addService(kind, name || undefined, url || undefined);
    addOpen = false;
  }

  function menuItems(service: ServiceView): MenuItem[] {
    return [
      { label: "Recarregar", icon: RotateCw, run: () => run(api.reload(service.id)) },
      { label: "Hibernar agora", icon: Snowflake, run: () => run(api.hibernate(service.id)) },
      service.notifications
        ? { label: "Silenciar notificações", icon: BellOff, run: () => run(api.setNotifications(service.id, false)) }
        : { label: "Ativar notificações", icon: Bell, run: () => run(api.setNotifications(service.id, true)) },
      {
        label: "Adicionar outra conta",
        icon: UserPlus,
        run: () => run(api.addService(service.kind, undefined, service.kind === "custom" ? service.url : undefined)),
      },
      { label: "Renomear…", icon: Pencil, run: () => (renaming = { service, name: service.name }) },
      { label: "Remover…", icon: Trash2, danger: true, run: () => (confirmRemove = service) },
    ];
  }

  function openAdd() {
    paletteOpen = false;
    addOpen = true;
  }

  function keydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (!mod || e.altKey || modalOpen) return;
    const key = e.key.toLowerCase();
    if (key === "k") {
      e.preventDefault();
      paletteOpen = true;
    } else if (key === "n") {
      e.preventDefault();
      openAdd();
    } else if (/^[0-9]$/.test(key)) {
      e.preventDefault();
      const n = Number(key);
      if (n === 0) select(null);
      else if (snap.services[n - 1]) select(snap.services[n - 1].id);
    }
  }

  onMount(() => {
    const unlisten = [
      onState((s) => (snap = s)),
      onCommand((cmd) => {
        if (cmd === "search" && !modalOpen) paletteOpen = true;
      }),
    ];
    api
      .getState()
      .then((s) => {
        snap = s;
        ready = true;
      })
      .catch(showError);

    const onScheme = (e: MediaQueryListEvent) => (systemIsDark = e.matches);
    systemDark.addEventListener("change", onScheme);

    const observer = new ResizeObserver(reportArea);
    if (content) observer.observe(content);
    window.addEventListener("resize", reportArea);

    return () => {
      unlisten.forEach((p) => p.then((off) => off()));
      systemDark.removeEventListener("change", onScheme);
      observer.disconnect();
      window.removeEventListener("resize", reportArea);
    };
  });
</script>

<svelte:window onkeydown={keydown} />

<div class="shell">
  <Sidebar
    {snap}
    {collapsed}
    onselect={select}
    onadd={openAdd}
    onmenu={(service, x, y) => (menu = { service, x, y })}
  />

  <main class="main">
    <Topbar
      {active}
      {dark}
      ontogglesidebar={toggleSidebar}
      onsearch={() => (paletteOpen = true)}
      ontheme={toggleTheme}
      onreload={() => active && run(api.reload(active.id))}
      onhibernate={() => active && run(api.hibernate(active.id))}
    />
    <section class="content" bind:this={content} aria-live="polite">
      {#if active}
        <div class="placeholder">
          {#if modalOpen}
            {active.name} oculto enquanto a janela de diálogo está aberta.
          {:else}
            Carregando {active.name}…
          {/if}
        </div>
      {:else}
        <Home
          {snap}
          onadd={openAdd}
          onopen={select}
          onhibernate={(id) => run(api.hibernate(id))}
          onremove={(service) => (confirmRemove = service)}
          onsethibernation={(id, minutes) => run(api.setHibernation(id, minutes))}
          onsetnotifications={(id, enabled) => run(api.setNotifications(id, enabled))}
        />
      {/if}
    </section>
  </main>
</div>

{#if menu}
  <ContextMenu
    x={menu.x}
    y={menu.y}
    maxX={collapsed ? 260 : Math.max(260, content?.getBoundingClientRect().left ?? 260)}
    items={menuItems(menu.service)}
    onclose={() => (menu = null)}
  />
{/if}

{#if addOpen}
  <AddServiceDialog onclose={() => (addOpen = false)} onsubmit={addService} />
{/if}

{#if paletteOpen}
  <CommandPalette
    services={snap.services}
    onclose={() => (paletteOpen = false)}
    onselect={select}
    onadd={openAdd}
  />
{/if}

{#if confirmRemove}
  {@const service = confirmRemove}
  <ConfirmDialog
    title="Remover {service.name}?"
    message="A sessão deste serviço será apagada. Para usá-lo de novo, será preciso fazer login outra vez."
    confirmLabel="Remover"
    onclose={() => (confirmRemove = null)}
    onconfirm={() => {
      confirmRemove = null;
      run(api.removeService(service.id));
    }}
  />
{/if}

{#if renaming}
  {@const target = renaming}
  <Modal label="Renomear serviço" width={380} onclose={() => (renaming = null)}>
    <form
      class="rename"
      onsubmit={(e) => {
        e.preventDefault();
        renaming = null;
        run(api.renameService(target.service.id, target.name));
      }}
    >
      <h2>Renomear</h2>
      <div class="field">
        <label for="rename-input">Nome</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="rename-input" class="input" bind:value={target.name} autofocus />
      </div>
      <div class="rename-actions">
        <button type="button" class="btn" onclick={() => (renaming = null)}>Cancelar</button>
        <button type="submit" class="btn btn-primary" disabled={!target.name.trim()}>Salvar</button>
      </div>
    </form>
  </Modal>
{/if}

{#if toast}
  <div class="toast" role="alert">{toast}</div>
{/if}

<style>
  .shell {
    display: flex;
    height: 100%;
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .content {
    flex: 1;
    min-height: 0;
    position: relative;
  }

  .placeholder {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--text-muted);
  }

  .rename {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .rename h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }

  .rename-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .toast {
    position: fixed;
    left: 50%;
    bottom: 20px;
    transform: translateX(-50%);
    z-index: 60;
    padding: 10px 16px;
    border-radius: 10px;
    background: var(--primary);
    color: var(--primary-text);
    box-shadow: var(--shadow-pop);
  }
</style>
