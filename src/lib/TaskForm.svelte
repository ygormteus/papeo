<script lang="ts">
  import { AlarmClock, CalendarDays, Link2, Repeat as RepeatIcon, Sparkles, X } from "@lucide/svelte";
  import { REPEAT_OPTIONS, type ChatLink, type Repeat, type Task, type TaskFields } from "./api";
  import { dateKey, fromInputs, quickTimes, toTimeInput, whenLabel } from "./tasks";
  import { parseWhen } from "./when";
  import { t } from "./i18n.svelte";

  let {
    link = null,
    linkLabel = "",
    task = null,
    keepContact = null,
    keepLabel = "",
    placeholder,
    initialText = "",
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
    /** Texto já escrito numa tarefa nova (ex.: a mensagem recebida). */
    initialText?: string;
    compact?: boolean;
    /** Relógio do app (avança a cada 30 s). */
    now: Date;
    onsave: (fields: TaskFields) => Promise<void>;
    oncancel?: () => void;
  } = $props();

  // Valores iniciais: a tarefa editada ou vazio (o formulário é recriado
  // para cada tarefa, então ler as props só no início é o esperado).
  const initial = (() => task)();
  let text = $state(initial?.text ?? (() => initialText)());
  let remindAt = $state<number | null>(initial?.remindAt ?? null);
  let due = $state<string | null>(initial?.due ?? null);
  let linked = $state(true);
  let repeat = $state<Repeat | "">(initial?.repeat ?? "");
  // Data escrita no texto ("amanhã 15h") que a pessoa preferiu ignorar.
  // Editando: o texto atual não é relido (só se for mudado).
  let ignoredText = $state<string | null>(initial?.text ?? null);
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
  // Só lê a data do texto se nenhuma foi escolhida nos botões.
  const parsed = $derived(
    remindAt == null && due == null && text !== ignoredText ? parseWhen(text, now) : null,
  );
  const parsedLabel = $derived(
    parsed
      ? whenLabel(
          {
            id: "",
            text: "",
            done: false,
            createdAt: 0,
            updatedAt: 0,
            remindAt: parsed.remindAt ?? undefined,
            due: parsed.due ?? undefined,
          },
          now,
        )
      : null,
  );
  const dated = $derived(remindAt != null || due != null || !!parsed);
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
        text: parsed ? parsed.text : text,
        due: parsed ? parsed.due : due,
        remindAt: parsed ? parsed.remindAt : remindAt,
        link: hasLink && link ? link : null,
        contactId: hasLink && !link ? keepContact : null,
        repeat: dated && repeat ? repeat : null,
      });
      if (!task) {
        text = "";
        clearWhen();
        linked = true;
        repeat = "";
        ignoredText = null;
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
    placeholder={placeholder ?? t("Nova tarefa…")}
    onkeydown={keydown}
    maxlength="500"
    aria-label={t("Texto da tarefa")}
  />
  {#if parsed && parsedLabel}
    <div class="row">
      <span class="chip set parsed" title={t("Lido do texto: “{trecho}”", { trecho: parsed.match })}>
        <Sparkles size={13} />
        {parsedLabel}
        <button type="button" class="x" onclick={() => (ignoredText = text)} aria-label={t("Não usar a data do texto")}>
          <X size={12} />
        </button>
      </span>
      <span class="hint">{t("“{trecho}” sai do texto", { trecho: parsed.match })}</span>
    </div>
  {/if}
  <div class="row">
    {#if when}
      <span class="chip set">
        <AlarmClock size={13} />
        {when}
        <button type="button" class="x" onclick={clearWhen} aria-label={t("Tirar a data")}>
          <X size={12} />
        </button>
      </span>
    {:else}
      {#each quick as q (q.label)}
        <button type="button" class="chip" onclick={() => choose(q.at)}>{q.label}</button>
      {/each}
    {/if}
    <button type="button" class="chip" class:on={picking} onclick={openPicker} aria-expanded={picking}>
      <CalendarDays size={13} /> {t("Escolher…")}
    </button>
  </div>
  {#if picking}
    <div class="row picker">
      <input type="date" bind:value={pickDate} aria-label={t("Dia")} />
      <input type="time" bind:value={pickTime} aria-label={t("Hora (vazia: só o dia, sem aviso)")} />
      <button type="button" class="btn small" onclick={applyPicker}>{t("Usar")}</button>
      <span class="hint">{t("Sem hora: vence no dia, sem aviso.")}</span>
    </div>
  {/if}
  {#if dated}
    <div class="row">
      <label class="chip repeat" class:set={!!repeat}>
        <RepeatIcon size={13} />
        <select bind:value={repeat} aria-label={t("Repetir")}>
          <option value="">{t("Não repete")}</option>
          {#each REPEAT_OPTIONS as o (o.value)}
            <option value={o.value}>{t(o.label)}</option>
          {/each}
        </select>
      </label>
    </div>
  {/if}
  {#if (link || keepContact) && label}
    <div class="row">
      {#if hasLink}
        <span class="chip set link">
          <Link2 size={13} />
          <span class="ellipsis">{label}</span>
          <button type="button" class="x" onclick={() => (linked = false)} aria-label={t("Não ligar a {nome}", { nome: label })}>
            <X size={12} />
          </button>
        </span>
      {:else}
        <button type="button" class="chip" onclick={() => (linked = true)}>
          <Link2 size={13} /> {t("Ligar a {nome}", { nome: label })}
        </button>
      {/if}
    </div>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row actions">
    {#if oncancel}
      <button type="button" class="btn small" onclick={oncancel}>{t("Cancelar")}</button>
    {/if}
    <button type="submit" class="btn btn-primary small" disabled={!text.trim() || busy}>
      {task ? t("Salvar") : remindAt ? t("Criar lembrete") : t("Criar tarefa")}
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

  .parsed {
    color: var(--accent);
  }

  .repeat select {
    border: 0;
    background: transparent;
    color: inherit;
    font-size: 12.5px;
    padding: 0 2px;
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
