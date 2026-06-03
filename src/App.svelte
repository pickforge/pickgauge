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
    type AppConfig,
    type UsageDisplayState,
    type UsageSnapshot,
  } from "./lib/usage";

  let config = $state<AppConfig>(defaultConfig);
  let snapshots = $state<UsageSnapshot[]>(fallbackSnapshots);
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let statusMessage = $state<string | null>(null);

  const serviceLabels: Record<UsageSnapshot["service"], string> = {
    codex: "Codex",
    claude: "Claude Code",
  };

  const serviceTone: Record<UsageSnapshot["service"], string> = {
    codex: "codex",
    claude: "claude",
  };

  const serviceIcons: Record<UsageSnapshot["service"], string> = {
    codex: trayCodexUrl,
    claude: trayClaudeUrl,
  };

  function formatPercent(value: number | null) {
    return value === null ? "Unknown" : `${Math.round(value)}%`;
  }

  function formatError(caught: unknown, fallback: string) {
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
          <span>{snapshot.confidence}</span>
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
              <dd>{snapshot.source}</dd>
            </div>
            <div>
              <dt>Updated</dt>
              <dd>{formatTimestamp(snapshot.lastUpdated)}</dd>
            </div>
          </dl>
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
    </div>

    <div class="number-grid">
      <label>
        Local refresh
        <input type="number" min="30" max="60" bind:value={config.intervals.localSeconds} />
      </label>
      <label>
        Web refresh
        <input type="number" min="15" max="60" bind:value={config.intervals.webMinutes} />
      </label>
      <label>
        Web cooldown
        <input
          type="number"
          min="60"
          bind:value={config.intervals.manualWebRefreshCooldownSeconds}
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
        />
      </label>
    </div>

    <button class="save-button" type="button" disabled={saving} onclick={saveSettings}>
      {saving ? "Saving…" : "Save settings"}
    </button>
  </section>

  {#if loading}
    <p class="status">Loading local ForgeGauge state…</p>
  {:else if statusMessage}
    <p class="status">{statusMessage}</p>
  {:else if error}
    <p class="status muted">{error}</p>
  {/if}
</main>
