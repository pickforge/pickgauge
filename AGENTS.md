# PickGauge

Linux/KDE tray app that shows how much Codex, Claude Code, Grok and Ollama quota is left. Tauri v2 with a Rust backend, Svelte 5 with Vite (no SvelteKit), Bun, plus a Playwright sidecar for the opt-in web readings.

```
bun install
bun run tauri dev      # tray-first: the main window starts hidden
bun run lint           # ESLint, frontend and scripts only
bun run check          # svelte-check
bun run test           # frontend + sidecar suites
cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked --all-targets
cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --locked --all-targets -- -D warnings
```

The Rust half is most of the app and none of the `bun` scripts touch it, so run both sides before calling something done. Run `bun run prepare:sidecar` after changing `sidecars/playwright/`. The `run` skill in `.agents/skills/run` explains how to launch an isolated copy for screenshots without touching your real tray.

Worth knowing:

- Privacy is the product. Tokens are read at refresh time, never stored or logged, and web reads stay opt-in in app-owned browser profiles. If you widen what the app reads or writes, update the README privacy section in the same PR.
- Releases bump the version in four places: `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`.
- This repo lints with ESLint, not oxlint.
- Only touch `landing-page/src/pages/products.ts` when install methods, platforms or positioning change.
