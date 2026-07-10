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

## Isolated lab setup (Linux)

Run this once. It creates an all-provider-off profile and persists the values in
`$PWD/.lab.env` for later shells — one lab per checkout; concurrent labs need
separate worktrees.

```bash
set -e
[ ! -f "$PWD/.lab.env" ] || { echo "lab state exists — clean up the previous lab first" >&2; false; }
bun install --frozen-lockfile
REAL_HOME="$HOME"; REAL_CARGO_HOME="${CARGO_HOME:-$REAL_HOME/.cargo}"; REAL_RUSTUP_HOME="${RUSTUP_HOME:-$REAL_HOME/.rustup}"
LAB_HOME="$(mktemp -d /tmp/pickgauge-lab-home.XXXX)"
mkdir -p "$LAB_HOME/.config/com.pickforge.pickgauge"
printf '%s\n' '{"version":6,"enabledServices":{"codex":false,"claude":false,"grok":false,"ollama":false},"providers":{"localEnabled":false,"webEnabled":false,"cliEnabled":false},"autostart":{"enabled":false},"crashReports":false}' > "$LAB_HOME/.config/com.pickforge.pickgauge/config.json"
command -v ss >/dev/null || { echo "ss (iproute2) required" >&2; false; }
for p in $(seq 1421 1499); do ss -ltnH "sport = :$p" | grep -q . || { PORT=$p; break; }; done
: "${PORT:?No free port in 1421-1499}"
unset DISPLAY_NUM; for n in $(seq 90 120); do [ ! -e "/tmp/.X11-unix/X$n" ] && DISPLAY_NUM=$n && break; done
: "${DISPLAY_NUM:?No free X display in 90-120}"
printf 'export PORT=%s\nexport DISPLAY_NUM=%s\nexport LAB_HOME=%s\nexport REAL_CARGO_HOME=%s\nexport REAL_RUSTUP_HOME=%s\n' "$PORT" "$DISPLAY_NUM" "$LAB_HOME" "$REAL_CARGO_HOME" "$REAL_RUSTUP_HOME" > "$PWD/.lab.env"
```

## Launch the lab

Run in a terminal and leave it open. Every later snippet starts by loading the same state.

```bash
source "$PWD/.lab.env"
set -e
Xvfb ":$DISPLAY_NUM" -screen 0 1440x1000x24 -nolisten tcp &
for _ in {1..50}; do [ -e "/tmp/.X11-unix/X$DISPLAY_NUM" ] && break; sleep 0.1; done
[ -e "/tmp/.X11-unix/X$DISPLAY_NUM" ] || { echo "Xvfb did not start" >&2; false; }
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 DISPLAY=":$DISPLAY_NUM" xfwm4 --display=":$DISPLAY_NUM" --compositor=off &
bunx vite --host 127.0.0.1 --port "$PORT" --strictPort &
dbus-run-session -- env -u WAYLAND_DISPLAY GDK_BACKEND=x11 DISPLAY=":$DISPLAY_NUM" \
  HOME="$LAB_HOME" XDG_CONFIG_HOME="$LAB_HOME/.config" XDG_DATA_HOME="$LAB_HOME/.local/share" XDG_CACHE_HOME="$LAB_HOME/.cache" \
  CARGO_HOME="$REAL_CARGO_HOME" RUSTUP_HOME="$REAL_RUSTUP_HOME" \
  bun run tauri dev --config "{\"identifier\":\"com.pickforge.pickgauge.labtest$DISPLAY_NUM\",\"build\":{\"devUrl\":\"http://127.0.0.1:$PORT\",\"beforeDevCommand\":\"\"}}"
```

`DISPLAY` alone is unsafe: GDK otherwise prefers `WAYLAND_DISPLAY` and opens on the live desktop; private D-Bus isolates the tray. The identifier suffix (`.labtest$DISPLAY_NUM`) prevents single-instance focus of the live app and of other agents' concurrent labs.
The lab profile disables providers and autostart. If enabled, autostart writes only to
`$LAB_HOME/.config/autostart/<app>.desktop`. Do not sign in or add credentials.

## Verify and screenshot

```bash
source "$PWD/.lab.env"
for _ in {1..50}; do curl -fsS "http://127.0.0.1:$PORT/" >/dev/null && break; sleep 0.1; done
curl -f "http://127.0.0.1:$PORT/"
DISPLAY=":$DISPLAY_NUM" xdotool mousemove --sync 168 124 mousedown 1 sleep 0.2 mouseup 1
sleep 1
import -display ":$DISPLAY_NUM" -window root /tmp/pickgauge-lab.png
ls -la "$LAB_HOME/.config" "$LAB_HOME/.local/share"
```

Inspect `/tmp/pickgauge-lab.png` before calling the UI verified.

## Cleanup

Stop each process with a PID from `pgrep`; run each as its own Bash call. Never put `pkill -f` in a compound command: it can match the wrapper shell and exit 144.

```bash
[ -f "$PWD/.lab.env" ] || { echo "no lab state in this checkout" >&2; false; }
source "$PWD/.lab.env"
: "${PORT:?}" "${DISPLAY_NUM:?}" "${LAB_HOME:?}"
bash -c 'for pid in $(pgrep -f "[t]arget/debug/pickgauge" || true); do tr "\0" "\n" <"/proc/$pid/environ" 2>/dev/null | grep -qF "$LAB_HOME" && kill "$pid" 2>/dev/null || true; done'
bash -c 'for pid in $(pgrep -f "[t]auri dev --config.*com\\.pickforge\\.pickgauge\\.labtest$DISPLAY_NUM" || true); do kill "$pid" 2>/dev/null || true; done'
bash -c 'for pid in $(pgrep -f "[v]ite --host 127.0.0.1 --port $PORT" || true); do kill "$pid" 2>/dev/null || true; done'
bash -c 'for pid in $(pgrep -f "[x]fwm4 --display=:$DISPLAY_NUM" || true); do kill "$pid" 2>/dev/null || true; done'
bash -c 'for pid in $(pgrep -f "[X]vfb :$DISPLAY_NUM" || true); do kill "$pid" 2>/dev/null || true; done'
rm -f "$PWD/.lab.env"
```

Keep `$LAB_HOME` until inspection is complete, then remove it if no longer needed.
