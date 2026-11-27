<script lang="ts">
  import { onMount } from "svelte";
  import Modal from "./Modal.svelte";
  import SignatureEditor from "./SignatureEditor.svelte";
  import { api, errorMessage, type ServiceView } from "./api";
  import { t } from "./i18n.svelte";

  // Assinatura do e-mail de uma conta (specs 16 e 22), editável do leitor,
  // das Configurações e da configuração do IMAP.
  let { service, onclose }: { service: ServiceView; onclose: () => void } = $props();

  let html = $state("");
  /** Como abriu: salvar sem mexer não reescreve a assinatura. */
  let opened = "";
  let loaded = $state(false);
  let busy = $state(false);
  let error = $state("");
  let editor: SignatureEditor | undefined = $state();

  /** Texto com quebras de linha em HTML (assinatura antiga, só texto). */
  function fromText(text: string): string {
    const esc = text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    return esc.split("\n").join("<br>");
  }

  onMount(async () => {
    try {
      const current = await api.mailSignature(service.id);
      html = current.html || fromText(current.text);
      opened = html;
    } catch (err) {
      error = errorMessage(err);
    } finally {
      loaded = true;
    }
  });

  /** Só vale guardar HTML se houver formatação, link ou imagem. */
  const rich = (value: string) => /<(?!\/?(br|div|p)\b)[a-z]/i.test(value) || /\sstyle=/i.test(value);

  async function save() {
    if (busy || !editor) return;
    if (html === opened) return onclose();
    busy = true;
    error = "";
    try {
      const text = editor.text();
      await api.setMailSignature(service.id, text, text && rich(html) ? html : null);
      onclose();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal {onclose} label={t("Assinatura de {conta}", { conta: service.name })} width={620}>
  <h2>{t("Assinatura")}</h2>
  <p class="hint">
    {t("Entra sozinha nas mensagens novas, respostas e encaminhamentos de {conta}, no leitor de e-mail do Papeo. Dá para apagar antes de enviar.", { conta: service.name })}
    {t("Cole a assinatura que você já usa (do Gmail, Outlook ou site) ou monte aqui, com logo, cores e links.")}
  </p>
  {#if loaded}
    <SignatureEditor bind:this={editor} bind:html onerror={(m) => (error = m)} onsave={save} />
  {:else}
    <p class="hint">{t("Carregando…")}</p>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="actions">
    {#if html}<button class="btn clear" onclick={() => { html = ""; editor?.clear(); }} disabled={busy}>{t("Sem assinatura")}</button>{/if}
    <button class="btn" onclick={onclose} disabled={busy}>{t("Cancelar")}</button>
    <button class="btn btn-primary" onclick={save} disabled={busy || !loaded}>{t("Salvar")}</button>
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
