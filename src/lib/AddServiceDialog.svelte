<script lang="ts">
  import { Check, CirclePlus, X } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import { KIND_META, errorMessage, needsUrl, type ServiceKind } from "./api";

  let {
    onclose,
    onsubmit,
  }: { onclose: () => void; onsubmit: (kind: ServiceKind, name: string, url: string) => Promise<void> } = $props();

  const kinds: ServiceKind[] = ["whatsapp", "telegram", "discord", "gmail", "email", "custom"];
  let kind = $state<ServiceKind>("whatsapp");
  let name = $state("");
  let url = $state("");
  let error = $state("");
  let busy = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (needsUrl(kind) && !url.trim()) {
      error = kind === "email" ? "Informe o endereço do webmail." : "Informe o endereço do site.";
      return;
    }
    busy = true;
    error = "";
    try {
      await onsubmit(kind, name.trim(), needsUrl(kind) ? url.trim() : "");
    } catch (err) {
      error = errorMessage(err);
      busy = false;
    }
  }
</script>

<Modal {onclose} label="Adicionar serviço" width={600} flush>
  <form onsubmit={submit}>
    <header class="head">
      <span class="head-icon" aria-hidden="true"><CirclePlus size={20} /></span>
      <div class="head-text">
        <h2>Adicionar serviço</h2>
        <p>Cada serviço tem sessão própria: dá para ter duas contas do mesmo mensageiro.</p>
      </div>
      <button type="button" class="close" onclick={onclose} aria-label="Fechar"><X size={18} /></button>
    </header>

    <div class="content">
    <fieldset class="kinds">
      <legend class="sr-only">Tipo de serviço</legend>
      {#each kinds as k (k)}
        <label class="kind" class:selected={kind === k}>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            type="radio"
            name="kind"
            value={k}
            bind:group={kind}
            class="sr-only"
            autofocus={k === "whatsapp"}
          />
          <ServiceAvatar service={{ kind: k }} size={30} />
          {#if kind === k}<span class="check" aria-hidden="true"><Check size={12} strokeWidth={3} /></span>{/if}
          <span class="kind-label">{KIND_META[k].label}</span>
          <span class="kind-hint">{KIND_META[k].hint}</span>
        </label>
      {/each}
    </fieldset>

    {#if needsUrl(kind)}
      <div class="field">
        <label for="svc-url">{kind === "email" ? "Endereço do webmail" : "Endereço"}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="svc-url"
          class="input"
          bind:value={url}
          placeholder={kind === "email" ? "webmail.seudominio.com.br" : "app.exemplo.com"}
          autofocus
        />
        {#if kind === "email"}
          <p class="note">
            O mesmo endereço em que você lê esse e-mail no navegador. No Roundcube, o Papeo também lê o assunto
            aberto e as não lidas.
          </p>
        {/if}
        {#if url.trim().toLowerCase().startsWith("http://")}
          <p class="warn" role="note">
            Endereço sem criptografia (http): mensagens e senhas podem ser lidas por quem estiver na mesma
            rede. Prefira https.
          </p>
        {/if}
      </div>
    {:else if kind === "gmail"}
      <p class="note">
        Para receber avisos de e-mails novos, ligue no Gmail: Configurações → Ver todas as configurações →
        Notificações da área de trabalho.
      </p>
    {/if}

    <div class="field">
      <label for="svc-name">Nome <span class="muted">(opcional)</span></label>
      <input
        id="svc-name"
        class="input"
        bind:value={name}
        placeholder={kind === "custom"
          ? "Ex.: Slack da empresa"
          : kind === "email"
            ? "Ex.: E-mail da empresa"
            : `Ex.: ${KIND_META[kind].label} trabalho`}
      />
    </div>

    {#if error}<p class="error" role="alert">{error}</p>{/if}
    </div>

    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>Cancelar</button>
      <button type="submit" class="btn btn-primary" disabled={busy}>{busy ? "Adicionando…" : "Adicionar"}</button>
    </div>
  </form>
</Modal>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 20px 20px 18px 24px;
    border-bottom: 1px solid var(--border);
  }

  .head-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 11px;
    border: 1px solid var(--border);
    background: var(--bg-subtle);
    color: var(--text-muted);
  }

  .head-text {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .head-text p {
    margin: 2px 0 0;
    font-size: 13px;
    color: var(--text-muted);
  }

  .close {
    display: grid;
    place-items: center;
    flex: none;
    align-self: flex-start;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
  }

  .close:hover {
    background: var(--bg-hover);
    color: var(--text);
  }

  .content {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px 24px;
    max-height: min(62vh, 560px);
    overflow-y: auto;
  }

  .muted {
    color: var(--text-muted);
    font-weight: 400;
  }

  .kinds {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin: 0;
    padding: 0;
    border: 0;
  }

  @media (max-width: 560px) {
    .kinds {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  .kind {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    padding: 14px 12px 12px;
    border: 1px solid var(--border);
    border-radius: 12px;
    cursor: pointer;
    transition:
      border-color 0.12s,
      background 0.12s;
  }

  .kind:hover {
    background: var(--bg-hover);
  }

  .kind.selected {
    border-color: var(--text);
    background: var(--bg-subtle);
  }

  .kind:has(:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .kind :global(.avatar) {
    margin-bottom: 8px;
  }

  .check {
    position: absolute;
    top: 10px;
    right: 10px;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--text);
    color: var(--bg);
  }

  .kind-label {
    font-weight: 550;
  }

  .kind-hint {
    font-size: 11.5px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .error {
    margin: 0;
    color: var(--danger);
  }

  .note {
    margin: 6px 0 0;
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .kinds + .note {
    margin: 0;
  }

  .warn {
    margin: 6px 0 0;
    padding: 8px 10px;
    border-radius: 8px;
    font-size: 13px;
    color: var(--warn-text);
    background: var(--warn-bg);
    border: 1px solid var(--warn-border);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 14px 20px;
    border-top: 1px solid var(--border);
  }
</style>
