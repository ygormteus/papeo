<script lang="ts">
  import { CircleAlert, CircleCheck, KeyRound, LoaderCircle, LogIn, WifiOff } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import Switch from "./Switch.svelte";
  import {
    GMAIL_IMAP,
    MAIL_PRESETS,
    api,
    errorMessage,
    presetForAddress,
    type MailInput,
    type ServiceView,
  } from "./api";

  let {
    service,
    onclose,
    onsave,
    onmicrosoft,
  }: {
    /** Conta ao vivo (a situação do IMAP muda enquanto o diálogo está aberto). */
    service: ServiceView;
    onclose: () => void;
    /** Grava (ou desliga, com `null`); erros sobem para o diálogo. */
    onsave: (account: MailInput | null) => Promise<void>;
    /** Outlook: login da Microsoft no navegador (liga ou troca a conta). */
    onmicrosoft: (sleepSite: boolean) => Promise<void>;
  } = $props();

  const gmail = $derived(service.kind === "gmail");
  // Outlook: só com o login da Microsoft (servidor e usuário vêm dele).
  const microsoft = $derived(service.kind === "outlook");
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
  let starttls = $state(start?.starttls ?? false);
  let wholeInbox = $state(start?.wholeInbox ?? false);
  // Provedor conhecido escolhido na lista (só preenche servidor e porta).
  // svelte-ignore state_referenced_locally
  let presetId = $state(MAIL_PRESETS.find((p) => p.host === start?.host)?.id ?? "");
  const preset = $derived(MAIL_PRESETS.find((p) => p.id === presetId));

  function choosePreset(id: string) {
    presetId = id;
    const chosen = MAIL_PRESETS.find((p) => p.id === id);
    if (chosen) {
      host = chosen.host;
      port = "993";
      starttls = false;
    }
  }

  // Endereço de um provedor conhecido (uol.com.br…) com o servidor vazio:
  // preenche sozinho.
  function guessFromUser() {
    if (gmail || host.trim()) return;
    const found = presetForAddress(user);
    if (found) choosePreset(found.id);
  }

  function chooseSecurity(useStarttls: boolean) {
    starttls = useStarttls;
    // Troca a porta só se ela ainda era a padrão do outro modo.
    if (useStarttls && port === "993") port = "143";
    if (!useStarttls && port === "143") port = "993";
  }
  let error = $state("");
  let busy = $state(false);
  // Esperando o login no navegador.
  let signingIn = $state(false);

  // Fechar o diálogo (de qualquer jeito) desiste do login no navegador.
  $effect(() => () => {
    if (signingIn) void api.mailMicrosoftCancel().catch(() => {});
  });
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
      sleepSite !== service.mail?.sleepSite ||
      starttls !== (service.mail?.starttls ?? false) ||
      wholeInbox !== (service.mail?.wholeInbox ?? false),
  );
  const canSave = $derived(
    microsoft
      ? !busy && configured && sleepSite !== service.mail?.sleepSite
      : !busy && dirty && !!host.trim() && !!user.trim() && (!needsPassword || password !== ""),
  );

  async function signIn() {
    busy = true;
    signingIn = true;
    error = "";
    try {
      await onmicrosoft(sleepSite);
      saved = true;
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
      signingIn = false;
    }
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (microsoft) {
      if (!configured) return signIn();
      busy = true;
      error = "";
      try {
        // Só o site hibernado muda; o resto vem do login.
        await onsave({ host: service.mail!.host, port: service.mail!.port, user: service.mail!.user, sleepSite });
        saved = true;
      } catch (err) {
        error = errorMessage(err);
      } finally {
        busy = false;
      }
      return;
    }
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
        starttls,
        wholeInbox: gmail && wholeInbox,
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
            {#if microsoft}
              O login da Microsoft expirou ou foi revogado. Entre de novo abaixo.
            {:else}
              Usuário ou senha recusados. {gmail ? "Use uma senha de app (veja abaixo), não a senha do Google." : ""}
            {/if}
            Enquanto isso, os avisos vêm pelo site.
          </span>
        {:else}
          <KeyRound size={16} />
          <span>
            {microsoft
              ? "Falta entrar com a Microsoft neste computador (o login não vai no backup)."
              : "Falta a senha neste computador (ela não vai no backup)."}
          </span>
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

    {#if microsoft}
      <section aria-label="Conta">
        <div class="signin">
          {#if configured}
            <p>Conta: <strong>{service.mail?.user}</strong></p>
          {:else}
            <p>
              Entre com a sua conta da Microsoft (Outlook.com, Hotmail ou Microsoft 365). A senha é digitada só no site
              da Microsoft, no navegador.
            </p>
          {/if}
          <button type="button" class="btn" disabled={busy} onclick={signIn}>
            {#if signingIn}
              <LoaderCircle size={15} class="spin" /> Esperando o navegador…
            {:else}
              <LogIn size={15} />
              {configured ? "Entrar de novo ou trocar de conta" : "Entrar com a Microsoft"}
            {/if}
          </button>
          {#if signingIn}
            <p class="note">Termine o login na janela do navegador que abriu.</p>
          {/if}
          <p class="note">
            Contas de empresa podem precisar que o administrador do Microsoft 365 libere o Papeo e o IMAP.
          </p>
        </div>
      </section>
    {:else}
    <section aria-label="Conta">
      {#if !gmail}
        <div class="field">
          <label for="ml-preset">Provedor</label>
          <select
            id="ml-preset"
            class="select"
            value={presetId}
            onchange={(e) => choosePreset(e.currentTarget.value)}
          >
            <option value="">Outro (preencher à mão)</option>
            {#each MAIL_PRESETS as p (p.id)}
              <option value={p.id}>{p.label}</option>
            {/each}
          </select>
          {#if preset?.note}<p class="note">{preset.note}</p>{/if}
        </div>
      {/if}
      <div class="row">
        <div class="field grow">
          <label for="ml-host">Servidor IMAP</label>
          <input
            id="ml-host"
            class="input"
            bind:value={host}
            oninput={() => (presetId = MAIL_PRESETS.find((p) => p.host === host.trim().toLowerCase())?.id ?? "")}
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
          onblur={guessFromUser}
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
        <fieldset>
          <legend>Conexão segura</legend>
          <div class="segmented">
            <label class:selected={!starttls}>
              <input type="radio" name="ml-sec" checked={!starttls} onchange={() => chooseSecurity(false)} class="sr-only" />
              TLS (porta 993)
            </label>
            <label class:selected={starttls}>
              <input type="radio" name="ml-sec" checked={starttls} onchange={() => chooseSecurity(true)} class="sr-only" />
              STARTTLS (porta 143)
            </label>
          </div>
          <p class="note">
            {starttls
              ? "Para provedores sem TLS direto. Se o servidor não aceitar STARTTLS, o Papeo não conecta: a senha nunca vai sem criptografia."
              : "O comum. Use os dados de IMAP do seu provedor."}
          </p>
        </fieldset>
      {/if}
    </section>
    {/if}

    {#if gmail}
      <section aria-label="Contador">
        <div class="toggle">
          <div>
            <span class="toggle-label">Contar a Caixa de Entrada inteira</span>
            <p class="note">
              {wholeInbox
                ? "Conta e avisa de tudo o que chega à Caixa de Entrada, inclusive Promoções e Social."
                : "Desligado, conta e avisa só da aba Principal, como o site. Ligue se você não usa as abas do Gmail."}
            </p>
          </div>
          <Switch
            checked={wholeInbox}
            label="Contar a Caixa de Entrada inteira"
            onchange={(v) => (wholeInbox = v)}
          />
        </div>
      </section>
    {/if}

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
      {microsoft ? "O acesso" : "A senha"} fica só neste computador, fora do backup e da sincronização. O Papeo lê só o
      remetente e o assunto dos e-mails novos e não marca nada como lido.
    </p>

    <div class="footer">
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <div class="actions">
        {#if configured}
          <button type="button" class="btn danger-text" disabled={busy} onclick={turnOff}>Desligar</button>
        {/if}
        <span class="spacer"></span>
        <button type="button" class="btn" onclick={onclose}>{saved && !dirty ? "Fechar" : "Cancelar"}</button>
        {#if !microsoft || configured}
          <button type="submit" class="btn btn-primary" disabled={!canSave}>
            {busy && !signingIn ? "Salvando…" : configured ? "Salvar" : "Ligar"}
          </button>
        {/if}
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
    .status :global(.spin),
    .signin :global(.spin) {
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

  .signin {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    font-size: 13px;
  }

  .signin p {
    margin: 0;
  }

  .signin .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .signin :global(.spin) {
    animation: spin 1s linear infinite;
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

  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  legend {
    padding: 0;
    margin-bottom: 6px;
    font-size: 13px;
    font-weight: 500;
  }

  .segmented {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    padding: 3px;
    gap: 2px;
    border-radius: 10px;
    background: var(--bg-subtle);
    border: 1px solid var(--border);
  }

  .segmented label {
    padding: 7px 10px;
    border-radius: 8px;
    font-size: 13px;
    text-align: center;
    color: var(--text-muted);
    cursor: pointer;
  }

  .segmented label.selected {
    background: var(--bg);
    color: var(--text);
    box-shadow: var(--shadow-card), 0 0 0 1px var(--border);
    font-weight: 550;
  }

  .segmented label:has(:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
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
