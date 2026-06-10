<script lang="ts">
  type BarItem = {
    key: string;
    label: string;
    value: number;
    title: string;
  };

  let {
    items,
    height = 120,
  }: {
    items: BarItem[];
    height?: number;
  } = $props();

  const max = $derived(Math.max(1, ...items.map((item) => item.value)));
</script>

<div class="bars" style={`--bars-height: ${height}px;`}>
  {#each items as item (item.key)}
    <div class="bar-col" title={item.title}>
      <div class="bar-track">
        <div
          class="bar"
          class:empty={item.value === 0}
          style={`height: ${Math.max(4, (item.value / max) * 100)}%;`}
        ></div>
      </div>
      <span class="bar-label">{item.label}</span>
    </div>
  {/each}
</div>

<style>
  .bars {
    display: flex;
    align-items: stretch;
    gap: 6px;
    width: 100%;
  }

  .bar-col {
    display: flex;
    flex: 1 1 0;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
  }

  .bar-track {
    display: flex;
    align-items: flex-end;
    height: var(--bars-height);
    border-radius: 6px;
    background: color-mix(in srgb, var(--text) 3%, transparent);
  }

  .bar {
    width: 100%;
    border-radius: 6px;
    background: linear-gradient(180deg, var(--ember-soft), var(--ember-deep));
    opacity: 0.85;
    transition: height 0.6s var(--ease-forge);
  }

  .bar.empty {
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .bar-label {
    overflow: hidden;
    text-align: center;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
</style>
