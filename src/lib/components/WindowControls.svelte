<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { controlOrder, hostPlatform, isTauri } from "../platform";
  import {
    closeWindow,
    minimizeWindow,
    readMaximized,
    toggleMaximizeWindow,
  } from "../windowChrome";

  const MAXIMIZED_CHECK_DELAY_MS = 120;

  const order = controlOrder(hostPlatform());
  let maximized = $state(false);

  onMount(() => {
    if (!isTauri()) {
      return;
    }

    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let unlisten: (() => void) | undefined;

    const refresh = async () => {
      const next = await readMaximized();
      if (!disposed) {
        maximized = next;
      }
    };

    const scheduleRead = () => {
      if (timer) {
        clearTimeout(timer);
      }
      timer = setTimeout(() => {
        timer = undefined;
        void refresh();
      }, MAXIMIZED_CHECK_DELAY_MS);
    };

    void refresh();
    getCurrentWindow()
      .onResized(scheduleRead)
      .then((off) => {
        if (disposed) {
          off();
        } else {
          unlisten = off;
        }
      })
      .catch(() => {});

    return () => {
      disposed = true;
      if (timer) {
        clearTimeout(timer);
      }
      unlisten?.();
    };
  });

  function minimize() {
    void minimizeWindow();
  }

  function toggleMax() {
    void toggleMaximizeWindow().then((value) => {
      maximized = value;
    });
  }

  function close() {
    void closeWindow();
  }
</script>

{#if isTauri()}
  <div class="pf-winctl" role="group" aria-label="Window controls">
    {#each order as kind (kind)}
      {#if kind === "minimize"}
        <button
          type="button"
          class="pf-winctl-btn"
          title="Minimize"
          aria-label="Minimize"
          onclick={minimize}
        >
          <svg
            class="pf-winctl-icon"
            width="10"
            height="10"
            viewBox="0 0 10 10"
            fill="none"
            stroke="currentColor"
            stroke-width="1.1"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M1.5 5h7" />
          </svg>
        </button>
      {:else if kind === "maximize"}
        <button
          type="button"
          class="pf-winctl-btn"
          title={maximized ? "Restore" : "Maximize"}
          aria-label={maximized ? "Restore" : "Maximize"}
          onclick={toggleMax}
        >
          <svg
            class="pf-winctl-icon"
            width="10"
            height="10"
            viewBox="0 0 10 10"
            fill="none"
            stroke="currentColor"
            stroke-width="1.1"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            {#if maximized}
              <path d="M3 3V1.6h5.4V7H7" />
              <rect x="1.5" y="3" width="5.5" height="5.5" rx="0.6" />
            {:else}
              <rect x="1.5" y="1.5" width="7" height="7" rx="0.6" />
            {/if}
          </svg>
        </button>
      {:else}
        <button
          type="button"
          class="pf-winctl-btn pf-winctl-btn--close"
          title="Close"
          aria-label="Close"
          onclick={close}
        >
          <svg
            class="pf-winctl-icon"
            width="10"
            height="10"
            viewBox="0 0 10 10"
            fill="none"
            stroke="currentColor"
            stroke-width="1.1"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M1.8 1.8l6.4 6.4M8.2 1.8l-6.4 6.4" />
          </svg>
        </button>
      {/if}
    {/each}
  </div>
{/if}
