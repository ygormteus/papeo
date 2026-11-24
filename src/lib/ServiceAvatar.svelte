<script lang="ts">
  import { Globe, MessageCircle, Send } from "@lucide/svelte";
  import { KIND_META, type ServiceView } from "./api";

  let { service, size = 22, dimmed = false }: { service: ServiceView; size?: number; dimmed?: boolean } =
    $props();

  const Icon = $derived(
    service.kind === "whatsapp" ? MessageCircle : service.kind === "telegram" ? Send : Globe,
  );
</script>

<span
  class="avatar"
  class:dimmed
  style:--c={KIND_META[service.kind].color}
  style:width="{size}px"
  style:height="{size}px"
  aria-hidden="true"
>
  <Icon size={Math.round(size * 0.58)} strokeWidth={2.4} />
</span>

<style>
  .avatar {
    display: inline-grid;
    place-items: center;
    flex: none;
    border-radius: 30%;
    background: var(--c);
    color: #fff;
  }

  .dimmed {
    filter: grayscale(1);
    opacity: 0.55;
  }
</style>
