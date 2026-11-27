<script lang="ts">
  import type { Snippet } from "svelte";
  import Modal from "./Modal.svelte";
  import { t } from "./i18n.svelte";

  let {
    title,
    message,
    confirmLabel,
    cancelLabel,
    danger = true,
    onclose,
    oncancel,
    onconfirm,
    children,
  }: {
    title: string;
    message: string;
    confirmLabel: string;
    cancelLabel?: string;
    /** Botão de confirmar em vermelho (ação destrutiva). */
    danger?: boolean;
    /** Esc, clique fora e, sem `oncancel`, o botão secundário. */
    onclose: () => void;
    /** Botão secundário, quando ele não equivale a só fechar. */
    oncancel?: () => void;
    onconfirm: () => void;
    /** Conteúdo extra entre a mensagem e os botões. */
    children?: Snippet;
  } = $props();
</script>

<Modal {onclose} label={title} width={400}>
  <h2>{title}</h2>
  <p>{message}</p>
  {@render children?.()}
  <div class="actions">
    <!-- svelte-ignore a11y_autofocus -->
    <button class="btn" onclick={oncancel ?? onclose} autofocus>{cancelLabel ?? t("Cancelar")}</button>
    <button class="btn btn-primary" class:danger onclick={onconfirm}>{confirmLabel}</button>
  </div>
</Modal>

<style>
  h2 {
    margin: 0 0 8px;
    font-size: 18px;
    font-weight: 600;
  }

  p {
    margin: 0 0 20px;
    color: var(--text-muted);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .danger {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }
</style>
