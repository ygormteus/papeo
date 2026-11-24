<script lang="ts">
  import Modal from "./Modal.svelte";

  let {
    title,
    message,
    confirmLabel,
    cancelLabel = "Cancelar",
    danger = true,
    onclose,
    oncancel,
    onconfirm,
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
  } = $props();
</script>

<Modal {onclose} label={title} width={400}>
  <h2>{title}</h2>
  <p>{message}</p>
  <div class="actions">
    <!-- svelte-ignore a11y_autofocus -->
    <button class="btn" onclick={oncancel ?? onclose} autofocus>{cancelLabel}</button>
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
