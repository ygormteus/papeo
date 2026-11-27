<script lang="ts">
  import { t } from "./i18n.svelte";
  import type { MenuItem } from "./api";

  let {
    x,
    y,
    items,
    maxX,
    onclose,
  }: { x: number; y: number; items: MenuItem[]; maxX: number; onclose: () => void } = $props();

  // Largura mínima; rótulos longos (um endereço de e-mail) alargam o menu até
  // MAX_WIDTH e, além disso, terminam em reticências com o texto no title.
  const WIDTH = 210;
  const MAX_WIDTH = 320;
  let menu: HTMLDivElement;
  let width = $state(WIDTH);
  let left = $derived(Math.max(8, Math.min(x, maxX - width - 8)));
  let top = $derived(Math.min(y, window.innerHeight - items.length * 36 - 24));

  $effect(() => {
    menu.querySelector<HTMLButtonElement>("button")?.focus();
    const away = (e: PointerEvent) => {
      if (!menu.contains(e.target as Node)) onclose();
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("blur", onclose);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("blur", onclose);
    };
  });

  function keydown(e: KeyboardEvent) {
    const buttons = [...menu.querySelectorAll<HTMLButtonElement>("button")];
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = (i + (e.key === "ArrowDown" ? 1 : buttons.length - 1)) % buttons.length;
      buttons[next]?.focus();
    }
  }
</script>

<div
  class="menu"
  role="menu"
  tabindex="-1"
  bind:this={menu}
  style:left="{left}px"
  style:top="{top}px"
  style:min-width="{WIDTH}px"
  style:max-width="{Math.min(MAX_WIDTH, maxX - 16)}px"
  bind:offsetWidth={width}
  onkeydown={keydown}
>
  {#each items as item (item.label)}
    <button
      role="menuitem"
      title={t(item.label)}
      class:danger={item.danger}
      onclick={() => {
        onclose();
        item.run();
      }}
    >
      <item.icon size={16} />
      <span class="label">{t(item.label)}</span>
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 50;
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--bg);
    box-shadow: var(--shadow-pop);
  }

  button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    text-align: left;
  }

  button:hover,
  button:focus-visible {
    background: var(--bg-hover);
    outline: none;
  }

  button :global(svg) {
    flex-shrink: 0;
  }

  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .danger {
    color: var(--danger);
  }
</style>
