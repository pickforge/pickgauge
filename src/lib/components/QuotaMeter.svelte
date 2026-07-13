<script lang="ts">
  let {
    label,
    value = null,
    resetLabel = null,
    low = false,
    focal = false,
    dimmed = false,
  }: {
    label: string;
    value?: number | null;
    resetLabel?: string | null;
    low?: boolean;
    focal?: boolean;
    dimmed?: boolean;
  } = $props();

  const normalized = $derived(value === null ? 0 : Math.min(Math.max(value, 0), 100));
  const description = $derived(
    `${label}: ${value === null ? "usage unknown" : `${Math.round(value)}% remaining`}${
      resetLabel ? `, resets ${resetLabel}` : ""
    }`,
  );
</script>

<div class="meter" class:low class:focal class:dimmed role="img" aria-label={description}>
  <span class="meter-label">{label}</span>
  <span class="meter-track" aria-hidden="true">
    <span class="meter-fill" style={`width: ${normalized}%`}></span>
    {#if focal && value !== null}<span class="meter-point" style={`left: ${normalized}%`}></span>{/if}
  </span>
  <strong class="meter-value tabular">{value === null ? "—" : `${Math.round(value)}%`}</strong>
  <span class="meter-reset tabular">{resetLabel ? `in ${resetLabel}` : "—"}</span>
</div>

<style>
  .meter {
    display: grid;
    grid-template-columns: 66px minmax(48px, 1fr) 42px 50px;
    align-items: center;
    gap: 8px;
    min-height: 27px;
    transition: opacity 200ms var(--ease-forge);
  }

  .meter-label {
    overflow: hidden;
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: 9.5px;
    letter-spacing: 0.13em;
    text-overflow: ellipsis;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .meter-track {
    position: relative;
    height: 5px;
    border-radius: var(--radius-pill);
    background: color-mix(in srgb, var(--text) 10%, transparent);
  }

  .meter-fill {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: inherit;
    background: color-mix(in srgb, var(--text) 48%, transparent);
    transition: width 600ms var(--ease-forge), background 250ms var(--ease-forge);
  }

  .meter-point {
    position: absolute;
    top: 50%;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ember);
    box-shadow: var(--glow-ember-soft);
    transform: translate(-50%, -50%);
    transition: left 600ms var(--ease-forge);
  }

  .focal .meter-fill {
    background: var(--ember);
  }

  .low .meter-fill {
    background: var(--warn);
  }

  .low .meter-point {
    background: var(--warn);
    box-shadow: none;
  }

  .meter-value {
    font-size: 13px;
    font-weight: 650;
    text-align: right;
  }

  .meter-reset {
    overflow: hidden;
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: 10px;
    text-align: right;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dimmed {
    opacity: 0.45;
  }

  @media (max-width: 760px) {
    .meter {
      grid-template-columns: 58px minmax(40px, 1fr) 40px 42px;
      gap: 6px;
    }
  }
</style>
