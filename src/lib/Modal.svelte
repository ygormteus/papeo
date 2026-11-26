<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    onclose,
    label,
    width = 440,
    top = false,
    flush = false,
    children,
  }: {
    onclose: () => void;
    label: string;
    width?: number;
    top?: boolean;
    /** Sem o espaçamento interno: o conteúdo cuida do próprio layout. */
    flush?: boolean;
    children: Snippet;
  } = $props();

  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
    return () => dialog.close();
  });
</script>

<!-- O <dialog> nativo cuida do foco preso e da tecla Esc. -->
<dialog
  bind:this={dialog}
  class:top
  class:flush
  aria-label={label}
  style:width="min({width}px, calc(100vw - 32px))"
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
  onclick={(e) => {
    if (e.target === dialog) onclose();
  }}
>
  <div class="body">
    {@render children()}
  </div>
</dialog>

<style>
  dialog {
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 16px;
    background: var(--bg);
    color: var(--text);
    box-shadow: var(--shadow-pop);
  }

  dialog.top {
    margin-top: 14vh;
  }

  dialog::backdrop {
    background: rgb(0 0 0 / 0.32);
  }

  /* Abertura: só opacidade e escala (a placa de vídeo faz, sem recalcular
     layout). Com "reduzir movimento", o app.css desliga as animações. */
  dialog[open] {
    animation: modal-in 0.2s cubic-bezier(0.2, 0.9, 0.3, 1);
  }

  dialog[open]::backdrop {
    animation: backdrop-in 0.2s ease-out;
  }

  @keyframes modal-in {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.97);
    }
  }

  @keyframes backdrop-in {
    from {
      opacity: 0;
    }
  }

  .body {
    padding: 22px;
  }

  .top .body,
  .flush .body {
    padding: 0;
  }

  dialog.flush {
    overflow: hidden;
  }
</style>
