<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "./i18n.svelte";
  import { Bell, FileDown, Layers, MemoryStick } from "@lucide/svelte";
  import { api, formatBytes, type Snapshot } from "./api";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { freeMemory, heavyIds, home, setConfirmFree, watchMemory } from "./homeState.svelte";

  // Resumo do Início na barra do topo: cada item é um atalho.
  let {
    snap,
    run,
    onopen,
  }: {
    snap: Snapshot;
    run: (action: Promise<Snapshot | void>) => Promise<unknown>;
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
  /** Contas a recarregar, fixadas no clique até o usuário confirmar. */
  let pending = $state<string[] | null>(null);
  let dontAsk = $state(false);
  const pendingList = $derived(
    (pending ?? [])
      .map((id) => snap.services.find((s) => s.id === id))
      .filter((s) => s !== undefined)
      .map((s) => `${s.name} (${formatBytes(home.memory?.services[s.id] ?? 0)})`)
      .join(", "),
  );

  function confirmFree() {
    const ids = pending ?? [];
    pending = null;
    if (dontAsk) setConfirmFree(false);
    freeMemory(ids, run);
  }

  function askFree() {
    if (!home.confirmFree) return freeMemory(heavy, run);
    dontAsk = false;
    pending = [...heavy];
  }

  const done = $derived(snap.downloads.filter((d) => d.state === "done").length);
  const downloading = $derived(snap.downloads.some((d) => d.state === "downloading"));

  const items = $derived([
    {
      icon: Layers,
      value: String(snap.services.length),
      label: snap.services.length === 1 ? t("serviço") : t("serviços"),
      title: t("{n} em memória · {m} dormindo. Clique para ver os serviços.", { n: loaded, m: snap.services.length - loaded }),
      onclick: () => (home.servicesOpen = true),
      disabled: false,
      tone: "",
    },
    {
      icon: Bell,
      value: String(snap.totalUnread),
      label: snap.totalUnread === 1 ? t("não lida") : t("não lidas"),
      title: mostUnread
        ? withUnread === 1
          ? t("Em {n} serviço. Clique para abrir {nome}.", { n: withUnread, nome: mostUnread.name })
          : t("Em {n} serviços. Clique para abrir {nome}.", { n: withUnread, nome: mostUnread.name })
        : t("Tudo em dia."),
      onclick: () => mostUnread && onopen(mostUnread.id),
      disabled: !mostUnread,
      tone: snap.totalUnread > 0 ? "unread" : "",
    },
    {
      icon: MemoryStick,
      value: bytes === null ? "—" : formatBytes(bytes),
      label: freeing ? t("liberando…") : t("memória"),
      title:
        (home.memory
          ? t("Contas: {contas} + {papeo} do Papeo. ", { contas: formatBytes(bytes ?? 0), papeo: formatBytes(home.memory.app) })
          : "") +
        (heavy.length
          ? t("Clique para liberar memória (recarrega as contas paradas; as notificações continuam).")
          : t("Nenhuma conta pesada em segundo plano.")),
      onclick: askFree,
      disabled: freeing || heavy.length === 0,
      tone: heavy.length && !freeing ? "heavy" : "",
    },
    {
      icon: FileDown,
      value: String(done),
      label: downloading ? t("baixando…") : done === 1 ? t("download") : t("downloads"),
      title: t("Clique para abrir a pasta de downloads."),
      onclick: () => run(api.openDownloadsDir()),
      disabled: false,
      tone: "",
    },
  ]);
</script>

<div class="stats" role="group" aria-label={t("Resumo")}>
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

{#if pending}
  <ConfirmDialog
    title={t("Liberar memória?")}
    message={pending.length === 1
      ? t("Esta conta parada será recarregada: {lista}. O que não foi enviado nelas pode se perder; as notificações continuam.", { lista: pendingList })
      : t("Estas contas paradas serão recarregadas: {lista}. O que não foi enviado nelas pode se perder; as notificações continuam.", { lista: pendingList })}
    confirmLabel={t("Liberar")}
    danger={false}
    onclose={() => (pending = null)}
    onconfirm={confirmFree}
  >
    <label class="dont-ask">
      <input type="checkbox" bind:checked={dontAsk} />
      {t("Não perguntar de novo")}
    </label>
  </ConfirmDialog>
{/if}

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

  .dont-ask {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: -8px 0 20px;
    color: var(--text-muted);
    font-size: 13px;
  }

  /* Janela estreita: só o número. */
  @media (max-width: 1100px) {
    .label {
      display: none;
    }
  }
</style>
