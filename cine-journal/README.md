# CinéJournal

A re-creation of the **Lumi Cinema Social** Google Stitch project — a film journal /
social platform in an "Editorial Minimalism" style. Static HTML + compiled CSS,
**no build step and no runtime dependencies**.

Source export: `../stitch_lumi_cinema_social/`
Stitch project: https://stitch.withgoogle.com/projects/8536155539744862

## Running it

Open `index.html` in a browser. That's it.

The two `*-mobile.html` screens are **mobile-only layouts** (see Quirks below) — view
them in a narrow window or with device emulation (in Chrome: ⌥⌘I → ⌘⇧M).

```
open index.html
```

## Screens

| File                 | Screen               | Notes                                            |
| -------------------- | -------------------- | ------------------------------------------------ |
| `index.html`         | Movie Feed — Desktop | Live Now cards, Recent Entries grid, Friends rail |
| `review.html`        | Friend Review — Desktop | Faded backdrop, sticky poster column, comments |
| `feed-mobile.html`   | Movie Feed — Mobile  | Stories row, 2-up poster grid, bottom nav         |
| `review-mobile.html` | Friend Review — Mobile | Full-bleed poster, sticky comment composer      |

The nav links are wired between screens (Feed ↔ Friends, "Back to Feed"), so you can
click through desktop → desktop and mobile → mobile. Links marked `#` are inert, as in
the export.

## Layout

```
index.html  feed-mobile.html  review.html  review-mobile.html
css/
  app.css        compiled Tailwind — the only stylesheet the pages load
  tokens.css     design tokens as CSS custom properties (reference; see below)
img/             30 assets from the export, downloaded locally
_build/          dev-only; not needed to view or deploy the site
  tailwind.config.js   theme transcribed verbatim from the export
  input.css            @tailwind directives + the export's custom utilities
  package.json
```

### `css/tokens.css`

A readable transcription of `DESIGN.md` (colors, type scale, spacing, radii,
elevation) as CSS custom properties. The pages don't load it — `app.css` already
carries these values, baked into Tailwind utilities. It's here as the reference for the
design system and for writing plain CSS against the same tokens.

## Rebuilding the CSS

Only needed if you change markup and use Tailwind classes not already in `app.css`.

```bash
cd _build
npm install
npm run build     # or: npm run dev   (watch mode)
```

Both scripts write to `../css/app.css`. The config's `content` globs cover the four
HTML files, so unused utilities are purged.

> Node on this machine currently fails with a missing `libllhttp` dylib (a broken
> Homebrew upgrade, unrelated to this project). `app.css` was compiled by running the
> same Tailwind 3.4.17 CLI through Deno:
> ```bash
> cd _build && deno run -A --node-modules-dir npm:tailwindcss@3.4.17 \
>   -c tailwind.config.js -i input.css -o ../css/app.css --minify
> ```

## Design system

From `../stitch_lumi_cinema_social/cinematic_minimalist/DESIGN.md`:

- **Primary** `#0041c8` electric blue — actions, active states, ratings
- **Secondary** `#bc000a` alert red — live indicators
- **Tertiary** `#972500` — star ratings in several contexts
- **Surface** `#fcf9f8` — warm off-white canvas
- **Type** — Hanken Grotesk for headlines/body, JetBrains Mono for labels and metadata
- **Elevation** — soft `0 4px 20px rgba(0,0,0,.04)` shadow + a 1px 10%-black inner
  stroke so light posters don't dissolve into the background

Fonts and the Material Symbols icon font load from Google Fonts via `<link>`, exactly
as the export did — so **the first load needs a network connection**. Images are local.

## Quirks preserved from the export

Reproduced deliberately, to keep the rendering 1:1 with the Stitch screenshots:

- **`review-mobile.html` is blank at ≥768px.** Its `<body>` carries `md:hidden`; Stitch
  generated it as a mobile-only view. Remove that one class to make it render at all widths.
- **Mobile feed poster titles are 20px** (`text-headline-md`), noticeably larger than
  the desktop grid's 16px equivalent.
- **Mobile feed posters have square corners.** The markup uses `rounded-DEFAULT`, which
  isn't a real Tailwind class — `DEFAULT` is emitted as bare `rounded`, so it produces
  no CSS.
- Dropped from the export because they were **inert**: the junk `flat no shadows` class
  triplet, duplicated `fixed bottom-0 w-full z-50` utilities, and `pb-safe` (not a stock
  Tailwind class, generates nothing).

`data-alt` attributes (Stitch's image-generation prompts) were converted to real `alt`
text for accessibility.
