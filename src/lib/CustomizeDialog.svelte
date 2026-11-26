<script lang="ts">
  import { Ban, Check } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import {
    CONTEXT_OPTIONS,
    EMOJI_SUGGESTIONS,
    KIND_META,
    MEDIA_OPTIONS,
    SERVICE_COLORS,
    errorMessage,
    mediaPolicy,
    type MediaPolicy,
    type ServiceContext,
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
      context: ServiceContext | null,
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
  // svelte-ignore state_referenced_locally
  let context = $state<ServiceContext | null>(service.context ?? null);
  let error = $state("");
  let busy = $state(false);

  const preview = $derived({ kind: service.kind, color: color ?? undefined, emoji: emoji.trim() || undefined });
  const defaultColor = $derived(KIND_META[service.kind].color);
  const contextLabel = $derived(
    context === "work" ? "Trabalho" : context === "personal" ? "Pessoal" : "Todos os modos",
  );

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!name.trim()) {
      error = "O nome não pode ficar vazio.";
      return;
    }
    busy = true;
    error = "";
    try {
      await onsubmit(name.trim(), color, emoji.trim() || null, media, context);
    } catch (err) {
      error = errorMessage(err);
      busy = false;
    }
  }
</script>

<Modal {onclose} label="Personalizar {service.name}" width={460}>
  <form onsubmit={submit}>
    <h2>Personalizar conta</h2>

    <!-- Prévia ao vivo: como a conta aparece na barra lateral. -->
    <div class="preview" style:--c={color ?? defaultColor}>
      <ServiceAvatar service={preview} size={48} />
      <div class="preview-text">
        <span class="preview-name">{name.trim() || "Sem nome"}</span>
        <span class="preview-meta">{KIND_META[service.kind].label} · {contextLabel}</span>
      </div>
    </div>

    <section aria-label="Identificação">
      <div class="field">
        <label for="cz-name">Nome</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="cz-name" class="input" bind:value={name} maxlength="40" placeholder="Ex.: Pessoal" autofocus />
      </div>

      <fieldset>
        <legend>Esta conta é de</legend>
        <div class="segmented">
          {#each CONTEXT_OPTIONS as opt (opt.label)}
            <label class:selected={context === opt.value}>
              <input type="radio" name="cz-context" value={opt.value} bind:group={context} class="sr-only" />
              {opt.label}
            </label>
          {/each}
        </div>
        <p class="note">
          {context === "work"
            ? "Aparece e notifica no modo Trabalho; some no modo Pessoal."
            : context === "personal"
              ? "Aparece e notifica no modo Pessoal; some no modo Trabalho."
              : "Aparece em todos os modos."}
        </p>
      </fieldset>
    </section>

    <section aria-label="Aparência">
      <fieldset>
        <legend>Cor</legend>
        <div class="swatches">
          <label class="swatch auto" style:--c={defaultColor} title="Padrão do {KIND_META[service.kind].label}">
            <input type="radio" name="cz-color" value={null} bind:group={color} class="sr-only" />
            <span class="sr-only">Padrão do {KIND_META[service.kind].label}</span>
            {#if color === null}<Check size={14} strokeWidth={3} />{/if}
          </label>
          {#each SERVICE_COLORS as c (c.value)}
            <label class="swatch" style:--c={c.value} title={c.label}>
              <input type="radio" name="cz-color" value={c.value} bind:group={color} class="sr-only" />
              <span class="sr-only">{c.label}</span>
              {#if color === c.value}<Check size={14} strokeWidth={3} />{/if}
            </label>
          {/each}
        </div>
        <p class="note">O primeiro círculo é a cor do {KIND_META[service.kind].label}.</p>
      </fieldset>

      <fieldset>
        <legend>Emoji <span class="hint">opcional, no lugar do ícone</span></legend>
        <div class="emojis">
          <button
            type="button"
            class="emoji-btn"
            class:selected={!emoji.trim()}
            aria-pressed={!emoji.trim()}
            aria-label="Sem emoji (usar o ícone do serviço)"
            title="Sem emoji"
            onclick={() => (emoji = "")}
          >
            <Ban size={16} />
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
          maxlength="24"
          placeholder="Outro: cole um emoji ou digite até 2 letras"
          aria-label="Emoji personalizado"
        />
      </fieldset>
    </section>

    <section aria-label="Permissões">
      <fieldset>
        <legend>Microfone e câmera <span class="hint">mensagens de voz e chamadas</span></legend>
        <div class="segmented">
          {#each MEDIA_OPTIONS as opt (opt.value)}
            <label class:selected={media === opt.value}>
              <input type="radio" name="cz-media" value={opt.value} bind:group={media} class="sr-only" />
              {opt.label}
            </label>
          {/each}
        </div>
        <p class="note">
          {media === "allow"
            ? "O site usa o microfone sem perguntar de novo (o sistema pode pedir autorização uma vez)."
            : media === "ask"
              ? "O site pede permissão a cada uso."
              : "Mensagens de voz e chamadas ficam bloqueadas neste serviço."}
        </p>
      </fieldset>
    </section>

    <!-- Rodapé fixo: em janelas baixas o diálogo rola, mas o erro e o Salvar
         ficam sempre à vista. -->
    <div class="footer">
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions">
        <button type="button" class="btn" onclick={onclose}>Cancelar</button>
        <button type="submit" class="btn btn-primary" disabled={busy || !name.trim()}>
          {busy ? "Salvando…" : "Salvar"}
        </button>
      </div>
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

  /* Prévia: faixa com um toque da cor escolhida. */
  .preview {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 16px;
    border-radius: 14px;
    border: 1px solid color-mix(in srgb, var(--c) 30%, var(--border));
    background: color-mix(in srgb, var(--c) 10%, var(--bg-subtle));
    transition:
      background-color 0.18s ease,
      border-color 0.18s ease;
  }

  .preview-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .preview-name {
    font-size: 16px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .preview-meta {
    font-size: 12.5px;
    color: var(--text-muted);
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  legend {
    padding: 0;
    margin-bottom: 8px;
    font-size: 13px;
    font-weight: 500;
  }

  .hint {
    margin-left: 4px;
    color: var(--text-muted);
    font-weight: 400;
    font-size: 12.5px;
  }

  .note {
    margin: 0;
    color: var(--text-muted);
    font-size: 12.5px;
  }

  /* Cores: círculos com anel na cor escolhida. */
  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .swatch {
    position: relative;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--c);
    color: #fff;
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.12);
    transition: transform 0.12s ease;
  }

  .swatch:hover {
    transform: scale(1.08);
  }

  .swatch:has(:checked) {
    box-shadow:
      0 0 0 2px var(--bg),
      0 0 0 4px var(--c);
  }

  .swatch:has(:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 4px;
  }

  /* A cor padrão do serviço, separada das demais. */
  .swatch.auto {
    margin-right: 6px;
  }

  .swatch.auto::after {
    content: "";
    position: absolute;
    right: -9px;
    top: 4px;
    bottom: 4px;
    width: 1px;
    background: var(--border);
  }

  /* Emojis: grade de blocos iguais. */
  .emojis {
    display: grid;
    /* "Sem emoji" + sugestões numa linha só; em telas estreitas, duas. */
    grid-template-columns: repeat(11, minmax(0, 1fr));
    gap: 5px;
  }

  .emoji-btn {
    display: grid;
    place-items: center;
    height: 36px;
    border-radius: 10px;
    border: 1px solid transparent;
    background: var(--bg-subtle);
    color: var(--text-muted);
    font-size: 18px;
    line-height: 1;
    transition: background-color 0.12s ease;
  }

  .emoji-btn:hover {
    background: var(--bg-hover);
  }

  .emoji-btn.selected {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, var(--bg-subtle));
    color: var(--text);
  }

  @media (max-width: 420px) {
    .emojis {
      grid-template-columns: repeat(6, minmax(0, 1fr));
    }
  }

  .emoji-input {
    margin-top: 2px;
  }

  /* Seletores de três opções ocupando a largura toda. */
  .segmented {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    padding: 3px;
    gap: 2px;
    border-radius: 10px;
    background: var(--bg-subtle);
    border: 1px solid var(--border);
  }

  .segmented label {
    padding: 7px 10px;
    border-radius: 8px;
    font-size: 13px;
    text-align: center;
    color: var(--text-muted);
    cursor: pointer;
    transition:
      background-color 0.12s ease,
      color 0.12s ease;
  }

  .segmented label:hover:not(.selected) {
    color: var(--text);
  }

  .segmented label.selected {
    background: var(--bg);
    color: var(--text);
    box-shadow: var(--shadow-card), 0 0 0 1px var(--border);
    font-weight: 550;
  }

  .segmented label:has(:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .error {
    margin: 0;
    color: var(--danger);
  }

  .footer {
    position: sticky;
    bottom: 0;
    z-index: 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0 -22px -22px;
    padding: 14px 22px 22px;
    border-top: 1px solid var(--border);
    background: var(--bg);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
