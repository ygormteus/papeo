<script lang="ts">
  import { CirclePlus, House, MessageCircle, Search } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import { searchable, type RecentMessage, type ServiceView } from "./api";

  let {
    services,
    recent,
    onclose,
    onselect,
    onadd,
    onchat,
  }: {
    services: ServiceView[];
    recent: RecentMessage[];
    onclose: () => void;
    onselect: (id: string | null) => void;
    onadd: () => void;
    /** Abre o serviço e procura a conversa pelo nome. */
    onchat: (serviceId: string, query: string, exact: boolean) => void;
  } = $props();

  type Kind = "service" | "chat" | "search" | "home" | "add";
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
    const has = (...texts: string[]) => !q || texts.some((t) => normalize(t).includes(q));
    const byId = new Map(services.map((s) => [s.id, s]));

    const accounts: Entry[] = services
      .filter((s) => has(s.name))
      .map((s) => ({
        key: `svc-${s.id}`,
        kind: "service",
        group: "Contas",
        label: s.name,
        detail: s.unread ? `${s.unread} não lidas` : s.loaded ? "" : "dormindo",
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
          group: "Conversas recentes",
          label: m.title,
          detail: m.body ? `${service.name} · ${clip(m.body, 36)}` : service.name,
          service,
          run: () => onchat(m.serviceId, m.title, true),
        };
      });

    const searches: Entry[] = raw
      ? services.filter(searchable).map((s) => ({
          key: `search-${s.id}`,
          kind: "search",
          group: "Buscar contato ou grupo",
          label: `Buscar “${clip(raw, 40)}” em ${s.name}`,
          detail: s.loaded ? "" : "vai acordar",
          service: s,
          run: () => onchat(s.id, raw, false),
        }))
      : [];

    const extras: Entry[] = [
      { key: "home", kind: "home", group: "Papeo", label: "Início", run: () => onselect(null) },
      { key: "add", kind: "add", group: "Papeo", label: "Adicionar serviço", run: onadd },
    ];
    return [...accounts, ...chats, ...searches, ...extras.filter((e) => has(e.label))];
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

<Modal {onclose} label="Buscar" width={560} top>
  <div class="search">
    <Search size={18} />
    <!-- svelte-ignore a11y_autofocus -->
    <input
      bind:value={query}
      onkeydown={keydown}
      placeholder="Ir para uma conta ou buscar um contato…"
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
  <ul id="palette-list" role="listbox" aria-label="Resultados">
    {#each entries as entry, i (entry.key)}
      {#if i === 0 || entries[i - 1].group !== entry.group}
        <li class="group" role="presentation">{entry.group}</li>
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
        {:else}
          <span class="glyph"><CirclePlus size={17} /></span>
        {/if}
        <span class="label">{entry.label}</span>
        {#if entry.detail}<span class="detail">{entry.detail}</span>{/if}
      </li>
    {:else}
      <li class="none" role="presentation">Nada encontrado.</li>
    {/each}
  </ul>
  {#if !query.trim() && services.some(searchable)}
    <p class="tip">Digite um nome para procurar contatos e grupos no WhatsApp ou no Telegram.</p>
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
