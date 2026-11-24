<script lang="ts">
  import { Bell, Layers, MemoryStick, Palette, Plus, Snowflake, Timer } from "@lucide/svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import { HIBERNATE_OPTIONS, type ServiceView, type Snapshot } from "./api";

  let {
    snap,
    onadd,
    onopen,
    onhibernate,
    onremove,
    onsethibernation,
    onsetnotifications,
    oncustomize,
  }: {
    snap: Snapshot;
    onadd: () => void;
    onopen: (id: string) => void;
    onhibernate: (id: string) => void;
    onremove: (service: ServiceView) => void;
    onsethibernation: (id: string, minutes: number) => void;
    onsetnotifications: (id: string, enabled: boolean) => void;
    oncustomize: (service: ServiceView) => void;
  } = $props();

  const hour = new Date().getHours();
  const greeting = hour < 12 ? "Bom dia" : hour < 18 ? "Boa tarde" : "Boa noite";

  const loaded = $derived(snap.services.filter((s) => s.loaded).length);
  const auto = $derived(snap.services.filter((s) => s.hibernateAfterMinutes > 0).length);
  const withUnread = $derived(snap.services.filter((s) => s.unread > 0).length);

  const stats = $derived([
    {
      icon: Layers,
      label: "Serviços",
      value: snap.services.length,
      note: snap.services.length === 1 ? "configurado" : "configurados",
    },
    { icon: MemoryStick, label: "Em memória", value: loaded, note: `${snap.services.length - loaded} dormindo` },
    {
      icon: Bell,
      label: "Não lidas",
      value: snap.totalUnread,
      note: withUnread ? `em ${withUnread} ${withUnread === 1 ? "serviço" : "serviços"}` : "tudo em dia",
    },
    { icon: Timer, label: "Hibernação automática", value: auto, note: auto === 1 ? "serviço ativo" : "serviços ativos" },
  ]);

  function host(url: string) {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }
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

  <section class="card services" aria-labelledby="services-title">
    <div class="services-head">
      <h2 id="services-title">Serviços</h2>
      <span class="hint">Clique para abrir · botão direito na barra lateral para mais opções</span>
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
            <th class="num">Não lidas</th>
            <th>Notificações</th>
            <th>Hibernar após</th>
            <th><span class="sr-only">Ações</span></th>
          </tr>
        </thead>
        <tbody>
          {#each snap.services as service (service.id)}
            <tr>
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
                {#if service.loaded}
                  <span class="pill pill-success">Em memória</span>
                {:else}
                  <span class="pill">Dormindo</span>
                {/if}
              </td>
              <td class="num">{service.unread || "—"}</td>
              <td>
                <label class="switch">
                  <input
                    type="checkbox"
                    role="switch"
                    checked={service.notifications}
                    onchange={(e) => onsetnotifications(service.id, e.currentTarget.checked)}
                  />
                  <span class="track" aria-hidden="true"></span>
                  <span class="sr-only">Notificações de {service.name}</span>
                </label>
              </td>
              <td>
                <select
                  class="select"
                  aria-label="Hibernar {service.name} após"
                  value={service.hibernateAfterMinutes}
                  onchange={(e) => onsethibernation(service.id, Number(e.currentTarget.value))}
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
                  title="Personalizar nome, cor e emoji"
                  aria-label="Personalizar {service.name}"
                >
                  <Palette size={16} />
                </button>
                <button
                  class="icon-btn"
                  disabled={!service.loaded}
                  onclick={() => onhibernate(service.id)}
                  title="Hibernar agora"
                  aria-label="Hibernar {service.name} agora"
                >
                  <Snowflake size={16} />
                </button>
                <button class="btn btn-danger" onclick={() => onremove(service)}>Remover</button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>
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
    font-size: 30px;
    font-weight: 450;
    line-height: 1.1;
    font-variant-numeric: tabular-nums;
  }

  .stat-note {
    font-size: 13px;
    color: var(--text-muted);
    text-align: right;
  }

  .services {
    padding: 20px 22px 8px;
  }

  .services-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
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
  }

  td {
    padding: 10px 8px;
    border-bottom: 1px solid var(--border);
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
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

  .switch {
    position: relative;
    display: inline-flex;
    cursor: pointer;
  }

  .switch input {
    position: absolute;
    opacity: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    cursor: pointer;
  }

  .track {
    width: 36px;
    height: 20px;
    border-radius: 999px;
    background: var(--border-strong);
    position: relative;
    transition: background 0.15s;
  }

  .track::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.2);
    transition: transform 0.15s;
  }

  .switch input:checked + .track {
    background: var(--success-text);
  }

  .switch input:checked + .track::after {
    transform: translateX(16px);
  }

  .switch input:focus-visible + .track {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 32px 0 28px;
    color: var(--text-muted);
  }
</style>
