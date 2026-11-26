<script lang="ts">
  import { onMount } from "svelte";
  import {
    ArrowDown,
    ArrowUp,
    Bell,
    BellOff,
    CalendarClock,
    LogOut,
    Palette,
    RotateCw,
    Snowflake,
    Trash2,
    UserPlus,
  } from "@lucide/svelte";
  import AddServiceDialog from "./lib/AddServiceDialog.svelte";
  import CommandPalette from "./lib/CommandPalette.svelte";
  import type { SettingsTarget } from "./lib/settingsIndex";
  import ContactPanel from "./lib/ContactPanel.svelte";
  import TaskList, { type TaskActions } from "./lib/TaskList.svelte";
  import Tasks from "./lib/Tasks.svelte";
  import { groupOf } from "./lib/tasks";
  import { minuteTick } from "./lib/when";
  import ConfirmDialog from "./lib/ConfirmDialog.svelte";
  import ContextMenu from "./lib/ContextMenu.svelte";
  import CustomizeDialog from "./lib/CustomizeDialog.svelte";
  import Home from "./lib/Home.svelte";
  import LockScreen from "./lib/LockScreen.svelte";
  import Settings from "./lib/Settings.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import Topbar from "./lib/Topbar.svelte";
  import HomeStats from "./lib/HomeStats.svelte";
  import WhatsNew from "./lib/WhatsNew.svelte";
  import { bundledNotes } from "./lib/bundledNotes";
  import {
    EMPTY_ORGANIZER,
    EMPTY_SNAPSHOT,
    THEME_OPTIONS,
    api,
    errorMessage,
    isMuted,
    mediaPolicy,
    keepsUrl,
    nextMode,
    serviceColor,
    usesModes,
    onCommand,
    onDownload,
    onOrganizer,
    onState,
    fresher,
    type ChatLink,
    type Contact,
    type Organizer,
    type TaskFields,
    type MenuItem,
    type Mode,
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
  let page = $state<"home" | "settings" | "tasks">("home");
  let org = $state<Organizer>(EMPTY_ORGANIZER);
  let panelOpen = $state(store.get("papeo.panel") === "open");
  // Relógio das tarefas ("Hoje", "Atrasada"): avança a cada 30 s.
  let now = $state(new Date());
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
  let restoring = $state<{
    path: string;
    services: string[];
    tasks?: number | null;
    notes?: number | null;
  } | null>(null);
  let menu = $state<{ service: ServiceView; x: number; y: number } | null>(null);
  let toast = $state<Toast | null>(null);
  // "O que mudou": da versão disponível ou da que acabou de ser instalada.
  let whatsNew = $state<{ version: string; notes: string | null; mode: "available" | "installed" } | null>(
    null,
  );

  function showNotes(mode: "available" | "installed") {
    if (mode === "available" && snap.update.state === "available") {
      whatsNew = { version: snap.update.version, notes: snap.update.notes ?? null, mode };
    } else if (mode === "installed") {
      whatsNew = { version: snap.appVersion, notes: bundledNotes(snap.appVersion), mode };
    }
  }
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  let content = $state<HTMLElement>();

  const active = $derived(snap.services.find((s) => s.id === snap.active) ?? null);
  // Outra conta do mesmo serviço: o risco de responder pela conta errada.
  const twin = $derived(
    !!active && snap.services.some((s) => s.id !== active.id && s.kind === active.kind),
  );
  // Moldura na cor da conta em volta do site aberto.
  const frame = $derived(
    active && snap.settings.accountFrame && (active.color || active.context || twin)
      ? serviceColor(active)
      : null,
  );
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
      (!!whatsNew && !snap.locked) ||
      updateNotice,
  );

  $effect(() => {
    // Ao bloquear, fecha tudo o que estava aberto por cima.
    if (locked) {
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

  // Tarefas e anotações só com o app desbloqueado; busca só quando o
  // bloqueio muda (o estado chega a todo momento).
  const locked = $derived(snap.locked);
  $effect(() => {
    if (!ready) return;
    if (locked) org = EMPTY_ORGANIZER;
    else api.getOrganizer().then((o) => (org = o)).catch(showError);
  });

  // Com a barra recolhida, o menu de contexto não cabe à esquerda da
  // webview do serviço, que é nativa e ficaria por cima dele.
  const menuCovers = $derived(!!menu && collapsed);
  const overlay = $derived(modalOpen || menuCovers);

  $effect(() => {
    // A webview do serviço fica por cima da interface; some enquanto há um modal aberto.
    if (ready) api.setOverlay(overlay).catch(() => {});
  });

  function showToast(next: Toast, ms = 5000) {
    toast = next;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), ms);
  }

  const showError = (err: unknown) => showToast({ text: errorMessage(err) });

  /** Executa a ação; `false` se falhou (o erro já apareceu). */
  async function run(action: Promise<Snapshot | void>): Promise<boolean> {
    try {
      const next = await action;
      if (next) snap = fresher(next, snap);
      return true;
    } catch (err) {
      showError(err);
      return false;
    }
  }

  function changeMode(mode: Mode) {
    run(api.setMode(mode));
  }

  // Ao mudar de modo, a conta aberta que ficou de fora fecha (volta ao Início).
  let lastMode: Mode | null = null;
  $effect(() => {
    const mode = snap.mode;
    if (lastMode !== null && mode !== lastMode && active && !active.inMode) select(null);
    lastMode = mode;
  });

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

  function openTasks() {
    paletteOpen = false;
    page = "tasks";
    if (snap.active !== null) run(api.activate(null));
  }

  function togglePanel() {
    panelOpen = !panelOpen;
    store.set("papeo.panel", panelOpen ? "open" : "closed");
  }

  let panel = $state<ContactPanel>();

  /** Grava a anotação em edição antes de bloquear (bloqueado, não grava). */
  async function lockApp() {
    await panel?.flush();
    run(api.lockNow());
  }

  async function orgRun(action: Promise<Organizer>) {
    try {
      org = await action;
    } catch (err) {
      showError(err);
      throw err;
    }
  }

  const quiet = (p: Promise<void>) => p.catch(() => {});

  /** Ações das listas de tarefas (Início, página Tarefas, painel). */
  const taskActions: TaskActions = {
    done: (task, done) => quiet(orgRun(api.setTaskDone(task.id, done, task))),
    snooze: (id, until) => quiet(orgRun(api.snoozeTask(id, until))),
    remove: (id) => {
      quiet(orgRun(api.deleteTask(id)));
    },
    edit: (id, fields) => orgRun(api.saveTask(id, fields)),
    open: (contact: Contact) => {
      if (!snap.services.some((s) => s.id === contact.serviceId)) return;
      if (!panelOpen) togglePanel();
      if (contact.chat) openChat(contact.serviceId, contact.chat, true);
      else select(contact.serviceId);
    },
  };

  async function createTask(fields: TaskFields) {
    await orgRun(api.saveTask(null, fields));
    if (!fields.link && !fields.contactId && active) {
      showToast({ text: "Tarefa criada sem conversa: está em Tarefas." });
    }
  }

  const replyLater = (serviceId: string, chat: string, at: number) =>
    orgRun(api.replyLater(serviceId, chat, at));

  const saveNote = (link: ChatLink, note: string) => orgRun(api.saveNote({ link }, note));

  const todayTasks = $derived(
    org.tasks.filter((t) => {
      const g = groupOf(t, now);
      return g === "overdue" || g === "today";
    }),
  );

  /** Opção das Configurações a mostrar (vinda da busca); objeto novo a cada pedido. */
  let settingsTarget = $state<SettingsTarget | null>(null);

  function openSettings(target?: SettingsTarget) {
    settingsTarget = target ?? null;
    paletteOpen = false;
    page = "settings";
    if (snap.active !== null) run(api.activate(null));
  }

  // Durante o redimensionamento chegam vários avisos por quadro: envia no
  // máximo um por quadro e só quando a área mudou.
  let areaFrame = 0;
  let lastArea = "";
  function reportArea() {
    if (areaFrame) return;
    areaFrame = requestAnimationFrame(() => {
      areaFrame = 0;
      if (!content) return;
      // Sem a borda (moldura da conta): a webview fica por dentro dela.
      const r = content.getBoundingClientRect();
      const area = {
        x: r.left + content.clientLeft,
        y: r.top + content.clientTop,
        width: content.clientWidth,
        height: content.clientHeight,
      };
      const key = JSON.stringify(area);
      if (key === lastArea) return;
      lastArea = key;
      api.setContentArea(area).catch(() => (lastArea = ""));
    });
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
    snap = fresher(await api.addService(kind, name || undefined, url || undefined), snap);
    addOpen = false;
  }

  function zoom(change: { zoom?: number; delta?: number }) {
    if (active) run(api.setZoom(active.id, change));
  }

  function menuItems(service: ServiceView): MenuItem[] {
    const index = snap.services.findIndex((s) => s.id === service.id);
    // Vizinhos visíveis na barra (contas fora do modo ficam ocultas).
    const shown = (s: ServiceView) => s.inMode || s.id === snap.active;
    const above = snap.services.map((s, i) => (i < index && shown(s) ? i : -1)).reduce((a, b) => Math.max(a, b), -1);
    const below = snap.services.findIndex((s, i) => i > index && shown(s));
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
    if (above >= 0) {
      items.push({ label: "Mover para cima", icon: ArrowUp, run: () => run(api.moveService(service.id, above)) });
    }
    if (below >= 0) {
      items.push({
        label: "Mover para baixo",
        icon: ArrowDown,
        run: () => run(api.moveService(service.id, below)),
      });
    }
    items.push(
      {
        label: "Adicionar outra conta",
        icon: UserPlus,
        run: () => run(api.addService(service.kind, undefined, keepsUrl(service.kind) ? service.url : undefined)),
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
    else if (cmd === "tasks") openTasks();
    else if (cmd === "panel") {
      if (active) togglePanel();
      else openTasks();
    }
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
    if (e.shiftKey && key === "n") {
      e.preventDefault();
      handleCommand("panel");
    } else if (e.shiftKey && (e.code === "Digit0" || key === ")")) {
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
      lockApp();
    } else if (e.shiftKey && key === "m" && usesModes(snap)) {
      e.preventDefault();
      changeMode(nextMode(snap.mode));
    } else if (/^[0-9]$/.test(key)) {
      e.preventDefault();
      const n = Number(key);
      // Mesma numeração da barra lateral: só as contas do modo atual.
      const inMode = snap.services.filter((s) => s.inMode);
      if (n === 0) select(null);
      else if (inMode[n - 1]) select(inMode[n - 1].id);
    }
  }

  onMount(() => {
    const unlisten = [
      onState((s) => (snap = fresher(s, snap))),
      onOrganizer((o) => {
        if (!snap.locked) org = o;
      }),
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
        snap = fresher(s, snap);
        ready = true;
        // Versões antigas guardavam o tema só na interface: leva para o app.
        const legacy = store.get("papeo.theme");
        if ((legacy === "light" || legacy === "dark") && s.settings.appearance === "system") {
          run(api.setAppearance(legacy));
        }
        store.remove("papeo.theme");
        // Primeira abertura depois de atualizar: mostra o que mudou (uma vez
        // por versão). Instalação nova não mostra: ainda não viu o aviso das
        // atualizações.
        if (s.appVersion && store.get("papeo.notesSeen") !== s.appVersion) {
          const notes = bundledNotes(s.appVersion);
          if (notes && s.settings.updates.noticeSeen) {
            whatsNew = { version: s.appVersion, notes, mode: "installed" };
          }
          store.set("papeo.notesSeen", s.appVersion);
        }
      })
      .catch(showError);

    const onScheme = (e: MediaQueryListEvent) => (systemIsDark = e.matches);
    systemDark.addEventListener("change", onScheme);

    const clock = setInterval(() => (now = minuteTick(now)), 30_000);
    const observer = new ResizeObserver(reportArea);
    if (content) observer.observe(content);
    window.addEventListener("resize", reportArea);

    return () => {
      unlisten.forEach((p) => p.then((off) => off()));
      systemDark.removeEventListener("change", onScheme);
      observer.disconnect();
      clearInterval(clock);
      cancelAnimationFrame(areaFrame);
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
    onsettings={() => openSettings()}
    onmenu={(service, x, y) => (menu = { service, x, y })}
    onmove={(id, to) => run(api.moveService(id, to))}
    onupdate={() => run(api.installUpdate())}
    onnotes={() => showNotes("available")}
    ontasks={openTasks}
    onmode={changeMode}
    onautomode={() => run(api.setModeSchedule(snap.settings.mode, snap.settings.workHours))}
  />

  <main class="main">
    <Topbar
      {active}
      {appearance}
      lockable={snap.settings.lock.enabled && snap.lockAvailable}
      onlock={lockApp}
      ontogglesidebar={toggleSidebar}
      onsearch={() => (paletteOpen = true)}
      ontheme={cycleTheme}
      onreload={() => active && run(api.reload(active.id))}
      onhibernate={() => active && run(api.hibernate(active.id))}
      onzoom={zoom}
      {panelOpen}
      onpanel={togglePanel}
      extra={!active && page === "home" && snap.services.length ? homeStats : undefined}
      {twin}
    />
    {#snippet homeStats()}
      <HomeStats {snap} {run} onopen={select} />
    {/snippet}
    <div class="body">
    <section
      class="content"
      class:framed={!!frame}
      style:--frame={frame}
      bind:this={content}
      aria-live="polite"
    >
      {#if active}
        <div class="placeholder">
          {#if menuCovers && !modalOpen}
            {active.name} oculto enquanto o menu está aberto.
          {:else if modalOpen}
            {active.name} oculto enquanto a janela de diálogo está aberta.
          {:else}
            Carregando {active.name}…
          {/if}
        </div>
      {:else if page === "settings"}
        <Settings
          {snap}
          {run}
          onrestore={(path, preview) => (restoring = { path, ...preview })}
          onnotes={showNotes}
          target={settingsTarget}
        />
      {:else if page === "tasks"}
        <Tasks {org} services={snap.services} actions={taskActions} {now} oncreate={createTask} />
      {:else}
        <Home
          {snap}
          {run}
          onadd={openAdd}
          onopen={select}
          onremove={(service) => (confirmRemove = service)}
          oncustomize={(service) => (customizing = service)}
          onchat={(id, chat) => openChat(id, chat, true)}
          today={todayTasks.length ? todayCard : undefined}
          oncreatetask={createTask}
          clock={now}
          onlater={replyLater}
        />
      {/if}
    </section>
    {#if active && panelOpen && !snap.locked}
      <ContactPanel
        bind:this={panel}
        service={active}
        {org}
        actions={taskActions}
        {now}
        oncreate={createTask}
        onnote={saveNote}
        onclose={togglePanel}
      />
    {/if}
    </div>
    {#snippet todayCard()}
      <section class="card today" aria-labelledby="today-title">
        <div class="today-head">
          <h2 id="today-title"><CalendarClock size={18} /> Para hoje</h2>
          <button class="link-btn" onclick={openTasks}>Ver todas</button>
        </div>
        <TaskList
          tasks={todayTasks.slice(0, 5)}
          {org}
          services={snap.services}
          actions={taskActions}
          {now}
        />
      </section>
    {/snippet}
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
    {org}
    ontasks={openTasks}
    oncontact={taskActions.open}
    onsetting={openSettings}
  />
{/if}

{#if confirmRemove}
  {@const service = confirmRemove}
  <ConfirmDialog
    title="Remover {service.name}?"
    message="A sessão deste serviço será apagada. Para usá-lo de novo, será preciso fazer login outra vez. Tarefas e anotações ligadas a ele continuam em Tarefas."
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
      if (await run(api.clearServiceData(id))) {
        showToast({ text: `Dados de ${name} apagados deste computador.` });
      }
    }}
  />
{/if}

{#if restoring}
  {@const backup = restoring}
  <ConfirmDialog
    title="Restaurar backup?"
    message={`Os serviços atuais serão substituídos por: ${backup.services.join(", ") || "nenhum serviço"}. ${
      backup.tasks != null
        ? `Tarefas e anotações também: ${backup.tasks} ${backup.tasks === 1 ? "tarefa aberta" : "tarefas abertas"} e ${backup.notes ?? 0} ${backup.notes === 1 ? "anotação" : "anotações"}. `
        : "As tarefas atuais continuam (este backup não tem tarefas). "
    }Logins que já existem neste computador continuam; os outros pedem login de novo.`}
    confirmLabel="Restaurar"
    onclose={() => (restoring = null)}
    onconfirm={async () => {
      const { path } = backup;
      restoring = null;
      if (await run(api.importConfig(path))) {
        page = "home";
        showToast({ text: "Configuração restaurada." });
      }
    }}
  />
{/if}

{#if customizing}
  {@const target = customizing}
  <CustomizeDialog
    service={target}
    onclose={() => (customizing = null)}
    onsubmit={async (name, color, emoji, media, context) => {
      // Erros sobem para o diálogo, que os mostra no formulário.
      snap = fresher(await api.customizeService(target.id, name, color, emoji, context), snap);
      if (media !== mediaPolicy(target)) snap = fresher(await api.setMediaPolicy(target.id, media), snap);
      customizing = null;
    }}
  />
{/if}

{#if updateNotice && !addOpen && !paletteOpen && !confirmRemove && !confirmClear && !customizing && !restoring}
  <ConfirmDialog
    title="Verificar atualizações?"
    message="A cada 2 horas, o Papeo consulta o GitHub para saber se há versão nova. O GitHub vê o seu endereço IP; nada mais é enviado. A instalação só acontece quando você clica em Atualizar. Dá para mudar em Configurações → Sistema."
    confirmLabel="Manter ligada"
    cancelLabel="Desligar"
    danger={false}
    onclose={() => run(api.setUpdates(true))}
    oncancel={() => run(api.setUpdates(false))}
    onconfirm={() => run(api.setUpdates(true))}
  />
{/if}

{#if whatsNew && !snap.locked}
  <!-- Trocar de "instalada" para "disponível" com o modal aberto recria o
       componente (a versão selecionada no histórico recomeça). -->
  {#key `${whatsNew.mode}-${whatsNew.version}`}
  <WhatsNew
    version={whatsNew.version}
    notes={whatsNew.notes}
    mode={whatsNew.mode}
    current={snap.appVersion}
    onclose={() => (whatsNew = null)}
    onupdate={() => {
      whatsNew = null;
      run(api.installUpdate());
    }}
  />
  {/key}
{/if}

{#if snap.locked}
  <LockScreen
    method={snap.lockMethod}
    onunlock={async (pin) => {
      snap = fresher(await api.unlock(pin), snap);
    }}
    onforget={async () => {
      snap = fresher(await api.forgetLockPin(), snap);
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

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    position: relative;
  }

  .content.framed {
    border: 3px solid var(--frame);
    border-top-width: 2px;
  }

  .today {
    padding: 14px 12px 10px;
  }

  .today-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 0 8px 6px;
  }

  .today-head h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }

  .link-btn {
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 13px;
  }

  .link-btn:hover {
    text-decoration: underline;
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
