<script lang="ts">
  import { t } from "./i18n.svelte";
  import { Lock } from "@lucide/svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { errorMessage, isMac, PIN_MAX, type LockMethod } from "./api";

  let {
    method,
    onunlock,
    onforget,
  }: {
    method: LockMethod;
    onunlock: (pin?: string) => Promise<void>;
    /** "Esqueci o PIN": sai de todas as contas e desbloqueia. */
    onforget: () => Promise<void>;
  } = $props();

  let busy = $state(false);
  let error = $state("");
  let pin = $state("");
  let confirmForget = $state(false);
  let forgetting = $state(false);
  let input = $state<HTMLInputElement>();

  async function unlock(e?: SubmitEvent) {
    e?.preventDefault();
    if (busy) return;
    busy = true;
    error = "";
    try {
      await onunlock(method === "pin" ? pin : undefined);
    } catch (err) {
      error = errorMessage(err);
      pin = "";
    } finally {
      busy = false;
      // Errou: o campo volta a ter foco para tentar de novo.
      if (method === "pin") requestAnimationFrame(() => input?.focus());
    }
  }

  async function forget() {
    confirmForget = false;
    busy = true;
    forgetting = true;
    error = "";
    try {
      await onforget();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
      forgetting = false;
    }
  }
</script>

<div class="lock" role="dialog" aria-modal="true" aria-labelledby="lock-title" data-tauri-drag-region>
  <div class="panel">
    <span class="badge" aria-hidden="true"><Lock size={26} /></span>
    <h1 id="lock-title">{t("O Papeo está bloqueado")}</h1>
    {#if method === "pin"}
      <p class="muted">{t("Digite o PIN do Papeo para ver suas conversas.")}</p>
      <form onsubmit={unlock}>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          bind:this={input}
          bind:value={pin}
          class="input"
          type="password"
          autocomplete="current-password"
          aria-label={t("PIN do Papeo")}
          maxlength={PIN_MAX}
          disabled={busy}
          autofocus
        />
        <button type="submit" class="btn btn-primary" disabled={busy || !pin}>
          {forgetting ? t("Saindo das contas…") : busy ? t("Conferindo…") : t("Desbloquear")}
        </button>
      </form>
    {:else}
      <p class="muted">
        {isMac
          ? t("Use o Touch ID ou a senha do Mac para ver suas conversas.")
          : t("Use o Windows Hello (rosto, digital ou PIN) para ver suas conversas.")}
      </p>
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn btn-primary" onclick={() => unlock()} disabled={busy} autofocus>
        {busy ? t("Aguardando o sistema…") : t("Desbloquear")}
      </button>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if method === "pin"}
      <button class="link" onclick={() => (confirmForget = true)} disabled={busy}>{t("Esqueci o PIN")}</button>
    {/if}
  </div>
</div>

{#if confirmForget}
  <ConfirmDialog
    title={t("Desbloquear sem o PIN?")}
    message={t("Para proteger suas conversas, o Papeo sai de todas as contas, apaga as tarefas e anotações deste computador e desliga a sincronização. Depois, entre em cada serviço de novo. Notificações já mostradas e arquivos baixados continuam no computador. Isso não pode ser desfeito.")}
    confirmLabel={t("Sair de tudo e desbloquear")}
    onclose={() => (confirmForget = false)}
    onconfirm={forget}
  />
{/if}

<style>
  .lock {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    padding: 16px;
    background: var(--bg);
  }

  .panel {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    max-width: 380px;
    text-align: center;
  }

  .badge {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    border-radius: 16px;
    background: var(--bg-hover);
    color: var(--text);
  }

  h1 {
    margin: 4px 0 0;
    font-size: 22px;
    font-weight: 600;
  }

  p {
    margin: 0;
  }

  .muted {
    color: var(--text-muted);
  }

  form {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
  }

  .input {
    width: 220px;
    max-width: 100%;
    text-align: center;
    letter-spacing: 0.2em;
  }

  .btn {
    margin-top: 8px;
    min-width: 180px;
    height: 38px;
  }

  form .btn {
    margin-top: 0;
  }

  .link {
    border: 0;
    background: none;
    padding: 4px;
    color: var(--text-muted);
    font-size: 13px;
    text-decoration: underline;
    cursor: pointer;
  }

  .link:hover:not(:disabled) {
    color: var(--text);
  }

  .error {
    color: var(--danger);
    font-size: 13.5px;
  }
</style>
