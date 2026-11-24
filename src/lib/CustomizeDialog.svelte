<script lang="ts">
  import { Check } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import {
    EMOJI_SUGGESTIONS,
    KIND_META,
    MEDIA_OPTIONS,
    SERVICE_COLORS,
    errorMessage,
    mediaPolicy,
    type MediaPolicy,
    type ServiceView,
  } from "./api";

  let {
    service,
    onclose,
    onsubmit,
  }: {
    service: ServiceView;
    onclose: () => void;
    onsubmit: (
      name: string,
      color: string | null,
      emoji: string | null,
      media: MediaPolicy,
    ) => Promise<void>;
  } = $props();

  // Cópia local editável; o serviço original só muda ao salvar.
  // svelte-ignore state_referenced_locally
  let name = $state(service.name);
  // svelte-ignore state_referenced_locally
  let color = $state<string | null>(service.color ?? null);
  // svelte-ignore state_referenced_locally
  let emoji = $state(service.emoji ?? "");
  // svelte-ignore state_referenced_locally
  let media = $state<MediaPolicy>(mediaPolicy(service));
  let error = $state("");
  let busy = $state(false);

  const preview = $derived({ kind: service.kind, color: color ?? undefined, emoji: emoji.trim() || undefined });
  const defaultColor = $derived(KIND_META[service.kind].color);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!name.trim()) {
      error = "O nome não pode ficar vazio.";
      return;
    }
    busy = true;
    error = "";
    try {
      await onsubmit(name.trim(), color, emoji.trim() || null, media);
    } catch (err) {
      error = errorMessage(err);
      busy = false;
    }
  }
</script>

<Modal {onclose} label="Personalizar {service.name}" width={460}>
  <form onsubmit={submit}>
    <div class="head">
      <ServiceAvatar service={preview} size={44} />
      <div>
        <h2>Personalizar</h2>
        <p class="muted">Nome, cor e emoji ajudam a diferenciar contas do mesmo mensageiro.</p>
      </div>
    </div>

    <div class="field">
      <label for="cz-name">Nome</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="cz-name" class="input" bind:value={name} maxlength="40" placeholder="Ex.: Pessoal" autofocus />
    </div>

    <fieldset>
      <legend>Cor</legend>
      <div class="swatches">
        <label class="swatch" style:--c={defaultColor} title="Padrão do {KIND_META[service.kind].label}">
          <input type="radio" name="cz-color" value={null} bind:group={color} class="sr-only" />
          <span class="sr-only">Padrão</span>
          {#if color === null}<Check size={14} strokeWidth={3} />{/if}
        </label>
        <span class="sep" aria-hidden="true"></span>
        {#each SERVICE_COLORS as c (c.value)}
          <label class="swatch" style:--c={c.value} title={c.label}>
            <input type="radio" name="cz-color" value={c.value} bind:group={color} class="sr-only" />
            <span class="sr-only">{c.label}</span>
            {#if color === c.value}<Check size={14} strokeWidth={3} />{/if}
          </label>
        {/each}
      </div>
    </fieldset>

    <fieldset>
      <legend>Emoji <span class="muted">(opcional, no lugar do ícone)</span></legend>
      <div class="emojis">
        <button
          type="button"
          class="emoji-btn none"
          class:selected={!emoji.trim()}
          aria-pressed={!emoji.trim()}
          onclick={() => (emoji = "")}
        >
          Nenhum
        </button>
        {#each EMOJI_SUGGESTIONS as e (e)}
          <button
            type="button"
            class="emoji-btn"
            class:selected={emoji.trim() === e}
            aria-pressed={emoji.trim() === e}
            aria-label="Usar {e}"
            onclick={() => (emoji = e)}
          >
            {e}
          </button>
        {/each}
      </div>
      <input
        class="input emoji-input"
        bind:value={emoji}
        maxlength="12"
        placeholder="Ou cole um emoji / digite até 2 letras"
        aria-label="Emoji personalizado"
      />
    </fieldset>

    <fieldset>
      <legend>Microfone e câmera <span class="muted">(mensagens de voz e chamadas)</span></legend>
      <div class="segmented">
        {#each MEDIA_OPTIONS as opt (opt.value)}
          <label class:selected={media === opt.value}>
            <input type="radio" name="cz-media" value={opt.value} bind:group={media} class="sr-only" />
            {opt.label}
          </label>
        {/each}
      </div>
      <p class="muted note">
        {media === "allow"
          ? "O site usa o microfone sem perguntar de novo (o sistema pode pedir autorização uma vez)."
          : media === "ask"
            ? "O site pede permissão a cada uso."
            : "Mensagens de voz e chamadas ficam bloqueadas neste serviço."}
      </p>
    </fieldset>

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>Cancelar</button>
      <button type="submit" class="btn btn-primary" disabled={busy || !name.trim()}>
        {busy ? "Salvando…" : "Salvar"}
      </button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }

  .muted {
    margin: 2px 0 0;
    color: var(--text-muted);
    font-weight: 400;
    font-size: 13px;
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  legend {
    padding: 0;
    margin-bottom: 8px;
    font-size: 13px;
    font-weight: 500;
  }

  legend .muted {
    display: inline;
    font-size: 12.5px;
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .swatch {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 9px;
    background: var(--c);
    color: #fff;
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.08);
  }

  .swatch:has(:checked) {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }

  .swatch:has(:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .sep {
    width: 1px;
    height: 20px;
    background: var(--border);
  }

  .emojis {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .emoji-btn {
    width: 34px;
    height: 34px;
    border-radius: 9px;
    border: 1px solid var(--border);
    background: var(--bg);
    font-size: 18px;
    line-height: 1;
  }

  .emoji-btn.none {
    width: auto;
    padding: 0 10px;
    font-size: 13px;
    color: var(--text-muted);
  }

  .emoji-btn:hover {
    background: var(--bg-hover);
  }

  .emoji-btn.selected {
    border-color: var(--text);
    box-shadow: inset 0 0 0 1px var(--text);
  }

  .emoji-input {
    margin-top: 4px;
  }

  .segmented {
    display: inline-flex;
    align-self: flex-start;
    padding: 3px;
    gap: 2px;
    border-radius: 10px;
    background: var(--bg-subtle);
    border: 1px solid var(--border);
  }

  .segmented label {
    padding: 6px 14px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
  }

  .segmented label.selected {
    background: var(--bg);
    box-shadow: var(--shadow-card), 0 0 0 1px var(--border);
    font-weight: 550;
  }

  .segmented label:has(:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .note {
    margin: 0;
  }

  .error {
    margin: 0;
    color: var(--danger);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
