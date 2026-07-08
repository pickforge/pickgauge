# AGENTS

Repo-local guide for agents working in PickGauge — local AI-usage visibility
from the tray (Tauri v2: Rust backend + sidecar, SvelteKit/Svelte 5 frontend,
bun).

## Commands

- `bun install` then `bun run tauri dev` to develop.
- `bun run check` type-checks the Svelte frontend (`build` alone is just
  `vite build`); `bun run test` runs the JS suite; `bun run test:coverage`
  enforces the frontend coverage ratchet; `cargo test --manifest-path
  src-tauri/Cargo.toml --locked --all-targets` covers the Rust side. Run these
  before calling work done.
- Write tests in the same PR as behavior changes. For bugs, start with a
  failing regression test when practical. For risky refactors, add
  characterization tests first.
- Do not lower coverage thresholds without explicit maintainer approval.
- Keep durable business/domain behavior in the existing Rust core or shared
  `src/lib` layers, not UI components. Do not add DDD ceremony.

## Invariants

- Privacy-first: provider tokens are read, never stored or logged; web reads
  are opt-in and use isolated profiles. Never widen what the app touches
  without updating README's privacy section.
- Follow the Pickforge design system: ember `#FF7A1A` accent, Geist/Geist Mono,
  tokens over raw values.

## Releasing

- Keep [`docs/releases/UNRELEASED.md`](docs/releases/UNRELEASED.md) current on
  PRs with user-facing or release-relevant changes. Track user-facing changes,
  internal/release changes, what was tested, what was not tested yet, and known
  blockers. At release time, copy and polish it into the GitHub release
  description, then reset the draft.
- Bump the version in `src-tauri/tauri.conf.json` and `package.json`, land on
  `main`, tag `vX.Y.Z`, push the tag. CI builds Linux/macOS/Windows bundles,
  signs the updater artifacts, and **auto-publishes** the release at the end
  of the workflow — make sure `main` is ready before tagging.
- The GitHub release description is the single source of release notes; polish
  it right after the workflow finishes. pickforge.dev/pickgauge shows the
  latest release via the GitHub API — no website change needed for a normal
  release.
- Only touch `landing-page` (`src/pages/products.ts`) when install methods,
  platforms, or positioning change.
## Pickforge workspace policy

This repo is part of the Pickforge workspace. Before substantial work, read `../AGENTS.md` (or `/home/dev/Projects/Pickforge/AGENTS.md`) and use the `plan-issue` workflow: GitHub Issues are the canonical plan/progress tracker; local todos are only a mirror. Link PRs to tracking issues and file follow-up issues for valid deferred review/CI problems.
