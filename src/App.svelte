<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import heroArtUrl from "../assets/branding/hero-art.png";
  import lockupUrl from "../assets/branding/logo-lockup-on-dark.svg";
  import logoUrl from "../assets/branding/logo-mark.svg";
  import patternUrl from "../assets/branding/brand-pattern.svg";
  import trayClaudeUrl from "../assets/branding/tray-claude.svg";
  import trayCodexUrl from "../assets/branding/tray-codex.svg";
  import {
    defaultConfig,
    fallbackSnapshots,
    providerStatusMessage,
    type AppConfig,
    type ClearedProviderProfile,
    type CommandError,
    type LogLocation,
    type OfficialUsagePage,
    type Service,
    type UsageConfidence,
    type UsageDisplayState,
    type UsageSnapshot,
    type UsageSource,
  } from "./lib/usage";

  let config = $state<AppConfig>(defaultConfig);
  let snapshots = $state<UsageSnapshot[]>(fallbackSnapshots);
  let loading = $state(true);
  let saving = $state(false);
  let refreshing = $state(false);
  let refreshingOfficial = $state<Service | null>(null);
  let clearingSnapshots = $state(false);
  let clearingProfile = $state<Service | null>(null);
  let locatingLogs = $state(false);
  let openingService = $state<Service | null>(null);
  let error = $state<string | null>(null);
  let statusMessage = $state<string | null>(null);

  const serviceLabels: Record<Service, string> = {
    codex: "Codex",
    claude: "Claude Code",
  };

  const serviceTone: Record<Service, string> = {
    codex: "codex",
    claude: "claude",
  };

  const serviceIcons: Record<Service, string> = {
    codex: trayCodexUrl,
    claude: trayClaudeUrl,
  };

  const sourceLabels: Record<UsageSource, string> = {
    local: "Local estimate",
    web: "Official web",
    merged: "Official + local",
    fake: "Preview",
  };

  const confidenceLabels: Record<UsageConfidence, string> = {
    high: "High",
    medium: "Medium",
    low: "Low",
    unknown: "Unknown",
  };

  function formatPercent(value: number | null) {
    return value === null ? "Unknown" : `${Math.round(value)}%`;
  }

  function formatCount(value: number) {
    return new Intl.NumberFormat().format(value);
  }

  function detailNumber(snapshot: UsageSnapshot, key: string) {
    const value = snapshot.details[key];
    return typeof value === "number" && Number.isFinite(value) ? value : null;
  }

  function detailString(snapshot: UsageSnapshot, key: string) {
    const value = snapshot.details[key];
    return typeof value === "string" && value.length > 0 ? value : null;
  }

  function plural(value: number, singular: string, pluralValue = `${singular}s`) {
    return value === 1 ? singular : pluralValue;
  }

  function localActivitySummary(snapshot: UsageSnapshot) {
    if (snapshot.source !== "local" || snapshot.remainingPercent !== null) {
      return null;
    }

    const totalTokens =
      detailNumber(snapshot, "totalTokens") ??
      (detailNumber(snapshot, "inputTokens") ?? 0) +
        (detailNumber(snapshot, "outputTokens") ?? 0) +
        (detailNumber(snapshot, "cacheCreationInputTokens") ?? 0) +
        (detailNumber(snapshot, "cacheReadInputTokens") ?? 0);

    if (totalTokens <= 0) {
      return null;
    }

    const activityCount =
      detailNumber(snapshot, "sessionCount") ??
      detailNumber(snapshot, "usageThreads") ??
      detailNumber(snapshot, "usageRecords");
    const activityLabel =
      detailNumber(snapshot, "sessionCount") !== null
        ? plural(activityCount ?? 0, "session")
        : detailNumber(snapshot, "usageThreads") !== null
          ? plural(activityCount ?? 0, "thread")
          : plural(activityCount ?? 0, "record");
    const modelCount = detailNumber(snapshot, "modelCount");
    const parts = [`${formatCount(totalTokens)} tokens`];

    if (activityCount !== null && activityCount > 0) {
      parts.push(`${formatCount(activityCount)} ${activityLabel}`);
    }

    if (modelCount !== null && modelCount > 0) {
      parts.push(`${formatCount(modelCount)} ${plural(modelCount, "model")}`);
    }

    return `Local activity: ${parts.join(" | ")}`;
  }

  function isCommandError(caught: unknown): caught is CommandError {
    if (typeof caught !== "object" || caught === null) {
      return false;
    }

    const error = caught as Partial<CommandError>;
    return typeof error.code === "string" && typeof error.message === "string";
  }

  function formatError(caught: unknown, fallback: string) {
    if (isCommandError(caught) && caught.message.length > 0) {
      return caught.message;
    }

    if (caught instanceof Error && caught.message) {
      return caught.message;
    }

    if (typeof caught === "string" && caught.length > 0) {
      return caught;
    }

    return fallback;
  }

  function formatTimestamp(value: string) {
    const parsed = new Date(value);

    if (Number.isNaN(parsed.getTime())) {
      return value;
    }

    return new Intl.DateTimeFormat(undefined, {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(parsed);
  }

  function snapshotIsStale(snapshot: UsageSnapshot) {
    return snapshot.details.stale === true;
  }

  function lastOfficialCheck(snapshot: UsageSnapshot) {
    const checkedAt = detailString(snapshot, "lastOfficialCheckAt");
    return checkedAt === null ? null : formatTimestamp(checkedAt);
  }

  function profilePathValue(value: string | null) {
    return value ?? "";
  }

  function updateProfilePath(field: keyof AppConfig["browserProfiles"], event: Event) {
    const target = event.currentTarget;

    if (!(target instanceof HTMLInputElement)) {
      return;
    }

    const value = target.value.trim();
    config.browserProfiles[field] = value.length > 0 ? value : null;
  }

  function updateQuotaLabel(service: keyof AppConfig["localQuotas"], event: Event) {
    const target = event.currentTarget;

    if (!(target instanceof HTMLInputElement)) {
      return;
    }

    config.localQuotas[service].planLabel = target.value;
  }

  onMount(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    listen<UsageDisplayState>("usage://snapshots-updated", (event) => {
      snapshots = event.payload.snapshots;
    })
      .then((cleanup) => {
        if (cancelled) {
          cleanup();
          return;
        }

        unlisten = cleanup;
      })
      .catch(() => {});

    async function loadState() {
      try {
        const [loadedConfig, displayState] = await Promise.all([
          invoke<AppConfig>("get_app_config"),
          invoke<UsageDisplayState>("get_display_state"),
        ]);

        if (cancelled) {
          return;
        }

        config = loadedConfig;
        snapshots = displayState.snapshots;
      } catch (caught) {
        error = formatError(caught, "Running in browser preview mode");
      } finally {
        if (!cancelled) {
          loading = false;
        }
      }
    }

    void loadState();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

  async function saveSettings() {
    saving = true;
    statusMessage = null;

    try {
      config = await invoke<AppConfig>("update_app_config", { config });
      snapshots = (await invoke<UsageDisplayState>("get_display_state")).snapshots;
      error = null;
      statusMessage = "Settings saved";
    } catch (caught) {
      error = formatError(caught, "Settings are only persisted in the app");
    } finally {
      saving = false;
    }
  }

  async function openOfficialPage(service: Service) {
    openingService = service;
    statusMessage = null;

    try {
      await invoke<OfficialUsagePage>("open_official_usage_page", { service });
      error = null;
      statusMessage = `Opened ${serviceLabels[service]} official usage page`;
    } catch (caught) {
      error = formatError(caught, `Could not open ${serviceLabels[service]} usage page`);
    } finally {
      openingService = null;
    }
  }

  async function refreshNow() {
    refreshing = true;
    statusMessage = null;

    try {
      const displayState = await invoke<UsageDisplayState>("refresh_usage");
      snapshots = displayState.snapshots;
      error = null;
      statusMessage = "Usage refreshed";
    } catch (caught) {
      error = formatError(caught, "Could not refresh usage");
    } finally {
      refreshing = false;
    }
  }

  async function refreshOfficialUsage(service: Service) {
    refreshingOfficial = service;
    statusMessage = null;

    try {
      const displayState = await invoke<UsageDisplayState>("refresh_provider", {
        service,
        source: "web",
      });
      snapshots = displayState.snapshots;
      error = null;
      statusMessage = `Official ${serviceLabels[service]} usage refreshed`;
    } catch (caught) {
      error = formatError(caught, `Could not refresh official ${serviceLabels[service]} usage`);
    } finally {
      refreshingOfficial = null;
    }
  }

  async function clearSnapshotCache() {
    if (!confirm("Clear cached usage snapshots?")) {
      return;
    }

    clearingSnapshots = true;
    statusMessage = null;

    try {
      const displayState = await invoke<UsageDisplayState>("clear_cached_snapshots");
      snapshots = displayState.snapshots;
      error = null;
      statusMessage = "Cached usage snapshots cleared";
    } catch (caught) {
      error = formatError(caught, "Could not clear cached usage");
    } finally {
      clearingSnapshots = false;
    }
  }

  async function showLogLocation() {
    locatingLogs = true;
    statusMessage = null;

    try {
      const location = await invoke<LogLocation>("get_log_location");
      const state = location.exists ? "created" : "not created yet";
      error = null;
      statusMessage = `Log file: ${location.path} (${state}). Policy: ${location.redactionPolicy}`;
    } catch (caught) {
      error = formatError(caught, "Could not read log location");
    } finally {
      locatingLogs = false;
    }
  }

  async function resetProviderSession(service: Service) {
    if (!confirm(`Reset the app-owned ${serviceLabels[service]} browser session data?`)) {
      return;
    }

    clearingProfile = service;
    statusMessage = null;

    try {
      const result = await invoke<ClearedProviderProfile>("reset_provider_session", { service });
      error = null;
      statusMessage = result.cleared
        ? `Reset ${serviceLabels[service]} browser session`
        : `${serviceLabels[service]} browser session was already clear`;
    } catch (caught) {
      error = formatError(caught, `Could not reset ${serviceLabels[service]} browser session`);
    } finally {
      clearingProfile = null;
    }
  }
</script>

<main class="shell" style={`--brand-pattern: url(${patternUrl});`}>
  <section class="hero">
    <div class="brand-row">
      <img class="brand-mark" src={logoUrl} alt="ForgeGauge logo mark" />
      <img class="brand-lockup" src={lockupUrl} alt="ForgeGauge, Pickforge AI Usage Tray" />
    </div>
    <p class="summary">Track Codex and Claude Code usage from a privacy-conscious desktop tray.</p>
    <img class="hero-art" src={heroArtUrl} alt="Abstract ForgeGauge usage gauge artwork" />
  </section>

  <section class="cards" aria-label="Usage snapshots">
    {#if snapshots.length === 0}
      <article class="usage-card empty">
        <h2>No services enabled</h2>
        <p>Enable Codex or Claude Code in settings to show usage snapshots.</p>
      </article>
    {/if}

    {#each snapshots as snapshot}
      <article class={`usage-card ${serviceTone[snapshot.service]}`}>
        <div class="usage-header">
          <div class="service-title">
            <img src={serviceIcons[snapshot.service]} alt="" aria-hidden="true" />
            <h2>{serviceLabels[snapshot.service]}</h2>
          </div>
          <span>{confidenceLabels[snapshot.confidence]}</span>
        </div>

        <div class="gauge-row">
          <div
            class="gauge"
            style={`--value: ${snapshot.remainingPercent ?? 0};`}
            aria-label={`${serviceLabels[snapshot.service]} remaining usage`}
          >
            <strong>{formatPercent(snapshot.remainingPercent)}</strong>
            <small>remaining</small>
          </div>

          <dl>
            <div>
              <dt>Source</dt>
              <dd>{sourceLabels[snapshot.source]}</dd>
            </div>
            <div>
              <dt>Updated</dt>
              <dd>{formatTimestamp(snapshot.lastUpdated)}</dd>
            </div>
            {#if lastOfficialCheck(snapshot)}
              <div>
                <dt>Official</dt>
                <dd>{lastOfficialCheck(snapshot)}</dd>
              </div>
            {/if}
          </dl>
        </div>

        {#if snapshotIsStale(snapshot)}
          <p class="snapshot-note">Stale data</p>
        {/if}

        {#if providerStatusMessage(snapshot)}
          <p class="snapshot-note">{providerStatusMessage(snapshot)}</p>
        {/if}

        {#if localActivitySummary(snapshot)}
          <p class="activity-summary">{localActivitySummary(snapshot)}</p>
        {/if}

        <div class="card-actions">
          <button
            class="secondary-button"
            type="button"
            disabled={!config.providers.webEnabled || refreshingOfficial === snapshot.service}
            aria-label={`Refresh official ${serviceLabels[snapshot.service]} usage`}
            onclick={() => refreshOfficialUsage(snapshot.service)}
          >
            {refreshingOfficial === snapshot.service ? "Refreshing..." : "Refresh official"}
          </button>
          <button
            class="secondary-button"
            type="button"
            disabled={openingService === snapshot.service}
            aria-label={`Open official ${serviceLabels[snapshot.service]} usage page`}
            onclick={() => openOfficialPage(snapshot.service)}
          >
            {openingService === snapshot.service ? "Opening..." : "Open official page"}
          </button>
        </div>
      </article>
    {/each}
  </section>

  <section class="settings-panel" aria-label="Settings">
    <div>
      <p class="eyebrow">Settings</p>
      <h2>Provider controls</h2>
    </div>

    <div class="settings-grid">
      <label>
        <input type="checkbox" bind:checked={config.enabledServices.codex} />
        Codex
      </label>
      <label>
        <input type="checkbox" bind:checked={config.enabledServices.claude} />
        Claude Code
      </label>
      <label>
        <input type="checkbox" bind:checked={config.providers.localEnabled} />
        Local providers
      </label>
      <label>
        <input type="checkbox" bind:checked={config.providers.webEnabled} />
        Experimental web providers
      </label>
      <label>
        <input type="checkbox" bind:checked={config.autostart.enabled} />
        Start at login
      </label>
    </div>

    <div class="number-grid">
      <label>
        Local refresh
        <input type="number" min="30" max="60" bind:value={config.intervals.localSeconds} />
      </label>
      <label>
        Web refresh
        <input
          type="number"
          min="15"
          max="60"
          bind:value={config.intervals.webMinutes}
          disabled={!config.providers.webEnabled}
        />
      </label>
      <label>
        Web cooldown
        <input
          type="number"
          min="60"
          bind:value={config.intervals.manualWebRefreshCooldownSeconds}
          disabled={!config.providers.webEnabled}
        />
      </label>
      <label>
        Tray switch
        <input type="number" min="5" max="10" bind:value={config.intervals.gaugeSwitchSeconds} />
      </label>
      <label>
        Low threshold
        <input type="number" min="1" max="100" bind:value={config.lowUsageThreshold} />
      </label>
    </div>

    <div class="path-grid" aria-label="Browser profile paths">
      <label>
        Profile root
        <input
          type="text"
          autocomplete="off"
          spellcheck="false"
          placeholder="Default app data path"
          value={profilePathValue(config.browserProfiles.rootPath)}
          oninput={(event) => updateProfilePath("rootPath", event)}
          disabled={!config.providers.webEnabled}
        />
      </label>
      <label>
        Codex profile
        <input
          type="text"
          autocomplete="off"
          spellcheck="false"
          placeholder="Default under root"
          value={profilePathValue(config.browserProfiles.codexPath)}
          oninput={(event) => updateProfilePath("codexPath", event)}
          disabled={!config.providers.webEnabled}
        />
      </label>
      <label>
        Claude profile
        <input
          type="text"
          autocomplete="off"
          spellcheck="false"
          placeholder="Default under root"
          value={profilePathValue(config.browserProfiles.claudePath)}
          oninput={(event) => updateProfilePath("claudePath", event)}
          disabled={!config.providers.webEnabled}
        />
      </label>
    </div>

    <div class="quota-grid" aria-label="Local quota calibration">
      <div class="quota-group">
        <label class="quota-enabled">
          <input type="checkbox" bind:checked={config.localQuotas.codex.enabled} />
          Codex calibration
        </label>
        <div class="quota-fields">
          <label>
            Codex plan
            <input
              type="text"
              autocomplete="off"
              spellcheck="false"
              placeholder="Optional"
              value={config.localQuotas.codex.planLabel}
              oninput={(event) => updateQuotaLabel("codex", event)}
            />
          </label>
          <label>
            Token limit
            <input type="number" min="0" step="1" bind:value={config.localQuotas.codex.limit} />
          </label>
          <label>
            Window hours
            <input
              type="number"
              min="1"
              max="744"
              step="1"
              bind:value={config.localQuotas.codex.windowHours}
            />
          </label>
        </div>
      </div>

      <div class="quota-group">
        <label class="quota-enabled">
          <input type="checkbox" bind:checked={config.localQuotas.claude.enabled} />
          Claude calibration
        </label>
        <div class="quota-fields">
          <label>
            Claude plan
            <input
              type="text"
              autocomplete="off"
              spellcheck="false"
              placeholder="Optional"
              value={config.localQuotas.claude.planLabel}
              oninput={(event) => updateQuotaLabel("claude", event)}
            />
          </label>
          <label>
            Token limit
            <input type="number" min="0" step="1" bind:value={config.localQuotas.claude.limit} />
          </label>
          <label>
            Window hours
            <input
              type="number"
              min="1"
              max="744"
              step="1"
              bind:value={config.localQuotas.claude.windowHours}
            />
          </label>
        </div>
      </div>
    </div>

    <button class="save-button" type="button" disabled={saving} onclick={saveSettings}>
      {saving ? "Saving…" : "Save settings"}
    </button>

    <div class="maintenance-grid" aria-label="Maintenance actions">
      <button class="secondary-button" type="button" disabled={refreshing} onclick={refreshNow}>
        {refreshing ? "Refreshing..." : "Refresh now"}
      </button>
      <button
        class="secondary-button"
        type="button"
        disabled={clearingSnapshots}
        onclick={clearSnapshotCache}
      >
        {clearingSnapshots ? "Clearing..." : "Clear cache"}
      </button>
      <button
        class="secondary-button"
        type="button"
        disabled={locatingLogs}
        onclick={showLogLocation}
      >
        {locatingLogs ? "Checking..." : "Show log location"}
      </button>
      <button
        class="secondary-button danger"
        type="button"
        disabled={clearingProfile === "codex"}
        onclick={() => resetProviderSession("codex")}
      >
        {clearingProfile === "codex" ? "Resetting..." : "Reset Codex session"}
      </button>
      <button
        class="secondary-button danger"
        type="button"
        disabled={clearingProfile === "claude"}
        onclick={() => resetProviderSession("claude")}
      >
        {clearingProfile === "claude" ? "Resetting..." : "Reset Claude session"}
      </button>
    </div>
  </section>

  {#if loading}
    <p class="status">Loading local ForgeGauge state…</p>
  {:else if statusMessage}
    <p class="status">{statusMessage}</p>
  {:else if error}
    <p class="status muted">{error}</p>
  {/if}
</main>
