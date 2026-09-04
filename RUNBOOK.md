# Runbook: archive PickGauge and deprecate the npm package

For Elberte. Nothing here has been run by an agent. Run it after the retirement
PR is merged.

## Checklist before archiving

- [ ] The retirement PR is merged into `main`.
- [ ] The README banner is visible on the default branch:
      https://github.com/pickforge/pickgauge
- [ ] The last release still lists its assets (AppImage, installers, sigs,
      latest.json): https://github.com/pickforge/pickgauge/releases/tag/v0.3.0
- [ ] You are logged in to npm as a maintainer of `pickgauge` (`npm whoami`).

## Deprecate the npm package

Do this first, while the repository is still writable. The deprecation happens at
archive time, not before: until then `pickgauge` stays a normal package.

```sh
npm deprecate pickgauge@'*' 'PickGauge is retired. Pickforge Studio now focuses on Pickforge: https://pickforge.dev'
```

Existing installs keep working. The message shows on install and on the package
page. `npm deprecate pickgauge@'*' ''` clears it.

## Archive the repository

```sh
gh repo archive pickforge/pickgauge --yes
```

The repository becomes read-only. Code, issues and releases stay visible and the
release assets stay downloadable. `gh repo unarchive pickforge/pickgauge` undoes
it if needed.
