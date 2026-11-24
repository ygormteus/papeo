<script lang="ts">
  import { Lock } from "@lucide/svelte";
  import { errorMessage, isMac } from "./api";

  let { onunlock }: { onunlock: () => Promise<void> } = $props();

  let busy = $state(false);
  let error = $state("");

  async function unlock() {
    if (busy) return;
    busy = true;
    error = "";
    try {
      await onunlock();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="lock" role="dialog" aria-modal="true" aria-labelledby="lock-title" data-tauri-drag-region>
  <div class="panel">
    <span class="badge" aria-hidden="true"><Lock size={26} /></span>
    <h1 id="lock-title">O Papeo está bloqueado</h1>
    <p class="muted">
      {isMac ? "Use o Touch ID ou a senha do Mac" : "Use o Windows Hello (rosto, digital ou PIN)"} para ver
      suas conversas.
    </p>
    <!-- svelte-ignore a11y_autofocus -->
    <button class="btn btn-primary" onclick={unlock} disabled={busy} autofocus>
      {busy ? "Aguardando o sistema…" : "Desbloquear"}
    </button>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
</div>

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

  .btn {
    margin-top: 8px;
    min-width: 180px;
    height: 38px;
  }

  .error {
    color: var(--danger);
    font-size: 13.5px;
  }
</style>
