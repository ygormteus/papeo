<script lang="ts">
  import { MessageSquareText, Plus, Trash2 } from "@lucide/svelte";
  import { t } from "./i18n.svelte";
  import { api, MAX_REPLY_CHARS, type QuickReply, type Snapshot } from "./api";

  // Respostas prontas (Configurações): atalho + texto, chamadas com "/" na
  // resposta rápida do Início e da barra de menus.
  let {
    replies,
    run,
  }: {
    replies: QuickReply[];
    run: (action: Promise<Snapshot | void>) => Promise<unknown>;
  } = $props();

  let shortcut = $state("");
  let text = $state("");
  // Cópia editável; volta a seguir o app quando ele publica outra lista.
  let items = $state<QuickReply[]>([]);
  $effect(() => {
    items = replies.map((r) => ({ ...r }));
  });

  const save = (list: QuickReply[]) => run(api.setQuickReplies(list));

  function changed(i: number) {
    const a = items[i];
    const b = replies[i];
    if (!b || a.shortcut !== b.shortcut || a.text !== b.text) save(items);
  }

  function remove(i: number) {
    save(items.filter((_, j) => j !== i));
  }

  async function add(e: Event) {
    e.preventDefault();
    if (!text.trim()) return;
    await save([...items, { shortcut, text }]);
    shortcut = "";
    text = "";
  }
</script>

<section class="card" aria-labelledby="s-replies">
  <h2 id="s-replies"><MessageSquareText size={18} /> {t("Respostas prontas")}</h2>
  <p class="hint intro">
    {t("Ao responder pelo Início ou pela barra de menus, digite")} <kbd>/</kbd> {t("e o atalho (ou parte do texto) para escolher uma delas.")}
  </p>

  {#each items as item, i (i)}
    <div class="item">
      <label class="shortcut">
        <span class="slash" aria-hidden="true">/</span>
        <input
          bind:value={item.shortcut}
          onchange={() => changed(i)}
          maxlength="40"
          placeholder={t("atalho")}
          aria-label={t("Atalho da resposta {n}", { n: i + 1 })}
        />
      </label>
      <textarea
        bind:value={item.text}
        onchange={() => changed(i)}
        rows="2"
        maxlength={MAX_REPLY_CHARS}
        aria-label={t("Texto da resposta {n}", { n: i + 1 })}
      ></textarea>
      <button class="icon-btn ghost" onclick={() => remove(i)} title={t("Apagar")} aria-label={t("Apagar a resposta {n}", { n: i + 1 })}>
        <Trash2 size={16} />
      </button>
    </div>
  {/each}

  <form class="item new" onsubmit={add}>
    <label class="shortcut">
      <span class="slash" aria-hidden="true">/</span>
      <input bind:value={shortcut} maxlength="40" placeholder={t("orcamento")} aria-label={t("Atalho da nova resposta")} />
    </label>
    <textarea
      bind:value={text}
      rows="2"
      maxlength={MAX_REPLY_CHARS}
      placeholder={t("Segue o orçamento, qualquer dúvida é só chamar.")}
      aria-label={t("Texto da nova resposta")}
    ></textarea>
    <button class="btn" type="submit" disabled={!text.trim()}><Plus size={16} /> {t("Adicionar")}</button>
  </form>
</section>

<style>
  .card {
    padding: 18px 22px 14px;
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

  .hint {
    font-size: 13px;
    color: var(--text-muted);
  }

  .intro {
    margin: 0 0 10px;
  }

  kbd {
    padding: 0 5px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg-subtle);
    font: inherit;
  }

  .item {
    display: grid;
    grid-template-columns: minmax(120px, 180px) 1fr auto;
    align-items: start;
    gap: 10px;
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }

  .shortcut {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 36px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
  }

  .slash {
    color: var(--text-faint);
  }

  .shortcut input {
    width: 100%;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    outline: none;
  }

  .shortcut:focus-within,
  textarea:focus {
    border-color: var(--accent);
  }

  textarea {
    min-height: 36px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    resize: vertical;
    outline: none;
  }

  .new .btn {
    height: 36px;
  }

  @media (max-width: 640px) {
    .item {
      grid-template-columns: 1fr auto;
    }

    .item textarea {
      grid-column: 1 / -1;
      grid-row: 2;
    }
  }
</style>
