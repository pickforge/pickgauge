<script lang="ts">
  import { isTauri } from "../platform";
  import { startResize, type ResizeDir } from "../windowChrome";

  const handles: { dir: ResizeDir; cls: string }[] = [
    { dir: "North", cls: "n" },
    { dir: "South", cls: "s" },
    { dir: "East", cls: "e" },
    { dir: "West", cls: "w" },
    { dir: "NorthWest", cls: "nw" },
    { dir: "NorthEast", cls: "ne" },
    { dir: "SouthWest", cls: "sw" },
    { dir: "SouthEast", cls: "se" },
  ];

  function start(dir: ResizeDir) {
    return (event: MouseEvent) => {
      if (event.button !== 0) {
        return;
      }
      event.preventDefault();
      void startResize(dir);
    };
  }
</script>

{#if isTauri()}
  {#each handles as handle (handle.cls)}
    <div
      class={`pf-resize pf-resize--${handle.cls}`}
      role="presentation"
      onmousedown={start(handle.dir)}
    ></div>
  {/each}
{/if}
