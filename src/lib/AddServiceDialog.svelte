<script lang="ts">
  import { Check, CirclePlus, X } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import ServiceAvatar from "./ServiceAvatar.svelte";
  import { t } from "./i18n.svelte";
  import {
    KIND_META,
    MAIL_PRESETS,
    OUTLOOK_WORK_URL,
    TEAMS_PERSONAL_URL,
    errorMessage,
    needsUrl,
    presetForAddress,
    webmailFor,
    type ServiceKind,
  } from "./api";

  let {
    onclose,
    onsubmit,
  }: {
    onclose: () => void;
    /** `mail`: e-mail por IMAP a configurar em seguida (servidor e usuário). */
    onsubmit: (
      kind: ServiceKind,
      name: string,
      url: string,
      mail?: { host: string; user: string },
    ) => Promise<void>;
  } = $props();

  // Outro e-mail: endereço e provedor (o IMAP vem a seguir); o webmail só
  // serve para "Abrir no site".
  let address = $state("");
  let presetId = $state("");
  let presetTouched = $state(false);
  const preset = $derived(MAIL_PRESETS.find((p) => p.id === presetId));
  const addressOk = $derived(/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(address.trim()));

  function guessPreset() {
    if (presetTouched) return;
    presetId = presetForAddress(address)?.id ?? "";
  }

  const kinds: ServiceKind[] = ["whatsapp", "telegram", "discord", "slack", "teams", "gmail", "outlook", "email", "custom"];
  let kind = $state<ServiceKind>("whatsapp");
  let name = $state("");
  let url = $state("");
  /** Outlook de trabalho ou escola (Microsoft 365) em vez do pessoal. */
  let outlookWork = $state(false);
  let teamsPersonal = $state(false);
  let error = $state("");
  let busy = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (kind === "email") {
      if (!addressOk) {
        error = t("Informe o endereço de e-mail.");
        return;
      }
      const site = url.trim() || webmailFor(address, preset);
      if (!site) {
        error = t("Informe o endereço do webmail.");
        return;
      }
      busy = true;
      error = "";
      try {
        await onsubmit(kind, name.trim(), site, {
          host: preset?.host ?? "",
          user: address.trim(),
        });
      } catch (err) {
        error = errorMessage(err);
        busy = false;
      }
      return;
    }
    if (needsUrl(kind) && !url.trim()) {
      error = t("Informe o endereço do site.");
      return;
    }
    busy = true;
    error = "";
    try {
      const site = needsUrl(kind)
        ? url.trim()
        : kind === "outlook" && outlookWork
          ? OUTLOOK_WORK_URL
          : kind === "teams" && teamsPersonal
            ? TEAMS_PERSONAL_URL
            : "";
      await onsubmit(kind, name.trim(), site);
    } catch (err) {
      error = errorMessage(err);
      busy = false;
    }
  }
</script>

<Modal {onclose} label={t("Adicionar serviço")} width={600} flush>
  <form onsubmit={submit}>
    <header class="head">
      <span class="head-icon" aria-hidden="true"><CirclePlus size={20} /></span>
      <div class="head-text">
        <h2>{t("Adicionar serviço")}</h2>
        <p>{t("Cada serviço tem sessão própria: dá para ter duas contas do mesmo mensageiro.")}</p>
      </div>
      <button type="button" class="close" onclick={onclose} aria-label={t("Fechar")}><X size={18} /></button>
    </header>

    <div class="content">
    <fieldset class="kinds">
      <legend class="sr-only">{t("Tipo de serviço")}</legend>
      {#each kinds as k (k)}
        <label class="kind" class:selected={kind === k}>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            type="radio"
            name="kind"
            value={k}
            bind:group={kind}
            class="sr-only"
            autofocus={k === "whatsapp"}
          />
          <ServiceAvatar service={{ kind: k }} size={30} />
          {#if kind === k}<span class="check" aria-hidden="true"><Check size={12} strokeWidth={3} /></span>{/if}
          <span class="kind-label">{t(KIND_META[k].label)}</span>
          <span class="kind-hint">{t(KIND_META[k].hint)}</span>
        </label>
      {/each}
    </fieldset>

    {#if kind === "email"}
      <div class="field">
        <label for="svc-address">{t("Seu e-mail")}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="svc-address"
          class="input"
          type="email"
          bind:value={address}
          oninput={guessPreset}
          placeholder={t("voce@seudominio.com.br")}
          autocomplete="email"
          autofocus
        />
      </div>
      <div class="field">
        <label for="svc-preset">{t("Provedor")}</label>
        <select
          id="svc-preset"
          class="input"
          bind:value={presetId}
          onchange={() => (presetTouched = true)}
        >
          <option value="">{t("Outro (informo o servidor IMAP)")}</option>
          {#each MAIL_PRESETS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
        </select>
        <p class="note">
          {preset?.note ? t(preset.note) : t("O Papeo lê e envia pelo IMAP, sem abrir o site. A senha vem no próximo passo.")}
        </p>
      </div>
      <div class="field">
        <label for="svc-url">{t("Webmail")} <span class="muted">{t("(opcional, para abrir no site)")}</span></label>
        <input
          id="svc-url"
          class="input"
          bind:value={url}
          placeholder={webmailFor(address, preset).replace(/^https:\/\//, "").replace(/\/$/, "") ||
            "webmail.seudominio.com.br"}
        />
      </div>
    {:else if needsUrl(kind)}
      <div class="field">
        <label for="svc-url">{t("Endereço")}</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="svc-url"
          class="input"
          bind:value={url}
          placeholder={t("app.exemplo.com")}
          autofocus
        />
        {#if url.trim().toLowerCase().startsWith("http://")}
          <p class="warn" role="note">
            {t("Endereço sem criptografia (http): mensagens e senhas podem ser lidas por quem estiver na mesma rede. Prefira https.")}
          </p>
        {/if}
      </div>
    {:else if kind === "outlook"}
      <fieldset class="account">
        <legend>{t("Tipo de conta")}</legend>
        <label class="choice">
          <input type="radio" name="outlook-account" value={false} bind:group={outlookWork} />
          <span>{t("Pessoal")} <span class="muted">{t("(Outlook.com, Hotmail, Live)")}</span></span>
        </label>
        <label class="choice">
          <input type="radio" name="outlook-account" value={true} bind:group={outlookWork} />
          <span>{t("Trabalho ou escola")} <span class="muted">{t("(Microsoft 365)")}</span></span>
        </label>
      </fieldset>
      <p class="note">
        {t("Para receber avisos de e-mails novos, ligue no Outlook: Configurações → Geral → Notificações → Notificações da área de trabalho.")}
      </p>
    {:else if kind === "teams"}
      <fieldset class="account">
        <legend>{t("Tipo de conta")}</legend>
        <label class="choice">
          <input type="radio" name="teams-account" value={false} bind:group={teamsPersonal} />
          <span>{t("Trabalho ou escola")} <span class="muted">{t("(Microsoft 365)")}</span></span>
        </label>
        <label class="choice">
          <input type="radio" name="teams-account" value={true} bind:group={teamsPersonal} />
          <span>{t("Pessoal")} <span class="muted">{t("(Teams gratuito)")}</span></span>
        </label>
      </fieldset>
    {:else if kind === "gmail"}
      <p class="note">
        {t("Para receber avisos de e-mails novos, ligue no Gmail: Configurações → Ver todas as configurações → Notificações da área de trabalho.")}
      </p>
    {/if}

    <div class="field">
      <label for="svc-name">{t("Nome")} <span class="muted">{t("(opcional)")}</span></label>
      <input
        id="svc-name"
        class="input"
        bind:value={name}
        placeholder={kind === "custom"
          ? t("Ex.: Slack da empresa")
          : kind === "email"
            ? t("Ex.: E-mail da empresa")
            : t("Ex.: {tipo} trabalho", { tipo: t(KIND_META[kind].label) })}
      />
    </div>

    {#if error}<p class="error" role="alert">{error}</p>{/if}
    </div>

    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>{t("Cancelar")}</button>
      <button type="submit" class="btn btn-primary" disabled={busy}>{busy ? t("Adicionando…") : t("Adicionar")}</button>
    </div>
  </form>
</Modal>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 20px 20px 18px 24px;
    border-bottom: 1px solid var(--border);
  }

  .head-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 11px;
    border: 1px solid var(--border);
    background: var(--bg-subtle);
    color: var(--text-muted);
  }

  .head-text {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .head-text p {
    margin: 2px 0 0;
    font-size: 13px;
    color: var(--text-muted);
  }

  .close {
    display: grid;
    place-items: center;
    flex: none;
    align-self: flex-start;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
  }

  .close:hover {
    background: var(--bg-hover);
    color: var(--text);
  }

  .content {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px 24px;
    max-height: min(62vh, 560px);
    overflow-y: auto;
  }

  .muted {
    color: var(--text-muted);
    font-weight: 400;
  }

  .kinds {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin: 0;
    padding: 0;
    border: 0;
  }

  @media (max-width: 560px) {
    .kinds {
      grid-template-columns: repeat(2, 1fr);
    }
  }

  .kind {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    padding: 14px 12px 12px;
    border: 1px solid var(--border);
    border-radius: 12px;
    cursor: pointer;
    transition:
      border-color 0.12s,
      background 0.12s;
  }

  .kind:hover {
    background: var(--bg-hover);
  }

  .kind.selected {
    border-color: var(--text);
    background: var(--bg-subtle);
  }

  .kind:has(:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .kind :global(.avatar) {
    margin-bottom: 8px;
  }

  .check {
    position: absolute;
    top: 10px;
    right: 10px;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--text);
    color: var(--bg);
  }

  .kind-label {
    font-weight: 550;
  }

  .kind-hint {
    font-size: 11.5px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .error {
    margin: 0;
    color: var(--danger);
  }

  .note {
    margin: 6px 0 0;
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .kinds + .note {
    margin: 0;
  }

  .account {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .account legend {
    width: 100%;
    margin-bottom: 6px;
    font-size: 13px;
    font-weight: 600;
  }

  .choice {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
    cursor: pointer;
  }

  .account + .note {
    margin: 0;
  }

  .warn {
    margin: 6px 0 0;
    padding: 8px 10px;
    border-radius: 8px;
    font-size: 13px;
    color: var(--warn-text);
    background: var(--warn-bg);
    border: 1px solid var(--warn-border);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 14px 20px;
    border-top: 1px solid var(--border);
  }
</style>
