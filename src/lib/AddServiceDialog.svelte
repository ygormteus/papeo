<script lang="ts">
  import Modal from "./Modal.svelte";
  import { KIND_META, errorMessage, type ServiceKind } from "./api";

  let {
    onclose,
    onsubmit,
  }: { onclose: () => void; onsubmit: (kind: ServiceKind, name: string, url: string) => Promise<void> } = $props();

  const kinds: ServiceKind[] = ["whatsapp", "telegram", "custom"];
  let kind = $state<ServiceKind>("whatsapp");
  let name = $state("");
  let url = $state("");
  let error = $state("");
  let busy = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (kind === "custom" && !url.trim()) {
      error = "Informe o endereço do site.";
      return;
    }
    busy = true;
    error = "";
    try {
      await onsubmit(kind, name.trim(), kind === "custom" ? url.trim() : "");
    } catch (err) {
      error = errorMessage(err);
      busy = false;
    }
  }
</script>

<Modal {onclose} label="Adicionar serviço">
  <form onsubmit={submit}>
    <h2>Adicionar serviço</h2>
    <p class="muted">Cada serviço tem sessão própria: dá para ter duas contas do mesmo mensageiro.</p>

    <fieldset class="kinds">
      <legend class="sr-only">Tipo de serviço</legend>
      {#each kinds as k (k)}
        <label class="kind" class:selected={kind === k}>
          <input type="radio" name="kind" value={k} bind:group={kind} class="sr-only" />
          <span class="swatch" style:background={KIND_META[k].color} aria-hidden="true"></span>
          <span class="kind-label">{KIND_META[k].label}</span>
          <span class="kind-hint">{KIND_META[k].hint}</span>
        </label>
      {/each}
    </fieldset>

    {#if kind === "custom"}
      <div class="field">
        <label for="svc-url">Endereço</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="svc-url" class="input" bind:value={url} placeholder="app.exemplo.com" autofocus />
      </div>
    {/if}

    <div class="field">
      <label for="svc-name">Nome <span class="muted">(opcional)</span></label>
      <input
        id="svc-name"
        class="input"
        bind:value={name}
        placeholder={kind === "custom" ? "Ex.: Slack da empresa" : `Ex.: ${KIND_META[kind].label} trabalho`}
      />
    </div>

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>Cancelar</button>
      <button type="submit" class="btn btn-primary" disabled={busy}>{busy ? "Adicionando…" : "Adicionar"}</button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }

  .muted {
    margin: -10px 0 0;
    color: var(--text-muted);
    font-weight: 400;
  }

  label .muted {
    margin: 0;
  }

  .kinds {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .kind {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 12px;
    cursor: pointer;
  }

  .kind:hover {
    background: var(--bg-hover);
  }

  .kind.selected {
    border-color: var(--text);
    box-shadow: inset 0 0 0 1px var(--text);
  }

  .kind:has(:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .swatch {
    width: 18px;
    height: 18px;
    border-radius: 6px;
    margin-bottom: 6px;
  }

  .kind-label {
    font-weight: 550;
  }

  .kind-hint {
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .error {
    margin: 0;
    color: var(--danger);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
</style>
