<script lang="ts">
  import FloppyDisk from "phosphor-svelte/lib/FloppyDisk";
  import { api, desktopApiAvailable } from "../api";
  import {
    profileInspectionSummary,
    profilePathFromInput,
    profilePathValue,
    serviceLabels,
    webProviderControlState,
  } from "../display";
  import { setTheme } from "../theme";
  import { redactedUserPath, type AppConfig, type Service } from "../usage";

  let {
    config = $bindable(),
    setStatus,
    onDirtyChange = () => {},
    bindActions = () => {},
  }: {
    config: AppConfig;
    setStatus: (message: string | null, error?: boolean) => void;
    onDirtyChange?: (dirty: boolean) => void;
    bindActions?: (actions: { save: () => Promise<boolean>; discard: () => void }) => void;
  } = $props();

  let saving = $state(false);
  let savedJson = $state(JSON.stringify($state.snapshot(config)));
  let clearingSnapshots = $state(false);
  let locatingLogs = $state(false);
  let clearingProfile = $state<Service | null>(null);
  let inspectingProfile = $state<Service | null>(null);

  const webControls = $derived(webProviderControlState(config));
  const dirty = $derived(
    savedJson !== "" && JSON.stringify($state.snapshot(config)) !== savedJson,
  );

  $effect(() => {
    onDirtyChange(dirty);
  });

  $effect(() => {
    bindActions({ save: saveSettings, discard: discardSettings });
  });

  function formatError(caught: unknown, fallback: string) {
    if (typeof caught === "object" && caught !== null && "message" in caught) {
      const message = (caught as { message: unknown }).message;
      if (typeof message === "string" && message.length > 0) {
        return message;
      }
    }
    if (typeof caught === "string" && caught.length > 0) {
      return caught;
    }
    return fallback;
  }

  function updateProfilePath(field: keyof AppConfig["browserProfiles"], event: Event) {
    const target = event.currentTarget;

    if (target instanceof HTMLInputElement) {
      config.browserProfiles[field] = profilePathFromInput(target.value);
    }
  }

  function updateQuotaLabel(service: keyof AppConfig["localQuotas"], event: Event) {
    const target = event.currentTarget;

    if (target instanceof HTMLInputElement) {
      config.localQuotas[service].planLabel = target.value;
    }
  }

  async function saveSettings(): Promise<boolean> {
    if (!desktopApiAvailable()) {
      setStatus("Settings are only persisted in the desktop app", true);
      return false;
    }

    saving = true;

    try {
      config = await api.updateAppConfig($state.snapshot(config));
      savedJson = JSON.stringify($state.snapshot(config));
      setStatus("Settings saved");
      return true;
    } catch (caught) {
      setStatus(formatError(caught, "Could not save settings"), true);
      return false;
    } finally {
      saving = false;
    }
  }

  function discardSettings() {
    if (!savedJson) {
      return;
    }

    config = JSON.parse(savedJson) as AppConfig;
    void setTheme(config.ui.theme);
  }

  async function clearSnapshotCache() {
    if (!desktopApiAvailable()) {
      setStatus("Cached usage is cleared in the desktop app", true);
      return;
    }

    if (!confirm("Clear cached usage snapshots?")) {
      return;
    }

    clearingSnapshots = true;

    try {
      await api.clearCachedSnapshots();
      setStatus("Cached usage snapshots cleared");
    } catch (caught) {
      setStatus(formatError(caught, "Could not clear cached usage"), true);
    } finally {
      clearingSnapshots = false;
    }
  }

  async function showLogLocation() {
    if (!desktopApiAvailable()) {
      setStatus("Log location is available in the desktop app", true);
      return;
    }

    locatingLogs = true;

    try {
      const location = await api.getLogLocation();
      const state = location.exists ? "created" : "not created yet";
      setStatus(`Log file: ${redactedUserPath(location.path)} (${state})`);
    } catch (caught) {
      setStatus(formatError(caught, "Could not read log location"), true);
    } finally {
      locatingLogs = false;
    }
  }

  async function resetProviderSession(service: Service) {
    if (!desktopApiAvailable()) {
      setStatus(`${serviceLabels[service]} sessions reset in the desktop app`, true);
      return;
    }

    if (!confirm(`Reset the app-owned ${serviceLabels[service]} browser session data?`)) {
      return;
    }

    clearingProfile = service;

    try {
      const result = await api.resetProviderSession(service);
      setStatus(
        result.cleared
          ? `Reset ${serviceLabels[service]} browser session`
          : `${serviceLabels[service]} browser session was already clear`,
      );
    } catch (caught) {
      setStatus(formatError(caught, `Could not reset ${serviceLabels[service]} browser session`), true);
    } finally {
      clearingProfile = null;
    }
  }

  async function inspectProviderProfile(service: Service) {
    if (!desktopApiAvailable()) {
      setStatus(`${serviceLabels[service]} profile inspection runs in the desktop app`, true);
      return;
    }

    inspectingProfile = service;

    try {
      const inspection = await api.inspectProviderProfile(service);
      setStatus(profileInspectionSummary(inspection));
    } catch (caught) {
      setStatus(formatError(caught, `Could not inspect ${serviceLabels[service]} profile`), true);
    } finally {
      inspectingProfile = null;
    }
  }
</script>

<section aria-label="Settings">
  <header class="section-head fade-up">
    <div>
      <p class="eyebrow ember">§ 03 · Settings</p>
      <h2>Make it yours</h2>
    </div>
    <button class="btn btn-primary" type="button" disabled={saving} onclick={saveSettings}>
      <FloppyDisk size={15} />
      {saving ? "Saving…" : "Save settings"}
    </button>
  </header>

  <div class="settings-grid fade-up">
    <div class="card group">
      <h4>Services & providers</h4>
      <label class="switch">
        <input type="checkbox" bind:checked={config.enabledServices.codex} />
        <span class="track"></span>
        Codex
      </label>
      <label class="switch">
        <input type="checkbox" bind:checked={config.enabledServices.claude} />
        <span class="track"></span>
        Claude Code
      </label>
      <label class="switch">
        <input type="checkbox" bind:checked={config.providers.localEnabled} />
        <span class="track"></span>
        Local estimates
      </label>
      <label class="switch">
        <input type="checkbox" bind:checked={config.providers.cliEnabled} />
        <span class="track"></span>
        Official readings via CLI login
      </label>
      <label class="switch">
        <input
          type="checkbox"
          bind:checked={config.providers.webEnabled}
          disabled={config.providers.cliEnabled}
        />
        <span class="track"></span>
        Official web readings (browser)
      </label>
      <p class="hint">
        CLI readings reuse the Codex and Claude Code logins already on this machine — the real usage
        number, auto-refreshed, no browser or captcha. Tokens are read locally and only sent to the
        official provider APIs. Falls back to local estimates if a CLI isn't signed in. The browser
        option is the older fallback and is disabled while CLI readings are on.
      </p>
    </div>

    <div class="card group">
      <h4>App</h4>
      <label class="field">
        <span>Theme</span>
        <select
          class="select"
          bind:value={config.ui.theme}
          onchange={() => void setTheme(config.ui.theme)}
        >
          <option value="system">System</option>
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </select>
      </label>
      <label class="switch">
        <input type="checkbox" bind:checked={config.autostart.enabled} />
        <span class="track"></span>
        Start at login
      </label>
      <label class="switch">
        <input type="checkbox" bind:checked={config.ui.sounds} />
        <span class="track"></span>
        Sound cues
      </label>
      <p class="hint">
        A short chime when a gauge crosses the low threshold, and another when it recovers. No
        desktop notifications.
      </p>
      <label class="switch">
        <input type="checkbox" bind:checked={config.ui.floatButton} />
        <span class="track"></span>
        Floating button
      </label>
      <p class="hint">
        A draggable capsule that stays above every window. Click it to open PickGauge,
        right-click to refresh.
      </p>
    </div>

    <div class="card group">
      <h4>Rhythm</h4>
      <div class="number-grid">
        <label class="field">
          <span>Local refresh (s)</span>
          <input class="input" type="number" min="30" max="60" bind:value={config.intervals.localSeconds} />
        </label>
        <label class="field">
          <span>Web refresh (min)</span>
          <input
            class="input"
            type="number"
            min="15"
            max="60"
            bind:value={config.intervals.webMinutes}
            disabled={webControls.webRefreshDisabled}
          />
        </label>
        <label class="field">
          <span>Web cooldown (s)</span>
          <input
            class="input"
            type="number"
            min="60"
            bind:value={config.intervals.manualWebRefreshCooldownSeconds}
            disabled={webControls.webCooldownDisabled}
          />
        </label>
        <label class="field">
          <span>Tray switch (s)</span>
          <input class="input" type="number" min="5" max="10" bind:value={config.intervals.gaugeSwitchSeconds} />
        </label>
        <label class="field">
          <span>Low threshold (%)</span>
          <input class="input" type="number" min="1" max="100" bind:value={config.lowUsageThreshold} />
        </label>
      </div>
    </div>

    {#each ["codex", "claude"] as service (service)}
      <div class="card group">
        <h4>{serviceLabels[service as Service]} calibration</h4>
        <label class="switch">
          <input type="checkbox" bind:checked={config.localQuotas[service as Service].enabled} />
          <span class="track"></span>
          Calibrate local estimates
        </label>
        <div class="number-grid">
          <label class="field">
            <span>Plan label</span>
            <input
              class="input"
              type="text"
              autocomplete="off"
              spellcheck="false"
              placeholder="Optional"
              value={config.localQuotas[service as Service].planLabel}
              oninput={(event) => updateQuotaLabel(service as Service, event)}
            />
          </label>
          <label class="field">
            <span>Token limit</span>
            <input
              class="input"
              type="number"
              min="0"
              step="1"
              bind:value={config.localQuotas[service as Service].limit}
            />
          </label>
          <label class="field">
            <span>Window (h)</span>
            <input
              class="input"
              type="number"
              min="1"
              max="744"
              step="1"
              bind:value={config.localQuotas[service as Service].windowHours}
            />
          </label>
        </div>
        <p class="hint">
          Match your plan's rolling window so local token counts map to a remaining percentage.
        </p>
      </div>
    {/each}

    <div class="card group">
      <h4>Browser profiles</h4>
      <label class="field">
        <span>Profile root</span>
        <input
          class="input"
          type="text"
          autocomplete="off"
          spellcheck="false"
          placeholder="Default app data path"
          value={profilePathValue(config.browserProfiles.rootPath)}
          oninput={(event) => updateProfilePath("rootPath", event)}
          disabled={webControls.profilePathInputsDisabled}
        />
      </label>
      <label class="field">
        <span>Codex profile</span>
        <input
          class="input"
          type="text"
          autocomplete="off"
          spellcheck="false"
          placeholder="Default under root"
          value={profilePathValue(config.browserProfiles.codexPath)}
          oninput={(event) => updateProfilePath("codexPath", event)}
          disabled={webControls.profilePathInputsDisabled}
        />
      </label>
      <label class="field">
        <span>Claude profile</span>
        <input
          class="input"
          type="text"
          autocomplete="off"
          spellcheck="false"
          placeholder="Default under root"
          value={profilePathValue(config.browserProfiles.claudePath)}
          oninput={(event) => updateProfilePath("claudePath", event)}
          disabled={webControls.profilePathInputsDisabled}
        />
      </label>
    </div>

    <div class="card group">
      <h4>Maintenance</h4>
      <div class="actions">
        <button class="btn small" type="button" disabled={clearingSnapshots} onclick={clearSnapshotCache}>
          {clearingSnapshots ? "Clearing…" : "Clear cache"}
        </button>
        <button class="btn small" type="button" disabled={locatingLogs} onclick={showLogLocation}>
          {locatingLogs ? "Checking…" : "Log location"}
        </button>
        {#each ["codex", "claude"] as service (service)}
          <button
            class="btn small"
            type="button"
            disabled={inspectingProfile === service}
            onclick={() => inspectProviderProfile(service as Service)}
          >
            {inspectingProfile === service ? "Inspecting…" : `Inspect ${serviceLabels[service as Service]}`}
          </button>
          <button
            class="btn small btn-danger"
            type="button"
            disabled={clearingProfile === service}
            onclick={() => resetProviderSession(service as Service)}
          >
            {clearingProfile === service ? "Resetting…" : `Reset ${serviceLabels[service as Service]} session`}
          </button>
        {/each}
      </div>
    </div>
  </div>

  {#if dirty}
    <div class="save-bar glass" role="status">
      <span class="save-dot"></span>
      <span class="save-text">Unsaved changes</span>
      <button class="btn btn-ghost small" type="button" onclick={discardSettings}>Discard</button>
      <button class="btn btn-primary small" type="button" disabled={saving} onclick={saveSettings}>
        {saving ? "Saving…" : "Save settings"}
      </button>
    </div>
  {/if}
</section>

<style>
  .section-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
  }

  .section-head h2 {
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .section-head .eyebrow {
    margin-bottom: 6px;
  }

  .settings-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(330px, 100%), 1fr));
    gap: 14px;
    align-items: start;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px;
  }

  .group h4 {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .hint {
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--muted);
  }

  .number-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(120px, 100%), 1fr));
    gap: 10px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .save-bar {
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
    animation: save-bar-in 400ms var(--ease-forge) both;
  }

  @keyframes save-bar-in {
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
</style>
