<script lang="ts">
  import { AlarmClock, Check, MessageCircle, Pencil, Trash2 } from "@lucide/svelte";
  import type { Contact, ServiceView, Task, TaskFields } from "./api";
  import TaskForm from "./TaskForm.svelte";
  import { contactLabel, groupOf, quickTimes, whenLabel } from "./tasks";

  let {
    task,
    contact,
    services,
    now,
    showContact = true,
    ondone,
    onsnooze,
    ondelete,
    onedit,
    onopen,
  }: {
    task: Task;
    contact?: Contact;
    services: ServiceView[];
    now: Date;
    showContact?: boolean;
    ondone: (done: boolean) => void;
    onsnooze: (until: number) => void;
    ondelete: () => void;
    onedit: (fields: TaskFields) => Promise<void>;
    /** Abre a conversa do contato. */
    onopen?: (contact: Contact) => void;
  } = $props();

  let editing = $state(false);
  let snoozing = $state(false);

  const service = $derived(contact ? services.find((s) => s.id === contact.serviceId) : undefined);
  const label = $derived(
    contact ? contactLabel(contact, service?.name) + (service ? "" : " (conta removida)") : "",
  );
  const when = $derived(whenLabel(task, now));
  const late = $derived(groupOf(task, now) === "overdue");
</script>

{#if editing}
  <TaskForm
    {task}
    keepContact={task.contact ?? null}
    keepLabel={label}
    {now}
    compact
    oncancel={() => (editing = false)}
    onsave={async (fields) => {
      await onedit(fields);
      editing = false;
    }}
  />
{:else}
  <div class="task" class:done={task.done}>
    <button
      class="check"
      class:on={task.done}
      onclick={() => ondone(!task.done)}
      aria-label={task.done ? `Reabrir: ${task.text}` : `Concluir: ${task.text}`}
      aria-pressed={task.done}
    >
      {#if task.done}<Check size={13} />{/if}
    </button>
    <div class="body">
      <span class="text">{task.text}</span>
      {#if (when && !task.done) || (showContact && contact)}
        <span class="meta">
          {#if when && !task.done}
            <span class="when" class:late>
              {#if task.remindAt}<AlarmClock size={12} />{/if}
              {late ? "Atrasada · " : ""}{when}
            </span>
          {/if}
          {#if showContact && contact}
            {#if service && onopen}
              <button class="contact" onclick={() => onopen(contact)} title="Abrir a conversa">
                <MessageCircle size={12} />
                <span>{label}</span>
              </button>
            {:else}
              <span class="contact static"><MessageCircle size={12} /><span>{label}</span></span>
            {/if}
          {/if}
        </span>
      {/if}
      {#if snoozing}
        <span class="snooze">
          {#each quickTimes(now) as q (q.label)}
            <button
              class="mini"
              onclick={() => {
                snoozing = false;
                onsnooze(q.at);
              }}>{q.label}</button
            >
          {/each}
          <button class="mini" onclick={() => (snoozing = false)}>Cancelar</button>
        </span>
      {/if}
    </div>
    <div class="tools">
      {#if !task.done && task.remindAt}
        <button class="tool" onclick={() => (snoozing = !snoozing)} title="Adiar" aria-label="Adiar {task.text}">
          <AlarmClock size={15} />
        </button>
      {/if}
      <button class="tool" onclick={() => (editing = true)} title="Editar" aria-label="Editar {task.text}">
        <Pencil size={15} />
      </button>
      <button class="tool danger" onclick={ondelete} title="Apagar" aria-label="Apagar {task.text}">
        <Trash2 size={15} />
      </button>
    </div>
  </div>
{/if}

<style>
  .task {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 8px 6px 8px 8px;
    border-radius: 8px;
  }

  .task:hover,
  .task:focus-within {
    background: var(--bg-hover);
  }

  .check {
    flex: none;
    display: inline-grid;
    place-items: center;
    width: 18px;
    height: 18px;
    margin-top: 1px;
    padding: 0;
    border: 1.5px solid var(--border-strong);
    border-radius: 50%;
    background: var(--bg);
  }

  .check:hover {
    border-color: var(--text-muted);
  }

  .check.on {
    background: var(--success-text);
    border-color: var(--success-text);
    color: var(--bg);
  }

  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .text {
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }

  .done .text {
    color: var(--text-muted);
    text-decoration: line-through;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .when,
  .contact {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }

  .when.late {
    color: var(--danger);
    font-weight: 550;
  }

  .contact {
    max-width: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-muted);
    font-size: inherit;
  }

  .contact span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  button.contact:hover {
    color: var(--accent);
  }

  .snooze {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 4px;
  }

  .mini {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    font-size: 12px;
  }

  .mini:hover {
    background: var(--bg-subtle);
  }

  .tools {
    flex: none;
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.12s;
  }

  .task:hover .tools,
  .task:focus-within .tools {
    opacity: 1;
  }

  .tool {
    display: inline-grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
  }

  .tool:hover {
    background: var(--bg-subtle);
    color: var(--text);
  }

  .tool.danger:hover {
    color: var(--danger);
  }
</style>
