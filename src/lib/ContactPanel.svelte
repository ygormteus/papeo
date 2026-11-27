<script lang="ts">
  import { onDestroy } from "svelte";
  import { NotebookPen, X } from "@lucide/svelte";
  import type { ChatLink, Organizer, ServiceView, TaskFields } from "./api";
  import TaskForm from "./TaskForm.svelte";
  import TaskList, { type TaskActions } from "./TaskList.svelte";
  import { contactFor, tasksFor } from "./tasks";
  import { isEmail, isMessenger } from "./api";
  import { t } from "./i18n.svelte";

  let {
    service,
    org,
    actions,
    now,
    oncreate,
    onnote,
    onclose,
  }: {
    service: ServiceView;
    org: Organizer;
    actions: TaskActions;
    now: Date;
    oncreate: (fields: TaskFields) => Promise<void>;
    onnote: (link: ChatLink, note: string) => Promise<void>;
    onclose: () => void;
  } = $props();

  const messenger = $derived(isMessenger(service));
  const email = $derived(isEmail(service));
  const chat = $derived(messenger || email ? (service.chat ?? "") : "");
  const noneOpen = $derived(email ? t("Nenhum e-mail aberto") : t("Nenhuma conversa aberta"));
  const link = $derived<ChatLink>({ serviceId: service.id, chat });
  const key = $derived(`${service.id}\u0000${chat}`);
  const title = $derived(chat || service.name);
  const tasks = $derived(tasksFor(org, service.id, chat));
  const savedNote = $derived(contactFor(org, service.id, chat)?.note ?? "");

  // Anotação: rascunho local, gravado sozinho um pouco depois de parar de
  // digitar (e ao trocar de conversa ou fechar o painel).
  let note = $state("");
  let dirty = $state(false);
  let noteKey = "";
  let noteLink: ChatLink | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let status = $state<"" | "saving" | "saved" | "error">("");
  // Digitando: atualizações de fora (que vêm sem espaços no fim) não
  // sobrescrevem o campo.
  let focused = $state(false);
  // Gravações em curso: enquanto houver, o campo não é sobrescrito.
  let saving = $state(0);

  /** Grava o rascunho, se houver. Falhou: o rascunho continua no campo. */
  export function flush(): Promise<void> {
    clearTimeout(timer);
    if (!dirty || !noteLink) return Promise.resolve();
    const target = noteLink;
    const text = note;
    const sentKey = noteKey;
    dirty = false;
    status = "saving";
    saving++;
    return onnote(target, text)
      .then(() => {
        // Já em outra conversa: não marca a nova como salva.
        if (noteKey === sentKey) status = "saved";
      })
      .catch(() => {
        // Ainda na mesma conversa: o rascunho continua no campo. Já em
        // outra: o erro aparece (pelo App), sem marcar a conversa nova.
        if (noteKey === sentKey) {
          status = "error";
          dirty = true;
        }
      })
      .finally(() => saving--);
  }

  $effect(() => {
    // Trocou de conversa: grava a anterior e carrega a nova.
    if (key !== noteKey) {
      flush();
      noteKey = key;
      noteLink = link;
      note = savedNote;
      dirty = false;
      status = "";
    } else if (!dirty && !focused && saving === 0) {
      // Mudou por fora (outro lugar do app): acompanha se não há rascunho.
      note = savedNote;
    }
  });

  function typed() {
    dirty = true;
    status = "";
    clearTimeout(timer);
    timer = setTimeout(flush, 800);
  }

  onDestroy(flush);
</script>

<!-- Janela perdeu o foco (o bloqueio automático vem depois): grava já. -->
<svelte:window onblur={flush} />

<aside class="panel" aria-label={t("Tarefas e anotações de {titulo}", { titulo: title })}>
  <header>
    <NotebookPen size={16} />
    <div class="titles">
      <strong title={title}>{title}</strong>
      <span>{chat ? service.name : messenger || email ? noneOpen : t("Conta inteira")}</span>
    </div>
    <button class="icon-btn ghost close" onclick={onclose} aria-label={t("Fechar o painel")}>
      <X size={16} />
    </button>
  </header>

  <div class="scroll">
    {#if messenger && !chat}
      <p class="hint">{t("Abra uma conversa para ligar tarefas e anotações a ela. Aqui ficam as da conta.")}</p>
    {:else if email && !chat}
      <p class="hint">
        {service.kind === "email"
          ? t("Abra um e-mail para ligar tarefas e anotações a ele (no Roundcube; em outros webmails, ficam na conta). Aqui ficam as da conta.")
          : t("Abra um e-mail para ligar tarefas e anotações a ele. Aqui ficam as da conta.")}
      </p>
    {/if}

    {#key key}
      <TaskForm
        {link}
        linkLabel={chat ? `${chat} · ${service.name}` : service.name}
        placeholder={chat ? t("Tarefa ou lembrete sobre {chat}…", { chat }) : t("Nova tarefa…")}
        {now}
        onsave={oncreate}
      />
    {/key}

    <div class="list">
      <TaskList
        {tasks}
        {org}
        services={[service]}
        {actions}
        {now}
        showContact={false}
        empty={chat
          ? email
            ? t("Nenhuma tarefa ligada a este e-mail.")
            : t("Nenhuma tarefa ligada a esta conversa.")
          : t("Nenhuma tarefa ligada a esta conta.")}
      />
    </div>

    <section class="notes">
      <div class="notes-head">
        <h3>{t("Anotações")}</h3>
        <span class="status" aria-live="polite">
          {status === "saving" ? t("Salvando…") : status === "saved" ? t("Salvo") : status === "error" ? t("Não salvou") : ""}
        </span>
      </div>
      <textarea
        bind:value={note}
        oninput={typed}
        onfocus={() => (focused = true)}
        onblur={() => {
          focused = false;
          flush();
        }}
        maxlength="20000"
        placeholder={chat
          ? t("O que lembrar sobre {chat}: preferências, combinados, histórico…", { chat })
          : t("Anotações sobre esta conta…")}
        aria-label={t("Anotações sobre {titulo}", { titulo: title })}
      ></textarea>
    </section>
  </div>
</aside>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    width: clamp(280px, 26vw, 340px);
    flex: none;
    min-height: 0;
    border-left: 1px solid var(--border);
    background: var(--bg-sidebar);
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 10px 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .titles {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .titles strong,
  .titles span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .titles span {
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .close {
    width: 30px;
    height: 30px;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .hint {
    margin: 0;
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .notes {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-height: 160px;
  }

  .notes-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding: 0 4px;
  }

  h3 {
    margin: 0;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .status {
    font-size: 12px;
    color: var(--text-muted);
  }

  textarea {
    flex: 1;
    min-height: 140px;
    padding: 10px;
    resize: none;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    line-height: 1.5;
    user-select: text;
    -webkit-user-select: text;
  }
</style>
