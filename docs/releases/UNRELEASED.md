# Unreleased

Working draft for the next PickGauge release. Keep this current while PRs land.
At release time, copy and polish it into the GitHub release description, then
reset this file.

## User-facing changes

- Added Claude's separate Fable weekly allowance to official usage readings.
- Reworked the dashboard into a compact four-provider quota board for Codex, Claude Code, Grok, and Ollama, with responsive linear meters, clearer provider states, and reduced-motion-aware transitions.
- Double-clicking empty titlebar space now maximizes or restores the window.
- Added a headless `pickgauge usage --json` export for agents and scripts, plus a repository-canonical usage-routing skill.
- Added zero-setup Grok plan detection through the local Grok CLI login. PickGauge shows the active
  plan and billing-period end without reporting a usage percentage.
- Added zero-setup Ollama plan detection from the signed-in local daemon. Usage limits remain unavailable.
- Added opt-in Grok weekly usage gauges through an isolated browser profile. The existing Grok CLI plan is carried into the official usage reading.

- The float capsule's glow now fades out smoothly instead of being clipped
  into a hard rectangle by the window edge; the transparent margin around
  the capsule is click-through (#38).

## Internal/release changes

- Switched AppImage libwayland post-processing to `pickforge-tauri-release fix-appimage`.
- Release CI now caches Rust builds (`Swatinem/rust-cache`).
- Grok reads its CLI bearer without refreshing, storing, or writing it.
- Grok web reads use only the managed profile's `grok.com/rest/grok/credits` request and return sanitized weekly usage data; on-demand dollar credits are not read.
- Claude web reads preserve available weekly and Fable quotas when the session meter is unavailable, while keeping fallback percentage labels fail-closed.

## Validation

### Tested

- Workflow YAML parse check:
  `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))"`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --all-targets`
- `bun run check`
- `bun run test:coverage` (74 tests, including the titlebar double-click regression)
- `bun run test:browser-preview` (four providers across 1000px, 820px, 680px, and 390px widths and all preview states)
- PickLab visual and interaction checks at 1000×700, 820×600, and 680×600, including official usage and login-required states.

### Not tested yet

- App build.
- Installer or updater flow.
- Platform smoke checks.
- `cargo fmt --check` (`rustfmt` is not installed in the current toolchain).

### Release blockers

- None known.
