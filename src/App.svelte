<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import ClockCounterClockwise from "phosphor-svelte/lib/ClockCounterClockwise";
  import Gauge from "phosphor-svelte/lib/Gauge";
  import GearSix from "phosphor-svelte/lib/GearSix";
  import logoUrl from "../assets/branding/logo-mark.svg";
  import { api, desktopApiAvailable } from "./lib/api";
  import { serviceLabels } from "./lib/display";
  import {
    browserPreviewSnapshots,
    browserPreviewStateFromSearch,
    defaultConfig,
    fallbackSnapshots,
    type AppConfig,
    type LoginRequiredEvent,
    type UsageDisplayState,
    type UsageSnapshot,
  } from "./lib/usage";
  import Dashboard from "./lib/views/Dashboard.svelte";
  import History from "./lib/views/History.svelte";
  import Settings from "./lib/views/Settings.svelte";

  type View = "dashboard" | "history" | "settings";

  let view = $state<View>("dashboard");
  let config = $state<AppConfig>(defaultConfig);
  let snapshots = $state<UsageSnapshot[]>(fallbackSnapshots);
  let loading = $state(true);
  let refreshing = $state(false);
  let statusMessage = $state<string | null>(null);
  let statusIsError = $state(false);
  let statusTimer: ReturnType<typeof setTimeout> | null = null;

  const navItems: { id: View; label: string; icon: typeof Gauge }[] = [
    { id: "dashboard", label: "Dashboard", icon: Gauge },
    { id: "history", label: "History", icon: ClockCounterClockwise },
    { id: "settings", label: "Settings", icon: GearSix },
  ];

  function setStatus(message: string | null, error = false) {
    statusMessage = message;
    statusIsError = error;

    if (statusTimer) {
      clearTimeout(statusTimer);
      statusTimer = null;
    }

    if (message && !error) {
      statusTimer = setTimeout(() => {
        statusMessage = null;
      }, 6000);
    }
  }

  onMount(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];

    if (!desktopApiAvailable()) {
      snapshots = browserPreviewSnapshots(browserPreviewStateFromSearch(window.location.search));
      loading = false;
      return () => {
        cancelled = true;
      };
    }

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
      listen<LoginRequiredEvent>("login://required", (event) => {
        setStatus(`${serviceLabels[event.payload.service]} login required`, true);
      }),
    );
    track(
      listen<AppConfig>("settings://updated", (event) => {
        config = event.payload;
      }),
    );
    track(
      listen("usage://refresh-started", () => {
        refreshing = true;
      }),
    );
    track(
      listen("usage://refresh-finished", () => {
        refreshing = false;
      }),
    );

    async function loadState() {
      try {
        const [loadedConfig, displayState] = await Promise.all([
          api.getAppConfig(),
          api.getDisplayState(),
        ]);

        if (cancelled) {
          return;
        }

        config = loadedConfig;
        snapshots = displayState.snapshots;
      } catch {
        setStatus("Running in browser preview mode", true);
      } finally {
        if (!cancelled) {
          loading = false;
        }
      }
    }

    void loadState();

    return () => {
      cancelled = true;
      cleanups.forEach((cleanup) => cleanup());
    };
  });
</script>

<div class="app bg-blueprint">
  <header class="chrome">
    <div class="chrome-dots" aria-hidden="true">
      <span></span>
      <span></span>
      <span></span>
    </div>
    <span class="chrome-title">Pickgauge · Usage</span>
    <span class="pill" class:ember={!refreshing}>
      <span class="dot" class:pulse={!refreshing}></span>
      {refreshing ? "syncing" : "watching"}
    </span>
  </header>

  <div class="body">
    <aside class="sidebar">
      <img class="mark" src={logoUrl} alt="PickGauge mark" />
      <nav aria-label="Main navigation">
        {#each navItems as item (item.id)}
          <button
            class="nav-btn"
            class:active={view === item.id}
            type="button"
            onclick={() => (view = item.id)}
          >
            <item.icon size={17} weight={view === item.id ? "fill" : "regular"} />
            {item.label}
          </button>
        {/each}
      </nav>
    </aside>

    <main class="content fade-up">
      {#if loading}
        <div class="loading">
          <div class="skeleton"></div>
          <div class="skeleton"></div>
        </div>
      {:else if view === "dashboard"}
        <Dashboard {snapshots} {config} {setStatus} />
      {:else if view === "history"}
        <History {setStatus} />
      {:else}
        <Settings bind:config {setStatus} />
      {/if}
    </main>
  </div>

  <footer class="footer">
    <span class="status" class:error={statusIsError}>{statusMessage ?? ""}</span>
    <span class="brand-line">© Pickforge · pickforge.dev · MIT</span>
  </footer>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background-color: var(--surface);
  }

  .chrome {
    display: flex;
    align-items: center;
    gap: 14px;
    flex: none;
    height: 44px;
    padding: 0 16px;
    border-bottom: 1px solid var(--hairline);
    background: color-mix(in srgb, var(--surface) 75%, transparent);
  }

  .chrome-dots {
    display: flex;
    gap: 6px;
  }

  .chrome-dots span {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-pill);
    background: color-mix(in srgb, var(--text) 15%, transparent);
  }

  .chrome-title {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 20px;
    flex: none;
    width: 176px;
    padding: 18px 12px;
    border-right: 1px solid var(--hairline);
    background: color-mix(in srgb, var(--surface-1) 55%, transparent);
  }

  .mark {
    width: 34px;
    height: 34px;
    margin-left: 6px;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .nav-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 12px;
    border: none;
    border-radius: 9px;
    background: transparent;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
    letter-spacing: -0.01em;
    cursor: pointer;
    transition:
      background 0.3s var(--ease-forge),
      color 0.3s var(--ease-forge);
  }

  .nav-btn:hover {
    color: var(--text);
    background: var(--wash);
  }

  .nav-btn.active {
    color: var(--ember);
    background: color-mix(in srgb, var(--ember) 8%, transparent);
  }

  .nav-btn:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--ember) 60%, transparent);
    outline-offset: -2px;
  }

  .content {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 24px 28px 32px;
  }

  .loading {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .skeleton {
    height: 180px;
    border-radius: var(--radius-card);
    background: linear-gradient(
      100deg,
      var(--surface-1) 40%,
      var(--surface-2) 50%,
      var(--surface-1) 60%
    );
    background-size: 220% 100%;
    animation: shimmer 1.6s linear infinite;
  }

  @keyframes shimmer {
    to {
      background-position: -120% 0;
    }
  }

  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex: none;
    height: 34px;
    padding: 0 16px;
    border-top: 1px solid var(--hairline);
    background: color-mix(in srgb, var(--surface) 75%, transparent);
  }

  .status {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--muted);
  }

  .status.error {
    color: var(--bad);
  }

  .brand-line {
    flex: none;
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.12em;
    color: var(--muted);
  }

  @media (max-width: 700px) {
    .sidebar {
      width: 60px;
      padding: 18px 8px;
      align-items: center;
    }

    .mark {
      margin-left: 0;
    }

    .nav-btn {
      justify-content: center;
      width: 44px;
      padding: 0;
      font-size: 0;
      gap: 0;
    }

    .content {
      padding: 18px 14px 24px;
    }
  }
</style>
