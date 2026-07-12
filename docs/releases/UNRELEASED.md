# Unreleased

Working draft for the next PickGauge release. Keep this current while PRs land.
At release time, copy and polish it into the GitHub release description, then
reset this file.

## User-facing changes

- Added Claude's separate Fable weekly allowance to official usage readings.
- Reworked the dashboard into a compact four-provider board for Codex, Claude Code, Grok, and Ollama, with truthful unsupported/local-only states and reduced-motion-aware transitions.
- Corrected Codex window labels so disabled, missing, or invalid primary windows are not shown as five-hour quota.
- Grok consumer quota is now marked unsupported until xAI provides a permitted third-party quota API; PickGauge no longer reads Grok login data or automates grok.com.
- Ollama now reports only local daemon availability and version. PickGauge does not own Ollama cloud sign-in or claim cloud quota.
- Settings save and supported provider login work now run off the UI thread, keeping the app responsive while refreshes and browser launches continue.
- Fixed provider action alignment, removed the Settings grid's blank wells, and widened the floating capsule to fit all four provider rings.

- The float capsule's glow now fades out smoothly instead of being clipped
  into a hard rectangle by the window edge; the transparent margin around
  the capsule is click-through (#38).

## Internal/release changes

- Switched AppImage libwayland post-processing to `pickforge-tauri-release fix-appimage`.
- Release CI now caches Rust builds (`Swatinem/rust-cache`).
- Managed browser profiles and web refreshes are restricted to Codex and Claude Code.
- Grok and Ollama browser automation, harvested-session HTTP requests, and managed profile actions were removed.
- Claude web reads preserve available weekly and Fable quotas when the session meter is unavailable, while keeping fallback percentage labels fail-closed.

## Validation

### Tested

- Workflow YAML parse check:
  `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))"`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --all-targets`
- `bun run check`
- `bun run test` (74 frontend tests and 18 Playwright sidecar tests)
- `bun run test:browser-preview` (four providers across 1000px, 820px, 680px, and 390px widths, Settings column breakpoints, and the 252×56 four-ring capsule)
- Browser-rendered visual checks at 1000×700 and 820×600, including truthful Grok/Ollama states, Settings layout, and exact floating-capsule geometry.

### Not tested yet

- App build.
- Installer or updater flow.
- Platform smoke checks.
- `cargo fmt --check` (`rustfmt` is not installed in the current toolchain).

### Release blockers

- None known.
