<script lang="ts">
  import type { MenuItem } from "./api";

  let {
    x,
    y,
    items,
    maxX,
    onclose,
  }: { x: number; y: number; items: MenuItem[]; maxX: number; onclose: () => void } = $props();

  const WIDTH = 210;
  let menu: HTMLDivElement;
  let left = $derived(Math.max(8, Math.min(x, maxX - WIDTH - 8)));
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
  style:width="{WIDTH}px"
  onkeydown={keydown}
>
  {#each items as item (item.label)}
    <button
      role="menuitem"
      class:danger={item.danger}
      onclick={() => {
        onclose();
        item.run();
      }}
    >
      <item.icon size={16} />
      {item.label}
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

  .danger {
    color: var(--danger);
  }
</style>
