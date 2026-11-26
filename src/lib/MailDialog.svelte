<script lang="ts">
  import { CircleAlert, CircleCheck, KeyRound, LoaderCircle, WifiOff } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import Switch from "./Switch.svelte";
  import { GMAIL_IMAP, api, errorMessage, type MailInput, type ServiceView } from "./api";

  let {
    service,
    onclose,
    onsave,
  }: {
    /** Conta ao vivo (a situação do IMAP muda enquanto o diálogo está aberto). */
    service: ServiceView;
    onclose: () => void;
    /** Grava (ou desliga, com `null`); erros sobem para o diálogo. */
    onsave: (account: MailInput | null) => Promise<void>;
  } = $props();

  const gmail = $derived(service.kind === "gmail");
  // Cópia local editável; a conta só muda ao salvar.
  // svelte-ignore state_referenced_locally
  const start = service.mail;
  // svelte-ignore state_referenced_locally
  let host = $state(start?.host ?? (service.kind === "gmail" ? GMAIL_IMAP.host : ""));
  // svelte-ignore state_referenced_locally
  let port = $state(String(start?.port ?? GMAIL_IMAP.port));
  let user = $state(start?.user ?? "");
  let password = $state("");
  let sleepSite = $state(start?.sleepSite ?? true);
  let error = $state("");
  let busy = $state(false);
  // Salvo nesta abertura: o diálogo fica aberto mostrando a conexão.
  let saved = $state(false);

  const configured = $derived(!!service.mail);
  // Servidor ou usuário trocados exigem a senha de novo (ela fica presa a eles).
  const needsPassword = $derived(
    !configured ||
      service.mailStatus?.state === "noPassword" ||
      host.trim().toLowerCase() !== service.mail?.host ||
      user.trim() !== service.mail?.user ||
      Number(port) !== service.mail?.port,
  );
  const dirty = $derived(
    !configured ||
      password !== "" ||
      host.trim().toLowerCase() !== service.mail?.host ||
      user.trim() !== service.mail?.user ||
      Number(port) !== service.mail?.port ||
      sleepSite !== service.mail?.sleepSite,
  );
  const canSave = $derived(
    !busy && dirty && !!host.trim() && !!user.trim() && (!needsPassword || password !== ""),
  );

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    const portNumber = Number(port);
    if (!Number.isInteger(portNumber) || portNumber < 1 || portNumber > 65535) {
      error = "Porta inválida.";
      return;
    }
    busy = true;
    error = "";
    try {
      await onsave({
        host: host.trim(),
        port: portNumber,
        user: user.trim(),
        password: password || undefined,
        sleepSite,
      });
      password = "";
      saved = true;
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function turnOff() {
    busy = true;
    error = "";
    try {
      await onsave(null);
      onclose();
    } catch (err) {
      error = errorMessage(err);
      busy = false;
    }
  }
</script>

<Modal {onclose} label="Avisos por IMAP de {service.name}" width={480}>
  <form onsubmit={submit}>
    <header>
      <h2>Avisos por IMAP</h2>
      <p class="lead">
        O Papeo recebe o remetente e o assunto dos e-mails novos de {service.name} direto do servidor, sem precisar
        do site aberto.
      </p>
    </header>

    {#if configured && service.mailStatus}
      {@const status = service.mailStatus}
      <div
        class="status"
        class:ok={status.state === "ok"}
        class:warn={status.state === "offline" || status.state === "noPassword"}
        class:bad={status.state === "auth"}
        role="status"
      >
        {#if status.state === "ok"}
          <CircleCheck size={16} />
          <span>
            Conectado.
            {service.unread === 0
              ? "Nenhum e-mail não lido."
              : service.unread === 1
                ? "1 e-mail não lido."
                : `${service.unread} e-mails não lidos.`}
          </span>
        {:else if status.state === "connecting"}
          <LoaderCircle size={16} class="spin" />
          <span>Conectando…</span>
        {:else if status.state === "offline"}
          <WifiOff size={16} />
          <span>{status.detail}</span>
        {:else if status.state === "auth"}
          <CircleAlert size={16} />
          <span>
            Usuário ou senha recusados. {gmail ? "Use uma senha de app (veja abaixo), não a senha do Google." : ""}
            Enquanto isso, os avisos vêm pelo site.
          </span>
        {:else}
          <KeyRound size={16} />
          <span>Falta a senha neste computador (ela não vai no backup).</span>
        {/if}
      </div>
    {/if}

    {#if gmail}
      <div class="help">
        <p>
          O Gmail pede uma <strong>senha de app</strong>, não a sua senha do Google. Ela exige a verificação em duas
          etapas ligada. Nas contas do Google Workspace, o administrador precisa permitir.
        </p>
        <button type="button" class="link" onclick={() => api.openUrl("https://myaccount.google.com/apppasswords")}>
          Criar uma senha de app no Google
        </button>
      </div>
    {/if}

    <section aria-label="Conta">
      <div class="row">
        <div class="field grow">
          <label for="ml-host">Servidor IMAP</label>
          <input
            id="ml-host"
            class="input"
            bind:value={host}
            placeholder="imap.seuprovedor.com.br"
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
          />
        </div>
        <div class="field port">
          <label for="ml-port">Porta</label>
          <input id="ml-port" class="input" bind:value={port} inputmode="numeric" maxlength="5" />
        </div>
      </div>
      <div class="field">
        <label for="ml-user">Usuário</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="ml-user"
          class="input"
          bind:value={user}
          placeholder="voce@exemplo.com"
          autocomplete="off"
          autocapitalize="off"
          spellcheck="false"
          autofocus={!configured}
        />
      </div>
      <div class="field">
        <label for="ml-pass">{gmail ? "Senha de app" : "Senha"}</label>
        <input
          id="ml-pass"
          class="input"
          type="password"
          bind:value={password}
          autocomplete="off"
          placeholder={needsPassword ? "" : "Guardada; digite só para trocar"}
        />
      </div>
      {#if !gmail}
        <p class="note">Use os dados de IMAP do seu provedor (conexão segura, normalmente na porta 993).</p>
      {/if}
    </section>

    <section aria-label="Site">
      <div class="toggle">
        <div>
          <span class="toggle-label">Hibernar o site enquanto o IMAP avisa</span>
          <p class="note">
            Gasta bem menos memória. O site abre quando você clicar na conta ou num aviso. Se o IMAP cair, o site
            volta a avisar.
          </p>
        </div>
        <Switch checked={sleepSite} label="Hibernar o site enquanto o IMAP avisa" onchange={(v) => (sleepSite = v)} />
      </div>
    </section>

    <p class="privacy">
      A senha fica só neste computador, fora do backup e da sincronização. O Papeo lê só o remetente e o assunto dos
      e-mails novos e não marca nada como lido.
      {#if gmail}No Gmail, o contador segue a aba Principal.{/if}
    </p>

    <div class="footer">
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions">
        {#if configured}
          <button type="button" class="btn danger-text" disabled={busy} onclick={turnOff}>Desligar</button>
        {/if}
        <span class="spacer"></span>
        <button type="button" class="btn" onclick={onclose}>{saved && !dirty ? "Fechar" : "Cancelar"}</button>
        <button type="submit" class="btn btn-primary" disabled={!canSave}>
          {busy ? "Salvando…" : configured ? "Salvar" : "Ligar"}
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
    margin: 0 0 6px;
    font-size: 18px;
    font-weight: 600;
  }

  .lead {
    margin: 0;
    color: var(--text-muted);
  }

  .status {
    --tone: var(--text-muted);
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid color-mix(in srgb, var(--tone) 30%, var(--border));
    background: color-mix(in srgb, var(--tone) 8%, var(--bg-subtle));
    font-size: 13px;
  }

  .status :global(svg) {
    flex: none;
    margin-top: 1px;
    color: var(--tone);
  }

  .status.ok {
    --tone: var(--success-text);
  }

  .status.warn {
    --tone: var(--warn);
  }

  .status.bad {
    --tone: var(--danger);
  }

  .status :global(.spin) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .status :global(.spin) {
      animation: none;
    }
  }

  .help {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 12px 14px;
    border-radius: 10px;
    background: var(--bg-subtle);
    border: 1px solid var(--border);
    font-size: 13px;
  }

  .help p {
    margin: 0;
    color: var(--text-muted);
  }

  .help strong {
    color: var(--text);
    font-weight: 600;
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    font-weight: 500;
    cursor: pointer;
  }

  .link:hover {
    text-decoration: underline;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  .row {
    display: flex;
    gap: 10px;
  }

  .grow {
    flex: 1;
    min-width: 0;
  }

  .port {
    width: 84px;
    flex: none;
  }

  .toggle {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  .toggle-label {
    font-size: 13px;
    font-weight: 500;
  }

  .note {
    margin: 2px 0 0;
    color: var(--text-muted);
    font-size: 12.5px;
  }

  .privacy {
    margin: 0;
    color: var(--text-muted);
    font-size: 12.5px;
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
    align-items: center;
    gap: 8px;
  }

  .spacer {
    flex: 1;
  }

  .danger-text {
    color: var(--danger);
  }
</style>
