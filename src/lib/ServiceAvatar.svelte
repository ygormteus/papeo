<script lang="ts">
  import { Globe, Headphones, MessageCircle, Send } from "@lucide/svelte";
  import { serviceColor, type ServiceView } from "./api";

  let {
    service,
    size = 22,
    dimmed = false,
  }: { service: Pick<ServiceView, "kind" | "color" | "emoji">; size?: number; dimmed?: boolean } = $props();

  const Icon = $derived(
    service.kind === "whatsapp"
      ? MessageCircle
      : service.kind === "telegram"
        ? Send
        : service.kind === "discord"
          ? Headphones
          : Globe,
  );
</script>

<span
  class="avatar"
  class:dimmed
  style:--c={serviceColor(service)}
  style:width="{size}px"
  style:height="{size}px"
  aria-hidden="true"
>
  {#if service.emoji}
    <span class="emoji" style:font-size="{Math.round(size * 0.6)}px">{service.emoji}</span>
  {:else}
    <Icon size={Math.round(size * 0.58)} strokeWidth={2.4} />
  {/if}
</span>

<style>
  .avatar {
    display: inline-grid;
    place-items: center;
    flex: none;
    border-radius: 30%;
    background: var(--c);
    color: #fff;
    overflow: hidden;
  }

  .emoji {
    line-height: 1;
    font-weight: 650;
    font-family: "Apple Color Emoji", "Segoe UI Emoji", "Noto Color Emoji", var(--font);
  }

  .dimmed {
    filter: grayscale(1);
    opacity: 0.55;
  }
</style>
