<script lang="ts">
  import { onMount } from "svelte";
  import { Cloud, CloudOff, KeyRound, RefreshCw } from "@lucide/svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { errorMessage, onSync, syncApi as api, type SyncStatus } from "./api";

  const MIN = 10;

  let status = $state<SyncStatus | null>(null);
  let busy = $state<"" | "connect" | "pass" | "now" | "off">("");
  let message = $state("");
  let pass = $state("");
  let pass2 = $state("");
  let resetting = $state(false);
  let confirmOff = $state(false);

  const create = $derived(status?.needsPassphrase === "create" || resetting);
  const passOk = $derived(pass.length >= MIN && (!create || pass === pass2));

  onMount(() => {
    api.syncStatus().then((s) => (status = s)).catch(() => {});
    const off = onSync((s) => (status = s));
    return () => {
      off.then((f) => f());
    };
  });

  async function act(kind: typeof busy, work: () => Promise<SyncStatus>) {
    busy = kind;
    message = "";
    try {
      status = await work();
    } catch (err) {
      message = errorMessage(err);
      status = await api.syncStatus().catch(() => status);
    } finally {
      busy = "";
    }
  }

  const connect = () => act("connect", api.syncConnect);

  function submitPass(e: Event) {
    e.preventDefault();
    if (!passOk) return;
    const p = pass;
    const reset = resetting;
    act("pass", () => (reset ? api.syncReset(p) : api.syncPassphrase(p))).then(() => {
      if (!message) {
        pass = pass2 = "";
        resetting = false;
      }
    });
  }

  function ago(secs: number | null | undefined): string {
    if (!secs) return "Ainda não sincronizou";
    const min = Math.floor((Date.now() / 1000 - secs) / 60);
    if (min < 1) return "Sincronizado agora";
    if (min < 60) return `Sincronizado há ${min} min`;
    const h = Math.floor(min / 60);
    return h < 24 ? `Sincronizado há ${h} h` : `Sincronizado há ${Math.floor(h / 24)} dias`;
  }
</script>

<!-- Em espera: só aparece em build com o cliente OAuth do Google
     (PAPEO_GOOGLE_CLIENT_ID/SECRET). Ver docs/PROGRESSO.md. -->
{#if status?.available}
<section class="card" aria-labelledby="s-sync">
  <h2 id="s-sync"><Cloud size={18} /> Sincronização</h2>

  {#if !status.signedIn}
    <div class="row">
      <div class="text">
        <span class="label">Tarefas e anotações em todos os seus computadores</span>
        <span class="hint">
          Pela sua conta Google, numa pasta oculta só do Papeo no Drive (ele não vê seus outros
          arquivos). Tudo vai criptografado com uma frase-senha que só você sabe.
        </span>
      </div>
      <div class="choices">
        <button class="btn btn-primary" disabled={busy === "connect"} onclick={connect}>
          <Cloud size={16} />
          {busy === "connect" ? "Conclua no navegador…" : "Conectar Google Drive"}
        </button>
      </div>
    </div>
  {:else if status.needsPassphrase || resetting || !status.connected}
    <form class="pass" onsubmit={submitPass}>
      <div class="text">
        <span class="label">
          <KeyRound size={15} />
          {create ? "Crie a frase-senha" : "Digite a frase-senha"}
        </span>
        <span class="hint">
          {#if resetting}
            Os dados na nuvem serão apagados e a sincronização recomeça com uma frase nova. As
            tarefas deste computador ficam. Os outros computadores vão pedir a frase nova.
          {:else if create}
            Ela cifra tudo antes de sair deste computador. Anote num lugar seguro: será pedida
            uma vez em cada computador e, se esquecida, os dados da nuvem não podem ser lidos
            (os de cada computador continuam).
          {:else}
            A mesma usada nos seus outros computadores.
          {/if}
        </span>
      </div>
      <input
        type="password"
        bind:value={pass}
        placeholder="Frase-senha (mínimo {MIN} caracteres)"
        autocomplete={create ? "new-password" : "current-password"}
        aria-label="Frase-senha"
      />
      {#if create}
        <input
          type="password"
          bind:value={pass2}
          placeholder="Repita a frase-senha"
          autocomplete="new-password"
          aria-label="Repita a frase-senha"
        />
        {#if pass2 && pass !== pass2}<span class="hint warn">As duas não são iguais.</span>{/if}
      {/if}
      <div class="actions">
        {#if !create}
          <button type="button" class="link" onclick={() => (resetting = true)}>Esqueci a frase-senha</button>
        {:else if resetting}
          <button type="button" class="link" onclick={() => (resetting = false)}>Voltar</button>
        {/if}
        <button type="button" class="btn" onclick={() => (confirmOff = true)}>Cancelar</button>
        <button type="submit" class="btn btn-primary" class:danger={resetting} disabled={!passOk || busy === "pass"}>
          {busy === "pass" ? "Aguarde…" : resetting ? "Apagar a nuvem e recomeçar" : create ? "Criar e sincronizar" : "Sincronizar"}
        </button>
      </div>
    </form>
  {:else}
    <div class="row">
      <div class="text">
        <span class="label">Conectado ao Google Drive</span>
        <span class="hint" role="status">
          {status.running ? "Sincronizando…" : ago(status.lastSync)}
          · este computador: {status.deviceName}
          {#if status.devices > 0}
            · {status.devices} {status.devices === 1 ? "outro computador" : "outros computadores"}
          {/if}
        </span>
        {#if status.error}<span class="hint warn">{status.error}</span>{/if}
      </div>
      <div class="choices">
        <button class="btn" disabled={status.running || busy === "now"} onclick={() => act("now", api.syncNow)}>
          <RefreshCw size={16} /> Sincronizar agora
        </button>
        <button class="btn" onclick={() => (confirmOff = true)}><CloudOff size={16} /> Desconectar</button>
      </div>
    </div>
  {/if}
  {#if message}<p class="message warn" role="alert">{message}</p>{/if}
</section>
{/if}

{#if confirmOff}
  <ConfirmDialog
    title="Desconectar a sincronização?"
    message="Este computador para de sincronizar e sai da conta Google no Papeo. As tarefas e anotações daqui continuam. A cópia deste computador na nuvem é apagada; a dos outros fica."
    confirmLabel="Desconectar"
    onclose={() => (confirmOff = false)}
    onconfirm={() => {
      confirmOff = false;
      resetting = false;
      pass = pass2 = "";
      act("off", () => api.syncDisconnect(true));
    }}
  />
{/if}

<style>
  .card {
    padding: 18px 22px 6px;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 6px;
    font-size: 16px;
    font-weight: 600;
  }

  h2 :global(svg) {
    color: var(--text-muted);
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 14px 0;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 500;
  }

  .hint {
    font-size: 13px;
    color: var(--text-muted);
  }

  .warn {
    color: var(--danger);
  }

  .choices {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }

  .choices .btn {
    height: 34px;
  }

  .pass {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 0;
    max-width: 560px;
  }

  .pass input {
    height: 36px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-subtle);
    user-select: text;
    -webkit-user-select: text;
  }

  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .link {
    margin-right: auto;
    border: 0;
    background: none;
    padding: 0;
    color: var(--accent);
    font-size: 13px;
  }

  .link:hover {
    text-decoration: underline;
  }

  .danger {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }

  .message {
    margin: 0 0 14px;
    font-size: 13px;
  }
</style>
