<script lang="ts">
  import { onMount } from "svelte";
  import { Bell, FileDown, Layers, MemoryStick } from "@lucide/svelte";
  import { api, formatBytes, type Snapshot } from "./api";
  import { freeMemory, heavyIds, home, watchMemory } from "./homeState.svelte";

  // Resumo do Início na barra do topo: cada item é um atalho.
  let {
    snap,
    run,
    onadd,
    onopen,
  }: {
    snap: Snapshot;
    run: (action: Promise<Snapshot | void>) => Promise<void>;
    onadd: () => void;
    onopen: (id: string) => void;
  } = $props();

  onMount(watchMemory);

  const loaded = $derived(snap.services.filter((s) => s.loaded).length);
  const withUnread = $derived(snap.services.filter((s) => s.unread > 0).length);
  const mostUnread = $derived(
    snap.services.filter((s) => s.unread > 0).sort((a, b) => b.unread - a.unread)[0] ?? null,
  );
  const bytes = $derived(
    home.memory ? Object.values(home.memory.services).reduce((a, b) => a + b, 0) : null,
  );
  const heavy = $derived(heavyIds(snap));
  const freeing = $derived(Object.values(home.reloading).some(Boolean));
  const done = $derived(snap.downloads.filter((d) => d.state === "done").length);
  const downloading = $derived(snap.downloads.some((d) => d.state === "downloading"));

  const items = $derived([
    {
      icon: Layers,
      value: String(snap.services.length),
      label: snap.services.length === 1 ? "serviço" : "serviços",
      title: `${loaded} em memória · ${snap.services.length - loaded} dormindo. Clique para adicionar um serviço.`,
      onclick: onadd,
      disabled: false,
      tone: "",
    },
    {
      icon: Bell,
      value: String(snap.totalUnread),
      label: snap.totalUnread === 1 ? "não lida" : "não lidas",
      title: mostUnread
        ? `Em ${withUnread} ${withUnread === 1 ? "serviço" : "serviços"}. Clique para abrir ${mostUnread.name}.`
        : "Tudo em dia.",
      onclick: () => mostUnread && onopen(mostUnread.id),
      disabled: !mostUnread,
      tone: snap.totalUnread > 0 ? "unread" : "",
    },
    {
      icon: MemoryStick,
      value: bytes === null ? "—" : formatBytes(bytes),
      label: freeing ? "liberando…" : "memória",
      title:
        (home.memory ? `Contas: ${formatBytes(bytes ?? 0)} + ${formatBytes(home.memory.app)} do Papeo. ` : "") +
        (heavy.length
          ? "Clique para liberar memória (recarrega as contas paradas; as notificações continuam)."
          : "Nenhuma conta pesada em segundo plano."),
      onclick: () => freeMemory(heavy, run),
      disabled: freeing || heavy.length === 0,
      tone: heavy.length && !freeing ? "heavy" : "",
    },
    {
      icon: FileDown,
      value: String(done),
      label: downloading ? "baixando…" : done === 1 ? "download" : "downloads",
      title: "Clique para abrir a pasta de downloads.",
      onclick: () => run(api.openDownloadsDir()),
      disabled: false,
      tone: "",
    },
  ]);
</script>

<div class="stats" role="group" aria-label="Resumo">
  {#each items as item (item.icon)}
    <button
      class="chip {item.tone}"
      title={item.title}
      aria-label="{item.value} {item.label}. {item.title}"
      disabled={item.disabled}
      onclick={item.onclick}
    >
      <item.icon size={15} />
      <strong>{item.value}</strong>
      <span class="label">{item.label}</span>
    </button>
  {/each}
</div>

<style>
  .stats {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 15px;
    background: var(--bg-subtle);
    color: var(--text-muted);
    font-size: 13px;
    white-space: nowrap;
  }

  .chip strong {
    color: var(--text);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .chip:not(:disabled):hover {
    border-color: var(--border-strong);
    background: var(--bg-hover);
  }

  .chip:disabled {
    cursor: default;
  }

  .chip.unread strong {
    color: var(--danger);
  }

  .chip.heavy strong {
    color: var(--warn-text);
  }

  /* Janela estreita: só o número. */
  @media (max-width: 1100px) {
    .label {
      display: none;
    }
  }
</style>
