<script lang="ts">
  import { t, tk } from "./i18n.svelte";
  import { CirclePlus, House, ListTodo, MessageCircle, NotebookPen, Search, Settings, SquareCheck } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import { searchable, type Contact, type Organizer, type RecentChat, type ServiceView } from "./api";
  import { contactLabel, contactOf, whenLabel } from "./tasks";
  import { TAB_LABEL, searchSettings, type SettingsTarget } from "./settingsIndex";

  let {
    services,
    recent,
    onclose,
    onselect,
    onadd,
    onchat,
    org,
    ontasks,
    oncontact,
    onsetting,
  }: {
    services: ServiceView[];
    recent: RecentChat[];
    org: Organizer;
    ontasks: () => void;
    /** Abre as Configurações na aba e na opção indicadas. */
    onsetting: (target?: SettingsTarget) => void;
    /** Abre a conversa do contato com o painel. */
    oncontact: (contact: Contact) => void;
    onclose: () => void;
    onselect: (id: string | null) => void;
    onadd: () => void;
    /** Abre o serviço e procura a conversa pelo nome. */
    onchat: (serviceId: string, query: string, exact: boolean) => void;
  } = $props();

  type Kind = "service" | "chat" | "search" | "home" | "add" | "task" | "note" | "tasks" | "setting";
  type Entry = {
    key: string;
    kind: Kind;
    group: string;
    label: string;
    detail?: string;
    service?: ServiceView;
    run: () => void;
  };

  let query = $state("");
  let index = $state(0);

  const normalize = (s: string) =>
    s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const clip = (s: string, n: number) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

  const entries = $derived.by<Entry[]>(() => {
    const raw = query.trim();
    const q = normalize(raw);
    const has = (...texts: string[]) => !q || texts.some((x) => normalize(x).includes(q));
    const byId = new Map(services.map((s) => [s.id, s]));

    const accounts: Entry[] = services
      .filter((s) => has(s.name))
      .map((s) => ({
        key: `svc-${s.id}`,
        kind: "service",
        group: tk("Contas"),
        label: s.name,
        detail: s.unread ? t("{n} não lidas", { n: s.unread }) : s.loaded ? "" : t("dormindo"),
        service: s,
        run: () => onselect(s.id),
      }));

    const chats: Entry[] = recent
      .filter((m) => byId.has(m.serviceId) && has(m.title, m.body))
      .slice(0, 6)
      .map((m, i) => {
        const service = byId.get(m.serviceId)!;
        return {
          key: `chat-${i}`,
          kind: "chat",
          group: tk("Conversas recentes"),
          label: m.title,
          detail: m.body ? `${service.name} · ${clip(m.body, 36)}` : service.name,
          service,
          run: () => onchat(m.serviceId, m.chat, true),
        };
      });

    const searches: Entry[] = raw
      ? services.filter(searchable).map((s) => ({
          key: `search-${s.id}`,
          kind: "search",
          group: tk("Buscar contato ou grupo"),
          label: t("Buscar “{busca}” em {conta}", { busca: clip(raw, 40), conta: s.name }),
          detail: s.loaded ? "" : t("vai acordar"),
          service: s,
          run: () => onchat(s.id, raw, false),
        }))
      : [];

    // Tarefas e anotações (busca no texto; "#cliente" acha anotações
    // marcadas assim).
    const now = new Date();
    const live = (c: Contact) => services.some((s) => s.id === c.serviceId);
    const tasks: Entry[] = raw
      ? org.tasks
          .filter((k) => has(k.text))
          .sort((a, b) => Number(a.done) - Number(b.done))
          .slice(0, 5)
          .map((k) => {
            const c = contactOf(org, k);
            const when = k.done ? t("feita") : whenLabel(k, now);
            const who = c ? contactLabel(c, services.find((s) => s.id === c.serviceId)?.name) : "";
            return {
              key: `task-${k.id}`,
              kind: "task",
              group: tk("Tarefas"),
              label: clip(k.text, 60),
              detail: [when, who].filter(Boolean).join(" · "),
              run: () => (c && live(c) ? oncontact(c) : ontasks()),
            };
          })
      : [];
    const notes: Entry[] = raw
      ? org.contacts
          .filter((c) => c.note && has(c.note, c.chat))
          .slice(0, 5)
          .map((c) => {
            const line = c.note.split("\n").find((l) => normalize(l).includes(q)) ?? c.note.split("\n")[0];
            return {
              key: `note-${c.id}`,
              kind: "note",
              group: tk("Anotações"),
              label: contactLabel(c, services.find((s) => s.id === c.serviceId)?.name),
              detail: clip(line.trim(), 44),
              run: () => (live(c) ? oncontact(c) : ontasks()),
            };
          })
      : [];

    const settings: Entry[] = searchSettings(raw, 5, t).map((e) => ({
      key: `setting-${e.label}`,
      kind: "setting",
      group: tk("Configurações"),
      label: t(e.label),
      detail: t(TAB_LABEL[e.tab]),
      run: () => onsetting({ tab: e.tab, label: e.label }),
    }));

    const extras: Entry[] = [
      { key: "home", kind: "home", group: "Papeo", label: tk("Início"), run: () => onselect(null) },
      { key: "tasks", kind: "tasks", group: "Papeo", label: tk("Tarefas"), run: ontasks },
      { key: "add", kind: "add", group: "Papeo", label: tk("Adicionar serviço"), run: onadd },
      { key: "settings", kind: "setting", group: "Papeo", label: tk("Configurações"), run: () => onsetting() },
    ];
    return [
      ...accounts,
      ...chats,
      ...tasks,
      ...notes,
      ...settings,
      ...searches,
      ...extras.filter((e) => has(e.label, t(e.label))).map((e) => ({ ...e, label: t(e.label) })),
    ];
  });

  $effect(() => {
    query;
    index = 0;
  });

  function keydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = entries.length;
      if (n) index = (index + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
    } else if (e.key === "Enter") {
      e.preventDefault();
      entries[index]?.run();
    }
  }
</script>

<Modal {onclose} label={t("Buscar")} width={560} top>
  <div class="search">
    <Search size={18} />
    <!-- svelte-ignore a11y_autofocus -->
    <input
      bind:value={query}
      onkeydown={keydown}
      placeholder={t("Conta, contato, tarefa, anotação ou configuração…")}
      autofocus
      autocomplete="off"
      autocorrect="off"
      autocapitalize="off"
      spellcheck="false"
      role="combobox"
      aria-expanded="true"
      aria-controls="palette-list"
      aria-activedescendant={entries[index] ? `palette-${entries[index].key}` : undefined}
    />
  </div>
  <ul id="palette-list" role="listbox" aria-label={t("Resultados")}>
    {#each entries as entry, i (entry.key)}
      {#if i === 0 || entries[i - 1].group !== entry.group}
        <li class="group" role="presentation">{t(entry.group)}</li>
      {/if}
      <li
        id="palette-{entry.key}"
        role="option"
        aria-selected={i === index}
        class:selected={i === index}
        onclick={entry.run}
        onkeydown={() => {}}
        onmousemove={() => (index = i)}
      >
        {#if entry.kind === "service" && entry.service}
          <ServiceAvatar service={entry.service} size={22} dimmed={!entry.service.loaded} />
        {:else if entry.kind === "chat"}
          <span class="glyph"><MessageCircle size={17} /></span>
        {:else if entry.kind === "search"}
          <span class="glyph"><Search size={17} /></span>
        {:else if entry.kind === "home"}
          <span class="glyph"><House size={17} /></span>
        {:else if entry.kind === "task"}
          <span class="glyph"><SquareCheck size={17} /></span>
        {:else if entry.kind === "note"}
          <span class="glyph"><NotebookPen size={17} /></span>
        {:else if entry.kind === "tasks"}
          <span class="glyph"><ListTodo size={17} /></span>
        {:else if entry.kind === "setting"}
          <span class="glyph"><Settings size={17} /></span>
        {:else}
          <span class="glyph"><CirclePlus size={17} /></span>
        {/if}
        <span class="label">{entry.label}</span>
        {#if entry.detail}<span class="detail">{entry.detail}</span>{/if}
      </li>
    {:else}
      <li class="none" role="presentation">{t("Nada encontrado.")}</li>
    {/each}
  </ul>
  {#if !query.trim() && services.some(searchable)}
    <p class="tip">{t("Digite um nome para procurar contatos e grupos no WhatsApp ou no Telegram.")}</p>
  {/if}
</Modal>

<style>
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 16px;
    height: 52px;
    border-bottom: 1px solid var(--border);
    color: var(--text-muted);
  }

  input {
    flex: 1;
    border: 0;
    outline: none;
    background: transparent;
    font-size: 16px;
    color: var(--text);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 6px;
    max-height: 340px;
    overflow-y: auto;
  }

  li {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 42px;
    padding: 0 10px;
    border-radius: 10px;
    cursor: pointer;
  }

  li.selected {
    background: var(--bg-hover);
  }

  .glyph {
    display: inline-grid;
    place-items: center;
    width: 22px;
    color: var(--text-muted);
  }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group {
    height: auto;
    padding: 10px 10px 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-muted);
    cursor: default;
  }

  .tip {
    margin: 0;
    padding: 10px 16px 14px;
    border-top: 1px solid var(--border);
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .detail {
    flex: none;
    max-width: 45%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .none {
    justify-content: center;
    color: var(--text-muted);
    cursor: default;
  }
</style>
