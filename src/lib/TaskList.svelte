<script lang="ts" module>
  import type { Contact, TaskFields } from "./api";

  /** Ações sobre tarefas, definidas uma vez no App. */
  export interface TaskActions {
    done: (id: string, done: boolean) => void;
    snooze: (id: string, until: number) => void;
    remove: (id: string) => void;
    edit: (id: string, fields: TaskFields) => Promise<void>;
    open: (contact: Contact) => void;
  }
</script>

<script lang="ts">
  import { ChevronDown, ChevronRight } from "@lucide/svelte";
  import type { Organizer, ServiceView, Task } from "./api";
  import TaskItem from "./TaskItem.svelte";
  import { GROUP_LABEL, contactOf, groupTasks } from "./tasks";

  let {
    tasks,
    org,
    services,
    actions,
    now,
    showContact = true,
    headings = true,
    empty = "Nenhuma tarefa.",
  }: {
    tasks: Task[];
    org: Organizer;
    services: ServiceView[];
    actions: TaskActions;
    now: Date;
    showContact?: boolean;
    headings?: boolean;
    empty?: string;
  } = $props();

  let showDone = $state(false);
  const groups = $derived(groupTasks(tasks, now));
</script>

{#if !groups.length}
  <p class="empty">{empty}</p>
{/if}
{#each groups as g (g.group)}
  <section class="group">
    {#if g.group === "done"}
      <button class="heading toggle" onclick={() => (showDone = !showDone)} aria-expanded={showDone}>
        {#if showDone}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
        {GROUP_LABEL.done} <span class="count">{g.tasks.length}</span>
      </button>
    {:else if headings}
      <h3 class="heading" class:late={g.group === "overdue"}>
        {GROUP_LABEL[g.group]} <span class="count">{g.tasks.length}</span>
      </h3>
    {/if}
    {#if g.group !== "done" || showDone}
      <div class="items">
        {#each g.tasks as task (task.id)}
          <TaskItem
            {task}
            contact={contactOf(org, task)}
            {services}
            {now}
            {showContact}
            ondone={(done) => actions.done(task.id, done)}
            onsnooze={(until) => actions.snooze(task.id, until)}
            ondelete={() => actions.remove(task.id)}
            onedit={(fields) => actions.edit(task.id, fields)}
            onopen={actions.open}
          />
        {/each}
      </div>
    {/if}
  </section>
{/each}

<style>
  .group + .group {
    margin-top: 10px;
  }

  .heading {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 2px;
    padding: 4px 8px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .heading.late {
    color: var(--danger);
  }

  .toggle {
    border: 0;
    background: none;
    border-radius: 6px;
  }

  .toggle:hover {
    background: var(--bg-hover);
  }

  .count {
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .items {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .empty {
    margin: 8px;
    color: var(--text-muted);
    font-size: 13px;
  }
</style>
