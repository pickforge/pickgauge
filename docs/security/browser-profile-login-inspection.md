# Browser Profile Login Inspection Checklist

Use this checklist after a future managed browser login test. Record only sanitized pass/fail notes, service names, timestamps, profile labels, and artifact/build identifiers. Do not copy cookies, tokens, account identifiers, browser storage, page text, screenshots, or full local paths into notes.

## Preconditions

- [ ] The browser automation backend has been selected and approved.
- [ ] The test uses the current ForgeGauge build or AppImage path.
- [ ] Experimental web providers are enabled explicitly by the user.
- [ ] The Codex and Claude profiles are app-owned and marker-guarded.
- [ ] The Codex and Claude profiles are separate directories under the configured profile root.
- [ ] The default system browser profile is closed or ignored and is not imported.

## Login Run

- [ ] Launch the managed login flow for Codex using the app-owned Codex profile.
- [ ] Complete only normal manual login steps. Do not bypass MFA, CAPTCHA, bot checks, or interstitials.
- [ ] Quit the managed browser through the app flow.
- [ ] Launch the managed login flow for Claude using the app-owned Claude profile.
- [ ] Complete only normal manual login steps. Do not bypass MFA, CAPTCHA, bot checks, or interstitials.
- [ ] Quit the managed browser through the app flow.
- [ ] Restart ForgeGauge and verify both services still use their own app-owned profile directories.

## Saved-Credential Inspection

Inspect only file names, directory names, metadata, and browser preference keys needed to prove password saving is not present. Do not open, print, or copy cookie/session databases, local storage, IndexedDB values, token stores, account IDs, or authenticated page content.

- [ ] Run `npm --silent run smoke:auth-profile -- --codex-profile <codex-profile> --claude-profile <claude-profile> --require-usage --require-disabled-storage-preferences --require-no-credential-store-files` or the equivalent environment-variable form, and record only the sanitized JSON result.
- [ ] Codex profile contains no password-store database such as `Login Data`.
- [ ] Codex profile contains no password-store journal or sidecar file.
- [ ] Codex profile contains no autofill-store database such as `Web Data`.
- [ ] Codex profile contains no autofill-store journal or sidecar file.
- [ ] Codex profile preferences do not enable password manager saving or autofill saving.
- [ ] Claude profile contains no password-store database such as `Login Data`.
- [ ] Claude profile contains no password-store journal or sidecar file.
- [ ] Claude profile contains no autofill-store database such as `Web Data`.
- [ ] Claude profile contains no autofill-store journal or sidecar file.
- [ ] Claude profile preferences do not enable password manager saving or autofill saving.
- [ ] Cookie/session files remain inside only the service-specific app-owned profile directory.
- [ ] No profile file is a symlink to the default browser profile.
- [ ] No profile file or preference references the user's default browser profile path.

## Log Inspection

- [ ] Normal app logs contain only stable provider/status codes, service names, timestamps, and sanitized profile labels.
- [ ] Normal app logs do not contain cookies, tokens, auth headers, account identifiers, browser storage, page HTML, visible page text, screenshots, or raw browser error dumps.
- [ ] Any path shown to the user redacts the home directory as `~` unless it is an app-owned path required for direct user action.

## Evidence To Record

- [ ] Run `npm run smoke:preflight` and keep only the sanitized JSON output with the manual smoke notes.
- [ ] Date and local session type.
- [ ] OS and desktop session.
- [ ] ForgeGauge commit and artifact/build path.
- [ ] Selected browser automation backend and browser version.
- [ ] Sanitized profile labels for Codex and Claude.
- [ ] Pass/fail result for each checklist group.
- [ ] Any blocker status code encountered, without raw page or account content.
