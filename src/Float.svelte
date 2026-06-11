<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { api, desktopApiAvailable } from "./lib/api";
  import type { AppConfig, Service, UsageDisplayState, UsageSnapshot } from "./lib/usage";
  import { defaultConfig } from "./lib/usage";

  let snapshots = $state<UsageSnapshot[]>([]);
  let config = $state<AppConfig>(defaultConfig);
  let refreshing = $state(false);

  // Distinguish click (open the app) from drag (move the window): once the
  // pointer travels past a small threshold, hand control to the compositor.
  let downAt: { x: number; y: number } | null = null;
  let dragged = false;

  const RING_RADIUS = 13;
  const RING_LENGTH = 2 * Math.PI * RING_RADIUS;

  const ringColors: Record<Service, string> = {
    codex: "var(--text)",
    claude: "#ff7a1a",
  };

  function ringColor(snapshot: UsageSnapshot) {
    if (
      snapshot.remainingPercent !== null &&
      snapshot.remainingPercent <= config.lowUsageThreshold
    ) {
      return "#c2410c";
    }

    return ringColors[snapshot.service];
  }

  function ringOffset(snapshot: UsageSnapshot) {
    const fraction =
      snapshot.remainingPercent === null
        ? 0
        : Math.min(Math.max(snapshot.remainingPercent, 0), 100) / 100;

    return RING_LENGTH * (1 - fraction);
  }

  function ringTitle(snapshot: UsageSnapshot) {
    const percent =
      snapshot.remainingPercent === null ? "unknown" : `${Math.round(snapshot.remainingPercent)}%`;

    return `${snapshot.service === "codex" ? "Codex" : "Claude Code"}: ${percent} remaining`;
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) {
      return;
    }

    downAt = { x: event.screenX, y: event.screenY };
    dragged = false;
  }

  function onPointerMove(event: PointerEvent) {
    if (!downAt || dragged) {
      return;
    }

    const dx = event.screenX - downAt.x;
    const dy = event.screenY - downAt.y;

    if (Math.hypot(dx, dy) > 5) {
      dragged = true;
      getCurrentWindow().startDragging().catch(() => {});
    }
  }

  function onPointerUp(event: PointerEvent) {
    if (event.button === 0 && downAt && !dragged) {
      api.showMainWindow().catch(() => {});
    } else if (event.button === 1) {
      // Middle-click dismisses the capsule (persisted; re-enable from the
      // tray menu or Settings).
      api.toggleFloatButton().catch(() => {});
    }

    downAt = null;
  }

  function onContextMenu(event: MouseEvent) {
    event.preventDefault();

    if (refreshing) {
      return;
    }

    refreshing = true;
    api
      .refreshUsage()
      .catch(() => {})
      .finally(() => {
        refreshing = false;
      });
  }

  onMount(() => {
    if (!desktopApiAvailable()) {
      return;
    }

    let cancelled = false;
    const cleanups: (() => void)[] = [];

    function track(promise: Promise<() => void>) {
      promise
        .then((cleanup) => {
          if (cancelled) {
            cleanup();
            return;
          }
          cleanups.push(cleanup);
        })
        .catch(() => {});
    }

    track(
      listen<UsageDisplayState>("usage://snapshots-updated", (event) => {
        snapshots = event.payload.snapshots;
      }),
    );
    track(
      listen<AppConfig>("settings://updated", (event) => {
        config = event.payload;
      }),
    );

    void Promise.all([api.getDisplayState(), api.getAppConfig()])
      .then(([displayState, loadedConfig]) => {
        if (!cancelled) {
          snapshots = displayState.snapshots;
          config = loadedConfig;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      cleanups.forEach((cleanup) => cleanup());
    };
  });
</script>

<div
  class="capsule"
  role="button"
  tabindex="-1"
  aria-label="PickGauge — click to open, right-click to refresh, middle-click to hide"
  title="PickGauge — click to open, right-click to refresh, middle-click to hide, drag to move"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  oncontextmenu={onContextMenu}
>
  <img class="mark mark-dark" src="/brand/pickgauge-mark-128.svg" alt="" draggable="false" />
  <img class="mark mark-light" src="/brand/pickgauge-mark-light.svg" alt="" draggable="false" />

  <div class="slot">
    {#if snapshots.length === 0}
      <span class="wordmark">PG</span>
    {:else}
      <div class="rings">
        {#each snapshots as snapshot (snapshot.service)}
          <div class="ring" title={ringTitle(snapshot)}>
            <svg viewBox="0 0 34 34" aria-hidden="true">
              <circle class="track" cx="17" cy="17" r={RING_RADIUS} />
              {#if snapshot.remainingPercent !== null}
                <circle
                  class="fill"
                  cx="17"
                  cy="17"
                  r={RING_RADIUS}
                  style={`stroke: ${ringColor(snapshot)}; stroke-dasharray: ${RING_LENGTH}; stroke-dashoffset: ${ringOffset(snapshot)};`}
                />
              {/if}
            </svg>
            <span class="ring-value">
              {snapshot.remainingPercent === null ? "–" : Math.round(snapshot.remainingPercent)}
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <span class="status-dot" class:busy={refreshing}></span>
</div>

<style>
  .capsule {
    display: flex;
    align-items: center;
    gap: 10px;
    width: calc(100vw - 4px);
    height: calc(100vh - 4px);
    margin: 2px;
    padding: 0 14px 0 10px;
    border: 1px solid var(--hairline-strong);
    border-radius: var(--radius-pill);
    background: var(--capsule-bg);
    backdrop-filter: blur(12px) saturate(140%);
    box-shadow: var(--glow-ember-soft);
    cursor: pointer;
    user-select: none;
    -webkit-user-select: none;
    overflow: hidden;
    transition: border-color 500ms var(--ease-forge), box-shadow 500ms var(--ease-forge);
  }
  .capsule:hover {
    border-color: color-mix(in srgb, var(--ember) 40%, transparent);
  }

  .mark {
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    pointer-events: none;
  }
  .mark-light {
    display: none;
  }
  :global([data-theme="light"]) .mark-dark {
    display: none;
  }
  :global([data-theme="light"]) .mark-light {
    display: block;
  }

  .slot {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    min-width: 0;
    pointer-events: none;
  }

  .status-dot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: var(--radius-pill);
    background: var(--ember);
    transition: background 300ms var(--ease-forge);
    animation: ember-pulse 2.4s var(--ease-forge) infinite;
  }

  .status-dot.busy {
    background: var(--warn);
  }

  .wordmark {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.18em;
    color: var(--muted);
  }

  .rings {
    display: flex;
    gap: 8px;
  }

  .ring {
    position: relative;
    width: 34px;
    height: 34px;
  }

  .ring svg {
    display: block;
    width: 100%;
    height: 100%;
    transform: rotate(-90deg);
  }

  .track {
    fill: none;
    stroke: color-mix(in srgb, var(--text) 14%, transparent);
    stroke-width: 3;
  }

  .fill {
    fill: none;
    stroke-width: 3;
    stroke-linecap: round;
    transition: stroke-dashoffset 0.9s var(--ease-forge);
  }

  .ring-value {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-mono);
    font-size: 9.5px;
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }
</style>
