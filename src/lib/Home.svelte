<script lang="ts">
  import { onMount } from "svelte";
  import { locale, t } from "./i18n.svelte";
  import {
    X,
    LayoutList,
    LogIn,
    Mail,
    MessagesSquare,
    MoonStar,
    RotateCw,
    FolderOpen,
    Palette,
    Plus,
    Snowflake,
    Trash2,
    TriangleAlert,
  } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import RecentChats from "./RecentChats.svelte";
  import { HEAVY_BYTES, freeMemory as freeMemoryOf, home, watchMemory } from "./homeState.svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import Switch from "./Switch.svelte";
  import Modal from "./Modal.svelte";
  import {
    HIBERNATE_OPTIONS,
    STATUS_LABEL,
    api,
    formatBytes,
    isEmail,
    isMuted,
    type ServiceView,
    type Snapshot,
    type TaskFields,
  } from "./api";

  let {
    snap,
    run,
    onadd,
    onopen,
    onremove,
    oncustomize,
    onchat,
    today,
    oncreatetask,
    clock,
    onlater,
  }: {
    snap: Snapshot;
    run: (action: Promise<Snapshot | void>) => Promise<unknown>;
    onadd: () => void;
    onopen: (id: string) => void;
    onremove: (service: ServiceView) => void;
    oncustomize: (service: ServiceView) => void;
    /** Abre a conversa (nome exato) no serviço. */
    onchat: (serviceId: string, chat: string) => void;
    /** Cartão "Para hoje" (tarefas atrasadas e de hoje). */
    today?: Snippet;
    /** "Lembrar disso" nas conversas recentes. */
    oncreatetask?: (fields: TaskFields) => Promise<void>;
    clock?: Date;
    /** "Responder depois" nas conversas recentes. */
    onlater?: (serviceId: string, chat: string, at: number) => Promise<void>;
  } = $props();

  /** Acima disso (`HEAVY_BYTES`), um serviço parado vira sugestão. */
  const IDLE_MINUTES = 10;

  const hour = new Date().getHours();
  const greeting = $derived(hour < 12 ? t("Bom dia") : hour < 18 ? t("Boa tarde") : t("Boa noite"));

  const memory = $derived(home.memory);
  const silence = $derived(snap.silenceSummary);
  /** Hora local (com o dia, se não for hoje). */
  function clockOf(unix: number) {
    const d = new Date(unix * 1000);
    const same = d.toDateString() === new Date().toDateString();
    return d.toLocaleString(locale(), same ? { hour: "2-digit", minute: "2-digit" } : { weekday: "short", hour: "2-digit", minute: "2-digit" });
  }

  const servicesSummary = $derived.by(() => {
    const n = snap.services.length;
    const parts = [n === 1 ? t("{n} conta", { n }) : t("{n} contas", { n })];
    if (memory) {
      const total = Object.values(memory.services).reduce((a, b) => a + b, 0);
      if (total > 0) parts.push(formatBytes(total));
    }
    const unread = snap.services.reduce((a, s) => a + s.unread, 0);
    if (unread > 0) parts.push(unread === 1 ? t("{n} não lida", { n: unread }) : t("{n} não lidas", { n: unread }));
    return parts.join(" · ");
  });

  // Conversas recentes: todas ou só as não lidas (escolha deste computador).
  const RECENT_KEY = "papeo.recentUnreadOnly";
  let unreadOnly = $state(readUnreadOnly());
  function readUnreadOnly(): boolean {
    try {
      return localStorage.getItem(RECENT_KEY) === "1";
    } catch {
      return false;
    }
  }
  function setUnreadOnly(value: boolean) {
    unreadOnly = value;
    try {
      if (value) localStorage.setItem(RECENT_KEY, "1");
      else localStorage.removeItem(RECENT_KEY);
    } catch {
      /* preferência local apenas */
    }
  }
  const unreadChats = $derived(snap.recent.filter((c) => c.unread));
  const shownChats = $derived(unreadOnly ? unreadChats : snap.recent);
  const reloading = $derived(home.reloading);
  const freeMemory = (ids: string[]) => freeMemoryOf(ids, run);

  const heaviest = $derived.by(() => {
    if (!memory) return null;
    let best: string | null = null;
    for (const [id, bytes] of Object.entries(memory.services)) {
      if (best === null || bytes > memory.services[best]) best = id;
    }
    return best;
  });

  /** "parado há 25 min", "parado há 3 h" ou, sem uso nesta sessão, "sem uso desde que o Papeo abriu". */
  function idleLabel(minutes: number | null | undefined): string {
    if (minutes == null) return t("sem uso desde que o Papeo abriu");
    return minutes < 60
      ? t("parado há {n} min", { n: minutes })
      : t("parado há {n} h", { n: Math.floor(minutes / 60) });
  }

  function suggestHibernate(service: ServiceView): boolean {
    const bytes = memory?.services[service.id] ?? 0;
    return (
      service.loaded &&
      service.id !== snap.active &&
      bytes >= HEAVY_BYTES &&
      (service.idleMinutes ?? Infinity) >= IDLE_MINUTES
    );
  }

  type Alert = {
    key: string;
    tone: "warn" | "danger" | "info";
    service: ServiceView;
    title: string;
    text: string;
    action: string;
    icon: typeof LogIn;
    busy?: boolean;
    onclick: () => void;
  };

  /** Problemas que pedem uma ação, cada um com o botão que resolve. */
  const alerts = $derived.by(() => {
    const out: Alert[] = [];
    for (const s of snap.services) {
      if (s.status === "loggedOut") {
        out.push({
          key: `out-${s.id}`,
          tone: "warn",
          service: s,
          title: t("{nome} saiu da conta", { nome: s.name }),
          text: t("as mensagens não chegam até você entrar de novo."),
          action: t("Entrar de novo"),
          icon: LogIn,
          onclick: () => onopen(s.id),
        });
      } else if (s.status === "unresponsive") {
        out.push({
          key: `stuck-${s.id}`,
          tone: "danger",
          service: s,
          title: t("{nome} não está respondendo", { nome: s.name }),
          text: t("recarregar costuma resolver (o login continua)."),
          action: reloading[s.id] ? t("Recarregando…") : t("Recarregar"),
          icon: RotateCw,
          busy: reloading[s.id],
          onclick: () => freeMemory([s.id]),
        });
      } else if (suggestHibernate(s) && !reloading[s.id]) {
        out.push({
          key: `heavy-${s.id}`,
          tone: "info",
          service: s,
          title: t("{nome} está usando {memoria}", { nome: s.name, memoria: formatBytes(memory?.services[s.id] ?? 0) }),
          text: t("{parado}; recarregar libera (as notificações continuam).", { parado: idleLabel(s.idleMinutes) }),
          action: t("Liberar memória"),
          icon: RotateCw,
          onclick: () => freeMemory([s.id]),
        });
      }
    }
    return out;
  });

  function host(url: string) {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }

  const mailboxes = $derived(snap.services.filter(isEmail));
  const mailUnread = $derived(mailboxes.reduce((a, s) => a + s.unread, 0));

  function mailState(s: ServiceView): string {
    // Pelo IMAP (leitor do Papeo ou avisos com o site hibernado): o número
    // vem do servidor, sem depender da página do site estar carregada.
    const imap = s.mailStatus?.state;
    if (imap === "auth" || imap === "noPassword") return t("Senha recusada: abra para entrar de novo");
    if (s.reader && imap === "connecting") return t("Conectando…");
    if (s.reader && imap === "offline") return t("Sem conexão; tentando de novo");
    // Só pelo site, sem IMAP: vale o estado da página.
    if (!s.reader && imap !== "ok") {
      if (s.status === "loggedOut") return t("Desconectado: abra para entrar");
      if (!s.loaded) return t("Hibernado: abra para atualizar");
    }
    if (s.chat) return t("Aberto: {nome}", { nome: s.chat });
    if (!s.unread) return t("Nenhuma não lida");
    return s.unread === 1 ? t("{n} não lida", { n: s.unread }) : t("{n} não lidas", { n: s.unread });
  }

  const closeServices = () => (home.servicesOpen = false);
  function openFromList(id: string) {
    closeServices();
    onopen(id);
  }
  // Sem contas, o modal não tem o que mostrar (o Início mostra o "Adicionar").
  $effect(() => {
    if (snap.services.length === 0) home.servicesOpen = false;
  });

  function serviceOf(id: string) {
    return snap.services.find((s) => s.id === id);
  }

  onMount(watchMemory);
</script>

<div class="home">
  <div class="head">
    <div>
      <h1>{greeting}!</h1>
      <p class="sub">{t("Seus mensageiros num só lugar, sem pesar no computador.")}</p>
    </div>
    <button class="btn btn-primary" onclick={onadd}><Plus size={16} /> {t("Adicionar serviço")}</button>
  </div>

  {#if alerts.length}
    <section class="alerts" aria-label={t("Precisa de atenção")}>
      {#each alerts as alert (alert.key)}
        <div class="alert {alert.tone}" role="status">
          <ServiceAvatar service={alert.service} size={20} />
          <p class="alert-text" title="{alert.title}: {alert.text}">
            <strong>{alert.title}</strong> — {alert.text}
          </p>
          <button class="btn small alert-action" disabled={alert.busy} onclick={alert.onclick}>
            <alert.icon size={14} />
            {alert.action}
          </button>
        </div>
      {/each}
    </section>
  {/if}

  {#if silence}
    <section class="card block" aria-labelledby="silence-title">
      <div class="block-head">
        <h2 id="silence-title"><MoonStar size={18} /> {t("Enquanto você estava em silêncio")}</h2>
        <button class="icon-btn ghost" onclick={() => run(api.dismissSilenceSummary())} aria-label={t("Fechar o resumo")} title={t("Fechar")}>
          <X size={16} />
        </button>
      </div>
      <p class="silence-head">
        {silence.total === 1
          ? t("{n} mensagem entre {de} e {ate}", { n: silence.total, de: clockOf(silence.since), ate: clockOf(silence.until) })
          : t("{n} mensagens entre {de} e {ate}", { n: silence.total, de: clockOf(silence.since), ate: clockOf(silence.until) })}
      </p>
      <ul class="silenced">
        {#each silence.accounts as a (a.serviceId)}
          {@const s = snap.services.find((x) => x.id === a.serviceId)}
          {#if s}
            <li>
              <ServiceAvatar service={s} size={26} dimmed={!s.loaded} />
              <span class="silenced-text">
                <span class="svc-name">{a.count === 1 ? t("{nome} · {n} mensagem", { nome: s.name, n: a.count }) : t("{nome} · {n} mensagens", { nome: s.name, n: a.count })}</span>
                {#if a.chats.length}
                  <span class="chats">
                    {#each a.chats as chat (chat)}
                      {#if s.kind === "whatsapp" || s.kind === "telegram"}
                        <button class="chip" onclick={() => onchat(s.id, chat)} title={t("Abrir {nome}", { nome: chat })}>{chat}</button>
                      {:else}
                        <span class="chip plain">{chat}</span>
                      {/if}
                    {/each}
                  </span>
                {/if}
              </span>
              <button class="btn small" onclick={() => onopen(s.id)} aria-label={t("Abrir {nome}", { nome: s.name })}>{t("Abrir")}</button>
            </li>
          {/if}
        {/each}
      </ul>
    </section>
  {/if}

  {@render today?.()}

  {#if mailboxes.length}
    <section class="card block" aria-labelledby="mail-title">
      <div class="block-head">
        <h2 id="mail-title"><Mail size={18} /> {t("E-mail")}</h2>
        {#if mailUnread}<span class="hint">{mailUnread === 1 ? t("{n} não lida", { n: mailUnread }) : t("{n} não lidas", { n: mailUnread })}</span>{/if}
      </div>
      <ul class="mailboxes">
        {#each mailboxes as s (s.id)}
          <li>
            <ServiceAvatar service={s} size={26} dimmed={!s.loaded} />
            <span class="mailbox-text">
              <span class="svc-name">{s.name}</span>
              <span class="mailbox-state" class:warn-text={s.status === "loggedOut"} title={mailState(s)}>
                {mailState(s)}
              </span>
            </span>
            {#if s.unread && s.loaded}<span class="count-badge" aria-hidden="true">{s.unread}</span>{/if}
            <button class="btn small" onclick={() => onopen(s.id)} aria-label={t("Abrir {nome}", { nome: s.name })}>{t("Abrir")}</button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if snap.recent.length}
    <section class="card block" aria-labelledby="recent-title">
      <div class="block-head">
        <h2 id="recent-title"><MessagesSquare size={18} /> {t("Conversas recentes")}</h2>
        <div class="segmented" role="group" aria-label={t("Mostrar conversas")}>
          <button aria-pressed={!unreadOnly} onclick={() => setUnreadOnly(false)}>{t("Todas")}</button>
          <button aria-pressed={unreadOnly} onclick={() => setUnreadOnly(true)}>
            {t("Não lidas")}{#if unreadChats.length}<span class="count">{unreadChats.length}</span>{/if}
          </button>
        </div>
      </div>
      {#if shownChats.length === 0}
        <p class="empty-note">{t("Nenhuma conversa não lida. Conversas lidas no WhatsApp, aqui ou no celular, saem desta lista.")}</p>
      {/if}
      <RecentChats
        chats={shownChats}
        services={snap.services}
        onsnap={(s) => run(Promise.resolve(s))}
        onopen={onchat}
        {oncreatetask}
        {clock}
        {onlater}
        workHours={snap.settings.mode === "auto" ? snap.settings.workHours : undefined}
        quickReplies={snap.settings.quickReplies}
      />
    </section>
  {/if}

  {#if snap.services.length === 0}
    <section class="card block" aria-labelledby="services-title">
      <div class="block-head">
        <h2 id="services-title">{t("Serviços")}</h2>
      </div>
      <div class="empty">
        <p>{t("Nenhum serviço configurado.")}</p>
        <button class="btn btn-primary" onclick={onadd}><Plus size={16} /> {t("Adicionar o primeiro")}</button>
      </div>
    </section>
  {/if}

  {#if snap.downloads.length > 0}
    <section class="card block" aria-labelledby="dl-title">
      <div class="block-head">
        <h2 id="dl-title">{t("Downloads recentes")}</h2>
        <div class="head-actions">
          <button class="btn small" onclick={() => run(api.openDownloadsDir())}><FolderOpen size={15} /> {t("Abrir pasta")}</button>
          <button class="btn small" onclick={() => run(api.clearDownloads())}>{t("Limpar lista")}</button>
        </div>
      </div>
      <ul class="downloads">
        {#each snap.downloads as d (d.id)}
          {@const svc = serviceOf(d.serviceId)}
          <li>
            {#if svc}<ServiceAvatar service={svc} size={22} />{/if}
            <span class="file">{d.fileName}</span>
            {#if d.state === "downloading"}
              <span class="pill">{t("Baixando…")}</span>
            {:else if d.state === "failed"}
              <span class="pill pill-danger">{t("Falhou")}</span>
            {:else}
              <button class="btn small" onclick={() => run(api.revealDownload(d.id))}>{t("Mostrar na pasta")}</button>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>

{#if home.servicesOpen && snap.services.length}
  <Modal onclose={closeServices} label={t("Serviços")} width={1040} flush>
    <div class="services">
      <header class="services-head">
        <span class="services-icon" aria-hidden="true"><LayoutList size={20} /></span>
        <div class="services-title">
          <h2>{t("Serviços")}</h2>
          <p>{servicesSummary}</p>
        </div>
        <button
          class="btn small"
          onclick={() => {
            closeServices();
            onadd();
          }}
        >
          <Plus size={15} /> {t("Adicionar serviço")}
        </button>
        <button class="close" onclick={closeServices} aria-label={t("Fechar")}><X size={18} /></button>
      </header>
      <div class="table-wrap" id="services-body">
      <table>
        <thead>
          <tr>
            <th>{t("Serviço")}</th>
            <th>{t("Estado")}</th>
            <th class="num">{t("Memória")}</th>
            <th class="num c-unread">{t("Não lidas")}</th>
            <th class="c-notif">{t("Notificações")}</th>
            <th class="c-hib">{t("Hibernar após")}</th>
            <th><span class="sr-only">{t("Ações")}</span></th>
          </tr>
        </thead>
        <tbody>
          {#each snap.services as service (service.id)}
            {@const bytes = memory?.services[service.id]}
            {@const suggest = suggestHibernate(service)}
            <tr class:suggest>
              <td>
                <button class="svc" onclick={() => openFromList(service.id)}>
                  <ServiceAvatar {service} size={30} dimmed={!service.loaded} />
                  <span class="svc-text">
                    <span class="svc-name">{service.name}</span>
                    <span class="svc-host">{host(service.url)}</span>
                  </span>
                </button>
              </td>
              <td>
                <span
                  class="pill"
                  class:pill-success={service.status === "ok"}
                  class:pill-warn={service.status === "loggedOut"}
                  class:pill-danger={service.status === "unresponsive"}
                >
                  {t(STATUS_LABEL[service.status])}
                </span>
                {#if service.mailStatus?.state === "ok"}
                  <span class="suggest-note">{t("Avisos por IMAP")}</span>
                {/if}
              </td>
              <td class="num">
                {#if bytes !== undefined}
                  <span class:heavy={service.id === heaviest && bytes >= HEAVY_BYTES}>{formatBytes(bytes)}</span>
                  {#if suggest}
                    <span class="suggest-note">{idleLabel(service.idleMinutes)}</span>
                  {/if}
                {:else}
                  <span class="muted">—</span>
                {/if}
              </td>
              <td class="num c-unread">{service.unread || "—"}</td>
              <td class="c-notif">
                <Switch
                  checked={!isMuted(service)}
                  label={t("Notificações de {nome}", { nome: service.name })}
                  onchange={(enabled) => run(api.setNotifications(service.id, enabled))}
                />
              </td>
              <td class="c-hib">
                <select
                  class="select"
                  aria-label={t("Hibernar {nome} após", { nome: service.name })}
                  value={service.hibernateAfterMinutes}
                  onchange={(e) => run(api.setHibernation(service.id, Number(e.currentTarget.value)))}
                >
                  {#each HIBERNATE_OPTIONS as opt (opt.minutes)}
                    <option value={opt.minutes}>{t(opt.label)}</option>
                  {/each}
                </select>
              </td>
              <td>
                <div class="actions">
                  <button class="btn c-open" onclick={() => openFromList(service.id)}>{t("Abrir")}</button>
                  <button
                    class="icon-btn"
                    onclick={() => oncustomize(service)}
                    title={t("Personalizar")}
                    aria-label={t("Personalizar {nome}", { nome: service.name })}
                  >
                    <Palette size={16} />
                  </button>
                  <button
                    class="icon-btn"
                    class:attention={suggest}
                    disabled={!service.loaded}
                    onclick={() => run(api.hibernate(service.id))}
                    title={suggest ? t("Sugestão: hibernar para liberar memória") : t("Hibernar agora")}
                    aria-label={t("Hibernar {nome} agora", { nome: service.name })}
                  >
                    <Snowflake size={16} />
                  </button>
                  <button
                    class="icon-btn danger"
                    onclick={() => onremove(service)}
                    title={t("Remover")}
                    aria-label={t("Remover {nome}", { nome: service.name })}
                  >
                    <Trash2 size={16} />
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      </div>
      <footer class="services-foot">
        {#if snap.services.some(suggestHibernate)}
          <p class="tip"><TriangleAlert size={15} /> {t("Destacados: pesados e parados. Hiberne para liberar memória.")}</p>
        {:else}
          <span>{t("Clique no nome para abrir a conta.")}</span>
        {/if}
        <span>{t("Arraste na barra lateral para reordenar · botão direito para mais opções")}</span>
      </footer>
    </div>
  </Modal>
{/if}

<style>
  .home {
    height: 100%;
    overflow-y: auto;
    padding: 30px 32px 40px;
    user-select: text;
    -webkit-user-select: text;
  }

  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 24px;
  }

  h1 {
    margin: 0;
    font-size: 34px;
    font-weight: 500;
    letter-spacing: -0.02em;
  }

  .sub {
    margin: 4px 0 0;
    color: var(--text-muted);
    font-size: 16px;
  }

  .alerts {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 18px;
  }

  .alert {
    --tone: var(--accent);
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 12px;
    border-radius: 10px;
    border: 1px solid color-mix(in srgb, var(--tone) 35%, var(--border));
    background: color-mix(in srgb, var(--tone) 8%, var(--bg));
  }

  .alert.warn {
    --tone: var(--warn);
  }

  .alert.danger {
    --tone: var(--danger);
  }

  .alert-text {
    flex: 1;
    min-width: 0;
    margin: 0;
    font-size: 13.5px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .alert-text strong {
    color: var(--text);
    font-weight: 600;
  }

  .alert-action {
    flex: none;
  }

  .block {
    padding: 20px 22px 8px;
    margin-bottom: 20px;
  }

  .block-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }







  .segmented {
    display: inline-flex;
    flex: none;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-subtle);
  }

  .segmented button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }

  .segmented button[aria-pressed="true"] {
    background: var(--bg);
    color: var(--text);
    box-shadow: var(--shadow-card);
  }

  .segmented .count {
    min-width: 16px;
    padding: 0 5px;
    border-radius: 8px;
    background: var(--danger);
    color: #fff;
    font-size: 11px;
    font-weight: 600;
    line-height: 16px;
    text-align: center;
  }

  .empty-note {
    margin: 4px 0 14px;
    font-size: 13px;
    color: var(--text-muted);
  }

  .head-actions {
    display: flex;
    gap: 6px;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 18px;
    font-weight: 550;
  }

  .hint {
    font-size: 12.5px;
    color: var(--text-muted);
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    text-align: left;
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-muted);
    padding: 10px 8px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }

  td {
    padding: 10px 8px;
    border-bottom: 1px solid var(--border);
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  tr.suggest td {
    background: color-mix(in srgb, var(--warn) 7%, transparent);
  }

  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  td.num {
    vertical-align: middle;
  }

  .heavy {
    color: var(--warn-text);
    font-weight: 600;
  }

  .suggest-note {
    display: block;
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .muted {
    color: var(--text-muted);
  }

  .svc {
    display: flex;
    align-items: center;
    gap: 12px;
    border: 0;
    background: none;
    padding: 0;
    text-align: left;
  }

  .svc-text {
    display: flex;
    flex-direction: column;
  }

  .svc-name {
    font-weight: 550;
  }

  .svc-host {
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .actions .btn {
    height: 32px;
    padding: 0 12px;
  }

  .actions .icon-btn {
    width: 32px;
    height: 32px;
  }

  .icon-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .icon-btn.attention {
    border-color: var(--warn);
    color: var(--warn-text);
  }

  .icon-btn.danger {
    color: var(--danger);
  }

  .tip {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0 12px;
    font-size: 13px;
    color: var(--warn-text);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 32px 0 28px;
    color: var(--text-muted);
  }

  .silence-head {
    margin: 0 0 4px;
    color: var(--text-muted);
    font-size: 13px;
  }

  .silenced {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
  }

  .silenced li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }

  .silenced li:first-child {
    border-top: 0;
  }

  .silenced-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .chats {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 2px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-subtle);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
  }

  button.chip {
    cursor: pointer;
  }

  button.chip:hover {
    background: var(--bg-hover);
  }

  .chip.plain {
    color: var(--text-muted);
  }

  .mailboxes {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
  }

  .mailboxes li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }

  .mailboxes li:first-child {
    border-top: 0;
  }

  .mailbox-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .mailbox-state {
    font-size: 12.5px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mailbox-state.warn-text {
    color: var(--warn-text);
  }

  .count-badge {
    flex: none;
    min-width: 22px;
    height: 22px;
    padding: 0 7px;
    border-radius: 11px;
    display: inline-grid;
    place-items: center;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .downloads {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .downloads li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }

  .downloads li:first-child {
    border-top: 0;
  }

  .file {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .btn.small {
    height: 30px;
    padding: 0 10px;
    font-size: 13px;
  }

  /* A tabela acompanha a largura disponível: some primeiro o "hibernar após"
     e as notificações (também no botão direito da conta); se ainda faltar
     espaço, ela rola na horizontal em vez de cortar. */
  /* Modal "Serviços": cabeçalho fixo e tabela rolando por baixo. */
  .services {
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    max-height: min(80vh, 760px);
  }

  .services-head {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 20px 20px 18px 24px;
    border-bottom: 1px solid var(--border);
  }

  .services-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 11px;
    border: 1px solid var(--border);
    background: var(--bg-subtle);
    color: var(--text-muted);
  }

  .services-title {
    flex: 1;
    min-width: 0;
  }

  .services-title h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .services-title p {
    margin: 2px 0 0;
    font-size: 13px;
    color: var(--text-muted);
  }

  .services .table-wrap {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 12px 8px;
  }

  .services thead th {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--bg);
  }

  .services-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 24px;
    border-top: 1px solid var(--border);
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .services-foot .tip {
    margin: 0;
  }

  .close {
    display: grid;
    place-items: center;
    flex: none;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
  }

  .close:hover {
    background: var(--bg-hover);
    color: var(--text);
  }

  .table-wrap {
    overflow-x: auto;
  }

  @container (max-width: 980px) {
    .c-hib {
      display: none;
    }
  }

  @container (max-width: 820px) {
    .c-notif {
      display: none;
    }
  }

  /* O nome da conta já abre o serviço; as não lidas aparecem na barra. */
  @container (max-width: 700px) {
    .c-open {
      display: none;
    }
  }

  @container (max-width: 560px) {
    .c-unread {
      display: none;
    }
  }
</style>
