<script lang="ts">
  // Leitor de uma conta em janela separada (spec 32): a mesma interface,
  // montando só o leitor (`src/main.ts`, `__PAPEO_VIEW = "reader"`).
  import { onMount } from "svelte";
  import MailReader from "./MailReader.svelte";
  import TalkReader from "./TalkReader.svelte";
  import { setLang, t } from "./i18n.svelte";
  import {
    api,
    EMPTY_SNAPSHOT,
    errorMessage,
    fresher,
    onMailReveal,
    onState,
    onTalkReveal,
    type Snapshot,
  } from "./api";

  const id = (window as { __PAPEO_READER?: string }).__PAPEO_READER ?? "";

  let snap = $state<Snapshot>(EMPTY_SNAPSHOT);
  $effect(() => setLang(snap.lang));
  const service = $derived(snap.services.find((s) => s.id === id) ?? null);

  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  let systemIsDark = $state(systemDark.matches);
  const appearance = $derived(snap.settings.appearance);
  $effect(() => {
    document.documentElement.dataset.theme =
      appearance === "system" ? (systemIsDark ? "dark" : "light") : appearance;
  });

  let mailReveal = $state<{ id: string; query: string; uid?: number | null; n: number } | null>(null);
  let talkReveal = $state<{ id: string; token: string; n: number } | null>(null);

  type Toast = { text: string; action?: { label: string; run: () => void } };
  let toast = $state<Toast | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(next: Toast, ms = 5000) {
    toast = next;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), ms);
  }

  async function run(action: Promise<Snapshot | void>) {
    try {
      const next = await action;
      if (next) snap = fresher(next, snap);
    } catch (err) {
      showToast({ text: errorMessage(err) });
    }
  }

  /** Site ou configuração da conta: na janela principal. */
  async function backToMain(then?: () => Promise<Snapshot | void>) {
    await run(api.attachService(id));
    if (then) await run(then());
  }

  onMount(() => {
    const onScheme = (e: MediaQueryListEvent) => (systemIsDark = e.matches);
    systemDark.addEventListener("change", onScheme);
    api
      .getState()
      .then((s) => (snap = s))
      .catch(() => {});
    const unlisten = [
      onState((s) => (snap = fresher(s, snap))),
      onMailReveal((r) => {
        if (r.id === id) mailReveal = { ...r, n: (mailReveal?.n ?? 0) + 1 };
      }),
      onTalkReveal((r) => {
        if (r.id === id) talkReveal = { ...r, n: (talkReveal?.n ?? 0) + 1 };
      }),
    ];
    return () => {
      systemDark.removeEventListener("change", onScheme);
      unlisten.forEach((p) => p.then((off) => off()));
      clearTimeout(toastTimer);
    };
  });
</script>

<main class="detached">
  {#if snap.locked}
    <p class="note">{t("O Papeo está bloqueado.")}</p>
  {:else if service?.reader}
    <!-- Zoom da conta também aqui (como na janela principal). -->
    <div class="reader-zoom" style:zoom={service.zoom}>
      {#key service.id}
        {#if service.kind === "nextcloud"}
          <TalkReader
            {service}
            reveal={talkReveal}
            onsite={() => backToMain(() => api.mailShowSite(id, true))}
            ontoast={(text, action) => showToast({ text, action }, action ? 8000 : 4000)}
          />
        {:else}
          <MailReader
            {service}
            reveal={mailReveal}
            undoSecs={snap.settings.mailUndoSecs}
            onsite={() => backToMain(() => api.mailShowSite(id, true))}
            onconfigure={() => backToMain()}
            ontoast={(text, action) => showToast({ text, action }, action ? 8000 : 4000)}
          />
        {/if}
      {/key}
    </div>
  {:else if snap.rev}
    <p class="note">{t("Esta conta voltou para a janela principal.")}</p>
  {/if}
</main>

{#if toast}
  <div class="toast" role="status">
    <span>{toast.text}</span>
    {#if toast.action}
      {@const action = toast.action}
      <button
        class="toast-action"
        onclick={() => {
          action.run();
          toast = null;
        }}
      >
        {action.label}
      </button>
    {/if}
  </div>
{/if}

<style>
  .detached {
    position: fixed;
    inset: 0;
    background: var(--bg);
    color: var(--text);
  }

  .reader-zoom {
    position: absolute;
    inset: 0;
  }

  .note {
    display: grid;
    place-items: center;
    height: 100%;
    margin: 0;
    color: var(--text-muted);
  }

  .toast {
    position: fixed;
    left: 50%;
    bottom: 20px;
    transform: translateX(-50%);
    max-width: min(560px, calc(100vw - 32px));
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--primary);
    color: var(--primary-text);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.25);
    z-index: 50;
  }

  .toast span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .toast-action {
    flex: none;
    height: 28px;
    padding: 0 10px;
    border: 0;
    border-radius: 7px;
    background: color-mix(in srgb, var(--primary-text) 16%, transparent);
    color: inherit;
    font-weight: 600;
  }
</style>
