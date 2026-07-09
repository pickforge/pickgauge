---
name: run
description: Launch PickGauge in dev mode or an isolated headless lab mode to verify changes when asked to run the app, screenshot it, or confirm a change works in the real app.
---

# Run PickGauge

## Normal dev launch

Use only for an interactive desktop session where starting or focusing the normal app is intended.

```bash
bun install --frozen-lockfile
bun run tauri dev
```

The app is tray-first: its main window starts hidden. Click the floating capsule to open it.

The Linux Playwright sidecar is checked in at `src-tauri/binaries/`. If its source under
`sidecars/playwright/` changed, run `bun run prepare:sidecar` before launching.

## Isolated lab launch (Linux)

Use this for agent verification. Choose a free port; do not reuse the normal Vite port `1420`.
Start Vite separately, then disable Tauri's `beforeDevCommand` in the lab config.

```bash
bun install --frozen-lockfile
DISPLAY_NUM=93
PORT=1431 # replace with an unused port
Xvfb ":$DISPLAY_NUM" -screen 0 1440x1000x24 -nolisten tcp &
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 DISPLAY=":$DISPLAY_NUM" \
  xfwm4 --display=":$DISPLAY_NUM" --compositor=off &
bunx vite --host 127.0.0.1 --port "$PORT" --strictPort &
dbus-run-session -- env -u WAYLAND_DISPLAY GDK_BACKEND=x11 DISPLAY=":$DISPLAY_NUM" \
  bun run tauri dev --config "{\"identifier\":\"com.pickforge.pickgauge.labtest\",\"build\":{\"devUrl\":\"http://127.0.0.1:$PORT\",\"beforeDevCommand\":\"\"}}"
```

`DISPLAY` alone is unsafe: GDK otherwise prefers `WAYLAND_DISPLAY` and opens on the live desktop.
The private D-Bus session also keeps the lab tray registration away from the user's desktop.
The `.labtest` identifier prevents the single-instance app from focusing the running PickGauge.

The identifier is not a data-safety boundary: `config.rs` still reads the real
`com.pickforge.pickgauge` config path and may reference real profile paths. Do not change
Settings or touch destructive UI in a lab: clear-all, delete, reset, sign-out,
browser-profile clearing, or autostart.

## Verify and screenshot

Wait for `curl -f "http://127.0.0.1:$PORT/"` to succeed. The capsule is at `(64,64)`,
208×60; click its center to show the main window:

```bash
DISPLAY=":$DISPLAY_NUM" xdotool mousemove 168 94 click 1
import -display ":$DISPLAY_NUM" -window root /tmp/pickgauge-lab.png
```

`import` is ImageMagick. Inspect `/tmp/pickgauge-lab.png` before calling the UI verified.
If the persisted capsule is disabled, do not re-enable it in lab Settings.

## Cleanup

Stop each process with a PID from `pgrep`; run each cleanup command as its own Bash call.
Never put `pkill -f` in a compound command: it can match the wrapper shell and exit 144.

```bash
bash -c 'kill $(pgrep -f "[t]auri dev --config.*com\\.pickforge\\.pickgauge\\.labtest.*1431")'
bash -c 'kill $(pgrep -f "[v]ite --host 127.0.0.1 --port 1431")'
bash -c 'kill $(pgrep -f "[x]fwm4 --display=:93")'
bash -c 'kill $(pgrep -f "[X]vfb :93")'
```

Replace `1431` and `93` with the selected values. Check that no lab processes remain.
