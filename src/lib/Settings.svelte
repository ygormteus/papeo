<script module lang="ts">
  /** Última aba aberta: volta nela ao reabrir as configurações (só nesta sessão). */
  let lastTab = "geral";
</script>

<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { TAB_LABEL, type SettingsTab, type SettingsTarget } from "./settingsIndex";
  import SyncSettings from "./SyncSettings.svelte";
  import {
    Activity,
    BellOff,
    Briefcase,
    Cloud,
    Bug,
    CodeXml,
    Download,
    Info,
    FolderOpen,
    HardDrive,
    Link,
    Mail,
    MemoryStick,
    Mic,
    Power,
    RefreshCw,
    Monitor,
    Palette,
    ShieldCheck,
    SlidersHorizontal,
    Upload,
    UserRound,
  } from "@lucide/svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import PinDialog from "./PinDialog.svelte";
  import Switch from "./Switch.svelte";
  import QuickReplies from "./QuickReplies.svelte";
  import { home, setConfirmFree } from "./homeState.svelte";
  import { notesHistory } from "./bundledNotes";
  import { dateKey } from "./tasks";
  import {
    ABOUT,
    LOCK_OPTIONS,
    MODE_CHOICE_OPTIONS,
    PREVIEW_OPTIONS,
    RELOAD_OPTIONS,
    UNDO_OPTIONS,
    LINK_OPTIONS,
    THEME_OPTIONS,
    api,
    errorMessage,
    isMac,
    isWindows,
    modKey,
    formatClock,
    parseClock,
    untilLabel,
    updateLabel,
    type LinkTarget,
    type AttachmentMode,
    ATTACHMENT_OPTIONS,
    VOICE_OPTIONS,
    KIND_META,
    PROBE_LABEL,
    PROBE_STATE_LABEL,
    probeReport,
    type ModeChoice,
    type Snapshot,
    type WorkHours,
  } from "./api";

  let {
    snap,
    run,
    onrestore,
    onnotes,
    target = null,
  }: {
    snap: Snapshot;
    run: (action: Promise<Snapshot | void>) => Promise<unknown>;
    onrestore: (path: string, preview: { services: string[]; tasks?: number | null; notes?: number | null }) => void;
    onnotes: (mode: "available" | "installed") => void;
    /** Opção a mostrar ao abrir (vinda da busca): troca de aba, rola e destaca a linha. */
    target?: SettingsTarget | null;
  } = $props();

  // Diagnóstico dos sites (spec 14): contas abertas com pontos relatados.
  const probed = $derived(snap.services.filter((s) => s.loaded && s.probes?.length));
  let copied = $state(false);
  async function copyReport() {
    try {
      await navigator.clipboard.writeText(probeReport(snap.appVersion, snap.services));
      copied = true;
      setTimeout(() => (copied = false), 4000);
    } catch (e) {
      run(Promise.reject(e));
    }
  }
  const hasNotes = $derived(notesHistory(snap.appVersion).length > 0);

  const settings = $derived(snap.settings);
  const dndManual = $derived((settings.dndUntil ?? 0) > Date.now() / 1000);
  let backupMessage = $state("");
  let privacyMessage = $state("");
  let pinDialog = $state<"create" | "change" | "remove" | null>(null);
  const systemName = isMac ? "Touch ID ou a senha do Mac" : "o Windows Hello";
  let orphans = $state<number | null>(null);
  let confirmOrphans = $state(false);

  const refreshOrphans = () =>
    api
      .getOrphanSessions()
      .then((n) => (orphans = n))
      .catch(() => (orphans = null));

  onMount(refreshOrphans);

  async function setLock(enabled: boolean, afterMinutes = settings.lock.afterMinutes) {
    privacyMessage = "";
    try {
      await run(api.setLock(enabled, afterMinutes));
    } catch (err) {
      privacyMessage = errorMessage(err);
    }
  }

  async function cleanOrphans() {
    confirmOrphans = false;
    privacyMessage = "";
    try {
      const n = await api.cleanOrphanSessions();
      privacyMessage = n === 1 ? "1 sessão antiga será apagada." : `${n} sessões antigas serão apagadas.`;
      orphans = 0;
      setTimeout(refreshOrphans, 4000);
    } catch (err) {
      privacyMessage = errorMessage(err);
    }
  }

  const DND_CHOICES = [
    { minutes: 60, label: "1 hora" },
    { minutes: 8 * 60, label: "8 horas" },
    { minutes: 0, label: "Até eu reativar" },
  ];

  const WEEKDAYS = [
    { day: 1, short: "S", label: "Segunda" },
    { day: 2, short: "T", label: "Terça" },
    { day: 3, short: "Q", label: "Quarta" },
    { day: 4, short: "Q", label: "Quinta" },
    { day: 5, short: "S", label: "Sexta" },
    { day: 6, short: "S", label: "Sábado" },
    { day: 0, short: "D", label: "Domingo" },
  ];

  const marked = $derived(snap.services.filter((s) => s.context).length);

  function setSchedule(mode: ModeChoice, change: Partial<WorkHours> = {}) {
    run(api.setModeSchedule(mode, { ...settings.workHours, ...change }));
  }

  function toggleDay(day: number) {
    const days = settings.workHours.days.includes(day)
      ? settings.workHours.days.filter((d) => d !== day)
      : [...settings.workHours.days, day];
    setSchedule(settings.mode, { days });
  }

  function workClock(which: "start" | "end", value: string) {
    const minutes = parseClock(value);
    if (minutes !== null) setSchedule(settings.mode, { [which]: minutes });
  }

  function setQuiet(change: Partial<typeof settings.quietHours>) {
    run(api.setQuietHours({ ...settings.quietHours, ...change }));
  }

  function clockChange(which: "start" | "end", value: string) {
    const minutes = parseClock(value);
    if (minutes !== null) setQuiet({ [which]: minutes });
  }

  async function chooseDownloads() {
    try {
      const dir = await open({ directory: true, title: "Pasta dos downloads" });
      if (typeof dir === "string") await run(api.setDownloadsDir(dir));
    } catch (err) {
      backupMessage = errorMessage(err);
    }
  }

  async function exportBackup() {
    backupMessage = "";
    try {
      // Data local (o ISO é UTC: à noite no Brasil já seria amanhã).
      const date = dateKey(new Date());
      const path = await save({
        title: "Exportar configuração",
        defaultPath: `papeo-backup-${date}.json`,
        filters: [{ name: "Backup do Papeo", extensions: ["json"] }],
      });
      if (!path) return;
      await api.exportConfig(path);
      backupMessage = "Configuração exportada.";
    } catch (err) {
      backupMessage = errorMessage(err);
    }
  }

  async function importBackup() {
    backupMessage = "";
    try {
      const path = await open({
        title: "Importar configuração",
        filters: [{ name: "Backup do Papeo", extensions: ["json"] }],
      });
      if (typeof path !== "string") return;
      const preview = await api.previewBackup(path);
      onrestore(path, preview);
    } catch (err) {
      backupMessage = errorMessage(err);
    }
  }

  const TAB_ICON = {
    geral: SlidersHorizontal,
    notificacoes: BellOff,
    privacidade: ShieldCheck,
    sistema: Monitor,
    arquivos: FolderOpen,
    dados: Cloud,
    sobre: Info,
  };
  const TABS = (Object.keys(TAB_LABEL) as SettingsTab[]).map((id) => ({
    id,
    label: TAB_LABEL[id],
    icon: TAB_ICON[id],
  }));
  type Tab = SettingsTab;

  // Com atualização pendente, abre direto onde está o botão de atualizar.
  let tab = $state<Tab>(
    untrack(() => target?.tab ?? (snap.update.state === "available" ? "sistema" : (lastTab as Tab))),
  );

  // Busca (⌘K): vai à aba, rola até a linha, destaca e põe o foco no controle dela.
  $effect(() => {
    const t = target;
    if (!t) return;
    untrack(() => selectTab(t.tab));
    // Algumas seções (Sincronização) só aparecem depois de carregar o status:
    // tenta por até ~1 s antes de desistir (fica só na aba).
    let tries = 0;
    const find = () => {
      const heads = [...(page?.querySelectorAll<HTMLElement>(".row .label, h2") ?? [])];
      const hit = heads.find((el) => el.textContent?.trim() === t.label);
      const row = hit?.closest<HTMLElement>(".row, section");
      if (!row) {
        if (++tries < 60 && target === t) requestAnimationFrame(find);
        return;
      }
      row.scrollIntoView({ block: "center" });
      row.classList.remove("found");
      void row.offsetWidth; // reinicia a animação se a mesma linha for buscada de novo
      row.classList.add("found");
      // Num grupo de opções, o foco vai na opção marcada.
      (row.querySelector<HTMLElement>("[role=radio][aria-checked=true], input[type=radio]:checked") ?? row
        .querySelector<HTMLElement>(
          ":is(button, input, select, textarea, [role=switch]):not(:disabled, [aria-disabled=true])",
        ))
        ?.focus({ preventScroll: true });
    };
    tick().then(find);
  });
  let page = $state<HTMLDivElement>();

  function selectTab(id: Tab) {
    tab = id;
    lastTab = id;
    if (page) page.scrollTop = 0;
  }

  /** Setas, Home e End entre as abas (padrão WAI-ARIA de abas com ativação automática). */
  function tabKey(e: KeyboardEvent) {
    const i = TABS.findIndex((t) => t.id === tab);
    const next =
      e.key === "ArrowRight" ? (i + 1) % TABS.length
      : e.key === "ArrowLeft" ? (i - 1 + TABS.length) % TABS.length
      : e.key === "Home" ? 0
      : e.key === "End" ? TABS.length - 1
      : -1;
    if (next === -1) return;
    e.preventDefault();
    selectTab(TABS[next].id);
    document.getElementById(`tab-${TABS[next].id}`)?.focus();
  }
</script>

<div class="page" bind:this={page}>
  <header class="head">
    <h1>Configurações</h1>
    <div class="tabs" role="tablist" aria-label="Seções das configurações">
      {#each TABS as t (t.id)}
        {@const Icon = t.icon}
        <button
          id="tab-{t.id}"
          class="tab"
          role="tab"
          aria-selected={tab === t.id}
          aria-controls={tab === t.id ? `panel-${t.id}` : undefined}
          tabindex={tab === t.id ? 0 : -1}
          onclick={() => selectTab(t.id)}
          onkeydown={tabKey}
        >
          <Icon size={15} />
          {t.label}
          {#if t.id === "sistema" && snap.update.state === "available"}
            <span class="tab-dot" aria-label="Atualização disponível"></span>
          {/if}
        </button>
      {/each}
    </div>
  </header>

  <div class="panel" id="panel-{tab}" role="tabpanel" aria-labelledby="tab-{tab}">
    {#if tab === "geral"}
      <section class="card" aria-labelledby="s-look">
        <h2 id="s-look"><Palette size={18} /> Aparência</h2>
        <div class="row">
          <div class="text">
            <span class="label">Tema</span>
            <span class="hint">
              OLED usa preto puro: economiza bateria em telas OLED. WhatsApp e Telegram acompanham o claro/escuro
              quando estão no tema do sistema.
            </span>
          </div>
          <div class="segmented" role="radiogroup" aria-label="Tema">
            {#each THEME_OPTIONS as option (option.value)}
              <button
                role="radio"
                aria-checked={settings.appearance === option.value}
                class:selected={settings.appearance === option.value}
                onclick={() => run(api.setAppearance(option.value))}
              >
                {option.label}
              </button>
            {/each}
          </div>
        </div>
      </section>

      <section class="card" aria-labelledby="s-mode">
        <h2 id="s-mode"><Briefcase size={18} /> Trabalho e pessoal</h2>

        <div class="row">
          <div class="text">
            <span class="label">Contas mostradas</span>
            <span class="hint">
              {#if marked === 0}
                Marque cada conta como Trabalho ou Pessoal em Personalizar (botão direito na conta). Contas sem
                marcação aparecem sempre.
              {:else}
                Fora do modo, a conta some da barra, não notifica e não entra nos contadores.
                {settings.mode === "auto" ? "No expediente, só trabalho; fora dele, só pessoal." : ""}
              {/if}
            </span>
          </div>
          <select
            class="input select"
            aria-label="Contas mostradas"
            value={settings.mode}
            onchange={(e) => setSchedule(e.currentTarget.value as ModeChoice)}
          >
            {#each MODE_CHOICE_OPTIONS as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </div>

        {#if settings.mode === "auto"}
          <div class="row">
            <div class="text">
              <span class="label">Expediente</span>
              <span class="hint">
                {modKey}⇧M ou o seletor da barra lateral trocam o modo até o expediente começar ou terminar.
              </span>
            </div>
            <div class="choices wrap">
              <div class="days" role="group" aria-label="Dias de trabalho">
                {#each WEEKDAYS as d (d.day)}
                  <button
                    class="day"
                    class:on={settings.workHours.days.includes(d.day)}
                    aria-pressed={settings.workHours.days.includes(d.day)}
                    aria-label={d.label}
                    title={d.label}
                    onclick={() => toggleDay(d.day)}
                  >
                    {d.short}
                  </button>
                {/each}
              </div>
              <input
                type="time"
                class="input time"
                aria-label="Início do expediente"
                value={formatClock(settings.workHours.start)}
                onchange={(e) => workClock("start", e.currentTarget.value)}
              />
              <span class="muted">até</span>
              <input
                type="time"
                class="input time"
                aria-label="Fim do expediente"
                value={formatClock(settings.workHours.end)}
                onchange={(e) => workClock("end", e.currentTarget.value)}
              />
            </div>
          </div>
        {/if}

        <div class="row">
          <div class="text">
            <span class="label">Hibernar as contas fora do modo</span>
            <span class="hint">Libera memória. Ao voltar para o modo, a conta recarrega quando for aberta.</span>
          </div>
          <Switch
            checked={settings.sleepOutsideMode}
            label="Hibernar as contas fora do modo"
            onchange={(v) => run(api.setPreferences({ sleepOutsideMode: v }))}
          />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Moldura com a cor da conta</span>
            <span class="hint">
              Contorna o site aberto com a cor da conta quando ela tem cor própria, está marcada como Trabalho ou
              Pessoal, ou há outra conta do mesmo serviço. Ajuda a não responder pela conta errada.
            </span>
          </div>
          <Switch
            checked={settings.accountFrame}
            label="Moldura com a cor da conta"
            onchange={(v) => run(api.setPreferences({ accountFrame: v }))}
          />
        </div>
      </section>

      <QuickReplies replies={settings.quickReplies} {run} />

      <section class="card" aria-labelledby="s-voice">
        <h2 id="s-voice"><Mic size={18} /> Mensagens de voz</h2>
        <div class="row stack">
          <div class="text">
            <span class="label">Gravação de mensagens de voz</span>
            <span class="hint">Como o Papeo grava as mensagens de voz do WhatsApp. Chamadas usam sempre a redução do sistema.</span>
          </div>
          <div class="voice-options" role="radiogroup" aria-label="Gravação de mensagens de voz">
            {#each VOICE_OPTIONS as option (option.value)}
              {@const current = isMac || settings.voiceCapture !== "clean" ? settings.voiceCapture : "instant"}
              <label class="voice-option" class:selected={current === option.value}>
                <input
                  type="radio"
                  name="voice-capture"
                  value={option.value}
                  checked={current === option.value}
                  aria-labelledby="voice-{option.value}"
                  aria-describedby="voice-{option.value}-desc"
                  onchange={() => run(api.setPreferences({ voiceCapture: option.value }))}
                />
                <span class="voice-text">
                  <span class="voice-title">
                    <span id="voice-{option.value}">{option.label}</span>
                    {#if option.value === "instant"}<span class="badge" aria-hidden="true">Padrão</span>{/if}
                  </span>
                  <span class="hint" id="voice-{option.value}-desc">{option.description}</span>
                </span>
              </label>
            {/each}
          </div>
          {#if isMac}
            <p class="hint voice-tip">
              Dica: durante uma gravação, dá para ligar o Isolamento de voz da Apple em Central de Controle → Modo do
              microfone.
            </p>
          {/if}
        </div>
      </section>

      <section class="card" aria-labelledby="s-mail">
        <h2 id="s-mail"><Mail size={18} /> E-mail</h2>
        <div class="row">
          <div class="text">
            <span class="label">Desfazer envio</span>
            <span class="hint">
              No leitor de e-mail do Papeo, o e-mail espera este tempo antes de sair, com o botão "Desfazer". Se o app fechar
              durante a espera, ele não é enviado e volta ao editor. A assinatura fica na configuração do e-mail de cada conta.
            </span>
          </div>
          <select
            class="select"
            aria-label="Desfazer envio"
            value={settings.mailUndoSecs}
            onchange={(e) => run(api.setPreferences({ mailUndoSecs: Number(e.currentTarget.value) }))}
          >
            {#each UNDO_OPTIONS as option (option.secs)}
              <option value={option.secs}>{option.label}</option>
            {/each}
          </select>
        </div>
      </section>
    {:else if tab === "notificacoes"}
      <section class="card" aria-labelledby="s-notif">
        <h2 id="s-notif"><BellOff size={18} /> Não perturbe</h2>

        <div class="row">
          <div class="text">
            <span class="label">Silenciar todas as notificações</span>
            <span class="hint">
              {#if dndManual}Ativo {untilLabel(settings.dndUntil)}.{:else}Os contadores continuam atualizando.{/if}
            </span>
          </div>
          <div class="choices" role="group" aria-label="Duração do não perturbe">
            {#if dndManual}
              <button class="btn" onclick={() => run(api.setDnd(null))}>Desativar</button>
            {:else}
              {#each DND_CHOICES as choice (choice.minutes)}
                <button class="btn" onclick={() => run(api.setDnd(choice.minutes))}>{choice.label}</button>
              {/each}
            {/if}
          </div>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Horário silencioso todos os dias</span>
            <span class="hint">Sem notificações entre os horários escolhidos.</span>
          </div>
          <div class="choices">
            <input
              type="time"
              class="input time"
              aria-label="Início do horário silencioso"
              value={formatClock(settings.quietHours.start)}
              disabled={!settings.quietHours.enabled}
              onchange={(e) => clockChange("start", e.currentTarget.value)}
            />
            <span class="muted">até</span>
            <input
              type="time"
              class="input time"
              aria-label="Fim do horário silencioso"
              value={formatClock(settings.quietHours.end)}
              disabled={!settings.quietHours.enabled}
              onchange={(e) => clockChange("end", e.currentTarget.value)}
            />
            <Switch
              checked={settings.quietHours.enabled}
              label="Usar horário silencioso"
              onchange={(enabled) => setQuiet({ enabled })}
            />
          </div>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Resumo ao sair do silêncio</span>
            <span class="hint">
              No fim do não perturbe ou do horário silencioso, uma notificação e um cartão no Início mostram quantas
              mensagens chegaram em cada conta e de quais conversas. Contas silenciadas sozinhas ficam de fora.
            </span>
          </div>
          <Switch
            checked={settings.silenceSummary}
            label="Resumo ao sair do silêncio"
            onchange={(v) => run(api.setPreferences({ silenceSummary: v }))}
          />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Trocar para a conta da notificação ao voltar</span>
            <span class="hint">
              Ao voltar ao Papeo até 20 s depois de uma notificação, mostra a conta que a enviou. Desligado, você
              continua na conversa em que estava. Clicar na própria notificação sempre abre a conversa dela.
            </span>
          </div>
          <Switch
            checked={settings.openOnNotificationFocus}
            label="Trocar para a conta da notificação ao voltar"
            onchange={(v) => run(api.setPreferences({ openOnNotificationFocus: v }))}
          />
        </div>
      </section>
    {:else if tab === "privacidade"}
      <section class="card" aria-labelledby="s-privacy">
        <h2 id="s-privacy"><ShieldCheck size={18} /> Privacidade</h2>

        <div class="row">
          <div class="text">
            <span class="label">Prévia nas notificações</span>
            <span class="hint">
              O que aparece na tela bloqueada e na central de notificações do sistema.
            </span>
          </div>
          <div class="segmented" role="radiogroup" aria-label="Prévia nas notificações">
            {#each PREVIEW_OPTIONS as option (option.value)}
              <button
                role="radio"
                aria-checked={settings.notificationPreview === option.value}
                class:selected={settings.notificationPreview === option.value}
                onclick={() => run(api.setPreferences({ notificationPreview: option.value }))}
              >
                {option.label}
              </button>
            {/each}
          </div>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Bloquear o Papeo</span>
            <span class="hint">
              {#if snap.lockMethod === "pin"}
                Pede o PIN do Papeo ao abrir e depois do tempo escolhido.
              {:else if snap.lockMethod === "system"}
                Pede {systemName} ao abrir e depois do tempo escolhido.
              {:else}
                Este computador não tem {isMac ? "senha no Mac" : "Windows Hello"}: crie um PIN do Papeo abaixo.
              {/if}
            </span>
          </div>
          <div class="choices">
            <select
              class="input select"
              aria-label="Quando bloquear"
              value={settings.lock.afterMinutes}
              disabled={!settings.lock.enabled}
              onchange={(e) => setLock(true, Number(e.currentTarget.value))}
            >
              {#each LOCK_OPTIONS as option (option.minutes)}
                <option value={option.minutes}>{option.label}</option>
              {/each}
            </select>
            <Switch
              checked={settings.lock.enabled}
              label="Bloquear o Papeo"
              disabled={!snap.lockAvailable && !settings.lock.enabled}
              onchange={(v) => setLock(v)}
            />
          </div>
        </div>

        {#if !snap.systemAuth || snap.lockMethod === "pin"}
          <div class="row">
            <div class="text">
              <span class="label">PIN do Papeo</span>
              <span class="hint">
                {#if snap.lockMethod === "pin"}
                  Um PIN ou senha só do Papeo, pedido no lugar {snap.systemAuth ? `de ${systemName}` : "da autenticação do sistema"}.
                {:else if isWindows}
                  Sem câmera ou leitor de digital? O PIN do Windows já ativa o Windows Hello. Ou use um PIN só do Papeo.
                {:else}
                  Um PIN ou senha só do Papeo para bloquear o app.
                {/if}
              </span>
            </div>
            <div class="choices">
              {#if snap.lockMethod === "pin"}
                <button class="btn" onclick={() => (pinDialog = "change")}>Trocar…</button>
                <button class="btn" onclick={() => (pinDialog = "remove")}>Remover…</button>
              {:else}
                {#if isWindows}
                  <button class="btn" onclick={() => run(api.openSigninSettings())}>PIN do Windows</button>
                {/if}
                <button class="btn" onclick={() => (pinDialog = "create")}>Criar PIN…</button>
              {/if}
            </div>
          </div>
        {/if}

        <div class="row">
          <div class="text">
            <span class="label">Esconder em capturas e compartilhamento de tela</span>
            <span class="hint">Em chamadas e gravações, a janela do Papeo aparece vazia.</span>
          </div>
          <Switch
            checked={settings.protectScreen}
            label="Esconder em capturas de tela"
            onchange={(v) => run(api.setPreferences({ protectScreen: v }))}
          />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Sessões antigas no computador</span>
            <span class="hint">
              {#if orphans === null}
                Logins de serviços que não estão mais na lista (ex.: depois de restaurar um backup).
              {:else if orphans === 0}
                Nenhuma sessão antiga guardada.
              {:else}
                {orphans === 1 ? "1 login antigo" : `${orphans} logins antigos`} de serviços que não estão mais na lista.
              {/if}
            </span>
          </div>
          <button class="btn" onclick={() => (confirmOrphans = true)} disabled={!orphans}>Apagar…</button>
        </div>
        {#if privacyMessage}<p class="message" role="status">{privacyMessage}</p>{/if}
      </section>
    {:else if tab === "sistema"}
      <section class="card" aria-labelledby="s-system">
        <h2 id="s-system"><Power size={18} /> Inicialização e janela</h2>
        <div class="row">
          <div class="text">
            <span class="label">Abrir com o sistema</span>
            <span class="hint">O Papeo inicia junto com o computador.</span>
          </div>
          <Switch checked={snap.autostart} label="Abrir com o sistema" onchange={(v) => run(api.setAutostart(v))} />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Começar escondido na bandeja</span>
            <span class="hint">Quando aberto com o sistema, fica só no ícone da bandeja/menu bar.</span>
          </div>
          <Switch
            checked={settings.startHidden}
            label="Começar escondido"
            disabled={!snap.autostart}
            onchange={(v) => run(api.setPreferences({ startHidden: v }))}
          />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Fechar a janela mantém o Papeo na bandeja</span>
            <span class="hint">As notificações continuam chegando. Para sair, use o ícone da bandeja → Sair.</span>
          </div>
          <Switch
            checked={settings.closeToTray}
            label="Manter na bandeja ao fechar"
            onchange={(v) => run(api.setPreferences({ closeToTray: v }))}
          />
        </div>
      </section>

      <section class="card" aria-labelledby="s-memory">
        <h2 id="s-memory"><MemoryStick size={18} /> Memória</h2>
        <div class="row">
          <div class="text">
            <span class="label">Recarregar serviço pesado</span>
            <span class="hint">
              Sites como o WhatsApp Web acumulam memória com o tempo. Recarrega o serviço quando ele passa
              do limite, só em segundo plano e parado há 10 min, no máximo uma vez por hora. Com o
              computador sem uso há 20 min, o limite cai para a metade (mínimo 1 GB), para tudo estar leve
              quando você voltar. Nunca durante chamadas, áudio ou vídeo. As notificações continuam.
            </span>
          </div>
          <select
            class="select"
            aria-label="Recarregar serviço pesado"
            value={settings.reloadAboveMb}
            onchange={(e) => run(api.setPreferences({ reloadAboveMb: Number(e.currentTarget.value) }))}
          >
            {#each RELOAD_OPTIONS as option (option.mb)}
              <option value={option.mb}>{option.label}</option>
            {/each}
          </select>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Confirmar antes de liberar memória</span>
            <span class="hint">Ao clicar no indicador de memória no topo, pergunta antes de recarregar as contas.</span>
          </div>
          <Switch
            checked={home.confirmFree}
            label="Confirmar antes de liberar memória"
            onchange={setConfirmFree}
          />
        </div>
      </section>

      <section class="card" aria-labelledby="s-diagnostics">
        <h2 id="s-diagnostics"><Activity size={18} /> Diagnóstico dos sites</h2>
        <div class="row">
          <div class="text">
            <span class="label">Modo de diagnóstico</span>
            <span class="hint">
              Avisa na barra do topo quando o Papeo deixa de encontrar no site de uma conta o que a resposta
              rápida, o contador ou a busca usam (sinal de que o site mudou). Mesmo desligado, a resposta rápida
              some sozinha da conta enquanto o site estiver assim.
            </span>
          </div>
          <Switch
            checked={settings.diagnostics}
            label="Modo de diagnóstico"
            onchange={(v) => run(api.setPreferences({ diagnostics: v }))}
          />
        </div>
        {#if probed.length}
          {#each probed as s (s.id)}
            <div class="row stack probes">
              <span class="label">{s.name} <span class="hint">· {KIND_META[s.kind].label}</span></span>
              <ul>
                {#each s.probes ?? [] as p (p.probe)}
                  <li class={p.state}>
                    <span>{PROBE_LABEL[p.probe]}</span>
                    <span class="state">{PROBE_STATE_LABEL[p.state]}</span>
                  </li>
                {/each}
              </ul>
            </div>
          {/each}
        {:else}
          <p class="hint probes-empty">Nenhuma conta aberta com pontos verificáveis (eles aparecem 30 s depois de a conta carregar).</p>
        {/if}
        <div class="row">
          <div class="text">
            <span class="hint" role="status">
              {copied ? "Relatório copiado. Ele não leva nomes de contas nem de conversas." : "Para relatar um problema: o relatório não leva nomes de contas nem de conversas."}
            </span>
          </div>
          <button class="btn" onclick={copyReport}>Copiar relatório</button>
        </div>
      </section>

      <section class="card" aria-labelledby="s-updates">
        <h2 id="s-updates"><RefreshCw size={18} /> Atualizações</h2>
        <div class="row">
          <div class="text">
            <span class="label">Verificar atualizações</span>
            <span class="hint">
              A cada 2 horas, o Papeo consulta o GitHub (que vê o seu endereço IP). A instalação só
              acontece quando você clica.
            </span>
          </div>
          <Switch
            checked={settings.updates.check}
            label="Verificar atualizações"
            onchange={(v) => run(api.setUpdates(v))}
          />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Papeo {snap.appVersion}</span>
            {#if updateLabel(snap.update)}
              <span class="hint" role="status">{updateLabel(snap.update)}</span>
            {/if}
            {#if snap.update.state === "available" && snap.update.notes}
              <button class="inline-link" onclick={() => onnotes("available")}>
                Ver o que mudou e as versões anteriores
              </button>
            {:else if hasNotes}
              <button class="inline-link" onclick={() => onnotes("installed")}>Histórico de versões</button>
            {/if}
          </div>
          {#if snap.update.state === "available"}
            <button class="btn btn-primary" onclick={() => run(api.installUpdate())}>Atualizar e reiniciar</button>
          {:else}
            <button
              class="btn"
              onclick={() => run(api.checkUpdate())}
              disabled={snap.update.state === "checking" || snap.update.state === "installing"}
            >
              Verificar agora
            </button>
          {/if}
        </div>
      </section>
    {:else if tab === "arquivos"}
      <section class="card" aria-labelledby="s-links">
        <h2 id="s-links"><Link size={18} /> Links e anexos</h2>
        <div class="row">
          <div class="text">
            <span class="label">Abrir links</span>
            <span class="hint">
              Na prévia, o link abre numa janela pequena sobre o Papeo, sem login e sem guardar nada.
              Fechar a prévia libera a memória; "Abrir no navegador" continua de onde parou.
            </span>
          </div>
          <select
            class="select"
            aria-label="Abrir links"
            value={settings.openLinks}
            onchange={(e) => run(api.setPreferences({ openLinks: e.currentTarget.value as LinkTarget }))}
          >
            {#each LINK_OPTIONS as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">Anexos</span>
            <span class="hint">
              Ao visualizar antes, planilhas, PDFs, documentos e imagens abrem numa janela do Papeo antes de ir para a pasta
              de downloads; de lá, salve ou abra no app (Excel, Word…) para editar. Outros arquivos são salvos direto.
              {#if !isMac}No Windows, Excel e Word abrem pelo próprio app.{/if}
            </span>
          </div>
          <select
            class="select"
            aria-label="Anexos"
            value={settings.attachments}
            onchange={(e) => run(api.setPreferences({ attachments: e.currentTarget.value as AttachmentMode }))}
          >
            {#each ATTACHMENT_OPTIONS as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </div>
      </section>

      <section class="card" aria-labelledby="s-dl">
        <h2 id="s-dl"><HardDrive size={18} /> Downloads</h2>
        <div class="row">
          <div class="text">
            <span class="label">Pasta dos arquivos baixados</span>
            <span class="hint path" title={snap.downloadsDir}>{snap.downloadsDir}</span>
          </div>
          <div class="choices">
            <button class="btn" onclick={() => run(api.openDownloadsDir())}><FolderOpen size={16} /> Abrir</button>
            <button class="btn" onclick={chooseDownloads}>Alterar…</button>
            {#if settings.downloadsDir}
              <button class="btn" onclick={() => run(api.setDownloadsDir(null))}>Usar padrão</button>
            {/if}
          </div>
        </div>
      </section>
    {:else if tab === "dados"}
      <SyncSettings />

      <section class="card" aria-labelledby="s-backup">
        <h2 id="s-backup"><Download size={18} /> Backup</h2>
        <div class="row">
          <div class="text">
            <span class="label">Exportar ou restaurar a configuração</span>
            <span class="hint">
              Serviços, nomes, cores, ordem, preferências, tarefas e anotações (o arquivo não é
              criptografado: guarde em lugar seguro). Conversas e logins não entram: em outro
              computador, cada serviço pede login de novo.
            </span>
          </div>
          <div class="choices">
            <button class="btn" onclick={exportBackup}><Upload size={16} /> Exportar…</button>
            <button class="btn" onclick={importBackup}><Download size={16} /> Importar…</button>
          </div>
        </div>
        {#if backupMessage}<p class="message" role="status">{backupMessage}</p>{/if}
      </section>
    {:else if tab === "sobre"}
      <section class="card about" aria-labelledby="s-about">
        <h2 id="s-about"><Info size={18} /> Sobre</h2>
        <div class="about-head">
          <img src="/favicon.svg" alt="" width="56" height="56" />
          <div class="text">
            <span class="about-name">Papeo</span>
            <span class="hint">Versão {snap.appVersion}</span>
            <span class="hint">
              Feito por
              <button class="link" onclick={() => run(api.openUrl(ABOUT.profile))}>{ABOUT.author}</button>
            </span>
          </div>
        </div>
        <div class="choices about-links">
          <button class="btn" onclick={() => run(api.openUrl(ABOUT.profile))}>
            <UserRound size={16} /> Perfil no GitHub
          </button>
          <button class="btn" onclick={() => run(api.openUrl(ABOUT.repo))}>
            <CodeXml size={16} /> Código-fonte
          </button>
          <button class="btn" onclick={() => run(api.openUrl(ABOUT.issues))}>
            <Bug size={16} /> Relatar um problema
          </button>
        </div>
        <p class="hint about-license">
          Software livre e gratuito, sob a licença GPL-3.0. Sem propaganda e sem coleta de dados.
        </p>
        <p class="hint about-license">
          Inclui o RNNoise (© Xiph.Org, Mozilla, Jean-Marc Valin e outros; licença BSD-3-Clause), compilado pelo Jitsi
          (Apache-2.0), na limpeza das mensagens de voz.
        </p>
      </section>
    {/if}
  </div>
</div>

{#if pinDialog}
  <PinDialog
    mode={pinDialog}
    onclose={() => (pinDialog = null)}
    ondone={(next) => {
      pinDialog = null;
      run(Promise.resolve(next));
    }}
  />
{/if}

{#if confirmOrphans}
  <ConfirmDialog
    title="Apagar {orphans === 1 ? '1 sessão antiga' : `${orphans} sessões antigas`}?"
    message="Os logins e conversas guardadas dessas contas serão apagados deste computador e não podem ser recuperados. Se você restaurar depois um backup que as use, cada serviço pedirá login de novo."
    confirmLabel="Apagar"
    onclose={() => (confirmOrphans = false)}
    onconfirm={cleanOrphans}
  />
{/if}

<style>
  .page {
    height: 100%;
    overflow-y: auto;
    padding: 0 32px 40px;
  }

  /* Título e abas ficam à vista enquanto o conteúdo rola. */
  .head {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 30px 0 0;
    /* Mesmo fundo da janela (`app.css`). */
    background: var(--bg-sidebar);
    border-bottom: 1px solid var(--border);
  }

  .tabs {
    display: flex;
    gap: 4px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  /* Área estreita (janela mínima com a barra lateral aberta): as abas perdem
     o ícone para todas caberem sem rolar. */
  .head {
    container-type: inline-size;
  }

  @container (max-width: 760px) {
    .tab :global(svg) {
      display: none;
    }
  }

  .tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 40px;
    padding: 0 12px;
    border: 0;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    background: none;
    color: var(--text-muted);
    font-size: 14px;
    white-space: nowrap;
  }

  .tab:hover {
    color: var(--text);
  }

  .tab[aria-selected="true"] {
    color: var(--text);
    font-weight: 550;
    border-bottom-color: var(--text);
  }

  .tab:focus-visible {
    outline-offset: -2px;
    border-radius: 6px 6px 0 0;
  }

  .tab-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }

  /* Linha encontrada pela busca: um realce que some sozinho. */
  .page :global(.found) {
    border-radius: 10px;
    animation: found 2.2s ease-out;
  }

  @keyframes found {
    0%,
    40% {
      background: color-mix(in srgb, var(--accent) 16%, transparent);
      box-shadow: 0 0 0 8px color-mix(in srgb, var(--accent) 16%, transparent);
    }
    100% {
      background: transparent;
      box-shadow: 0 0 0 8px transparent;
    }
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-top: 20px;
  }

  h1 {
    margin: 0 0 14px;
    font-size: 30px;
    font-weight: 500;
    letter-spacing: -0.02em;
  }

  .card {
    padding: 18px 22px 6px;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 6px;
    font-size: 16px;
    font-weight: 600;
  }

  h2 :global(svg) {
    color: var(--text-muted);
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 14px 0;
    border-top: 1px solid var(--border);
  }

  h2 + .row {
    border-top: 0;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .label {
    font-weight: 500;
  }

  .hint {
    font-size: 13px;
    color: var(--text-muted);
  }

  .probes ul {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .probes li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 13px;
  }
  .probes .state {
    color: var(--text-muted);
  }
  .probes li.found .state {
    color: var(--success-text);
  }
  .probes li.broken .state,
  .probes li.missing .state {
    color: var(--danger);
    font-weight: 600;
  }
  .probes-empty {
    margin: 4px 0 8px;
  }

  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: text;
    -webkit-user-select: text;
  }

  .choices {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }

  .choices .btn {
    height: 34px;
  }

  .time {
    height: 34px;
    width: 124px;
  }

  .select {
    height: 34px;
    width: auto;
  }

  .row.stack {
    flex-direction: column;
    align-items: stretch;
    gap: 12px;
  }

  .voice-options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 8px;
  }

  .voice-option {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    cursor: pointer;
    transition: border-color 0.15s, background-color 0.15s;
  }

  .voice-option:hover {
    background: var(--bg-hover);
  }

  .voice-option.selected {
    border-color: var(--accent);
  }

  .voice-option:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .voice-option input {
    flex: none;
    margin: 3px 0 0;
    accent-color: var(--accent);
  }

  .voice-text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .voice-title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    font-weight: 500;
  }

  .badge {
    padding: 1px 6px;
    border-radius: 6px;
    background: var(--bg-hover);
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
  }

  .voice-tip {
    margin: 0;
  }

  .choices.wrap {
    flex: 0 1 auto;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .days {
    display: inline-flex;
    gap: 3px;
  }

  .day {
    width: 30px;
    height: 30px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: var(--text-muted);
    font-size: 12.5px;
  }

  .day.on {
    border-color: var(--text);
    background: var(--text);
    color: var(--bg);
    font-weight: 600;
  }

  .segmented {
    display: inline-flex;
    flex: none;
    padding: 3px;
    gap: 2px;
    border-radius: 10px;
    background: var(--bg-hover);
  }

  .segmented button {
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
    font-size: 13.5px;
  }

  .segmented button.selected {
    background: var(--bg);
    color: var(--text);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.08);
  }

  .muted {
    color: var(--text-muted);
  }

  .message {
    margin: 0 0 14px;
    color: var(--text-muted);
  }

  .about-head {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 14px 0;
    border-top: 1px solid var(--border);
  }

  .about-head img {
    border-radius: 13px;
    flex: none;
  }

  .about-name {
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-weight: 500;
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }

  .about-links {
    flex-wrap: wrap;
  }

  .about-license {
    margin: 14px 0 16px;
  }

  .inline-link {
    align-self: flex-start;
    margin-top: 2px;
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font-size: 13px;
  }

  .inline-link:hover {
    text-decoration: underline;
  }
</style>
