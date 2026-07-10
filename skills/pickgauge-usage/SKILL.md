---
name: pickgauge-usage
description: Check PickGauge quota headroom before pool routing, dispatching a multi-task agent wave, or deciding whether Codex, Claude Code, Grok, or Ollama has room.
---

Run this before a multi-task wave:

```sh
pickgauge usage --json || ~/.local/bin/pickgauge usage --json
```

Read `services`: `status` tells whether a reading is usable; `remainingPercent`
and `windows.fiveHour`/`windows.week` are the gauges; `plan` can exist without a
gauge; `source`, `confidence`, and `staleSeconds` say how much to trust it.

Route by available pool headroom, not sticker price. A cheaper lane near its cap
loses to a pricier lane with room. Check once per dispatch wave, not before every
call. A plan-only row with `remainingPercent: null` means no gauge: treat it as
unknown and never assume the pool is empty.
