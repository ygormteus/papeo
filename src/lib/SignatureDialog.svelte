<script lang="ts">
  import Modal from "./Modal.svelte";
  import { api, errorMessage, type ServiceView } from "./api";
  import { t } from "./i18n.svelte";

  // Assinatura do e-mail de uma conta (spec 16), editável do leitor e das
  // Configurações sem abrir a configuração do IMAP.
  let { service, onclose }: { service: ServiceView; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let text = $state(service.mail?.signature ?? "");
  let busy = $state(false);
  let error = $state("");
  const dirty = $derived(text.trim() !== (service.mail?.signature ?? ""));

  async function save() {
    if (busy) return;
    if (!dirty) return onclose();
    busy = true;
    error = "";
    try {
      await api.setMailSignature(service.id, text);
      onclose();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {onclose} label={t("Assinatura de {conta}", { conta: service.name })} width={460}>
  <h2>{t("Assinatura")}</h2>
  <p class="hint">
    {t("Entra sozinha nas mensagens novas, respostas e encaminhamentos de {conta}, no leitor de e-mail do Papeo. Dá para apagar antes de enviar.", { conta: service.name })}
  </p>
  <!-- svelte-ignore a11y_autofocus -->
  <textarea
    class="input"
    bind:value={text}
    rows="5"
    maxlength="2000"
    autofocus
    aria-label={t("Assinatura")}
    placeholder={t("Ana Souza\nLoja Exemplo · (11) 0000-0000")}
    onkeydown={(e) => {
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        void save();
      }
    }}
  ></textarea>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="actions">
    {#if text}<button class="btn clear" onclick={() => (text = "")} disabled={busy}>{t("Sem assinatura")}</button>{/if}
    <button class="btn" onclick={onclose} disabled={busy}>{t("Cancelar")}</button>
    <button class="btn btn-primary" onclick={save} disabled={busy}>{t("Salvar")}</button>
  </div>
</Modal>

<style>
  h2 {
    margin: 0 0 6px;
    font-size: 18px;
    font-weight: 600;
  }
  .hint {
    margin: 0 0 12px;
    color: var(--text-muted);
    font-size: 13px;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    font: inherit;
  }
  .error {
    margin: 8px 0 0;
    color: var(--danger);
    font-size: 13px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
  .clear {
    margin-right: auto;
  }
</style>
