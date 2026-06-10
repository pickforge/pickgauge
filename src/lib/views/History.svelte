<script lang="ts">
  import { onMount } from "svelte";
  import { api, desktopApiAvailable, type DailyGaugeStat, type DailyTokenUsage } from "../api";
  import Bars from "../components/Bars.svelte";
  import { serviceLabels } from "../display";
  import type { Service } from "../usage";

  let {
    setStatus,
  }: {
    setStatus: (message: string | null, error?: boolean) => void;
  } = $props();

  type Range = "days" | "weeks" | "months";

  type PeriodRow = {
    key: string;
    label: string;
    codexTokens: number;
    claudeTokens: number;
    activity: number;
  };

  const RANGE_DAYS = 365;

  let range = $state<Range>("days");
  let loading = $state(true);
  let daily = $state<{ codex: DailyTokenUsage[]; claude: DailyTokenUsage[] }>({
    codex: [],
    claude: [],
  });
  let gaugeTrail = $state<{ codex: DailyGaugeStat[]; claude: DailyGaugeStat[] }>({
    codex: [],
    claude: [],
  });

  const rangeOptions: { id: Range; label: string }[] = [
    { id: "days", label: "Days" },
    { id: "weeks", label: "Weeks" },
    { id: "months", label: "Months" },
  ];

  function formatTokens(value: number) {
    if (value >= 1_000_000) {
      return `${(value / 1_000_000).toFixed(1)}M`;
    }
    if (value >= 1_000) {
      return `${(value / 1_000).toFixed(value >= 100_000 ? 0 : 1)}k`;
    }
    return `${value}`;
  }

  function isoWeekKey(day: string) {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local computation, not reactive state
    const date = new Date(`${day}T12:00:00Z`);
    const dayOfWeek = date.getUTCDay() || 7;
    date.setUTCDate(date.getUTCDate() + 4 - dayOfWeek);
    const yearStart = Date.UTC(date.getUTCFullYear(), 0, 1);
    const week = Math.ceil(((date.getTime() - yearStart) / 86_400_000 + 1) / 7);

    return `${date.getUTCFullYear()}-W${String(week).padStart(2, "0")}`;
  }

  function periodKey(day: string, grouping: Range) {
    if (grouping === "days") {
      return day;
    }
    if (grouping === "weeks") {
      return isoWeekKey(day);
    }
    return day.slice(0, 7);
  }

  function periodLabel(key: string, grouping: Range) {
    if (grouping === "months") {
      const [year, month] = key.split("-");
      return new Date(Number(year), Number(month) - 1, 1).toLocaleDateString(undefined, {
        month: "short",
        year: "numeric",
      });
    }
    if (grouping === "weeks") {
      return key;
    }
    return new Date(`${key}T12:00:00`).toLocaleDateString(undefined, {
      day: "2-digit",
      month: "short",
      year: "numeric",
    });
  }

  const periods = $derived.by(() => {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- scratch map inside a derived
    const buckets = new Map<string, PeriodRow>();

    for (const [service, entries] of [
      ["codex", daily.codex],
      ["claude", daily.claude],
    ] as const) {
      for (const entry of entries) {
        const key = periodKey(entry.day, range);
        let bucket = buckets.get(key);

        if (!bucket) {
          bucket = {
            key,
            label: periodLabel(key, range),
            codexTokens: 0,
            claudeTokens: 0,
            activity: 0,
          };
          buckets.set(key, bucket);
        }

        if (service === "codex") {
          bucket.codexTokens += entry.tokens;
        } else {
          bucket.claudeTokens += entry.tokens;
        }
        bucket.activity += entry.activity;
      }
    }

    return [...buckets.values()].sort((a, b) => b.key.localeCompare(a.key));
  });

  const chartLimit = $derived(range === "days" ? 30 : range === "weeks" ? 16 : 12);

  const chartRows = $derived([...periods].slice(0, chartLimit).reverse());

  function chartItems(service: Service) {
    return chartRows.map((row) => ({
      key: row.key,
      label:
        range === "days"
          ? row.key.slice(8)
          : range === "weeks"
            ? row.key.slice(5)
            : row.label.split(" ")[0],
      value: service === "codex" ? row.codexTokens : row.claudeTokens,
      title: `${row.label}: ${(service === "codex" ? row.codexTokens : row.claudeTokens).toLocaleString()} tokens`,
    }));
  }

  const totalTokens = $derived(
    periods.reduce((sum, row) => sum + row.codexTokens + row.claudeTokens, 0),
  );

  const trailItems = $derived.by(() => {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- scratch map inside a derived
    const merged = new Map<string, { min: number | null }>();

    for (const entries of [gaugeTrail.codex, gaugeTrail.claude]) {
      for (const stat of entries) {
        const current = merged.get(stat.day) ?? { min: null };
        if (stat.minRemainingPercent !== null) {
          current.min =
            current.min === null
              ? stat.minRemainingPercent
              : Math.min(current.min, stat.minRemainingPercent);
        }
        merged.set(stat.day, current);
      }
    }

    return [...merged.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .slice(-30)
      .map(([day, value]) => ({
        key: day,
        label: day.slice(8),
        value: value.min ?? 0,
        title: `${day}: lowest ${value.min === null ? "unknown" : `${Math.round(value.min)}%`} remaining`,
      }));
  });

  onMount(() => {
    if (!desktopApiAvailable()) {
      loading = false;
      return;
    }

    void (async () => {
      try {
        const [usageReport, historyReport] = await Promise.all([
          api.getLocalDailyUsage(RANGE_DAYS),
          api.getUsageHistory(RANGE_DAYS),
        ]);

        daily = { codex: usageReport.codex, claude: usageReport.claude };
        gaugeTrail = { codex: historyReport.codex, claude: historyReport.claude };
      } catch {
        setStatus("Could not load usage history", true);
      } finally {
        loading = false;
      }
    })();
  });
</script>

<section aria-label="Usage history">
  <header class="section-head fade-up">
    <div>
      <p class="eyebrow ember">§ 02 · Usage history</p>
      <h2>Look back</h2>
    </div>
    <div class="range-tabs" role="tablist" aria-label="History grouping">
      {#each rangeOptions as option (option.id)}
        <button
          class="range-tab"
          class:active={range === option.id}
          type="button"
          role="tab"
          aria-selected={range === option.id}
          onclick={() => (range = option.id)}
        >
          {option.label}
        </button>
      {/each}
    </div>
  </header>

  {#if loading}
    <div class="card empty-card fade-up">
      <p class="muted">Scanning local usage…</p>
    </div>
  {:else if periods.length === 0}
    <div class="card empty-card fade-up">
      <h3>No history yet</h3>
      <p class="muted">
        Local Codex and Claude Code activity will appear here as soon as usage files are found.
      </p>
    </div>
  {:else}
    <div class="chart-grid fade-up">
      {#each ["codex", "claude"] as service (service)}
        <div class="card chart-card">
          <header class="chart-head">
            <h4>{serviceLabels[service as Service]}</h4>
            <span class="muted tabular small-text">
              {formatTokens(
                periods.reduce(
                  (sum, row) => sum + (service === "codex" ? row.codexTokens : row.claudeTokens),
                  0,
                ),
              )} tokens total
            </span>
          </header>
          <Bars items={chartItems(service as Service)} height={110} />
        </div>
      {/each}
    </div>

    <div class="card table-card fade-up">
      <header class="table-head">
        <h4>Periods</h4>
        <span class="muted tabular small-text">{formatTokens(totalTokens)} tokens · all time scanned</span>
      </header>
      <div class="table" role="table" aria-label="Usage per period">
        <div class="row head" role="row">
          <span role="columnheader">Period</span>
          <span role="columnheader" class="num">Codex</span>
          <span role="columnheader" class="num">Claude Code</span>
          <span role="columnheader" class="num">Total</span>
          <span role="columnheader" class="num">Activity</span>
        </div>
        {#each periods.slice(0, 30) as row (row.key)}
          <div class="row" role="row">
            <span role="cell">{row.label}</span>
            <span role="cell" class="num tabular">{formatTokens(row.codexTokens)}</span>
            <span role="cell" class="num tabular">{formatTokens(row.claudeTokens)}</span>
            <span role="cell" class="num tabular total">{formatTokens(row.codexTokens + row.claudeTokens)}</span>
            <span role="cell" class="num tabular">{row.activity.toLocaleString()}</span>
          </div>
        {/each}
      </div>
    </div>

    <div class="card trail-card fade-up">
      <header class="chart-head">
        <h4>Gauge trail</h4>
        <span class="muted small-text">lowest remaining % per day · last 30 days</span>
      </header>
      {#if trailItems.length === 0}
        <p class="muted small-text empty-chart">
          The gauge trail builds while PickGauge runs with calibrated quotas or official readings.
        </p>
      {:else}
        <Bars items={trailItems} height={80} />
      {/if}
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

  .small-text {
    font-size: 12px;
  }

  .range-tabs {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    border: 1px solid var(--hairline);
    border-radius: var(--radius-pill);
    background: var(--surface-1);
  }

  .range-tab {
    height: 28px;
    padding: 0 14px;
    border: none;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition:
      background 0.3s var(--ease-forge),
      color 0.3s var(--ease-forge);
  }

  .range-tab:hover {
    color: var(--text);
  }

  .range-tab.active {
    background: color-mix(in srgb, var(--ember) 12%, transparent);
    color: var(--ember);
  }

  .chart-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(330px, 100%), 1fr));
    gap: 14px;
    margin-bottom: 14px;
  }

  .chart-card,
  .table-card,
  .trail-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 18px;
  }

  .trail-card {
    margin-top: 14px;
  }

  .chart-head,
  .table-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .chart-head h4,
  .table-head h4 {
    font-size: 13px;
    font-weight: 600;
  }

  .empty-card {
    padding: 28px;
    text-align: center;
  }

  .empty-card h3 {
    margin-bottom: 6px;
    font-size: 15px;
  }

  .empty-chart {
    padding: 18px 0;
    text-align: center;
  }

  .table {
    display: flex;
    flex-direction: column;
  }

  .row {
    display: grid;
    grid-template-columns: 1.6fr 1fr 1fr 1fr 1fr;
    gap: 10px;
    padding: 8px 4px;
    border-top: 1px solid var(--hairline);
    font-size: 12.5px;
  }

  .row.head {
    border-top: none;
    padding-top: 0;
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .num {
    text-align: right;
  }

  .total {
    color: var(--ember);
    font-weight: 600;
  }
</style>
