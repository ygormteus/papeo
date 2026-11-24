<script lang="ts">
  let {
    checked,
    label,
    onchange,
    disabled = false,
  }: { checked: boolean; label: string; onchange: (value: boolean) => void; disabled?: boolean } =
    $props();
</script>

<label class="switch" class:disabled>
  <input
    type="checkbox"
    role="switch"
    {checked}
    {disabled}
    onchange={(e) => onchange(e.currentTarget.checked)}
  />
  <span class="track" aria-hidden="true"></span>
  <span class="sr-only">{label}</span>
</label>

<style>
  .switch {
    position: relative;
    display: inline-flex;
    flex: none;
    cursor: pointer;
  }

  .switch.disabled {
    opacity: 0.5;
    cursor: default;
  }

  input {
    position: absolute;
    opacity: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    cursor: inherit;
  }

  .track {
    width: 36px;
    height: 20px;
    border-radius: 999px;
    background: var(--border-strong);
    position: relative;
    transition: background 0.15s;
  }

  .track::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.2);
    transition: transform 0.15s;
  }

  input:checked + .track {
    background: var(--success-text);
  }

  input:checked + .track::after {
    transform: translateX(16px);
  }

  input:focus-visible + .track {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
