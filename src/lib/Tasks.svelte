<script lang="ts">
  import { ChartNoAxesColumn, Inbox, MessageCircle, NotebookPen, Users } from "@lucide/svelte";
  import type { Contact, Organizer, ServiceView, TaskFields } from "./api";
  import TaskForm from "./TaskForm.svelte";
  import TaskList, { type TaskActions } from "./TaskList.svelte";
  import { GROUP_LABEL, contactLabel, groupOf, type Group } from "./tasks";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import { t } from "./i18n.svelte";

  let {
    org,
    services,
    actions,
    now,
    oncreate,
  }: {
    org: Organizer;
    services: ServiceView[];
    actions: TaskActions;
    now: Date;
    oncreate: (fields: TaskFields) => Promise<void>;
  } = $props();

  /** "all", "loose" (sem vínculo) ou o id de uma conta. */
  let filter = $state("all");

  const serviceOf = (c: Contact) => services.find((s) => s.id === c.serviceId);
  const tasks = $derived(
    org.tasks.filter((t) => {
      if (filter === "all") return true;
      const c = t.contact ? org.contacts.find((x) => x.id === t.contact) : undefined;
      if (filter === "loose") return !c;
      return c?.serviceId === filter;
    }),
  );
  const notes = $derived(
    org.contacts
      .filter((c) => c.note.trim())
      .filter((c) => filter === "all" || c.serviceId === filter)
      .sort((a, b) => b.updatedAt - a.updatedAt),
  );
  const firstLine = (note: string) => note.trim().split("\n")[0];

  // Contagem por grupo no filtro atual (resumo do topo e painel lateral).
  const byGroup = $derived.by(() => {
    const c: Record<Group, number> = { overdue: 0, today: 0, upcoming: 0, someday: 0, done: 0 };
    for (const t of tasks) c[groupOf(t, now)]++;
    return c;
  });
  const summary = $derived({
    overdue: byGroup.overdue,
    today: byGroup.today,
    open: byGroup.overdue + byGroup.today + byGroup.upcoming + byGroup.someday,
  });
  const GROUPS: Group[] = ["overdue", "today", "upcoming", "someday", "done"];

  // "Por conta": tarefas abertas de cada conta (todas, sem o filtro), que viram atalho de filtro.
  const perAccount = $derived.by(() => {
    const open = org.tasks.filter((t) => !t.done);
    const count = (id: string | null) =>
      open.filter((t) => {
        const c = t.contact ? org.contacts.find((x) => x.id === t.contact) : undefined;
        return id === null ? !c : c?.serviceId === id;
      }).length;
    return {
      loose: count(null),
      services: services.map((s) => ({ service: s, n: count(s.id) })).filter((x) => x.n > 0),
    };
  });
  const toggleFilter = (value: string) => (filter = filter === value ? "all" : value);
</script>

<div class="page">
  <div class="head">
    <div>
      <h1>{t("Tarefas")}</h1>
      <p class="sub">{t("Lembretes e tarefas, soltos ou ligados a uma conversa.")}</p>
    </div>
    <label class="filter">
      <span>{t("Mostrar")}</span>
      <select class="select" bind:value={filter}>
        <option value="all">{t("Todas")}</option>
        <option value="loose">{t("Sem conversa")}</option>
        {#each services as s (s.id)}
          <option value={s.id}>{s.name}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if summary.open}
    <div class="stats" aria-label={t("Resumo")}>
      {#if summary.overdue}
        <span class="stat late"><strong>{summary.overdue}</strong> {summary.overdue === 1 ? t("atrasada") : t("atrasadas")}</span>
      {/if}
      <span class="stat"><strong>{summary.today}</strong> {t("para hoje")}</span>
      <span class="stat"><strong>{summary.open}</strong> {summary.open === 1 ? t("aberta") : t("abertas")}</span>
    </div>
  {/if}

  <div class="layout">
    <div class="main-col">
      <TaskForm placeholder={t("Nova tarefa ou lembrete…")} {now} onsave={oncreate} />

      <section class="card block">
        <TaskList
          {tasks}
          {org}
          {services}
          {actions}
          {now}
          empty={filter === "all"
            ? t("Nada por aqui. Crie uma tarefa acima ou, numa conversa, pelo painel do contato.")
            : t("Nenhuma tarefa neste filtro.")}
        />
      </section>
    </div>

    <aside class="side" aria-label={t("Resumo das tarefas")}>
      <section class="card side-card">
        <h2><ChartNoAxesColumn size={16} /> {t("Resumo")}</h2>
        <ul class="tally">
          {#each GROUPS as g (g)}
            <li class:late={g === "overdue" && byGroup.overdue > 0} class:zero={byGroup[g] === 0}>
              <span>{t(GROUP_LABEL[g])}</span>
              <strong>{byGroup[g]}</strong>
            </li>
          {/each}
        </ul>
      </section>

      {#if perAccount.services.length || perAccount.loose}
        <section class="card side-card">
          <h2><Users size={16} /> {t("Por conta")}</h2>
          <ul class="accounts">
            {#each perAccount.services as { service, n } (service.id)}
              <li>
                <button
                  class="account"
                  aria-pressed={filter === service.id}
                  onclick={() => toggleFilter(service.id)}
                  title={filter === service.id ? t("Mostrar todas") : t("Só {nome}", { nome: service.name })}
                >
                  <ServiceAvatar {service} size={22} />
                  <span class="acc-name">{service.name}</span>
                  <span class="acc-n">{n}</span>
                </button>
              </li>
            {/each}
            {#if perAccount.loose}
              <li>
                <button
                  class="account"
                  aria-pressed={filter === "loose"}
                  onclick={() => toggleFilter("loose")}
                  title={filter === "loose" ? t("Mostrar todas") : t("Só as sem conversa")}
                >
                  <span class="loose-icon"><Inbox size={14} /></span>
                  <span class="acc-name">{t("Sem conversa")}</span>
                  <span class="acc-n">{perAccount.loose}</span>
                </button>
              </li>
            {/if}
          </ul>
        </section>
      {/if}

      {#if notes.length}
        <section class="card side-card notes-card">
          <h2><NotebookPen size={16} /> {t("Anotações")} <span class="count">{notes.length}</span></h2>
          <ul class="notes">
            {#each notes as c (c.id)}
              {@const service = serviceOf(c)}
              <li>
                <button class="note" disabled={!service} onclick={() => actions.open(c)}>
                  <MessageCircle size={14} />
                  <span class="who">{contactLabel(c, service?.name)}{service ? "" : " " + t("(conta removida)")}</span>
                  <span class="line">{firstLine(c.note)}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    </aside>
  </div>
</div>

<style>
  /* Uma coluna em janela estreita; em tela larga, lista + painel lateral
     (resumo, por conta e anotações), até 1320 px e centralizado. */
  .page {
    height: 100%;
    overflow-y: auto;
    padding: 30px 32px 40px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    container-type: inline-size;
  }

  .page > :global(*) {
    width: 100%;
    max-width: 1320px;
    margin-inline: auto;
  }

  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 16px;
    align-items: start;
  }

  .main-col,
  .side {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }

  /* Estreito: o painel só traz as anotações (as contagens ficam no topo). */
  .side-card:not(.notes-card) {
    display: none;
  }

  @container (min-width: 1040px) {
    .layout {
      grid-template-columns: minmax(0, 1fr) 320px;
    }

    .side {
      position: sticky;
      top: 0;
    }

    .side-card:not(.notes-card) {
      display: block;
    }

    .page .stats {
      display: none;
    }
  }

  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  h1 {
    margin: 0;
    font-size: 30px;
    font-weight: 500;
    letter-spacing: -0.02em;
  }

  .sub {
    margin: 4px 0 0;
    color: var(--text-muted);
  }

  .filter {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
    font-size: 13px;
  }

  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: -4px;
  }

  .stat {
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .stat strong {
    color: var(--text);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .stat.late,
  .stat.late strong {
    color: var(--danger);
  }

  .block {
    padding: 12px;
  }

  .side-card {
    padding: 14px 12px 10px;
  }

  .side-card h2 {
    margin: 0 4px 8px;
  }

  .tally,
  .accounts {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .tally li {
    display: flex;
    justify-content: space-between;
    padding: 6px 4px;
    font-size: 13.5px;
    color: var(--text-muted);
  }

  .tally li + li {
    border-top: 1px solid var(--border);
  }

  .tally strong {
    color: var(--text);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .tally .zero strong {
    color: var(--text-faint);
    font-weight: 500;
  }

  .tally .late,
  .tally .late strong {
    color: var(--danger);
  }

  .account {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 4px;
    border: 0;
    border-radius: 8px;
    background: none;
    text-align: left;
  }

  .account:hover {
    background: var(--bg-hover);
  }

  .account[aria-pressed="true"] {
    background: var(--bg-hover);
    box-shadow: inset 0 0 0 1px var(--border-strong);
  }

  .loose-icon {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 30%;
    background: var(--bg-hover);
    color: var(--text-muted);
  }

  .acc-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .acc-n {
    font-size: 12.5px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 8px 8px;
    font-size: 15px;
    font-weight: 600;
  }

  h2 :global(svg) {
    color: var(--text-muted);
  }

  .count {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-faint);
  }

  .notes {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .note {
    display: grid;
    grid-template-columns: auto minmax(0, max-content) minmax(0, 1fr);
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px;
    border: 0;
    border-radius: 8px;
    background: none;
    text-align: left;
  }

  .note:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  .note:disabled {
    cursor: default;
  }

  .who {
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .line {
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
