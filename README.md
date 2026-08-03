# CinéJournal

A 1:1 re-creation of a **Google Stitch** export — "Lumi Cinema Social", a film journal /
social platform in an *Editorial Minimalism* style. Four screens, static HTML + compiled
CSS, no build step and no runtime dependencies.

```
cine-journal/                 the re-created site  →  open cine-journal/index.html
stitch_lumi_cinema_social/    the original Stitch export, kept as provenance
```

## Run it

Open `cine-journal/index.html` directly, or serve the folder:

```bash
cd cine-journal && python3 -m http.server 8000
```

Then http://localhost:8000. The two `*-mobile.html` screens are mobile-only layouts —
view them in a narrow window or with device emulation (Chrome: ⌥⌘I → ⌘⇧M).

## What's here

| Screen | File |
| --- | --- |
| Movie Feed — Desktop | `cine-journal/index.html` |
| Friend Review — Desktop | `cine-journal/review.html` |
| Movie Feed — Mobile | `cine-journal/feed-mobile.html` |
| Friend Review — Mobile | `cine-journal/review-mobile.html` |

The export's Tailwind CDN `<script>` and inline config were compiled once into a real
stylesheet (`cine-journal/css/app.css`), and all 30 remote images were downloaded
locally — so the site is self-contained apart from the Google Fonts `<link>` tags, which
the export used and which are preserved. Fidelity was verified by rendering each screen
in headless Chrome against the export's reference screenshots.

See [`cine-journal/README.md`](cine-journal/README.md) for the design system, rebuild
instructions, and the export quirks that were deliberately preserved.
