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
If `sidecars/playwright/` changed, run `bun run prepare:sidecar` before launching.

## Isolated lab launch (Linux)

Use this for agent verification. The lab starts empty with every provider disabled; use a free display and port, never Vite's normal `1420`.

```bash
bun install --frozen-lockfile
REAL_HOME="$HOME"
REAL_CARGO_HOME="${CARGO_HOME:-$REAL_HOME/.cargo}"
REAL_RUSTUP_HOME="${RUSTUP_HOME:-$REAL_HOME/.rustup}"
LAB_HOME="$(mktemp -d /tmp/pickgauge-lab-home.XXXX)"
mkdir -p "$LAB_HOME/.config/com.pickforge.pickgauge"
printf '%s\n' '{"version":6,"enabledServices":{"codex":false,"claude":false,"grok":false,"ollama":false},"providers":{"localEnabled":false,"webEnabled":false,"cliEnabled":false},"autostart":{"enabled":false},"crashReports":false}' > "$LAB_HOME/.config/com.pickforge.pickgauge/config.json"
PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
unset DISPLAY_NUM
for n in $(seq 90 120); do [ ! -e "/tmp/.X11-unix/X$n" ] && DISPLAY_NUM=$n && break; done
: "${DISPLAY_NUM:?No free X display in 90-120}"
export DISPLAY_NUM PORT LAB_HOME
Xvfb ":$DISPLAY_NUM" -screen 0 1440x1000x24 -nolisten tcp &
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 DISPLAY=":$DISPLAY_NUM" \
  xfwm4 --display=":$DISPLAY_NUM" --compositor=off &
bunx vite --host 127.0.0.1 --port "$PORT" --strictPort &
dbus-run-session -- env -u WAYLAND_DISPLAY GDK_BACKEND=x11 DISPLAY=":$DISPLAY_NUM" \
  HOME="$LAB_HOME" XDG_CONFIG_HOME="$LAB_HOME/.config" XDG_DATA_HOME="$LAB_HOME/.local/share" \
  XDG_CACHE_HOME="$LAB_HOME/.cache" CARGO_HOME="$REAL_CARGO_HOME" RUSTUP_HOME="$REAL_RUSTUP_HOME" \
  bun run tauri dev --config "{\"identifier\":\"com.pickforge.pickgauge.labtest\",\"build\":{\"devUrl\":\"http://127.0.0.1:$PORT\",\"beforeDevCommand\":\"\"}}"
```

`DISPLAY` alone is unsafe: GDK otherwise prefers `WAYLAND_DISPLAY` and opens on the live desktop; the private D-Bus session keeps lab tray registration away too.
The `.labtest` identifier prevents single-instance focus of the running app.

The default lab config disables autostart. If it is ever enabled, the Linux autostart entry is
`$LAB_HOME/.config/autostart/<app>.desktop`, never the user's profile. The blank provider state
is the expected screenshot; do not sign in or add credentials to a lab.

## Verify and screenshot

Wait for `curl -f "http://127.0.0.1:$PORT/"` to succeed. Under this Xvfb/xfwm4 lab,
click the capsule center at `(168,124)` to show the main window:

```bash
DISPLAY=":$DISPLAY_NUM" xdotool mousemove --sync 168 124 mousedown 1 sleep 0.2 mouseup 1
import -display ":$DISPLAY_NUM" -window root /tmp/pickgauge-lab.png
ls -la "$LAB_HOME/.config" "$LAB_HOME/.local/share"
```

`import` is ImageMagick. Inspect `/tmp/pickgauge-lab.png` before calling the UI verified.

## Cleanup

Stop each process with a PID from `pgrep`; run each cleanup command as its own Bash call.
Never put `pkill -f` in a compound command: it can match the wrapper shell and exit 144.

```bash
bash -c 'kill $(pgrep -f "[t]auri dev --config.*com\\.pickforge\\.pickgauge\\.labtest.*$PORT")'
bash -c 'kill $(pgrep -f "[v]ite --host 127.0.0.1 --port $PORT")'
bash -c 'kill $(pgrep -f "[x]fwm4 --display=:$DISPLAY_NUM")'
bash -c 'kill $(pgrep -f "[X]vfb :$DISPLAY_NUM")'
```

Keep `$LAB_HOME` until inspection is complete, then remove it if no longer needed.
