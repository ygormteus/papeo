<script lang="ts">
  import { AlarmClock, CalendarDays, Link2, X } from "@lucide/svelte";
  import type { ChatLink, Task, TaskFields } from "./api";
  import { dateKey, fromInputs, quickTimes, toTimeInput, whenLabel } from "./tasks";

  let {
    link = null,
    linkLabel = "",
    task = null,
    keepContact = null,
    keepLabel = "",
    placeholder = "Nova tarefa…",
    compact = false,
    now,
    onsave,
    oncancel,
  }: {
    /** Conversa a que a tarefa nova se liga (pode ser removida). */
    link?: ChatLink | null;
    linkLabel?: string;
    /** Editando esta tarefa. */
    task?: Task | null;
    /** Contato atual da tarefa editada (mantido se não for removido). */
    keepContact?: string | null;
    keepLabel?: string;
    placeholder?: string;
    compact?: boolean;
    /** Relógio do app (avança a cada 30 s). */
    now: Date;
    onsave: (fields: TaskFields) => Promise<void>;
    oncancel?: () => void;
  } = $props();

  // Valores iniciais: a tarefa editada ou vazio (o formulário é recriado
  // para cada tarefa, então ler as props só no início é o esperado).
  const initial = (() => task)();
  let text = $state(initial?.text ?? "");
  let remindAt = $state<number | null>(initial?.remindAt ?? null);
  let due = $state<string | null>(initial?.due ?? null);
  let linked = $state(true);
  let picking = $state(false);
  let pickDate = $state("");
  let pickTime = $state("");
  let busy = $state(false);
  let error = $state("");
  let input = $state<HTMLInputElement>();

  const quick = $derived(quickTimes(now));
  const when = $derived(
    whenLabel(
      { id: "", text, done: false, createdAt: 0, updatedAt: 0, remindAt: remindAt ?? undefined, due: due ?? undefined },
      now,
    ),
  );
  const hasLink = $derived(linked && (!!link || !!keepContact));
  const label = $derived(link ? linkLabel : keepLabel);

  function choose(at: number) {
    remindAt = at;
    due = null;
    picking = false;
  }

  function openPicker() {
    picking = !picking;
    const base = remindAt ? new Date(remindAt * 1000) : null;
    pickDate = due ?? (base ? dateKey(base) : dateKey(new Date()));
    pickTime = remindAt ? toTimeInput(remindAt) : "";
  }

  function applyPicker() {
    if (!pickDate) return;
    if (pickTime) {
      const at = fromInputs(pickDate, pickTime);
      if (at == null) return;
      remindAt = at;
      due = null;
    } else {
      due = pickDate;
      remindAt = null;
    }
    picking = false;
  }

  function clearWhen() {
    remindAt = null;
    due = null;
  }

  async function submit(e: Event) {
    e.preventDefault();
    if (!text.trim() || busy) return;
    busy = true;
    error = "";
    try {
      await onsave({
        text,
        due,
        remindAt,
        link: hasLink && link ? link : null,
        contactId: hasLink && !link ? keepContact : null,
      });
      if (!task) {
        text = "";
        clearWhen();
        linked = true;
        input?.focus();
      }
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape" && oncancel) {
      e.preventDefault();
      oncancel();
    }
  }
</script>

<form class="task-form" class:compact onsubmit={submit}>
  <input
    class="text"
    bind:this={input}
    bind:value={text}
    {placeholder}
    onkeydown={keydown}
    maxlength="500"
    aria-label="Texto da tarefa"
  />
  <div class="row">
    {#if when}
      <span class="chip set">
        <AlarmClock size={13} />
        {when}
        <button type="button" class="x" onclick={clearWhen} aria-label="Tirar a data">
          <X size={12} />
        </button>
      </span>
    {:else}
      {#each quick as q (q.label)}
        <button type="button" class="chip" onclick={() => choose(q.at)}>{q.label}</button>
      {/each}
    {/if}
    <button type="button" class="chip" class:on={picking} onclick={openPicker} aria-expanded={picking}>
      <CalendarDays size={13} /> Escolher…
    </button>
  </div>
  {#if picking}
    <div class="row picker">
      <input type="date" bind:value={pickDate} aria-label="Dia" />
      <input type="time" bind:value={pickTime} aria-label="Hora (vazia: só o dia, sem aviso)" />
      <button type="button" class="btn small" onclick={applyPicker}>Usar</button>
      <span class="hint">Sem hora: vence no dia, sem aviso.</span>
    </div>
  {/if}
  {#if (link || keepContact) && label}
    <div class="row">
      {#if hasLink}
        <span class="chip set link">
          <Link2 size={13} />
          <span class="ellipsis">{label}</span>
          <button type="button" class="x" onclick={() => (linked = false)} aria-label="Não ligar a {label}">
            <X size={12} />
          </button>
        </span>
      {:else}
        <button type="button" class="chip" onclick={() => (linked = true)}>
          <Link2 size={13} /> Ligar a {label}
        </button>
      {/if}
    </div>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row actions">
    {#if oncancel}
      <button type="button" class="btn small" onclick={oncancel}>Cancelar</button>
    {/if}
    <button type="submit" class="btn btn-primary small" disabled={!text.trim() || busy}>
      {task ? "Salvar" : remindAt ? "Criar lembrete" : "Criar tarefa"}
    </button>
  </div>
</form>

<style>
  .task-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
  }

  .text {
    height: 36px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-subtle);
    user-select: text;
    -webkit-user-select: text;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    max-width: 100%;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg);
    color: var(--text-muted);
    font-size: 12.5px;
  }

  button.chip:hover,
  .chip.on {
    background: var(--bg-hover);
    color: var(--text);
  }

  .chip.set {
    color: var(--text);
    background: var(--bg-subtle);
    padding-right: 3px;
  }

  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .x {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-muted);
  }

  .x:hover {
    background: var(--bg-hover);
    color: var(--text);
  }

  .picker input {
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-subtle);
  }

  .hint {
    font-size: 12px;
    color: var(--text-muted);
  }

  .btn.small {
    height: 30px;
    padding: 0 12px;
    font-size: 13px;
  }

  .actions {
    justify-content: flex-end;
  }

  .compact .actions {
    margin-top: -2px;
  }

  .error {
    margin: 0;
    font-size: 12.5px;
    color: var(--danger);
  }
</style>
