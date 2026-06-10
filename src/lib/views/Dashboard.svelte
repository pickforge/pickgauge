<script lang="ts">
  import { onMount } from "svelte";
  import ArrowSquareOut from "phosphor-svelte/lib/ArrowSquareOut";
  import ArrowsClockwise from "phosphor-svelte/lib/ArrowsClockwise";
  import SignIn from "phosphor-svelte/lib/SignIn";
  import { api, desktopApiAvailable, type DailyTokenUsage } from "../api";
  import Bars from "../components/Bars.svelte";
  import Gauge from "../components/Gauge.svelte";
  import {
    confidenceLabels,
    formatTimestamp,
    lastOfficialCheck,
    localActivitySummary,
    loginPromptVisible,
    serviceLabels,
    snapshotIsStale,
    sourceLabels,
    webProviderControlState,
  } from "../display";
  import {
    providerStatusMessage,
    type AppConfig,
    type Service,
    type UsageSnapshot,
  } from "../usage";

  let {
    snapshots,
    config,
    setStatus,
  }: {
    snapshots: UsageSnapshot[];
    config: AppConfig;
    setStatus: (message: string | null, error?: boolean) => void;
  } = $props();

  const DAILY_RANGE = 14;

  let daily = $state<{ codex: DailyTokenUsage[]; claude: DailyTokenUsage[] }>({
    codex: [],
    claude: [],
  });
  let refreshing = $state(false);
  let refreshingOfficial = $state<Service | null>(null);
  let startingLogin = $state<Service | null>(null);
  let openingService = $state<Service | null>(null);

  const webControls = $derived(webProviderControlState(config));

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

  function formatTokens(value: number) {
    if (value >= 1_000_000) {
      return `${(value / 1_000_000).toFixed(1)}M`;
    }
    if (value >= 1_000) {
      return `${(value / 1_000).toFixed(value >= 100_000 ? 0 : 1)}k`;
    }
    return `${value}`;
  }

  function lastDays(count: number): string[] {
    const days: string[] = [];
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local computation, not reactive state
    const cursor = new Date();
    cursor.setDate(cursor.getDate() - (count - 1));

    for (let index = 0; index < count; index += 1) {
      days.push(
        `${cursor.getFullYear()}-${String(cursor.getMonth() + 1).padStart(2, "0")}-${String(
          cursor.getDate(),
        ).padStart(2, "0")}`,
      );
      cursor.setDate(cursor.getDate() + 1);
    }

    return days;
  }

  function chartItems(usage: DailyTokenUsage[]) {
    const byDay = new Map(usage.map((entry) => [entry.day, entry]));

    return lastDays(DAILY_RANGE).map((day) => {
      const entry = byDay.get(day);
      const weekday = new Date(`${day}T12:00:00`).toLocaleDateString(undefined, {
        weekday: "narrow",
      });

      return {
        key: day,
        label: weekday,
        value: entry?.tokens ?? 0,
        title: `${day}: ${(entry?.tokens ?? 0).toLocaleString()} tokens`,
      };
    });
  }

  const todayKey = $derived(lastDays(1)[0]);
  const allDaily = $derived([...daily.codex, ...daily.claude]);
  const tokensToday = $derived(
    allDaily.filter((entry) => entry.day === todayKey).reduce((sum, entry) => sum + entry.tokens, 0),
  );
  const tokensRange = $derived(allDaily.reduce((sum, entry) => sum + entry.tokens, 0));
  const activityToday = $derived(
    allDaily
      .filter((entry) => entry.day === todayKey)
      .reduce((sum, entry) => sum + entry.activity, 0),
  );
  const activeDays = $derived(new Set(allDaily.map((entry) => entry.day)).size);

  function snapshotLow(snapshot: UsageSnapshot) {
    return (
      snapshot.remainingPercent !== null && snapshot.remainingPercent <= config.lowUsageThreshold
    );
  }

  async function loadDaily() {
    if (!desktopApiAvailable()) {
      return;
    }

    try {
      const report = await api.getLocalDailyUsage(DAILY_RANGE);
      daily = { codex: report.codex, claude: report.claude };
    } catch {
      // Local activity files may be absent; the chart shows an empty state.
    }
  }

  async function refreshNow() {
    if (!desktopApiAvailable()) {
      setStatus("Usage refresh is available in the desktop app", true);
      return;
    }

    refreshing = true;

    try {
      await api.refreshUsage();
      await loadDaily();
      setStatus("Usage refreshed");
    } catch (caught) {
      setStatus(formatError(caught, "Could not refresh usage"), true);
    } finally {
      refreshing = false;
    }
  }

  async function refreshOfficialUsage(service: Service) {
    if (!desktopApiAvailable()) {
      setStatus(`Official ${serviceLabels[service]} usage refreshes in the desktop app`, true);
      return;
    }

    refreshingOfficial = service;

    try {
      const displayState = await api.refreshProvider(service, "web");
      const refreshed = displayState.snapshots.find((snapshot) => snapshot.service === service);
      const providerMessage = refreshed ? providerStatusMessage(refreshed) : null;
      setStatus(
        providerMessage
          ? `Official ${serviceLabels[service]}: ${providerMessage}`
          : `Official ${serviceLabels[service]} usage refreshed`,
      );
    } catch (caught) {
      setStatus(formatError(caught, `Could not refresh official ${serviceLabels[service]} usage`), true);
    } finally {
      refreshingOfficial = null;
    }
  }

  async function startProviderLogin(service: Service) {
    if (!desktopApiAvailable()) {
      setStatus(`${serviceLabels[service]} login starts from the desktop app`, true);
      return;
    }

    startingLogin = service;

    try {
      const login = await api.startProviderLogin(service);
      const messages: Record<string, string> = {
        already_authenticated: `${serviceLabels[service]} already logged in`,
        login_required: `${serviceLabels[service]} login required`,
        launched: `Started ${serviceLabels[service]} login`,
        preflight_unavailable: `Could not verify ${serviceLabels[service]} login state`,
      };
      setStatus(messages[login.status] ?? `${serviceLabels[service]} login state unknown`);
    } catch (caught) {
      setStatus(formatError(caught, `Could not start ${serviceLabels[service]} login`), true);
    } finally {
      startingLogin = null;
    }
  }

  async function openOfficialPage(service: Service) {
    if (!desktopApiAvailable()) {
      setStatus(`${serviceLabels[service]} usage opens from the desktop app`, true);
      return;
    }

    openingService = service;

    try {
      await api.openOfficialUsagePage(service);
      setStatus(`Opened ${serviceLabels[service]} official usage page`);
    } catch (caught) {
      setStatus(formatError(caught, `Could not open ${serviceLabels[service]} usage page`), true);
    } finally {
      openingService = null;
    }
  }

  function serviceDaily(service: Service) {
    return service === "codex" ? daily.codex : daily.claude;
  }

  onMount(() => {
    void loadDaily();
  });
</script>

<section aria-label="Live gauges">
  <header class="section-head fade-up">
    <div>
      <p class="eyebrow ember">§ 01 · Live gauges</p>
      <h2>Remaining usage</h2>
    </div>
    <button class="btn btn-primary" type="button" disabled={refreshing} onclick={refreshNow}>
      <ArrowsClockwise size={15} />
      {refreshing ? "Refreshing…" : "Refresh now"}
    </button>
  </header>

  {#if snapshots.length === 0}
    <div class="card empty-card fade-up">
      <h3>No services enabled</h3>
      <p class="muted">Enable Codex or Claude Code in Settings to start watching usage.</p>
    </div>
  {:else}
    <div class="gauge-grid fade-up">
      {#each snapshots as snapshot (snapshot.service)}
        <article class="card service-card usage-card">
          <header class="service-head">
            <h3>{serviceLabels[snapshot.service]}</h3>
            <span
              class="pill"
              class:ember={snapshot.confidence === "high" || snapshot.confidence === "medium"}
            >
              {confidenceLabels[snapshot.confidence]}
            </span>
          </header>

          <div class="gauge-row">
            <Gauge value={snapshot.remainingPercent} low={snapshotLow(snapshot)} />

            <dl class="meta">
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
            <p class="note warn-note">Stale data</p>
          {/if}
          {#if providerStatusMessage(snapshot)}
            <p class="note">{providerStatusMessage(snapshot)}</p>
          {/if}
          {#if localActivitySummary(snapshot)}
            <p class="note muted">{localActivitySummary(snapshot)}</p>
          {/if}

          <footer class="service-actions">
            <button
              class="btn small"
              type="button"
              aria-label={`Refresh official ${serviceLabels[snapshot.service]} usage`}
              disabled={webControls.officialRefreshDisabled || refreshingOfficial === snapshot.service}
              onclick={() => refreshOfficialUsage(snapshot.service)}
            >
              <ArrowsClockwise size={13} />
              {refreshingOfficial === snapshot.service ? "Refreshing…" : "Refresh official"}
            </button>
            {#if loginPromptVisible(snapshot)}
              <button
                class="btn small"
                type="button"
                aria-label={`Start ${serviceLabels[snapshot.service]} login`}
                disabled={webControls.startLoginDisabled || startingLogin === snapshot.service}
                onclick={() => startProviderLogin(snapshot.service)}
              >
                <SignIn size={13} />
                {startingLogin === snapshot.service ? "Starting…" : "Start login"}
              </button>
            {/if}
            <button
              class="btn small btn-ghost"
              type="button"
              aria-label={`Open official ${serviceLabels[snapshot.service]} usage page`}
              disabled={openingService === snapshot.service}
              onclick={() => openOfficialPage(snapshot.service)}
            >
              <ArrowSquareOut size={13} />
              Official page
            </button>
          </footer>
        </article>
      {/each}
    </div>
  {/if}
</section>

<section aria-label="Local activity">
  <header class="section-head fade-up">
    <h2>Local activity</h2>
    <span class="muted small-text">last {DAILY_RANGE} days · estimates from local files</span>
  </header>

  <div class="stat-grid fade-up">
    <div class="card stat">
      <span class="stat-value tabular">{formatTokens(tokensToday)}</span>
      <span class="stat-label">tokens today</span>
    </div>
    <div class="card stat">
      <span class="stat-value tabular">{formatTokens(tokensRange)}</span>
      <span class="stat-label">tokens · {DAILY_RANGE}d</span>
    </div>
    <div class="card stat">
      <span class="stat-value tabular">{activityToday.toLocaleString()}</span>
      <span class="stat-label">sessions today</span>
    </div>
    <div class="card stat">
      <span class="stat-value tabular">{activeDays}</span>
      <span class="stat-label">active days</span>
    </div>
  </div>

  <div class="chart-grid fade-up">
    {#each ["codex", "claude"] as service (service)}
      <div class="card chart-card">
        <header class="chart-head">
          <h4>{serviceLabels[service as Service]}</h4>
          <span class="muted tabular small-text">
            {formatTokens(serviceDaily(service as Service).reduce((sum, entry) => sum + entry.tokens, 0))} tokens
          </span>
        </header>
        {#if serviceDaily(service as Service).length === 0}
          <p class="muted small-text empty-chart">No local activity found yet.</p>
        {:else}
          <Bars items={chartItems(serviceDaily(service as Service))} height={96} />
        {/if}
      </div>
    {/each}
  </div>
</section>

<style>
  section + section {
    margin-top: 28px;
  }

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

  .small-text {
    font-size: 12px;
  }

  .gauge-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(330px, 100%), 1fr));
    gap: 14px;
  }

  .service-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px;
  }

  .service-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .service-head h3 {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .gauge-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 20px;
  }

  .meta {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    min-width: 0;
  }

  .meta div {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .meta dt {
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .meta dd {
    margin: 0;
    font-size: 12.5px;
  }

  .note {
    font-size: 12px;
    color: var(--text);
  }

  .warn-note {
    color: var(--warn);
  }

  .service-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: auto;
  }

  .empty-card {
    padding: 28px;
    text-align: center;
  }

  .empty-card h3 {
    margin-bottom: 6px;
    font-size: 15px;
  }

  .stat-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(150px, 100%), 1fr));
    gap: 14px;
    margin-bottom: 14px;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 16px 18px;
  }

  .stat-value {
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1;
  }

  .stat-label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .chart-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(330px, 100%), 1fr));
    gap: 14px;
  }

  .chart-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 18px;
  }

  .chart-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .chart-head h4 {
    font-size: 13px;
    font-weight: 600;
  }

  .empty-chart {
    padding: 24px 0;
    text-align: center;
  }
</style>
