<script lang="ts">
  import { ListTodo, MessageCircle, NotebookPen } from "@lucide/svelte";
  import type { Contact, Organizer, ServiceView, TaskFields } from "./api";
  import TaskForm from "./TaskForm.svelte";
  import TaskList, { type TaskActions } from "./TaskList.svelte";
  import { contactLabel } from "./tasks";

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
</script>

<div class="page">
  <div class="head">
    <div>
      <h1><ListTodo size={22} /> Tarefas</h1>
      <p class="sub">Lembretes e tarefas, soltos ou ligados a uma conversa.</p>
    </div>
    <label class="filter">
      <span>Mostrar</span>
      <select bind:value={filter}>
        <option value="all">Todas</option>
        <option value="loose">Sem conversa</option>
        {#each services as s (s.id)}
          <option value={s.id}>{s.name}</option>
        {/each}
      </select>
    </label>
  </div>

  <TaskForm placeholder="Nova tarefa ou lembrete…" {now} onsave={oncreate} />

  <section class="card block">
    <TaskList
      {tasks}
      {org}
      {services}
      {actions}
      {now}
      empty={filter === "all"
        ? "Nada por aqui. Crie uma tarefa acima ou, numa conversa, pelo painel do contato."
        : "Nenhuma tarefa neste filtro."}
    />
  </section>

  {#if notes.length}
    <section class="card block">
      <h2><NotebookPen size={17} /> Anotações</h2>
      <ul class="notes">
        {#each notes as c (c.id)}
          {@const service = serviceOf(c)}
          <li>
            <button class="note" disabled={!service} onclick={() => actions.open(c)}>
              <MessageCircle size={14} />
              <span class="who">{contactLabel(c, service?.name)}{service ? "" : " (conta removida)"}</span>
              <span class="line">{firstLine(c.note)}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>

<style>
  .page {
    height: 100%;
    overflow-y: auto;
    padding: 28px 32px 40px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 860px;
  }

  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  h1 {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    font-size: 24px;
    font-weight: 650;
    letter-spacing: -0.01em;
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

  select {
    height: 32px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
  }

  .block {
    padding: 12px;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 8px 8px;
    font-size: 15px;
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
