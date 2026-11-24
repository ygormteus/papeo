<script lang="ts">
  import { CirclePlus, House, Search } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import type { ServiceView } from "./api";

  let {
    services,
    onclose,
    onselect,
    onadd,
  }: {
    services: ServiceView[];
    onclose: () => void;
    onselect: (id: string | null) => void;
    onadd: () => void;
  } = $props();

  type Entry = { key: string; label: string; detail?: string; service?: ServiceView; run: () => void };

  let query = $state("");
  let index = $state(0);

  const normalize = (s: string) =>
    s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();

  const entries = $derived.by<Entry[]>(() => {
    const all: Entry[] = [
      ...services.map((s) => ({
        key: s.id,
        label: s.name,
        detail: s.unread ? `${s.unread} não lidas` : s.loaded ? "" : "dormindo",
        service: s,
        run: () => onselect(s.id),
      })),
      { key: "home", label: "Início", run: () => onselect(null) },
      { key: "add", label: "Adicionar serviço", run: onadd },
    ];
    const q = normalize(query.trim());
    return q ? all.filter((e) => normalize(e.label).includes(q)) : all;
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

<Modal {onclose} label="Buscar serviço" width={520} top>
  <div class="search">
    <Search size={18} />
    <!-- svelte-ignore a11y_autofocus -->
    <input
      bind:value={query}
      onkeydown={keydown}
      placeholder="Ir para…"
      autofocus
      role="combobox"
      aria-expanded="true"
      aria-controls="palette-list"
      aria-activedescendant={entries[index] ? `palette-${entries[index].key}` : undefined}
    />
  </div>
  <ul id="palette-list" role="listbox" aria-label="Resultados">
    {#each entries as entry, i (entry.key)}
      <li
        id="palette-{entry.key}"
        role="option"
        aria-selected={i === index}
        class:selected={i === index}
        onclick={entry.run}
        onkeydown={() => {}}
        onmousemove={() => (index = i)}
      >
        {#if entry.service}
          <ServiceAvatar service={entry.service} size={22} dimmed={!entry.service.loaded} />
        {:else if entry.key === "home"}
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
  }

  .detail {
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .none {
    justify-content: center;
    color: var(--text-muted);
    cursor: default;
  }
</style>
