<script lang="ts">
  import { Copy, PenLine } from "@lucide/svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import { t } from "./i18n.svelte";
  import type { MenuItem } from "./api";

  // Remetentes e destinatários de um e-mail (spec 24): cada endereço abre um
  // menu (escrever, copiar); com muitos, os primeiros e "e mais N".
  import type { MailAddr as Addr } from "./api";
  let {
    label = "",
    list,
    full = false,
    oncompose,
    oncopied,
  }: {
    /** "Para:", "Cc:"… (já traduzido); vazio no remetente. */
    label?: string;
    list: Addr[];
    /** Sempre "nome <endereço>" (o remetente). */
    full?: boolean;
    oncompose: (a: Addr) => void;
    oncopied: (email: string) => void;
  } = $props();

  const SHOWN = 3;
  let open = $state(false);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const visible = $derived(open ? list : list.slice(0, SHOWN));
  const hidden = $derived(list.length - visible.length);

  function choose(e: MouseEvent, a: Addr) {
    const target = e.currentTarget as HTMLElement;
    const r = target.getBoundingClientRect();
    menu = {
      x: r.left,
      y: r.bottom + 4,
      items: [
        { label: t("Escrever para {nome}", { nome: a.name || a.email }), icon: PenLine, run: () => oncompose(a) },
        {
          label: t("Copiar endereço"),
          icon: Copy,
          run: () => {
            void navigator.clipboard.writeText(a.email).then(() => oncopied(a.email));
          },
        },
      ],
    };
  }
</script>

<div class="addresses" class:full>
  {#if label}<span class="label">{label}</span>{/if}
  <ul>
    {#each visible as a, i (i)}
      <li>
        <button
          type="button"
          class="address"
          title={a.email}
          onclick={(e) => choose(e, a)}
        >
          {#if (open || full) && a.name && a.email}<span class="name">{a.name}</span> <span class="email">&lt;{a.email}&gt;</span>{:else}<span class="name">{a.name || a.email}</span>{/if}
        </button>
      </li>
    {/each}
    {#if hidden > 0}
      <li><button type="button" class="more" onclick={() => (open = true)}>{t("e mais {n}", { n: hidden })}</button></li>
    {:else if open && list.length > SHOWN}
      <li><button type="button" class="more" onclick={() => (open = false)}>{t("mostrar menos")}</button></li>
    {/if}
  </ul>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} maxX={window.innerWidth} onclose={() => (menu = null)} />
{/if}

<style>
  .addresses {
    display: flex;
    gap: 6px;
    margin-top: 3px;
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .label {
    flex-shrink: 0;
  }
  .full {
    margin-top: 0;
    font-size: inherit;
  }
  .full .name {
    font-weight: 600;
  }
  ul {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 4px;
    margin: 0;
    padding: 0;
    list-style: none;
    min-width: 0;
  }
  li:not(:last-child) .address::after {
    content: ",";
  }
  button {
    border: 0;
    background: none;
    padding: 0 2px;
    font: inherit;
    color: var(--text);
    cursor: pointer;
    border-radius: 4px;
    overflow-wrap: anywhere;
    text-align: left;
  }
  button:hover {
    background: var(--bg-hover);
  }
  .email {
    color: var(--text-muted);
  }
  .more {
    color: var(--accent, var(--text));
    text-decoration: underline;
  }
</style>
