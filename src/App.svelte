<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import heroArtUrl from "../assets/branding/hero-art.png";
  import lockupUrl from "../assets/branding/logo-lockup-on-dark.svg";
  import logoUrl from "../assets/branding/logo-mark.svg";
  import patternUrl from "../assets/branding/brand-pattern.svg";
  import trayClaudeUrl from "../assets/branding/tray-claude.svg";
  import trayCodexUrl from "../assets/branding/tray-codex.svg";
  import { fallbackSnapshots, type UsageSnapshot } from "./lib/usage";

  let snapshots = $state<UsageSnapshot[]>(fallbackSnapshots);
  let loading = $state(true);
  let error = $state<string | null>(null);

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

  onMount(async () => {
    try {
      snapshots = await invoke<UsageSnapshot[]>("get_usage_snapshots");
    } catch (caught) {
      error = caught instanceof Error ? caught.message : "Running in browser preview mode";
    } finally {
      loading = false;
    }
  });
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
              <dd>{snapshot.lastUpdated}</dd>
            </div>
          </dl>
        </div>
      </article>
    {/each}
  </section>

  {#if loading}
    <p class="status">Loading local ForgeGauge state…</p>
  {:else if error}
    <p class="status muted">{error}</p>
  {/if}
</main>
