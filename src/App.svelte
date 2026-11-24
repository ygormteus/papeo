<script lang="ts">
  import { onMount } from "svelte";
  import {
    ArrowDown,
    ArrowUp,
    Bell,
    BellOff,
    LogOut,
    Palette,
    RotateCw,
    Snowflake,
    Trash2,
    UserPlus,
  } from "@lucide/svelte";
  import AddServiceDialog from "./lib/AddServiceDialog.svelte";
  import CommandPalette from "./lib/CommandPalette.svelte";
  import ConfirmDialog from "./lib/ConfirmDialog.svelte";
  import ContextMenu from "./lib/ContextMenu.svelte";
  import CustomizeDialog from "./lib/CustomizeDialog.svelte";
  import Home from "./lib/Home.svelte";
  import LockScreen from "./lib/LockScreen.svelte";
  import Settings from "./lib/Settings.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import Topbar from "./lib/Topbar.svelte";
  import {
    EMPTY_SNAPSHOT,
    THEME_OPTIONS,
    api,
    errorMessage,
    isMuted,
    mediaPolicy,
    onCommand,
    onDownload,
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
    remove(key: string) {
      try {
        localStorage.removeItem(key);
      } catch {
        /* preferência local apenas */
      }
    },
  };

  type Toast = { text: string; action?: { label: string; run: () => void } };

  let snap = $state<Snapshot>(EMPTY_SNAPSHOT);
  let ready = $state(false);
  let page = $state<"home" | "settings">("home");
  let collapsed = $state(store.get("papeo.sidebar") === "collapsed");
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  let systemIsDark = $state(systemDark.matches);
  // O tema fica nas configurações do app (o backend aplica na janela, e os
  // sites dos serviços seguem o claro/escuro dela).
  const appearance = $derived(snap.settings.appearance);
  const resolvedTheme = $derived(
    appearance === "system" ? (systemIsDark ? "dark" : "light") : appearance,
  );

  let addOpen = $state(false);
  let paletteOpen = $state(false);
  let confirmRemove = $state<ServiceView | null>(null);
  let confirmClear = $state<ServiceView | null>(null);
  let customizing = $state<ServiceView | null>(null);
  let restoring = $state<{ path: string; services: string[] } | null>(null);
  let menu = $state<{ service: ServiceView; x: number; y: number } | null>(null);
  let toast = $state<Toast | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  let content = $state<HTMLElement>();

  const active = $derived(snap.services.find((s) => s.id === snap.active) ?? null);
  // Aviso único de que a verificação de atualizações consulta o GitHub;
  // até a resposta, nada é consultado automaticamente.
  const updateNotice = $derived(ready && !snap.locked && !snap.settings.updates.noticeSeen);
  const modalOpen = $derived(
    addOpen ||
      paletteOpen ||
      !!confirmRemove ||
      !!confirmClear ||
      !!customizing ||
      !!restoring ||
      updateNotice,
  );

  $effect(() => {
    // Ao bloquear, fecha tudo o que estava aberto por cima.
    if (snap.locked) {
      menu = null;
      addOpen = paletteOpen = false;
      confirmRemove = confirmClear = customizing = null;
      restoring = null;
      toast = null;
    }
  });

  $effect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
  });

  $effect(() => {
    // A webview do serviço fica por cima da interface; some enquanto há um modal aberto.
    if (ready) api.setOverlay(modalOpen).catch(() => {});
  });

  function showToast(next: Toast, ms = 5000) {
    toast = next;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), ms);
  }

  const showError = (err: unknown) => showToast({ text: errorMessage(err) });

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
    if (id === null) page = "home";
    run(api.activate(id));
  };

  /** Abre a conta e procura a conversa na busca do próprio WhatsApp/Telegram. */
  function openChat(serviceId: string, query: string, exact: boolean) {
    paletteOpen = false;
    run(api.openChat(serviceId, query, exact));
  }

  function openSettings() {
    page = "settings";
    if (snap.active !== null) run(api.activate(null));
  }

  function reportArea() {
    if (!content) return;
    const r = content.getBoundingClientRect();
    api.setContentArea({ x: r.left, y: r.top, width: r.width, height: r.height }).catch(() => {});
  }

  /** Botão da barra superior: Sistema → Claro → Escuro → OLED → Sistema. */
  function cycleTheme() {
    const i = THEME_OPTIONS.findIndex((o) => o.value === appearance);
    run(api.setAppearance(THEME_OPTIONS[(i + 1) % THEME_OPTIONS.length].value));
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

  function zoom(change: { zoom?: number; delta?: number }) {
    if (active) run(api.setZoom(active.id, change));
  }

  function menuItems(service: ServiceView): MenuItem[] {
    const index = snap.services.findIndex((s) => s.id === service.id);
    const items: MenuItem[] = [
      { label: "Recarregar", icon: RotateCw, run: () => run(api.reload(service.id)) },
      { label: "Hibernar agora", icon: Snowflake, run: () => run(api.hibernate(service.id)) },
    ];
    if (isMuted(service)) {
      items.push({
        label: "Reativar notificações",
        icon: Bell,
        run: () => run(api.setNotifications(service.id, true)),
      });
    } else {
      items.push(
        { label: "Silenciar por 1 hora", icon: BellOff, run: () => run(api.muteService(service.id, 60)) },
        { label: "Silenciar por 8 horas", icon: BellOff, run: () => run(api.muteService(service.id, 480)) },
        {
          label: "Silenciar sempre",
          icon: BellOff,
          run: () => run(api.setNotifications(service.id, false)),
        },
      );
    }
    if (index > 0) {
      items.push({ label: "Mover para cima", icon: ArrowUp, run: () => run(api.moveService(service.id, index - 1)) });
    }
    if (index < snap.services.length - 1) {
      items.push({
        label: "Mover para baixo",
        icon: ArrowDown,
        run: () => run(api.moveService(service.id, index + 1)),
      });
    }
    items.push(
      {
        label: "Adicionar outra conta",
        icon: UserPlus,
        run: () => run(api.addService(service.kind, undefined, service.kind === "custom" ? service.url : undefined)),
      },
      { label: "Personalizar…", icon: Palette, run: () => (customizing = service) },
      { label: "Sair e apagar dados…", icon: LogOut, danger: true, run: () => (confirmClear = service) },
      { label: "Remover…", icon: Trash2, danger: true, run: () => (confirmRemove = service) },
    );
    return items;
  }

  function openAdd() {
    paletteOpen = false;
    addOpen = true;
  }

  function handleCommand(cmd: string) {
    if (modalOpen || snap.locked) return;
    if (cmd === "search") paletteOpen = true;
    else if (cmd === "settings") openSettings();
    else if (cmd === "zoom:in") zoom({ delta: 0.1 });
    else if (cmd === "zoom:out") zoom({ delta: -0.1 });
    else if (cmd === "zoom:reset") zoom({ zoom: 1 });
  }

  function keydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (!mod || e.altKey || modalOpen || snap.locked) return;
    const key = e.key.toLowerCase();
    const commands: Record<string, string> = {
      k: "search",
      ",": "settings",
      "=": "zoom:in",
      "+": "zoom:in",
      "-": "zoom:out",
    };
    if (e.shiftKey && (e.code === "Digit0" || key === ")")) {
      e.preventDefault();
      handleCommand("zoom:reset");
    } else if (commands[key]) {
      e.preventDefault();
      handleCommand(commands[key]);
    } else if (key === "n") {
      e.preventDefault();
      openAdd();
    } else if (key === "l" && snap.settings.lock.enabled) {
      e.preventDefault();
      run(api.lockNow());
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
      onCommand(handleCommand),
      onDownload((d) =>
        !snap.locked &&
        showToast(
          d.success
            ? {
                text: `Download concluído: ${d.fileName}`,
                action: { label: "Mostrar na pasta", run: () => run(api.revealDownload(d.id)) },
              }
            : { text: `Download falhou: ${d.fileName}` },
          8000,
        ),
      ),
    ];
    api
      .getState()
      .then((s) => {
        snap = s;
        ready = true;
        // Versões antigas guardavam o tema só na interface: leva para o app.
        const legacy = store.get("papeo.theme");
        if ((legacy === "light" || legacy === "dark") && s.settings.appearance === "system") {
          run(api.setAppearance(legacy));
        }
        store.remove("papeo.theme");
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

<!-- Bloqueado: nada atrás da tela de desbloqueio recebe foco ou cliques. -->
<div class="shell" inert={snap.locked}>
  <Sidebar
    {snap}
    {collapsed}
    {page}
    onselect={select}
    onadd={openAdd}
    onsettings={openSettings}
    onmenu={(service, x, y) => (menu = { service, x, y })}
    onmove={(id, to) => run(api.moveService(id, to))}
    onupdate={() => run(api.installUpdate())}
  />

  <main class="main">
    <Topbar
      {active}
      {appearance}
      lockable={snap.settings.lock.enabled && snap.lockAvailable}
      onlock={() => run(api.lockNow())}
      ontogglesidebar={toggleSidebar}
      onsearch={() => (paletteOpen = true)}
      ontheme={cycleTheme}
      onreload={() => active && run(api.reload(active.id))}
      onhibernate={() => active && run(api.hibernate(active.id))}
      onzoomreset={() => zoom({ zoom: 1 })}
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
      {:else if page === "settings"}
        <Settings {snap} {run} onrestore={(path, services) => (restoring = { path, services })} />
      {:else}
        <Home
          {snap}
          {run}
          onadd={openAdd}
          onopen={select}
          onremove={(service) => (confirmRemove = service)}
          oncustomize={(service) => (customizing = service)}
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
    recent={snap.recent}
    onclose={() => (paletteOpen = false)}
    onselect={select}
    onadd={openAdd}
    onchat={openChat}
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
      // `{@const}` é derivado: lê o id antes de fechar o diálogo.
      const { id } = service;
      confirmRemove = null;
      run(api.removeService(id));
    }}
  />
{/if}

{#if confirmClear}
  {@const service = confirmClear}
  <ConfirmDialog
    title="Sair de {service.name} e apagar dados?"
    message="Apaga deste computador os cookies, o cache e as conversas guardadas desta conta. O serviço continua na lista e pede login de novo."
    confirmLabel="Sair e apagar"
    onclose={() => (confirmClear = null)}
    onconfirm={async () => {
      const { id, name } = service;
      confirmClear = null;
      await run(api.clearServiceData(id));
      showToast({ text: `Dados de ${name} apagados deste computador.` });
    }}
  />
{/if}

{#if restoring}
  {@const backup = restoring}
  <ConfirmDialog
    title="Restaurar backup?"
    message={`Os serviços atuais serão substituídos por: ${backup.services.join(", ") || "nenhum serviço"}. Logins que já existem neste computador continuam; os outros pedem login de novo.`}
    confirmLabel="Restaurar"
    onclose={() => (restoring = null)}
    onconfirm={async () => {
      const { path } = backup;
      restoring = null;
      await run(api.importConfig(path));
      page = "home";
      showToast({ text: "Configuração restaurada." });
    }}
  />
{/if}

{#if customizing}
  {@const target = customizing}
  <CustomizeDialog
    service={target}
    onclose={() => (customizing = null)}
    onsubmit={async (name, color, emoji, media) => {
      // Erros sobem para o diálogo, que os mostra no formulário.
      snap = await api.customizeService(target.id, name, color, emoji);
      if (media !== mediaPolicy(target)) snap = await api.setMediaPolicy(target.id, media);
      customizing = null;
    }}
  />
{/if}

{#if updateNotice && !addOpen && !paletteOpen && !confirmRemove && !confirmClear && !customizing && !restoring}
  <ConfirmDialog
    title="Verificar atualizações?"
    message="Uma vez por dia, o Papeo consulta o GitHub para saber se há versão nova. O GitHub vê o seu endereço IP; nada mais é enviado. A instalação só acontece quando você clica em Atualizar. Dá para mudar em Configurações → Sistema."
    confirmLabel="Manter ligada"
    cancelLabel="Desligar"
    danger={false}
    onclose={() => run(api.setUpdates(true))}
    oncancel={() => run(api.setUpdates(false))}
    onconfirm={() => run(api.setUpdates(true))}
  />
{/if}

{#if snap.locked}
  <LockScreen
    onunlock={async () => {
      snap = await api.unlock();
    }}
  />
{/if}

{#if toast}
  <!-- No topo: a área de conteúdo pode estar coberta pela webview do serviço. -->
  <div class="toast" role="status">
    <span>{toast.text}</span>
    {#if toast.action}
      {@const action = toast.action}
      <button
        class="toast-action"
        onclick={() => {
          action.run();
          toast = null;
        }}
      >
        {action.label}
      </button>
    {/if}
  </div>
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

  .toast {
    position: fixed;
    top: 9px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 60;
    display: flex;
    align-items: center;
    gap: 14px;
    max-width: min(560px, calc(100vw - 32px));
    padding: 8px 10px 8px 16px;
    border-radius: 10px;
    background: var(--primary);
    color: var(--primary-text);
    box-shadow: var(--shadow-pop);
    font-size: 13.5px;
  }

  .toast span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .toast-action {
    flex: none;
    height: 28px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: color-mix(in srgb, var(--primary-text) 16%, transparent);
    color: inherit;
    font-weight: 600;
  }
</style>
