<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import ClockCounterClockwise from "phosphor-svelte/lib/ClockCounterClockwise";
  import FloppyDisk from "phosphor-svelte/lib/FloppyDisk";
  import Gauge from "phosphor-svelte/lib/Gauge";
  import GearSix from "phosphor-svelte/lib/GearSix";
  import logoUrl from "../assets/branding/pickgauge-mark-128.svg";
  import ResizeHandles from "./lib/components/ResizeHandles.svelte";
  import Titlebar from "./lib/components/Titlebar.svelte";
  import {
    api,
    desktopApiAvailable,
    EVENT_LOGIN_REQUIRED,
    EVENT_REFRESH_FINISHED,
    EVENT_REFRESH_STARTED,
    EVENT_SETTINGS,
    EVENT_SNAPSHOTS,
  } from "./lib/api";
  import { serviceLabels, settingsSaveDisplayState } from "./lib/display";
  import { flagEnabled } from "./lib/flags";
  import { mountUpdateDialog, type UpdateDialogHost } from "./lib/updateDialog";
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
  let settingsDirty = $state(false);
  let settingsSaving = $state(false);
  let pendingView = $state<View | null>(null);
  let settingsActions: { save: () => Promise<boolean>; discard: () => void } | null = null;
  const settingsSaveDisplay = $derived(settingsSaveDisplayState(settingsDirty));
  let config = $state<AppConfig>(defaultConfig);
  let snapshots = $state<UsageSnapshot[]>(fallbackSnapshots);
  let loading = $state(true);
  let refreshing = $state(false);
  let statusMessage = $state<string | null>(null);
  let statusIsError = $state(false);
  let statusTimer: ReturnType<typeof setTimeout> | null = null;
  let updateDialogEl: HTMLElement | undefined = $state();
  const showUpdateDialog = flagEnabled("studioUpdateDialog");

  const navItems: { id: View; label: string; icon: typeof Gauge }[] = [
    { id: "dashboard", label: "Dashboard", icon: Gauge },
    { id: "history", label: "History", icon: ClockCounterClockwise },
    { id: "settings", label: "Settings", icon: GearSix },
  ];

  function navigate(target: View) {
    if (view === "settings" && settingsDirty && target !== "settings") {
      pendingView = target;
      return;
    }
    view = target;
  }

  async function saveAndContinue() {
    if (!settingsActions || !pendingView) {
      return;
    }
    if (await settingsActions.save()) {
      view = pendingView;
    }
    pendingView = null;
  }

  function discardAndContinue() {
    if (!pendingView) {
      return;
    }
    settingsActions?.discard();
    settingsDirty = false;
    view = pendingView;
    pendingView = null;
  }

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

    if (showUpdateDialog && updateDialogEl) {
      mountUpdateDialog(updateDialogEl as unknown as UpdateDialogHost);
    }

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
      listen<UsageDisplayState>(EVENT_SNAPSHOTS, (event) => {
        snapshots = event.payload.snapshots;
      }),
    );
    track(
      listen<LoginRequiredEvent>(EVENT_LOGIN_REQUIRED, (event) => {
        setStatus(`${serviceLabels[event.payload.service]} login required`, true);
      }),
    );
    track(
      listen<AppConfig>(EVENT_SETTINGS, (event) => {
        config = event.payload;
      }),
    );
    track(
      listen(EVENT_REFRESH_STARTED, () => {
        refreshing = true;
      }),
    );
    track(
      listen(EVENT_REFRESH_FINISHED, () => {
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
  <ResizeHandles />
  <Titlebar {refreshing} />

  <div class="body">
    <aside class="sidebar">
      <img class="mark" src={logoUrl} alt="PickGauge mark" />
      <nav aria-label="Main navigation">
        {#each navItems as item (item.id)}
          <button
            class="nav-btn"
            class:active={view === item.id}
            type="button"
            onclick={() => navigate(item.id)}
          >
            {#if item.id === "settings" && settingsDirty}
              <span class="dirty-dot" title="Unsaved changes"></span>
            {/if}
            <item.icon size={17} weight={view === item.id ? "fill" : "regular"} />
            {item.label}
          </button>
        {/each}
      </nav>
    </aside>

    <main class="content fade-up">
      {#if loading}
        <div class="loading">
          {#each [1, 2] as item (item)}
            <div class="skeleton"></div>
          {/each}
        </div>
      {:else if view === "dashboard"}
        <Dashboard {snapshots} {config} {setStatus} />
      {:else if view === "history"}
        <History {setStatus} />
      {:else}
        <Settings
          bind:config
          {setStatus}
          onDirtyChange={(dirty) => (settingsDirty = dirty)}
          onSavingChange={(saving) => (settingsSaving = saving)}
          bindActions={(actions) => (settingsActions = actions)}
        />
      {/if}
    </main>
  </div>

  <footer class="pf-statusbar">
    <div class="pf-statusbar-left">
      <span class="pf-statusbar-item" class:error={statusIsError}>{statusMessage ?? ""}</span>
    </div>
    <div class="pf-statusbar-right">
      <span>© Pickforge · pickforge.dev · MIT</span>
    </div>
  </footer>
</div>

{#if view === "settings" && settingsSaveDisplay.overlayVisible}
  <div class="settings-save-overlay glass" role="status" aria-live="polite">
    <span class="save-dot" aria-hidden="true"></span>
    <span class="save-text">Unsaved changes</span>
    <button
      class="btn btn-ghost btn-sm save-discard"
      type="button"
      onclick={() => settingsActions?.discard()}
    >
      Discard
    </button>
    <button
      class="btn btn-primary btn-sm"
      type="button"
      disabled={settingsSaving}
      onclick={() => settingsActions?.save()}
    >
      <FloppyDisk size={15} />
      <span class="save-label-wide">{settingsSaving ? "Saving…" : "Save settings"}</span>
      <span class="save-label-compact">{settingsSaving ? "Saving…" : "Save"}</span>
    </button>
  </div>
{/if}

{#if pendingView}
  <div class="dialog-backdrop" role="presentation" onclick={() => (pendingView = null)}>
    <div
      class="dialog card"
      role="alertdialog"
      aria-label="Unsaved settings"
      tabindex="-1"
      onclick={(event) => event.stopPropagation()}
      onkeydown={(event) => event.key === "Escape" && (pendingView = null)}
    >
      <h3>Unsaved settings</h3>
      <p>You changed settings but haven't saved them yet.</p>
      <div class="dialog-actions">
        <button class="btn btn-ghost btn-sm" type="button" onclick={() => (pendingView = null)}>
          Keep editing
        </button>
        <button class="btn btn-danger btn-sm" type="button" onclick={discardAndContinue}>
          Discard
        </button>
        <button class="btn btn-primary btn-sm" type="button" onclick={saveAndContinue}>
          Save and continue
        </button>
      </div>
    </div>
  </div>
{/if}

{#if showUpdateDialog}
  <pickforge-update-dialog bind:this={updateDialogEl}></pickforge-update-dialog>
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background-color: var(--surface);
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

  .dirty-dot {
    width: 4px;
    height: 10px;
    border: var(--pf-bracket-width) solid var(--ember);
    border-right: none;
    border-radius: 2px 0 0 2px;
    flex: none;
    margin-left: -2px;
  }

  .dialog-backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(4px);
    animation: backdrop-in 250ms var(--ease-forge) both;
  }

  @keyframes backdrop-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .dialog {
    width: min(420px, calc(100vw - 48px));
    padding: 22px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    animation: dialog-in 350ms var(--ease-forge) both;
  }

  @keyframes dialog-in {
    from {
      opacity: 0;
      transform: translateY(14px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .dialog h3 {
    font-size: 16px;
  }

  .dialog p {
    font-size: 13px;
    color: var(--muted);
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }

  .content {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 24px 28px 32px;
  }

  .loading {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .skeleton {
    height: 176px;
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

  .pf-statusbar-item.error {
    color: var(--bad);
  }

  /*
   * Rendered here (outside .app / .content) so it stays a fixed-position
   * child of the viewport instead of the scrolling Settings surface, whose
   * .fade-up entrance transform would otherwise clip or reposition it. See
   * issue #47.
   */
  .settings-save-overlay {
    position: fixed;
    bottom: 48px;
    right: 28px;
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px 10px 16px;
    border-radius: var(--radius-pill);
    border-color: color-mix(in srgb, var(--ember) 35%, transparent);
    box-shadow: var(--glow-ember-soft);
    animation: settings-save-overlay-in 400ms var(--ease-forge) both;
  }

  @keyframes settings-save-overlay-in {
    from {
      opacity: 0;
      transform: translateY(16px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .save-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
    background: var(--ember);
    animation: ember-pulse 2.4s var(--ease-forge) infinite;
  }

  .save-text {
    font-size: 13px;
    font-weight: 600;
  }

  .save-label-compact {
    display: none;
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

    .loading {
      grid-template-columns: 1fr;
    }

    .settings-save-overlay {
      bottom: 20px;
      right: 16px;
      gap: 8px;
      padding: 10px 14px;
    }

    .save-text,
    .save-discard,
    .save-label-wide {
      display: none;
    }

    .save-label-compact {
      display: inline;
    }
  }
</style>
