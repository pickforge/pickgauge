# PickGauge Branding

Brand assets for PickGauge, the Pickforge Studio AI usage tray.

## Core Idea

PickGauge uses the Pickforge Studio bracket mark with a compact gauge needle and ember status dot. The visual should feel local-first, quiet, and operational: dark charcoal surfaces, off-white structure, muted grey metadata, and ember only for action or attention.

## Colors

| Token | Hex | Use |
| --- | --- | --- |
| Surface | `#0A0A0B` | App background |
| Panel | `#0F0F11` | Cards, icon backs, tray surface |
| Text | `#F2F2F3` | Primary text and bracket strokes |
| Muted | `#6E6E75` | Metadata, inactive ticks |
| Ember | `#FF7A1A` | Primary action and attention dot |
| Low | `#C2410C` | Low usage warning |

## Files

| File | Purpose |
| --- | --- |
| `pickgauge-mark-128.svg` | Primary transparent logo mark |
| `pickgauge-mark-1024.png` | High-resolution raster logo mark |
| `pickgauge-lockup-horizontal.svg` and `.png` | Horizontal wordmark lockup |
| `pickgauge-lockup-on-dark.svg` and `.png` | Dark-surface wordmark lockup |
| `pickgauge-lockup-on-surface.png` | Raster lockup for README/app usage |
| `pickgauge-lockup-on-charcoal.png` | Raster lockup for charcoal surfaces |
| `pickgauge-app-icon.svg` | Source app icon |
| `pickgauge-app-icon-*.png` | App icon PNG exports |
| `pickgauge-tray-*.svg` and `pickgauge-tray-*.png` | Tray state icons |
| `pickgauge-favicon.svg` and `pickgauge-favicon-*.png` | Browser/repository favicon assets |
| `pickgauge-hero-art.png` | Croppable social preview artwork used inside the tray popup |
| `pickgauge-og-image.svg` and `pickgauge-og-image.png` | Repository/social preview image |
| `pickgauge-palette.svg` and `pickgauge-palette.png` | Brand color palette |
| `pickgauge-brand-pattern.svg` and `pickgauge-brand-pattern.png` | Subtle background pattern |

## Icon Provenance

After changing `pickgauge-app-icon.svg`, regenerate platform icons with:

```bash
bun run tauri icon assets/branding/pickgauge-app-icon.svg
```

## Generated Hero Prompt

Use case: stylized-concept

Asset type: brand hero artwork for a privacy-conscious desktop utility app called PickGauge; wide repository/README visual, 16:9 composition.

Primary request: Create a polished text-free brand hero image for a Linux/KDE tray app that monitors remaining AI usage across two services.

Scene/backdrop: A dark charcoal desktop workspace with a slim system tray/panel impression, subtle utility-app UI shapes, and a central bracket/gauge motif.

Subject: A bracketed circular gauge with one ember status point, small neutral confidence/status ticks, and a subtle privacy/security cue integrated into the gauge.

Constraints: no readable text, no letters, no words, no company logos, no OpenAI logo, no Anthropic logo, no Claude logo, no Codex logo, no watermarks, no mascots, no people.
