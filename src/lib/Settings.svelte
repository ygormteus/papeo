<script lang="ts">
  import { onMount } from "svelte";
  import SyncSettings from "./SyncSettings.svelte";
  import {
    BellOff,
    Bug,
    CodeXml,
    Download,
    Info,
    FolderOpen,
    HardDrive,
    Monitor,
    Palette,
    ShieldCheck,
    Upload,
    UserRound,
  } from "@lucide/svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Switch from "./Switch.svelte";
  import { bundledNotes } from "./bundledNotes";
  import {
    ABOUT,
    LOCK_OPTIONS,
    PREVIEW_OPTIONS,
    RELOAD_OPTIONS,
    THEME_OPTIONS,
    api,
    errorMessage,
    isMac,
    formatClock,
    parseClock,
    untilLabel,
    updateLabel,
    type Snapshot,
  } from "./api";

  let {
    snap,
    run,
    onrestore,
    onnotes,
  }: {
    snap: Snapshot;
    run: (action: Promise<Snapshot | void>) => Promise<void>;
    onrestore: (path: string, preview: { services: string[]; tasks?: number | null; notes?: number | null }) => void;
    onnotes: (mode: "available" | "installed") => void;
  } = $props();
  const hasNotes = $derived(!!bundledNotes(snap.appVersion));

  const settings = $derived(snap.settings);
  const dndManual = $derived((settings.dndUntil ?? 0) > Date.now() / 1000);
  let backupMessage = $state("");
  let privacyMessage = $state("");
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
      const date = new Date().toISOString().slice(0, 10);
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
</script>

<div class="page">
  <h1>Configurações</h1>

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
          {#if !snap.lockAvailable}
            Indisponível: configure {isMac ? "uma senha no Mac" : "o Windows Hello (PIN, rosto ou digital)"}.
          {:else}
            Pede {isMac ? "Touch ID ou a senha do Mac" : "o Windows Hello"} ao abrir e depois do tempo escolhido.
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

  <section class="card" aria-labelledby="s-system">
    <h2 id="s-system"><Monitor size={18} /> Sistema</h2>

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
          <button class="inline-link" onclick={() => onnotes("available")}>Ver o que mudou</button>
        {:else if hasNotes}
          <button class="inline-link" onclick={() => onnotes("installed")}>O que mudou nesta versão</button>
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
  </section>
</div>

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
    padding: 30px 32px 40px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  h1 {
    margin: 0 0 8px;
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
