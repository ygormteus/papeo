<script lang="ts">
  import { api, type MailAddr } from "./api";
  import { t } from "./i18n.svelte";

  // Campo de endereços (Para, Cc, Cco) que completa com os endereços que a
  // conta já usou (spec 25): o pedaço depois da última vírgula é a busca.
  let {
    value = $bindable(""),
    field = $bindable(),
    accountId,
    label,
    placeholder = "",
  }: {
    value: string;
    field?: HTMLInputElement;
    accountId: string;
    /** Nome acessível do campo. */
    label: string;
    placeholder?: string;
  } = $props();

  const uid = $props.id();
  let found = $state<MailAddr[]>([]);
  let active = $state(0);
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  /** Pedaço sendo digitado (depois da última vírgula ou ponto e vírgula). */
  function current(text: string) {
    const cut = Math.max(text.lastIndexOf(","), text.lastIndexOf(";"));
    return { head: text.slice(0, cut + 1), query: text.slice(cut + 1).trim() };
  }

  function lookup() {
    clearTimeout(timer);
    const { query } = current(value);
    if (query.length < 1 || query.includes("<")) {
      found = [];
      return;
    }
    const mine = ++seq;
    timer = setTimeout(async () => {
      try {
        const list = await api.mailContacts(accountId, query);
        if (mine !== seq) return;
        // O que já está no campo não aparece de novo.
        const typed = value.toLowerCase();
        found = list.filter((a) => !typed.includes(`<${a.email.toLowerCase()}>`) && !typed.split(/[,;]/).some((p) => p.trim() === a.email.toLowerCase()));
        active = 0;
      } catch {
        found = [];
      }
    }, 120);
  }

  function format(a: MailAddr) {
    if (!a.name) return a.email;
    const name = /[",;<>@]/.test(a.name) ? `"${a.name.replace(/["\\]/g, "")}"` : a.name;
    return `${name} <${a.email}>`;
  }

  function pick(a: MailAddr) {
    const { head } = current(value);
    value = `${head}${head ? " " : ""}${format(a)}, `;
    found = [];
    seq++;
    field?.focus();
  }

  function keydown(e: KeyboardEvent) {
    if (!found.length) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      active = (active + step + found.length) % found.length;
    } else if (e.key === "Enter" || (e.key === "Tab" && !e.shiftKey)) {
      e.preventDefault();
      e.stopPropagation();
      pick(found[active]);
    } else if (e.key === "Escape") {
      // Só fecha a lista (o Esc do editor fecharia o e-mail).
      e.preventDefault();
      e.stopPropagation();
      found = [];
      seq++;
    }
  }
</script>

<span class="box">
  <input
    bind:this={field}
    bind:value
    oninput={lookup}
    onkeydown={keydown}
    onblur={() => setTimeout(() => ((found = []), seq++), 150)}
    autocomplete="off"
    spellcheck="false"
    {placeholder}
    aria-label={label}
    role="combobox"
    aria-expanded={found.length > 0}
    aria-controls="{uid}-list"
    aria-autocomplete="list"
    aria-activedescendant={found.length ? `${uid}-${active}` : undefined}
  />
  {#if found.length}
    <ul class="suggestions" id="{uid}-list" role="listbox" aria-label={t("Endereços conhecidos")}>
      {#each found as a, i (a.email)}
        <li id="{uid}-{i}" role="option" aria-selected={i === active}>
          <button
            type="button"
            tabindex="-1"
            class:active={i === active}
            onmousedown={(e) => e.preventDefault()}
            onclick={() => pick(a)}
            onmouseenter={() => (active = i)}
          >
            {#if a.name}<span class="name">{a.name}</span>{/if}
            <span class="email">{a.email}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</span>

<style>
  .box {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    padding: 9px 0;
    font: inherit;
    outline: none;
  }
  .suggestions {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    z-index: 20;
    margin: 0;
    padding: 4px;
    list-style: none;
    max-height: 260px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.16);
  }
  .suggestions button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }
  .suggestions button.active {
    background: var(--bg-hover);
  }
  .name {
    font-weight: 600;
  }
  .email {
    color: var(--text-muted);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
</style>
