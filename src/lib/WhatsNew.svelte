<script lang="ts">
  import { Sparkles, TrendingUp, Wrench, ListChecks } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import { parseNotes, type NoteKind } from "./notes";
  import { ABOUT, api } from "./api";

  // "O que mudou": antes de atualizar (com o botão de atualizar) ou logo
  // depois (versão nova já instalada).
  let {
    version,
    notes,
    mode,
    onclose,
    onupdate,
  }: {
    version: string;
    notes: string | null | undefined;
    mode: "available" | "installed";
    onclose: () => void;
    onupdate?: () => void;
  } = $props();

  const sections = $derived(parseNotes(notes));
  const META: Record<NoteKind, { icon: typeof Sparkles; label: string }> = {
    new: { icon: Sparkles, label: "Novidades" },
    improved: { icon: TrendingUp, label: "Melhorias" },
    fixed: { icon: Wrench, label: "Correções" },
    other: { icon: ListChecks, label: "" },
  };
</script>

<Modal {onclose} label="O que mudou na versão {version}" width={520}>
  <div class="head">
    <img src="/favicon.svg" alt="" width="40" height="40" />
    <div>
      <p class="eyebrow">{mode === "installed" ? "Atualizado!" : "Nova versão disponível"}</p>
      <h2>O que mudou no Papeo {version}</h2>
    </div>
  </div>

  <div class="sections">
    {#each sections as section, i (i)}
      {@const meta = META[section.kind]}
      <section class="section {section.kind}">
        <h3><meta.icon size={16} strokeWidth={2.2} /> {section.title || meta.label}</h3>
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
      <p class="muted">Esta versão traz ajustes e correções internas.</p>
    {/each}
  </div>

  <div class="actions">
    <button class="link" onclick={() => api.openUrl(`${ABOUT.repo}/releases/tag/v${version}`).catch(() => {})}>
      Ver no GitHub
    </button>
    {#if mode === "available"}
      <button class="btn" onclick={onclose}>Depois</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn btn-primary" onclick={onupdate} autofocus>Atualizar e reiniciar</button>
    {:else}
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn btn-primary" onclick={onclose} autofocus>Entendi</button>
    {/if}
  </div>
</Modal>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 18px;
  }

  .eyebrow {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--success-text);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  h2 {
    margin: 2px 0 0;
    font-size: 19px;
  }

  .sections {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-height: min(52vh, 460px);
    overflow-y: auto;
    user-select: text;
    -webkit-user-select: text;
    cursor: auto;
  }

  .section {
    --tone: var(--text-muted);
    --tone-bg: var(--bg-subtle);
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--tone-bg);
  }

  .section.new {
    --tone: #16a34a;
    --tone-bg: color-mix(in srgb, #16a34a 9%, var(--bg));
  }

  .section.improved {
    --tone: #2563eb;
    --tone-bg: color-mix(in srgb, #2563eb 8%, var(--bg));
  }

  .section.fixed {
    --tone: #d97706;
    --tone-bg: color-mix(in srgb, #d97706 9%, var(--bg));
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

  h3 {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 0 0 6px;
    font-size: 13.5px;
    color: var(--tone);
  }

  ul {
    margin: 0;
    padding-left: 18px;
  }

  li {
    margin: 4px 0;
    line-height: 1.5;
  }

  li::marker {
    color: var(--tone);
  }

  code {
    padding: 1px 5px;
    border-radius: 5px;
    background: var(--bg);
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
    margin-top: 18px;
  }

  .link {
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
</style>
