<script lang="ts">
  import { onMount } from "svelte";
  import {
    Bell,
    FileDown,
    FolderOpen,
    Layers,
    MemoryStick,
    Palette,
    Plus,
    Snowflake,
    Trash2,
    TriangleAlert,
  } from "@lucide/svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import Switch from "./Switch.svelte";
  import {
    HIBERNATE_OPTIONS,
    STATUS_LABEL,
    api,
    formatBytes,
    isMuted,
    type MemoryReport,
    type ServiceView,
    type Snapshot,
  } from "./api";

  let {
    snap,
    run,
    onadd,
    onopen,
    onremove,
    oncustomize,
  }: {
    snap: Snapshot;
    run: (action: Promise<Snapshot | void>) => Promise<void>;
    onadd: () => void;
    onopen: (id: string) => void;
    onremove: (service: ServiceView) => void;
    oncustomize: (service: ServiceView) => void;
  } = $props();

  /** Acima disso, um serviço parado vira sugestão de hibernação. */
  const HEAVY_BYTES = 300 * 1024 ** 2;
  const IDLE_MINUTES = 10;

  const hour = new Date().getHours();
  const greeting = hour < 12 ? "Bom dia" : hour < 18 ? "Boa tarde" : "Boa noite";

  let memory = $state<MemoryReport | null>(null);

  const loaded = $derived(snap.services.filter((s) => s.loaded).length);
  const withUnread = $derived(snap.services.filter((s) => s.unread > 0).length);
  const servicesBytes = $derived(
    memory ? Object.values(memory.services).reduce((a, b) => a + b, 0) : 0,
  );
  const heaviest = $derived.by(() => {
    if (!memory) return null;
    let best: string | null = null;
    for (const [id, bytes] of Object.entries(memory.services)) {
      if (best === null || bytes > memory.services[best]) best = id;
    }
    return best;
  });

  function suggestHibernate(service: ServiceView): boolean {
    const bytes = memory?.services[service.id] ?? 0;
    return (
      service.loaded &&
      service.id !== snap.active &&
      bytes >= HEAVY_BYTES &&
      (service.idleMinutes ?? Infinity) >= IDLE_MINUTES
    );
  }

  const stats = $derived([
    {
      icon: Layers,
      label: "Serviços",
      value: String(snap.services.length),
      note: `${loaded} em memória · ${snap.services.length - loaded} dormindo`,
    },
    {
      icon: Bell,
      label: "Não lidas",
      value: String(snap.totalUnread),
      note: withUnread ? `em ${withUnread} ${withUnread === 1 ? "serviço" : "serviços"}` : "tudo em dia",
    },
    {
      icon: MemoryStick,
      label: "Memória dos serviços",
      value: memory ? formatBytes(servicesBytes) : "—",
      note: memory ? `+ ${formatBytes(memory.app)} do Papeo` : "medindo…",
    },
    {
      icon: FileDown,
      label: "Downloads",
      value: String(snap.downloads.filter((d) => d.state === "done").length),
      note: snap.downloads.some((d) => d.state === "downloading") ? "baixando agora" : "nesta sessão",
    },
  ]);

  function host(url: string) {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }

  function serviceOf(id: string) {
    return snap.services.find((s) => s.id === id);
  }

  onMount(() => {
    const refresh = () =>
      api
        .getMemory()
        .then((m) => (memory = m))
        .catch(() => {});
    refresh();
    const timer = setInterval(refresh, 10_000);
    return () => clearInterval(timer);
  });
</script>

<div class="home">
  <div class="head">
    <div>
      <h1>{greeting}!</h1>
      <p class="sub">Seus mensageiros num só lugar, sem pesar no computador.</p>
    </div>
    <button class="btn btn-primary" onclick={onadd}><Plus size={16} /> Adicionar serviço</button>
  </div>

  <section class="stats" aria-label="Resumo">
    {#each stats as stat (stat.label)}
      <div class="stat card">
        <div class="stat-label"><stat.icon size={17} /> {stat.label}</div>
        <div class="stat-body">
          <span class="stat-value">{stat.value}</span>
          <span class="stat-note">{stat.note}</span>
        </div>
      </div>
    {/each}
  </section>

  <section class="card block" aria-labelledby="services-title">
    <div class="block-head">
      <h2 id="services-title">Serviços</h2>
      <span class="hint">Arraste na barra lateral para reordenar · botão direito para mais opções</span>
    </div>

    {#if snap.services.length === 0}
      <div class="empty">
        <p>Nenhum serviço configurado.</p>
        <button class="btn btn-primary" onclick={onadd}><Plus size={16} /> Adicionar o primeiro</button>
      </div>
    {:else}
      <table>
        <thead>
          <tr>
            <th>Serviço</th>
            <th>Estado</th>
            <th class="num">Memória</th>
            <th class="num">Não lidas</th>
            <th>Notificações</th>
            <th>Hibernar após</th>
            <th><span class="sr-only">Ações</span></th>
          </tr>
        </thead>
        <tbody>
          {#each snap.services as service (service.id)}
            {@const bytes = memory?.services[service.id]}
            {@const suggest = suggestHibernate(service)}
            <tr class:suggest>
              <td>
                <button class="svc" onclick={() => onopen(service.id)}>
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
                  {STATUS_LABEL[service.status]}
                </span>
              </td>
              <td class="num">
                {#if bytes !== undefined}
                  <span class:heavy={service.id === heaviest && bytes >= HEAVY_BYTES}>{formatBytes(bytes)}</span>
                  {#if suggest}
                    <span class="suggest-note">parado há {service.idleMinutes} min</span>
                  {/if}
                {:else}
                  <span class="muted">—</span>
                {/if}
              </td>
              <td class="num">{service.unread || "—"}</td>
              <td>
                <Switch
                  checked={!isMuted(service)}
                  label="Notificações de {service.name}"
                  onchange={(enabled) => run(api.setNotifications(service.id, enabled))}
                />
              </td>
              <td>
                <select
                  class="select"
                  aria-label="Hibernar {service.name} após"
                  value={service.hibernateAfterMinutes}
                  onchange={(e) => run(api.setHibernation(service.id, Number(e.currentTarget.value)))}
                >
                  {#each HIBERNATE_OPTIONS as opt (opt.minutes)}
                    <option value={opt.minutes}>{opt.label}</option>
                  {/each}
                </select>
              </td>
              <td>
                <div class="actions">
                  <button class="btn" onclick={() => onopen(service.id)}>Abrir</button>
                  <button
                    class="icon-btn"
                    onclick={() => oncustomize(service)}
                    title="Personalizar"
                    aria-label="Personalizar {service.name}"
                  >
                    <Palette size={16} />
                  </button>
                  <button
                    class="icon-btn"
                    class:attention={suggest}
                    disabled={!service.loaded}
                    onclick={() => run(api.hibernate(service.id))}
                    title={suggest ? "Sugestão: hibernar para liberar memória" : "Hibernar agora"}
                    aria-label="Hibernar {service.name} agora"
                  >
                    <Snowflake size={16} />
                  </button>
                  <button
                    class="icon-btn danger"
                    onclick={() => onremove(service)}
                    title="Remover"
                    aria-label="Remover {service.name}"
                  >
                    <Trash2 size={16} />
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if snap.services.some(suggestHibernate)}
        <p class="tip"><TriangleAlert size={15} /> Serviços destacados estão pesados e parados: hiberne para liberar memória.</p>
      {/if}
    {/if}
  </section>

  {#if snap.downloads.length > 0}
    <section class="card block" aria-labelledby="dl-title">
      <div class="block-head">
        <h2 id="dl-title">Downloads recentes</h2>
        <div class="head-actions">
          <button class="btn small" onclick={() => run(api.openDownloadsDir())}><FolderOpen size={15} /> Abrir pasta</button>
          <button class="btn small" onclick={() => run(api.clearDownloads())}>Limpar lista</button>
        </div>
      </div>
      <ul class="downloads">
        {#each snap.downloads as d (d.id)}
          {@const svc = serviceOf(d.serviceId)}
          <li>
            {#if svc}<ServiceAvatar service={svc} size={22} />{/if}
            <span class="file">{d.fileName}</span>
            {#if d.state === "downloading"}
              <span class="pill">Baixando…</span>
            {:else if d.state === "failed"}
              <span class="pill pill-danger">Falhou</span>
            {:else}
              <button class="btn small" onclick={() => run(api.revealDownload(d.id))}>Mostrar na pasta</button>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>

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

  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
    gap: 16px;
    margin-bottom: 20px;
  }

  .stat {
    padding: 6px;
    background: var(--bg-subtle);
  }

  .stat-label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px 10px;
    font-weight: 550;
    font-size: 15px;
  }

  .stat-label :global(svg) {
    color: var(--text-muted);
  }

  .stat-body {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 8px;
    padding: 12px 14px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--bg);
  }

  .stat-value {
    font-size: 28px;
    font-weight: 450;
    line-height: 1.1;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .stat-note {
    font-size: 12.5px;
    color: var(--text-muted);
    text-align: right;
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

  .head-actions {
    display: flex;
    gap: 6px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 550;
  }

  .hint {
    font-size: 12.5px;
    color: var(--text-faint);
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
    color: var(--text-faint);
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
</style>
