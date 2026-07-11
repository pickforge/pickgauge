<script lang="ts">
  import { controlsSide, hostPlatform } from "../platform";
  import { handleTitlebarMouseDown } from "../windowChrome";
  import WindowControls from "./WindowControls.svelte";

  let { refreshing = false }: { refreshing?: boolean } = $props();

  const side = controlsSide(hostPlatform());
</script>

<header
  class="pf-titlebar"
  class:pf-titlebar--controls-left={side === "left"}
  role="presentation"
  onmousedown={handleTitlebarMouseDown}
>
  <div class="pf-titlebar-left">
    {#if side === "left"}
      <WindowControls />
    {/if}
    <div class="pf-brand">
      <span class="pf-mark"></span>
      <span class="pf-wordmark">PickGauge</span>
    </div>
  </div>

  <div class="titlebar-center" aria-hidden="true"></div>

  <div class="pf-titlebar-right">
    <span
      class="pf-pill"
      title={refreshing ? "syncing" : "watching"}
      style={`--pf-intent: ${refreshing ? "var(--pf-info)" : "var(--pf-ember)"}`}
    >
      <span class="pf-dot" class:pf-dot--pulsing={!refreshing}></span>
      {refreshing ? "syncing" : "watching"}
    </span>
    {#if side === "right"}
      <WindowControls />
    {/if}
  </div>
</header>

<style>
  .titlebar-center {
    min-width: 0;
    height: 100%;
  }
</style>
