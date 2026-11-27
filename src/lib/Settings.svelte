<script module lang="ts">
  /** Última aba aberta: volta nela ao reabrir as configurações (só nesta sessão). */
  let lastTab = "geral";
</script>

<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { t, tk } from "./i18n.svelte";
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
  import SignatureDialog from "./SignatureDialog.svelte";
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
    type Language,
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
  // Contas com o e-mail por IMAP: a assinatura de cada uma (spec 16).
  const mailAccounts = $derived(snap.services.filter((s) => s.mail));
  let signingId = $state<string | null>(null);
  const signing = $derived(snap.services.find((s) => s.id === signingId && s.mail) ?? null);
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
  // "Português" e "English" ficam cada um no seu idioma (sem tradução).
  const LANGUAGE_OPTIONS: { value: Language; label: string }[] = [
    { value: "system", label: "" },
    { value: "pt", label: "Português" },
    { value: "en", label: "English" },
  ];
  const hasNotes = $derived(notesHistory(snap.appVersion).length > 0);

  const settings = $derived(snap.settings);
  const dndManual = $derived((settings.dndUntil ?? 0) > Date.now() / 1000);
  let backupMessage = $state("");
  let privacyMessage = $state("");
  let pinDialog = $state<"create" | "change" | "remove" | null>(null);
  const systemName = $derived(isMac ? t("Touch ID ou a senha do Mac") : t("o Windows Hello"));
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
      privacyMessage = n === 1 ? t("1 sessão antiga será apagada.") : t("{n} sessões antigas serão apagadas.", { n });
      orphans = 0;
      setTimeout(refreshOrphans, 4000);
    } catch (err) {
      privacyMessage = errorMessage(err);
    }
  }

  const DND_CHOICES = [
    { minutes: 60, label: tk("1 hora") },
    { minutes: 8 * 60, label: tk("8 horas") },
    { minutes: 0, label: tk("Até eu reativar") },
  ];

  const WEEKDAYS = [
    { day: 1, label: tk("Segunda") },
    { day: 2, label: tk("Terça") },
    { day: 3, label: tk("Quarta") },
    { day: 4, label: tk("Quinta") },
    { day: 5, label: tk("Sexta") },
    { day: 6, label: tk("Sábado") },
    { day: 0, label: tk("Domingo") },
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
      const dir = await open({ directory: true, title: t("Pasta dos downloads") });
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
        title: t("Exportar configuração"),
        defaultPath: `papeo-backup-${date}.json`,
        filters: [{ name: t("Backup do Papeo"), extensions: ["json"] }],
      });
      if (!path) return;
      await api.exportConfig(path);
      backupMessage = t("Configuração exportada.");
    } catch (err) {
      backupMessage = errorMessage(err);
    }
  }

  async function importBackup() {
    backupMessage = "";
    try {
      const path = await open({
        title: t("Importar configuração"),
        filters: [{ name: t("Backup do Papeo"), extensions: ["json"] }],
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
    label: TAB_LABEL[id], // traduzido na exibição
    icon: TAB_ICON[id],
  }));
  type Tab = SettingsTab;

  // Com atualização pendente, abre direto onde está o botão de atualizar.
  let tab = $state<Tab>(
    untrack(() => target?.tab ?? (snap.update.state === "available" ? "sistema" : (lastTab as Tab))),
  );

  // Busca (⌘K): vai à aba, rola até a linha, destaca e põe o foco no controle dela.
  $effect(() => {
    const tg = target;
    if (!tg) return;
    untrack(() => selectTab(tg.tab));
    // Algumas seções (Sincronização) só aparecem depois de carregar o status:
    // tenta por até ~1 s antes de desistir (fica só na aba).
    let tries = 0;
    const find = () => {
      const heads = [...(page?.querySelectorAll<HTMLElement>(".row .label, h2") ?? [])];
      const hit = heads.find((el) => el.textContent?.trim() === t(tg.label));
      const row = hit?.closest<HTMLElement>(".row, section");
      if (!row) {
        if (++tries < 60 && target === tg) requestAnimationFrame(find);
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
    const i = TABS.findIndex((x) => x.id === tab);
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
    <h1>{t("Configurações")}</h1>
    <div class="tabs" role="tablist" aria-label={t("Seções das configurações")}>
      {#each TABS as tb (tb.id)}
        {@const Icon = tb.icon}
        <button
          id="tab-{tb.id}"
          class="tab"
          role="tab"
          aria-selected={tab === tb.id}
          aria-controls={tab === tb.id ? `panel-${tb.id}` : undefined}
          tabindex={tab === tb.id ? 0 : -1}
          onclick={() => selectTab(tb.id)}
          onkeydown={tabKey}
        >
          <Icon size={15} />
          {t(tb.label)}
          {#if tb.id === "sistema" && snap.update.state === "available"}
            <span class="tab-dot" aria-label={t("Atualização disponível")}></span>
          {/if}
        </button>
      {/each}
    </div>
  </header>

  <div class="panel" id="panel-{tab}" role="tabpanel" aria-labelledby="tab-{tab}">
    {#if tab === "geral"}
      <section class="card" aria-labelledby="s-look">
        <h2 id="s-look"><Palette size={18} /> {t("Aparência")}</h2>
        <div class="row">
          <div class="text">
            <span class="label">{t("Tema")}</span>
            <span class="hint">
              {t("OLED usa preto puro: economiza bateria em telas OLED. WhatsApp e Telegram acompanham o claro/escuro quando estão no tema do sistema.")}
            </span>
          </div>
          <div class="segmented" role="radiogroup" aria-label={t("Tema")}>
            {#each THEME_OPTIONS as option (option.value)}
              <button
                role="radio"
                aria-checked={settings.appearance === option.value}
                class:selected={settings.appearance === option.value}
                onclick={() => run(api.setAppearance(option.value))}
              >
                {t(option.label)}
              </button>
            {/each}
          </div>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Idioma")}</span>
            <span class="hint">{t("Idioma da interface do Papeo. Os sites continuam no idioma deles.")}</span>
          </div>
          <div class="segmented" role="radiogroup" aria-label={t("Idioma")}>
            {#each LANGUAGE_OPTIONS as option (option.value)}
              <button
                role="radio"
                aria-checked={settings.language === option.value}
                class:selected={settings.language === option.value}
                onclick={() => run(api.setPreferences({ language: option.value }))}
              >
                {option.label ? option.label : t("Sistema")}
              </button>
            {/each}
          </div>
        </div>
      </section>

      <section class="card" aria-labelledby="s-mode">
        <h2 id="s-mode"><Briefcase size={18} /> {t("Trabalho e pessoal")}</h2>

        <div class="row">
          <div class="text">
            <span class="label">{t("Contas mostradas")}</span>
            <span class="hint">
              {#if marked === 0}
                {t("Marque cada conta como Trabalho ou Pessoal em Personalizar (botão direito na conta). Contas sem marcação aparecem sempre.")}
              {:else}
                {t("Fora do modo, a conta some da barra, não notifica e não entra nos contadores.")}
                {settings.mode === "auto" ? t("No expediente, só trabalho; fora dele, só pessoal.") : ""}
              {/if}
            </span>
          </div>
          <select
            class="input select"
            aria-label={t("Contas mostradas")}
            value={settings.mode}
            onchange={(e) => setSchedule(e.currentTarget.value as ModeChoice)}
          >
            {#each MODE_CHOICE_OPTIONS as option (option.value)}
              <option value={option.value}>{t(option.label)}</option>
            {/each}
          </select>
        </div>

        {#if settings.mode === "auto"}
          <div class="row">
            <div class="text">
              <span class="label">{t("Expediente")}</span>
              <span class="hint">
                {t("{tecla}⇧M ou o seletor da barra lateral trocam o modo até o expediente começar ou terminar.", { tecla: modKey })}
              </span>
            </div>
            <div class="choices wrap">
              <div class="days" role="group" aria-label={t("Dias de trabalho")}>
                {#each WEEKDAYS as d (d.day)}
                  <button
                    class="day"
                    class:on={settings.workHours.days.includes(d.day)}
                    aria-pressed={settings.workHours.days.includes(d.day)}
                    aria-label={t(d.label)}
                    title={t(d.label)}
                    onclick={() => toggleDay(d.day)}
                  >
                    {t(d.label).charAt(0)}
                  </button>
                {/each}
              </div>
              <input
                type="time"
                class="input time"
                aria-label={t("Início do expediente")}
                value={formatClock(settings.workHours.start)}
                onchange={(e) => workClock("start", e.currentTarget.value)}
              />
              <span class="muted">{t("até")}</span>
              <input
                type="time"
                class="input time"
                aria-label={t("Fim do expediente")}
                value={formatClock(settings.workHours.end)}
                onchange={(e) => workClock("end", e.currentTarget.value)}
              />
            </div>
          </div>
        {/if}

        <div class="row">
          <div class="text">
            <span class="label">{t("Hibernar as contas fora do modo")}</span>
            <span class="hint">{t("Libera memória. Ao voltar para o modo, a conta recarrega quando for aberta.")}</span>
          </div>
          <Switch
            checked={settings.sleepOutsideMode}
            label={t("Hibernar as contas fora do modo")}
            onchange={(v) => run(api.setPreferences({ sleepOutsideMode: v }))}
          />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Moldura com a cor da conta")}</span>
            <span class="hint">
              {t("Contorna o site aberto com a cor da conta quando ela tem cor própria, está marcada como Trabalho ou Pessoal, ou há outra conta do mesmo serviço. Ajuda a não responder pela conta errada.")}
            </span>
          </div>
          <Switch
            checked={settings.accountFrame}
            label={t("Moldura com a cor da conta")}
            onchange={(v) => run(api.setPreferences({ accountFrame: v }))}
          />
        </div>
      </section>

      <QuickReplies replies={settings.quickReplies} {run} />

      <section class="card" aria-labelledby="s-voice">
        <h2 id="s-voice"><Mic size={18} /> {t("Mensagens de voz")}</h2>
        <div class="row stack">
          <div class="text">
            <span class="label">{t("Gravação de mensagens de voz")}</span>
            <span class="hint">{t("Como o Papeo grava as mensagens de voz do WhatsApp. Chamadas usam sempre a redução do sistema.")}</span>
          </div>
          <div class="voice-options" role="radiogroup" aria-label={t("Gravação de mensagens de voz")}>
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
                    <span id="voice-{option.value}">{t(option.label)}</span>
                    {#if option.value === "instant"}<span class="badge" aria-hidden="true">{t("Padrão")}</span>{/if}
                  </span>
                  <span class="hint" id="voice-{option.value}-desc">{t(option.description)}</span>
                </span>
              </label>
            {/each}
          </div>
          {#if isMac}
            <p class="hint voice-tip">
              {t("Dica: durante uma gravação, dá para ligar o Isolamento de voz da Apple em Central de Controle → Modo do microfone.")}
            </p>
          {/if}
        </div>
      </section>

      <section class="card" aria-labelledby="s-mail">
        <h2 id="s-mail"><Mail size={18} /> {t("E-mail")}</h2>
        <div class="row">
          <div class="text">
            <span class="label">{t("Desfazer envio")}</span>
            <span class="hint">
              {t("No leitor de e-mail do Papeo, o e-mail espera este tempo antes de sair, com o botão \"Desfazer\". Se o app fechar durante a espera, ele não é enviado e volta ao editor.")}
            </span>
          </div>
          <select
            class="select"
            aria-label={t("Desfazer envio")}
            value={settings.mailUndoSecs}
            onchange={(e) => run(api.setPreferences({ mailUndoSecs: Number(e.currentTarget.value) }))}
          >
            {#each UNDO_OPTIONS as option (option.secs)}
              <option value={option.secs}>{t(option.label)}</option>
            {/each}
          </select>
        </div>
        <div class="row signatures">
          <div class="text">
            <span class="label">{t("Assinaturas")}</span>
            <span class="hint">
              {#if mailAccounts.length}
                {t("Entra sozinha nas mensagens novas, respostas e encaminhamentos de cada conta, no leitor de e-mail do Papeo.")}
              {:else}
                {t("Para usar assinatura, configure o e-mail numa conta de e-mail: clique com o botão direito nela e escolha \"E-mail por IMAP…\".")}
              {/if}
            </span>
            {#if mailAccounts.length}
              <ul class="sig-list">
                {#each mailAccounts as s (s.id)}
                  <li>
                    <span class="sig-name">{s.name}</span>
                    <span class="sig-text">{s.mail?.signature ? s.mail.signature.split("\n")[0] : t("Sem assinatura")}</span>
                    <button class="btn" onclick={() => (signingId = s.id)}>
                      {s.mail?.signature ? t("Editar") : t("Criar")}
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        </div>
      </section>
      {#if signing}<SignatureDialog service={signing} onclose={() => (signingId = null)} />{/if}
    {:else if tab === "notificacoes"}
      <section class="card" aria-labelledby="s-notif">
        <h2 id="s-notif"><BellOff size={18} /> {t("Não perturbe")}</h2>

        <div class="row">
          <div class="text">
            <span class="label">{t("Silenciar todas as notificações")}</span>
            <span class="hint">
              {#if dndManual}{t("Ativo {ate}.", { ate: untilLabel(settings.dndUntil) })}{:else}{t("Os contadores continuam atualizando.")}{/if}
            </span>
          </div>
          <div class="choices" role="group" aria-label={t("Duração do não perturbe")}>
            {#if dndManual}
              <button class="btn" onclick={() => run(api.setDnd(null))}>{t("Desativar")}</button>
            {:else}
              {#each DND_CHOICES as choice (choice.minutes)}
                <button class="btn" onclick={() => run(api.setDnd(choice.minutes))}>{t(choice.label)}</button>
              {/each}
            {/if}
          </div>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Horário silencioso todos os dias")}</span>
            <span class="hint">{t("Sem notificações entre os horários escolhidos.")}</span>
          </div>
          <div class="choices">
            <input
              type="time"
              class="input time"
              aria-label={t("Início do horário silencioso")}
              value={formatClock(settings.quietHours.start)}
              disabled={!settings.quietHours.enabled}
              onchange={(e) => clockChange("start", e.currentTarget.value)}
            />
            <span class="muted">{t("até")}</span>
            <input
              type="time"
              class="input time"
              aria-label={t("Fim do horário silencioso")}
              value={formatClock(settings.quietHours.end)}
              disabled={!settings.quietHours.enabled}
              onchange={(e) => clockChange("end", e.currentTarget.value)}
            />
            <Switch
              checked={settings.quietHours.enabled}
              label={t("Usar horário silencioso")}
              onchange={(enabled) => setQuiet({ enabled })}
            />
          </div>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Resumo ao sair do silêncio")}</span>
            <span class="hint">
              {t("No fim do não perturbe ou do horário silencioso, uma notificação e um cartão no Início mostram quantas mensagens chegaram em cada conta e de quais conversas. Contas silenciadas sozinhas ficam de fora.")}
            </span>
          </div>
          <Switch
            checked={settings.silenceSummary}
            label={t("Resumo ao sair do silêncio")}
            onchange={(v) => run(api.setPreferences({ silenceSummary: v }))}
          />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Trocar para a conta da notificação ao voltar")}</span>
            <span class="hint">
              {t("Ao voltar ao Papeo até 20 s depois de uma notificação, mostra a conta que a enviou. Desligado, você continua na conversa em que estava. Clicar na própria notificação sempre abre a conversa dela.")}
            </span>
          </div>
          <Switch
            checked={settings.openOnNotificationFocus}
            label={t("Trocar para a conta da notificação ao voltar")}
            onchange={(v) => run(api.setPreferences({ openOnNotificationFocus: v }))}
          />
        </div>
      </section>
    {:else if tab === "privacidade"}
      <section class="card" aria-labelledby="s-privacy">
        <h2 id="s-privacy"><ShieldCheck size={18} /> {t("Privacidade")}</h2>

        <div class="row">
          <div class="text">
            <span class="label">{t("Prévia nas notificações")}</span>
            <span class="hint">
              {t("O que aparece na tela bloqueada e na central de notificações do sistema.")}
            </span>
          </div>
          <div class="segmented" role="radiogroup" aria-label={t("Prévia nas notificações")}>
            {#each PREVIEW_OPTIONS as option (option.value)}
              <button
                role="radio"
                aria-checked={settings.notificationPreview === option.value}
                class:selected={settings.notificationPreview === option.value}
                onclick={() => run(api.setPreferences({ notificationPreview: option.value }))}
              >
                {t(option.label)}
              </button>
            {/each}
          </div>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Bloquear o Papeo")}</span>
            <span class="hint">
              {#if snap.lockMethod === "pin"}
                {t("Pede o PIN do Papeo ao abrir e depois do tempo escolhido.")}
              {:else if snap.lockMethod === "system"}
                {t("Pede {sistema} ao abrir e depois do tempo escolhido.", { sistema: systemName })}
              {:else}
                {isMac
                  ? t("Este computador não tem senha no Mac: crie um PIN do Papeo abaixo.")
                  : t("Este computador não tem Windows Hello: crie um PIN do Papeo abaixo.")}
              {/if}
            </span>
          </div>
          <div class="choices">
            <select
              class="input select"
              aria-label={t("Quando bloquear")}
              value={settings.lock.afterMinutes}
              disabled={!settings.lock.enabled}
              onchange={(e) => setLock(true, Number(e.currentTarget.value))}
            >
              {#each LOCK_OPTIONS as option (option.minutes)}
                <option value={option.minutes}>{t(option.label)}</option>
              {/each}
            </select>
            <Switch
              checked={settings.lock.enabled}
              label={t("Bloquear o Papeo")}
              disabled={!snap.lockAvailable && !settings.lock.enabled}
              onchange={(v) => setLock(v)}
            />
          </div>
        </div>

        {#if !snap.systemAuth || snap.lockMethod === "pin"}
          <div class="row">
            <div class="text">
              <span class="label">{t("PIN do Papeo")}</span>
              <span class="hint">
                {#if snap.lockMethod === "pin"}
                  {snap.systemAuth
                    ? t("Um PIN ou senha só do Papeo, pedido no lugar de {sistema}.", { sistema: systemName })
                    : t("Um PIN ou senha só do Papeo, pedido no lugar da autenticação do sistema.")}
                {:else if isWindows}
                  {t("Sem câmera ou leitor de digital? O PIN do Windows já ativa o Windows Hello. Ou use um PIN só do Papeo.")}
                {:else}
                  {t("Um PIN ou senha só do Papeo para bloquear o app.")}
                {/if}
              </span>
            </div>
            <div class="choices">
              {#if snap.lockMethod === "pin"}
                <button class="btn" onclick={() => (pinDialog = "change")}>{t("Trocar…")}</button>
                <button class="btn" onclick={() => (pinDialog = "remove")}>{t("Remover…")}</button>
              {:else}
                {#if isWindows}
                  <button class="btn" onclick={() => run(api.openSigninSettings())}>{t("PIN do Windows")}</button>
                {/if}
                <button class="btn" onclick={() => (pinDialog = "create")}>{t("Criar PIN…")}</button>
              {/if}
            </div>
          </div>
        {/if}

        <div class="row">
          <div class="text">
            <span class="label">{t("Esconder em capturas e compartilhamento de tela")}</span>
            <span class="hint">{t("Em chamadas e gravações, a janela do Papeo aparece vazia.")}</span>
          </div>
          <Switch
            checked={settings.protectScreen}
            label={t("Esconder em capturas de tela")}
            onchange={(v) => run(api.setPreferences({ protectScreen: v }))}
          />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Sessões antigas no computador")}</span>
            <span class="hint">
              {#if orphans === null}
                {t("Logins de serviços que não estão mais na lista (ex.: depois de restaurar um backup).")}
              {:else if orphans === 0}
                {t("Nenhuma sessão antiga guardada.")}
              {:else}
                {orphans === 1
                  ? t("1 login antigo de serviços que não estão mais na lista.")
                  : t("{n} logins antigos de serviços que não estão mais na lista.", { n: orphans })}
              {/if}
            </span>
          </div>
          <button class="btn" onclick={() => (confirmOrphans = true)} disabled={!orphans}>{t("Apagar…")}</button>
        </div>
        {#if privacyMessage}<p class="message" role="status">{privacyMessage}</p>{/if}
      </section>
    {:else if tab === "sistema"}
      <section class="card" aria-labelledby="s-system">
        <h2 id="s-system"><Power size={18} /> {t("Inicialização e janela")}</h2>
        <div class="row">
          <div class="text">
            <span class="label">{t("Abrir com o sistema")}</span>
            <span class="hint">{t("O Papeo inicia junto com o computador.")}</span>
          </div>
          <Switch checked={snap.autostart} label={t("Abrir com o sistema")} onchange={(v) => run(api.setAutostart(v))} />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Começar escondido na bandeja")}</span>
            <span class="hint">{t("Quando aberto com o sistema, fica só no ícone da bandeja/menu bar.")}</span>
          </div>
          <Switch
            checked={settings.startHidden}
            label={t("Começar escondido")}
            disabled={!snap.autostart}
            onchange={(v) => run(api.setPreferences({ startHidden: v }))}
          />
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Fechar a janela mantém o Papeo na bandeja")}</span>
            <span class="hint">{t("As notificações continuam chegando. Para sair, use o ícone da bandeja → Sair.")}</span>
          </div>
          <Switch
            checked={settings.closeToTray}
            label={t("Manter na bandeja ao fechar")}
            onchange={(v) => run(api.setPreferences({ closeToTray: v }))}
          />
        </div>
      </section>

      <section class="card" aria-labelledby="s-memory">
        <h2 id="s-memory"><MemoryStick size={18} /> {t("Memória")}</h2>
        <div class="row">
          <div class="text">
            <span class="label">{t("Recarregar serviço pesado")}</span>
            <span class="hint">
              {t("Sites como o WhatsApp Web acumulam memória com o tempo. Recarrega o serviço quando ele passa do limite, só em segundo plano e parado há 10 min, no máximo uma vez por hora. Com o computador sem uso há 20 min, o limite cai para a metade (mínimo 1 GB), para tudo estar leve quando você voltar. Nunca durante chamadas, áudio ou vídeo. As notificações continuam.")}
            </span>
          </div>
          <select
            class="select"
            aria-label={t("Recarregar serviço pesado")}
            value={settings.reloadAboveMb}
            onchange={(e) => run(api.setPreferences({ reloadAboveMb: Number(e.currentTarget.value) }))}
          >
            {#each RELOAD_OPTIONS as option (option.mb)}
              <option value={option.mb}>{t(option.label)}</option>
            {/each}
          </select>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Confirmar antes de liberar memória")}</span>
            <span class="hint">{t("Ao clicar no indicador de memória no topo, pergunta antes de recarregar as contas.")}</span>
          </div>
          <Switch
            checked={home.confirmFree}
            label={t("Confirmar antes de liberar memória")}
            onchange={setConfirmFree}
          />
        </div>
      </section>

      <section class="card" aria-labelledby="s-diagnostics">
        <h2 id="s-diagnostics"><Activity size={18} /> {t("Diagnóstico dos sites")}</h2>
        <div class="row">
          <div class="text">
            <span class="label">{t("Modo de diagnóstico")}</span>
            <span class="hint">
              {t("Avisa na barra do topo quando o Papeo deixa de encontrar no site de uma conta o que a resposta rápida, o contador ou a busca usam (sinal de que o site mudou). Mesmo desligado, a resposta rápida some sozinha da conta enquanto o site estiver assim.")}
            </span>
          </div>
          <Switch
            checked={settings.diagnostics}
            label={t("Modo de diagnóstico")}
            onchange={(v) => run(api.setPreferences({ diagnostics: v }))}
          />
        </div>
        {#if probed.length}
          {#each probed as s (s.id)}
            <div class="row stack probes">
              <span class="label">{s.name} <span class="hint">· {t(KIND_META[s.kind].label)}</span></span>
              <ul>
                {#each s.probes ?? [] as p (p.probe)}
                  <li class={p.state}>
                    <span>{t(PROBE_LABEL[p.probe])}</span>
                    <span class="state">{t(PROBE_STATE_LABEL[p.state])}</span>
                  </li>
                {/each}
              </ul>
            </div>
          {/each}
        {:else}
          <p class="hint probes-empty">{t("Nenhuma conta aberta com pontos verificáveis (eles aparecem 30 s depois de a conta carregar).")}</p>
        {/if}
        <div class="row">
          <div class="text">
            <span class="hint" role="status">
              {copied ? t("Relatório copiado. Ele não leva nomes de contas nem de conversas.") : t("Para relatar um problema: o relatório não leva nomes de contas nem de conversas.")}
            </span>
          </div>
          <button class="btn" onclick={copyReport}>{t("Copiar relatório")}</button>
        </div>
      </section>

      <section class="card" aria-labelledby="s-updates">
        <h2 id="s-updates"><RefreshCw size={18} /> {t("Atualizações")}</h2>
        <div class="row">
          <div class="text">
            <span class="label">{t("Verificar atualizações")}</span>
            <span class="hint">
              {t("A cada 2 horas, o Papeo consulta o GitHub (que vê o seu endereço IP). A instalação só acontece quando você clica.")}
            </span>
          </div>
          <Switch
            checked={settings.updates.check}
            label={t("Verificar atualizações")}
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
                {t("Ver o que mudou e as versões anteriores")}
              </button>
            {:else if hasNotes}
              <button class="inline-link" onclick={() => onnotes("installed")}>{t("Histórico de versões")}</button>
            {/if}
          </div>
          {#if snap.update.state === "available"}
            <button class="btn btn-primary" onclick={() => run(api.installUpdate())}>{t("Atualizar e reiniciar")}</button>
          {:else}
            <button
              class="btn"
              onclick={() => run(api.checkUpdate())}
              disabled={snap.update.state === "checking" || snap.update.state === "installing"}
            >
              {t("Verificar agora")}
            </button>
          {/if}
        </div>
      </section>
    {:else if tab === "arquivos"}
      <section class="card" aria-labelledby="s-links">
        <h2 id="s-links"><Link size={18} /> {t("Links e anexos")}</h2>
        <div class="row">
          <div class="text">
            <span class="label">{t("Abrir links")}</span>
            <span class="hint">
              {t("Na prévia, o link abre numa janela pequena sobre o Papeo, sem login e sem guardar nada. Fechar a prévia libera a memória; \"Abrir no navegador\" continua de onde parou.")}
            </span>
          </div>
          <select
            class="select"
            aria-label={t("Abrir links")}
            value={settings.openLinks}
            onchange={(e) => run(api.setPreferences({ openLinks: e.currentTarget.value as LinkTarget }))}
          >
            {#each LINK_OPTIONS as option (option.value)}
              <option value={option.value}>{t(option.label)}</option>
            {/each}
          </select>
        </div>

        <div class="row">
          <div class="text">
            <span class="label">{t("Anexos")}</span>
            <span class="hint">
              {t("Ao visualizar antes, planilhas, PDFs, documentos e imagens abrem numa janela do Papeo antes de ir para a pasta de downloads; de lá, salve ou abra no app (Excel, Word…) para editar. Outros arquivos são salvos direto.")}
              {#if !isMac}{t("No Windows, Excel e Word abrem pelo próprio app.")}{/if}
            </span>
          </div>
          <select
            class="select"
            aria-label={t("Anexos")}
            value={settings.attachments}
            onchange={(e) => run(api.setPreferences({ attachments: e.currentTarget.value as AttachmentMode }))}
          >
            {#each ATTACHMENT_OPTIONS as option (option.value)}
              <option value={option.value}>{t(option.label)}</option>
            {/each}
          </select>
        </div>
      </section>

      <section class="card" aria-labelledby="s-dl">
        <h2 id="s-dl"><HardDrive size={18} /> {t("Downloads")}</h2>
        <div class="row">
          <div class="text">
            <span class="label">{t("Pasta dos arquivos baixados")}</span>
            <span class="hint path" title={snap.downloadsDir}>{snap.downloadsDir}</span>
          </div>
          <div class="choices">
            <button class="btn" onclick={() => run(api.openDownloadsDir())}><FolderOpen size={16} /> {t("Abrir")}</button>
            <button class="btn" onclick={chooseDownloads}>{t("Alterar…")}</button>
            {#if settings.downloadsDir}
              <button class="btn" onclick={() => run(api.setDownloadsDir(null))}>{t("Usar padrão")}</button>
            {/if}
          </div>
        </div>
      </section>
    {:else if tab === "dados"}
      <SyncSettings />

      <section class="card" aria-labelledby="s-backup">
        <h2 id="s-backup"><Download size={18} /> {t("Backup")}</h2>
        <div class="row">
          <div class="text">
            <span class="label">{t("Exportar ou restaurar a configuração")}</span>
            <span class="hint">
              {t("Serviços, nomes, cores, ordem, preferências, tarefas e anotações (o arquivo não é criptografado: guarde em lugar seguro). Conversas e logins não entram: em outro computador, cada serviço pede login de novo.")}
            </span>
          </div>
          <div class="choices">
            <button class="btn" onclick={exportBackup}><Upload size={16} /> {t("Exportar…")}</button>
            <button class="btn" onclick={importBackup}><Download size={16} /> {t("Importar…")}</button>
          </div>
        </div>
        {#if backupMessage}<p class="message" role="status">{backupMessage}</p>{/if}
      </section>
    {:else if tab === "sobre"}
      <section class="card about" aria-labelledby="s-about">
        <h2 id="s-about"><Info size={18} /> {t("Sobre")}</h2>
        <div class="about-head">
          <img src="/favicon.svg" alt="" width="56" height="56" />
          <div class="text">
            <span class="about-name">Papeo</span>
            <span class="hint">{t("Versão {versao}", { versao: snap.appVersion })}</span>
            <span class="hint">
              {t("Feito por")}
              <button class="link" onclick={() => run(api.openUrl(ABOUT.profile))}>{ABOUT.author}</button>
            </span>
          </div>
        </div>
        <div class="choices about-links">
          <button class="btn" onclick={() => run(api.openUrl(ABOUT.profile))}>
            <UserRound size={16} /> {t("Perfil no GitHub")}
          </button>
          <button class="btn" onclick={() => run(api.openUrl(ABOUT.repo))}>
            <CodeXml size={16} /> {t("Código-fonte")}
          </button>
          <button class="btn" onclick={() => run(api.openUrl(ABOUT.issues))}>
            <Bug size={16} /> {t("Relatar um problema")}
          </button>
        </div>
        <p class="hint about-license">
          {t("Software livre e gratuito, sob a licença GPL-3.0. Sem propaganda e sem coleta de dados.")}
        </p>
        <p class="hint about-license">
          {t("Inclui o RNNoise (© Xiph.Org, Mozilla, Jean-Marc Valin e outros; licença BSD-3-Clause), compilado pelo Jitsi (Apache-2.0), na limpeza das mensagens de voz.")}
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
    title={orphans === 1 ? t("Apagar 1 sessão antiga?") : t("Apagar {n} sessões antigas?", { n: orphans ?? 0 })}
    message={t("Os logins e conversas guardadas dessas contas serão apagados deste computador e não podem ser recuperados. Se você restaurar depois um backup que as use, cada serviço pedirá login de novo.")}
    confirmLabel={t("Apagar")}
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

  .signatures .text {
    flex: 1;
    min-width: 0;
  }
  .sig-list {
    list-style: none;
    margin: 10px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .sig-list li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .sig-name {
    font-weight: 500;
    flex-shrink: 0;
  }
  .sig-text {
    flex: 1;
    min-width: 0;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
