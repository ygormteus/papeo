<script lang="ts">
  import { ExternalLink, ListChecks, Sparkles, TrendingUp, Wrench, X } from "@lucide/svelte";
  import { t, tk } from "./i18n.svelte";
  import Modal from "./Modal.svelte";
  import { parseNotes, type NoteKind } from "./notes";
  import { notesHistory } from "./bundledNotes";
  import { ABOUT, api } from "./api";

  // "O que mudou": a versão disponível (com o botão de atualizar) ou a que
  // acabou de ser instalada, e o histórico das anteriores embutido no app.
  let {
    version,
    notes,
    mode,
    current,
    onclose,
    onupdate,
  }: {
    version: string;
    notes: string | null | undefined;
    mode: "available" | "installed";
    /** Versão instalada: o histórico vai até ela. */
    current: string;
    onclose: () => void;
    onupdate?: () => void;
  } = $props();

  type Entry = { version: string; notes: string | null | undefined; badge?: string };

  const entries = $derived.by<Entry[]>(() => {
    const out: Entry[] = [{ version, notes, badge: mode === "available" ? tk("Nova") : tk("Atual") }];
    for (const n of notesHistory(current)) {
      if (n.version === version) continue;
      out.push({ version: n.version, notes: n.notes, badge: n.version === current ? tk("Atual") : undefined });
    }
    return out;
  });

  // svelte-ignore state_referenced_locally
  let selected = $state(version);
  const entry = $derived(entries.find((e) => e.version === selected) ?? entries[0]);
  const sections = $derived(parseNotes(entry.notes));
  const count = (e: Entry) => parseNotes(e.notes).reduce((n, s) => n + s.items.length, 0);

  const META: Record<NoteKind, { icon: typeof Sparkles; label: string }> = {
    new: { icon: Sparkles, label: tk("Novidades") },
    improved: { icon: TrendingUp, label: tk("Melhorias") },
    fixed: { icon: Wrench, label: tk("Correções") },
    other: { icon: ListChecks, label: tk("Outras mudanças") },
  };

  let notesEl = $state<HTMLElement>();

  function pick(v: string) {
    selected = v;
    if (notesEl) notesEl.scrollTop = 0;
  }

  /** Setas, Home e End entre as versões (abas verticais). */
  function versionKey(e: KeyboardEvent) {
    const i = entries.findIndex((x) => x.version === entry.version);
    const n = entries.length;
    const next =
      e.key === "ArrowDown" || e.key === "ArrowRight"
        ? (i + 1) % n
        : e.key === "ArrowUp" || e.key === "ArrowLeft"
          ? (i - 1 + n) % n
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? n - 1
              : -1;
    if (next === -1) return;
    e.preventDefault();
    pick(entries[next].version);
    document.getElementById(`ver-${entries[next].version}`)?.focus();
  }
</script>

<Modal {onclose} label={t("O que mudou no Papeo")} width={entries.length > 1 ? 760 : 560} flush>
  <header class="hero">
    <img src="/favicon.svg" alt="" width="44" height="44" />
    <div class="hero-text">
      <span class="eyebrow" class:fresh={mode === "available"}>
        {mode === "available" ? t("Nova versão disponível") : t("Atualizado")}
      </span>
      <h2>{t("O que mudou no Papeo")}</h2>
      <p>
        {#if mode === "available"}
          {t("A versão {versao} está pronta para instalar. Você está na {atual}.", { versao: version, atual: current })}
        {:else}
          {t("Você está na versão {atual}.", { atual: current })}
        {/if}
      </p>
    </div>
    <button class="close" onclick={onclose} aria-label={t("Fechar")}><X size={18} /></button>
  </header>

  <div class="layout" class:single={entries.length === 1}>
    {#if entries.length > 1}
      <div class="versions" role="tablist" aria-label={t("Versões")} aria-orientation="vertical">
        {#each entries as e (e.version)}
          <button
            id="ver-{e.version}"
            class="version"
            role="tab"
            aria-selected={e.version === entry.version}
            aria-controls={e.version === entry.version ? "ver-panel" : undefined}
            tabindex={e.version === entry.version ? 0 : -1}
            onclick={() => pick(e.version)}
            onkeydown={versionKey}
          >
            <span class="v">{e.version}</span>
            {#if e.badge}
              <span class="badge" class:fresh={e.badge === "Nova"}>{t(e.badge)}</span>
            {:else}
              <span class="n" aria-label={count(e) === 1 ? t("{n} mudança", { n: count(e) }) : t("{n} mudanças", { n: count(e) })}>{count(e)}</span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}

    <!-- Área rolável: precisa de foco para rolar pelo teclado. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="notes"
      id="ver-panel"
      role={entries.length > 1 ? "tabpanel" : "region"}
      aria-labelledby={entries.length > 1 ? `ver-${entry.version}` : undefined}
      aria-label={entries.length > 1 ? undefined : t("Versão {versao}", { versao: entry.version })}
      tabindex="0"
      bind:this={notesEl}
    >
      <h3 class="title">
        {t("Versão {versao}", { versao: entry.version })}
        {#if entry.badge}<span class="badge" class:fresh={entry.badge === "Nova"}>{t(entry.badge)}</span>{/if}
      </h3>
      {#each sections as section, i (i)}
        {@const meta = META[section.kind]}
        <section class="section {section.kind}">
          <h4>
            <span class="icon"><meta.icon size={14} strokeWidth={2.4} /></span>
            {section.title || t(meta.label)}
            <span class="count">{section.items.length}</span>
          </h4>
          <ul>
            {#each section.items as item, j (j)}
              <li>
                {#each item as span, k (k)}{#if span.bold}<strong>{span.text}</strong>{:else if span.code}<code
                      >{span.text}</code
                    >{:else}{span.text}{/if}{/each}
              </li>
            {/each}
          </ul>
        </section>
      {:else}
        <p class="muted">{t("Esta versão traz ajustes e correções internas.")}</p>
      {/each}
    </div>
  </div>

  <footer class="actions">
    <button
      class="link"
      onclick={() => api.openUrl(`${ABOUT.repo}/releases/tag/v${entry.version}`).catch(() => {})}
    >
      <ExternalLink size={14} /> {t("Ver {versao} no GitHub", { versao: entry.version })}
    </button>
    {#if mode === "available"}
      <button class="btn" onclick={onclose}>{t("Depois")}</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn btn-primary" onclick={onupdate} autofocus>{t("Atualizar e reiniciar")}</button>
    {:else}
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn btn-primary" onclick={onclose} autofocus>{t("Entendi")}</button>
    {/if}
  </footer>
</Modal>

<style>
  .hero {
    position: relative;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 24px 24px 20px;
    border-bottom: 1px solid var(--border);
  }

  /* Sóbrio: cor só nos ícones das seções e no aviso de versão nova. */
  .hero img {
    flex: none;
    border-radius: 10px;
  }

  .hero-text {
    min-width: 0;
  }

  .eyebrow {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-muted);
  }

  .eyebrow.fresh {
    color: var(--success-text);
  }

  h2 {
    margin: 2px 0 2px;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .hero p {
    margin: 0;
    color: var(--text-muted);
    font-size: 13px;
  }

  .close {
    position: absolute;
    top: 14px;
    right: 14px;
    display: grid;
    place-items: center;
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

  .layout {
    display: grid;
    grid-template-columns: 150px 1fr;
    height: min(56vh, 480px);
  }

  .layout.single {
    grid-template-columns: 1fr;
  }

  .versions {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 8px;
    border-right: 1px solid var(--border);
    background: var(--bg-subtle);
    overflow-y: auto;
  }

  .version {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    text-align: left;
  }

  .version:hover {
    background: var(--bg-hover);
    color: var(--text);
  }

  .version[aria-selected="true"] {
    background: var(--bg);
    color: var(--text);
    font-weight: 600;
    box-shadow: 0 0 0 1px var(--border);
  }

  .version:focus-visible {
    outline-offset: -2px;
  }

  .badge {
    font-size: 11px;
    font-weight: 500;
    color: var(--text-faint);
  }

  .badge.fresh {
    color: var(--success-text);
  }

  .n {
    font-size: 11.5px;
    color: var(--text-faint);
  }

  .notes {
    padding: 18px 24px 24px;
    overflow-y: auto;
    user-select: text;
    -webkit-user-select: text;
    cursor: auto;
  }

  .notes:focus-visible {
    outline-offset: -2px;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 14px;
    font-size: 15px;
    font-weight: 600;
  }

  .section {
    --tone: var(--text-muted);
  }

  .section + .section {
    margin-top: 18px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  .section.new {
    --tone: #16a34a;
  }

  .section.improved {
    --tone: #2563eb;
  }

  .section.fixed {
    --tone: #d97706;
  }

  :global(:root[data-theme="dark"]) .section.new,
  :global(:root[data-theme="oled"]) .section.new {
    --tone: #4ade80;
  }

  :global(:root[data-theme="dark"]) .section.improved,
  :global(:root[data-theme="oled"]) .section.improved {
    --tone: #60a5fa;
  }

  :global(:root[data-theme="dark"]) .section.fixed,
  :global(:root[data-theme="oled"]) .section.fixed {
    --tone: #fbbf24;
  }

  h4 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 8px;
    font-size: 13px;
    font-weight: 600;
  }

  .icon {
    display: inline-flex;
    color: var(--tone);
  }

  .count {
    font-size: 11.5px;
    font-weight: 500;
    color: var(--text-faint);
  }

  ul {
    margin: 0;
    padding: 0 0 0 22px;
  }

  li {
    margin: 6px 0;
    line-height: 1.55;
  }

  li::marker {
    color: var(--text-faint);
  }

  code {
    padding: 1px 5px;
    border-radius: 5px;
    background: var(--bg-subtle);
    border: 1px solid var(--border);
    font-size: 12px;
  }

  .muted {
    color: var(--text-muted);
  }

  .actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 14px 20px;
    border-top: 1px solid var(--border);
  }

  .link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-right: auto;
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font-size: 13px;
  }

  .link:hover {
    text-decoration: underline;
  }

  /* Janela estreita: as versões viram uma faixa horizontal acima das notas. */
  @media (max-width: 680px) {
    .layout {
      grid-template-columns: 1fr;
      grid-template-rows: auto 1fr;
    }

    .versions {
      flex-direction: row;
      overflow-x: auto;
      border-right: 0;
      border-bottom: 1px solid var(--border);
      padding: 8px;
    }

    .version {
      flex: none;
    }

    .link {
      display: none;
    }
  }
</style>
