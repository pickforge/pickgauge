<script lang="ts">
  let {
    value = null,
    low = false,
  }: {
    value?: number | null;
    low?: boolean;
  } = $props();

  const ARC_LENGTH = Math.PI * 80;

  const fraction = $derived(value === null ? 0 : Math.min(Math.max(value, 0), 100) / 100);
  const sweepDegrees = $derived(fraction * 180);
  const accent = $derived(low ? "#c2410c" : "var(--ember)");
</script>

<div class="gauge" role="img" aria-label={value === null ? "Usage unknown" : `${Math.round(value)}% remaining`}>
  <svg viewBox="0 0 200 118" aria-hidden="true">
    <path class="track" d="M20,100 A80,80 0 0 1 180,100" />

    <g class="ticks">
      <line x1="43.4" y1="43.4" x2="50.5" y2="50.5" />
      <line x1="100" y1="20" x2="100" y2="30" />
      <line x1="156.6" y1="43.4" x2="149.5" y2="50.5" />
    </g>

    {#if value !== null}
      <path
        class="fill"
        d="M20,100 A80,80 0 0 1 180,100"
        style={`stroke: ${accent}; stroke-dasharray: ${ARC_LENGTH}; stroke-dashoffset: ${ARC_LENGTH * (1 - fraction)};`}
      />
      <g class="pointer" style={`transform: rotate(${sweepDegrees}deg);`}>
        <line class="needle" x1="100" y1="100" x2="44" y2="100" />
        <circle class="ember-dot" cx="20" cy="100" r="6" style={`fill: ${accent};`} />
      </g>
    {/if}
  </svg>

  <div class="readout">
    <strong class="tabular">{value === null ? "—" : `${Math.round(value)}%`}</strong>
    <span>remaining</span>
  </div>
</div>

<style>
  .gauge {
    position: relative;
    width: 200px;
  }

  svg {
    display: block;
    width: 100%;
  }

  .track {
    fill: none;
    stroke: color-mix(in srgb, var(--text) 12%, transparent);
    stroke-width: 10;
    stroke-linecap: round;
  }

  .fill {
    fill: none;
    stroke-width: 10;
    stroke-linecap: round;
    opacity: 0.8;
    transition: stroke-dashoffset 0.9s var(--ease-forge);
  }

  .ticks line {
    stroke: color-mix(in srgb, var(--text) 35%, transparent);
    stroke-width: 1.5;
    stroke-linecap: round;
  }

  .pointer {
    transform-origin: 100px 100px;
    transition: transform 0.9s var(--ease-forge);
  }

  .needle {
    stroke: color-mix(in srgb, var(--text) 65%, transparent);
    stroke-width: 2.4;
    stroke-linecap: round;
  }

  .ember-dot {
    filter: drop-shadow(0 0 6px color-mix(in srgb, var(--ember) 65%, transparent));
  }

  .readout {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    pointer-events: none;
  }

  .readout strong {
    font-size: 34px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1;
  }

  .readout span {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--muted);
  }
</style>
