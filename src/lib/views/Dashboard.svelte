<script lang="ts">
  import { onMount } from "svelte";
  import ArrowSquareOut from "phosphor-svelte/lib/ArrowSquareOut";
  import ArrowsClockwise from "phosphor-svelte/lib/ArrowsClockwise";
  import SignIn from "phosphor-svelte/lib/SignIn";
  import { api, desktopApiAvailable, type DailyTokenUsage } from "../api";
  import Bars from "../components/Bars.svelte";
  import QuotaMeter from "../components/QuotaMeter.svelte";
  import {
    confidenceLabels,
    detailString,
    formatTimestamp,
    localActivitySummary,
    loginPromptVisible,
    serviceLabels,
    snapshotSourceLabel,
    snapshotIsStale,
    webProviderControlState,
  } from "../display";
  import {
    providerStatusMessage,
    providerStatusKind,
    usageWindows,
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

  type QuotaRow = {
    key: "five-hour" | "week" | "fable";
    label: string;
    remainingPercent: number | null;
    low: boolean;
    resetLabel: string | null;
  };

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
    if (value >= 1_000_000_000) {
      return `${(value / 1_000_000_000).toFixed(1)}B`;
    }
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

  function formatReset(resetAt: string | null): string | null {
    if (!resetAt) return null;
    const target = new Date(resetAt).getTime();
    if (Number.isNaN(target)) return null;
    const diffMs = target - Date.now();
    if (diffMs <= 0) return "soon";
    const minutes = Math.round(diffMs / 60000);
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.round(minutes / 60);
    if (hours < 24) return `${hours}h`;
    const days = Math.round(hours / 24);
    return `${days}d`;
  }

  function windowsFor(snapshot: UsageSnapshot): QuotaRow[] {
    const { fiveHour, week, fable } = usageWindows(snapshot);
    const low = (remaining: number | null) =>
      remaining !== null && remaining <= config.lowUsageThreshold;
    const rows: QuotaRow[] = [];
    if (fiveHour) {
      rows.push({
        key: "five-hour",
        label: "5-hour",
        remainingPercent: fiveHour.remainingPercent,
        low: low(fiveHour.remainingPercent),
        resetLabel: formatReset(fiveHour.resetAt),
      });
    }
    if (week) {
      rows.push({
        key: "week",
        label: "Weekly",
        remainingPercent: week.remainingPercent,
        low: low(week.remainingPercent),
        resetLabel: formatReset(week.resetAt),
      });
    }
    if (snapshot.service === "claude") {
      rows.push({
        key: "fable",
        label: "Fable",
        remainingPercent: fable?.remainingPercent ?? null,
        low: low(fable?.remainingPercent ?? null),
        resetLabel: formatReset(fable?.resetAt ?? null),
      });
    }
    return rows;
  }

  const focalWindowKey = $derived.by(() => {
    let focal: { key: string; remaining: number } | null = null;

    for (const snapshot of snapshots) {
      for (const row of windowsFor(snapshot)) {
        if (row.remainingPercent === null) continue;

        if (focal === null) {
          focal = {
            key: `${snapshot.service}:${row.key}`,
            remaining: row.remainingPercent,
          };
          continue;
        }

        if (row.remainingPercent < focal.remaining) {
          focal = {
            key: `${snapshot.service}:${row.key}`,
            remaining: row.remainingPercent,
          };
        }
      }
    }

    return focal?.key ?? null;
  });

  function planOnly(snapshot: UsageSnapshot) {
    if (snapshot.remainingPercent !== null) {
      return null;
    }
    return detailString(snapshot, "plan");
  }

  function serviceStateMessage(snapshot: UsageSnapshot) {
    return [
      providerStatusMessage(snapshot) ?? localActivitySummary(snapshot),
      snapshotIsStale(snapshot) ? "Stale data" : null,
    ]
      .filter((message): message is string => message !== null)
      .join(" · ");
  }

  function billingPeriodEnd(snapshot: UsageSnapshot) {
    const value = snapshot.details.billingPeriodEnd;
    return typeof value === "string" && value.length > 0 ? formatTimestamp(value) : null;
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
      <p class="eyebrow ember pf-eyebrow-row"><span class="pf-eyebrow-tick"></span>§ 01 · Live gauges</p>
      <h2>Remaining usage</h2>
    </div>
    <button class="btn btn-sm" type="button" disabled={refreshing} onclick={refreshNow}>
      <span class="btn-icon" class:spinning={refreshing}>
        <ArrowsClockwise size={15} />
      </span>
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
            <div class="service-identity">
              <span class={`status-dot ${providerStatusKind(snapshot)}`} aria-hidden="true"></span>
              <h3>{serviceLabels[snapshot.service]}</h3>
            </div>
              <div class="header-actions">
                <button
                  class="icon-btn"
                  type="button"
                  title={`Refresh official ${serviceLabels[snapshot.service]} usage`}
                  aria-label={`Refresh official ${serviceLabels[snapshot.service]} usage`}
                  disabled={webControls.officialRefreshDisabled || refreshingOfficial === snapshot.service}
                  onclick={() => refreshOfficialUsage(snapshot.service)}
                >
                  <span class="btn-icon" class:spinning={refreshingOfficial === snapshot.service}>
                    <ArrowsClockwise size={13} />
                  </span>
                </button>
                <button
                  class="icon-btn"
                  type="button"
                  title={`Open ${serviceLabels[snapshot.service]} official usage page`}
                  aria-label={`Open official ${serviceLabels[snapshot.service]} usage page`}
                  disabled={openingService === snapshot.service}
                  onclick={() => openOfficialPage(snapshot.service)}
                >
                  <ArrowSquareOut size={13} />
                </button>
              </div>
          </header>

          {#if planOnly(snapshot)}
            <div class="plan-row">
              <span class="window-label">Plan</span>
              <strong>{planOnly(snapshot)}</strong>
              <span class="plan-note">Usage —</span>
            </div>
            {#if billingPeriodEnd(snapshot)}
              <p class="note muted">Billing period ends {billingPeriodEnd(snapshot)}</p>
            {/if}
          {:else}
            <div class="quota-list">
              {#each windowsFor(snapshot) as win (win.key)}
                <QuotaMeter
                  label={win.label}
                  value={win.remainingPercent}
                  resetLabel={win.resetLabel}
                  low={win.low}
                  focal={focalWindowKey === `${snapshot.service}:${win.key}`}
                  dimmed={providerStatusMessage(snapshot) !== null && win.remainingPercent === null}
                />
              {/each}
            </div>
          {/if}

          <div class="service-meta" title={`Updated ${formatTimestamp(snapshot.lastUpdated)}`}>
            <span>{snapshotSourceLabel(snapshot)}</span>
            <span aria-hidden="true">·</span>
            <span>{confidenceLabels[snapshot.confidence]}</span>
            <span aria-hidden="true">·</span>
            <span class="truncate">{formatTimestamp(snapshot.lastUpdated)}</span>
          </div>

          {#if snapshotIsStale(snapshot) || providerStatusMessage(snapshot) || localActivitySummary(snapshot)}
            <div
              class="service-state"
              class:warn={snapshotIsStale(snapshot) || providerStatusKind(snapshot) === "warn"}
              class:bad={providerStatusKind(snapshot) === "bad"}
            >
              <span>
                {serviceStateMessage(snapshot)}
              </span>
              {#if loginPromptVisible(snapshot)}
                <button
                  class="state-action"
                  type="button"
                  aria-label={`Start ${serviceLabels[snapshot.service]} login`}
                  disabled={webControls.startLoginDisabled || startingLogin === snapshot.service}
                  onclick={() => startProviderLogin(snapshot.service)}
                >
                  <SignIn size={13} />
                  {startingLogin === snapshot.service ? "Starting…" : "Start login"}
                </button>
              {/if}
            </div>
          {/if}
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

  .btn-icon {
    display: inline-flex;
  }

  .btn-icon.spinning {
    animation: spin 0.9s linear infinite;
  }

  .gauge-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(270px, 100%), 1fr));
    gap: 12px;
  }

  .service-card {
    display: flex;
    flex-direction: column;
    gap: 9px;
    min-height: 176px;
    padding: 14px 16px;
    animation: card-in 420ms var(--ease-forge) both;
  }

  .service-card:nth-child(2) {
    animation-delay: 40ms;
  }

  .service-card:nth-child(3) {
    animation-delay: 80ms;
  }

  .service-card:nth-child(4) {
    animation-delay: 120ms;
  }

  @keyframes card-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .service-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .service-identity,
  .header-actions,
  .service-meta,
  .service-state {
    display: flex;
    align-items: center;
  }

  .service-identity {
    min-width: 0;
    gap: 8px;
  }

  .service-head h3 {
    font-size: 15px;
    font-weight: 650;
    letter-spacing: -0.02em;
  }

  .status-dot {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: color-mix(in srgb, var(--text) 28%, transparent);
  }

  .status-dot.ok {
    background: var(--ok);
  }

  .status-dot.warn {
    background: var(--warn);
  }

  .status-dot.bad {
    background: var(--bad);
  }

  .header-actions {
    gap: 3px;
  }

  .icon-btn {
    display: inline-grid;
    width: 26px;
    height: 26px;
    padding: 0;
    place-items: center;
    border: 1px solid transparent;
    border-radius: 7px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    transition: color 180ms var(--ease-forge), background 180ms var(--ease-forge), border-color 180ms var(--ease-forge);
  }

  .icon-btn:hover:not(:disabled) {
    border-color: var(--hairline);
    background: var(--wash);
    color: var(--text);
  }

  .icon-btn:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--ember) 60%, transparent);
    outline-offset: 1px;
  }

  .icon-btn:disabled,
  .state-action:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .quota-list,
  .plan-row {
    min-height: 81px;
  }

  .quota-list {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0;
  }

  .plan-row {
    display: grid;
    grid-template-columns: 56px auto minmax(0, 1fr);
    align-items: center;
    gap: 10px;
    padding: 0 2px;
  }

  .plan-row strong {
    font-size: 17px;
    line-height: 1.2;
  }

  .plan-note {
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    text-align: right;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .window-label {
    font-family: var(--font-mono);
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    color: var(--muted);
  }

  .service-meta {
    gap: 5px;
    min-width: 0;
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.02em;
    white-space: nowrap;
  }

  .truncate {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .service-state {
    justify-content: space-between;
    gap: 10px;
    min-height: 22px;
    padding: 4px 7px;
    border-left: 2px solid var(--hairline-strong);
    background: var(--wash);
    color: var(--muted);
    font-size: 11px;
    animation: state-in 200ms var(--ease-forge) both;
  }

  .service-state.warn {
    border-left-color: var(--warn);
    color: var(--warn);
  }

  .service-state.bad {
    border-left-color: var(--bad);
    color: var(--bad);
  }

  @keyframes state-in {
    from {
      opacity: 0;
      transform: translateY(-3px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .state-action {
    display: inline-flex;
    min-height: 24px;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--ember);
    font-size: 11px;
    font-weight: 650;
    line-height: 16px;
    cursor: pointer;
    white-space: nowrap;
  }

  .state-action :global(svg) {
    display: block;
    flex: none;
  }

  .state-action:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--ember) 60%, transparent);
    outline-offset: 2px;
  }

  .note {
    font-size: 11px;
    color: var(--text);
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
    gap: 12px;
    margin-bottom: 12px;
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
    gap: 12px;
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
