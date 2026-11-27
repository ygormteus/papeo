<script lang="ts">
  import { t, tk } from "./i18n.svelte";
  import Modal from "./Modal.svelte";
  import { api, errorMessage, PIN_MAX, PIN_MIN, type Snapshot } from "./api";

  let {
    mode,
    onclose,
    ondone,
  }: {
    /** Criar, trocar ou remover o PIN do Papeo. */
    mode: "create" | "change" | "remove";
    onclose: () => void;
    ondone: (snap: Snapshot) => void;
  } = $props();

  const TITLE = { create: tk("Criar PIN do Papeo"), change: tk("Trocar PIN"), remove: tk("Remover PIN") };
  const ACTION = { create: tk("Criar e bloquear"), change: tk("Trocar"), remove: tk("Remover") };

  let current = $state("");
  let pin = $state("");
  let confirm = $state("");
  let busy = $state(false);
  let error = $state("");

  const needsCurrent = $derived(mode !== "create");
  const needsNew = $derived(mode !== "remove");

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (busy) return;
    error = "";
    if (needsNew) {
      if ([...pin].length < PIN_MIN) return void (error = t("Use pelo menos {n} caracteres.", { n: PIN_MIN }));
      if (pin !== confirm) return void (error = t("Os dois PINs não são iguais."));
    }
    busy = true;
    try {
      ondone(await api.setLockPin(needsCurrent ? current : null, needsNew ? pin : null));
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<!-- Fechar no meio deixaria o pedido (e o Touch ID) correndo sem tela. -->
<Modal onclose={() => !busy && onclose()} label={t(TITLE[mode])} width={400}>
  <form onsubmit={submit}>
    <h2>{t(TITLE[mode])}</h2>
    <p class="muted">
      {#if mode === "create"}
        {t("Um PIN ou senha só do Papeo, pedido ao abrir o app e depois do tempo escolhido. Se você esquecer, o Papeo sai de todas as contas e apaga as tarefas deste computador para desbloquear.")}
      {:else if mode === "change"}
        {t("Digite o PIN atual e escolha um novo.")}
      {:else}
        {t("Sem o PIN, o bloqueio volta a usar a autenticação do sistema, se houver; senão, é desligado.")}
      {/if}
    </p>
    {#if needsCurrent}
      <div class="field">
        <label for="pin-current">{t("PIN atual")}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="pin-current"
          class="input"
          type="password"
          autocomplete="current-password"
          maxlength={PIN_MAX}
          bind:value={current}
          autofocus
          required
        />
      </div>
    {/if}
    {#if needsNew}
      <div class="field">
        <label for="pin-new">{mode === "change" ? t("Novo PIN") : t("PIN ou senha")}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="pin-new"
          class="input"
          type="password"
          autocomplete="new-password"
          minlength={PIN_MIN}
          maxlength={PIN_MAX}
          bind:value={pin}
          autofocus={!needsCurrent}
          required
          aria-describedby="pin-rule"
        />
        <span id="pin-rule" class="rule">{t("Pelo menos {n} caracteres; números ou letras.", { n: PIN_MIN })}</span>
      </div>
      <div class="field">
        <label for="pin-confirm">{t("Repita o PIN")}</label>
        <input
          id="pin-confirm"
          class="input"
          type="password"
          autocomplete="new-password"
          maxlength={PIN_MAX}
          bind:value={confirm}
          required
        />
      </div>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <button type="button" class="btn" onclick={onclose} disabled={busy}>{t("Cancelar")}</button>
      <button type="submit" class="btn btn-primary" class:danger={mode === "remove"} disabled={busy}>
        {busy ? t("Aguarde…") : t(ACTION[mode])}
      </button>
    </div>
  </form>
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }

  p {
    margin: 0;
  }

  .muted,
  .rule {
    color: var(--text-muted);
  }

  .rule {
    font-size: 12.5px;
  }

  .error {
    color: var(--danger);
    font-size: 13.5px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }

  .danger {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }
</style>
