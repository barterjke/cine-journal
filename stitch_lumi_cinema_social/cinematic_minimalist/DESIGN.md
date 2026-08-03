---
name: Cinematic Minimalist
colors:
  surface: '#fcf9f8'
  surface-dim: '#dcd9d9'
  surface-bright: '#fcf9f8'
  surface-container-lowest: '#ffffff'
  surface-container-low: '#f6f3f2'
  surface-container: '#f0eded'
  surface-container-high: '#eae7e7'
  surface-container-highest: '#e5e2e1'
  on-surface: '#1c1b1b'
  on-surface-variant: '#434656'
  inverse-surface: '#313030'
  inverse-on-surface: '#f3f0ef'
  outline: '#737688'
  outline-variant: '#c3c5d9'
  surface-tint: '#004dea'
  primary: '#0041c8'
  on-primary: '#ffffff'
  primary-container: '#0055ff'
  on-primary-container: '#e3e6ff'
  inverse-primary: '#b6c4ff'
  secondary: '#bc000a'
  on-secondary: '#ffffff'
  secondary-container: '#e2241f'
  on-secondary-container: '#fffbff'
  tertiary: '#972500'
  on-tertiary: '#ffffff'
  tertiary-container: '#c13301'
  on-tertiary-container: '#ffe1d9'
  error: '#ba1a1a'
  on-error: '#ffffff'
  error-container: '#ffdad6'
  on-error-container: '#93000a'
  primary-fixed: '#dce1ff'
  primary-fixed-dim: '#b6c4ff'
  on-primary-fixed: '#001551'
  on-primary-fixed-variant: '#0039b3'
  secondary-fixed: '#ffdad5'
  secondary-fixed-dim: '#ffb4aa'
  on-secondary-fixed: '#410001'
  on-secondary-fixed-variant: '#930005'
  tertiary-fixed: '#ffdbd1'
  tertiary-fixed-dim: '#ffb5a0'
  on-tertiary-fixed: '#3b0900'
  on-tertiary-fixed-variant: '#872100'
  background: '#fcf9f8'
  on-background: '#1c1b1b'
  surface-variant: '#e5e2e1'
typography:
  display-lg:
    fontFamily: Hanken Grotesk
    fontSize: 48px
    fontWeight: '700'
    lineHeight: 56px
    letterSpacing: -0.02em
  headline-lg:
    fontFamily: Hanken Grotesk
    fontSize: 32px
    fontWeight: '600'
    lineHeight: 40px
    letterSpacing: -0.01em
  headline-lg-mobile:
    fontFamily: Hanken Grotesk
    fontSize: 24px
    fontWeight: '600'
    lineHeight: 32px
  headline-md:
    fontFamily: Hanken Grotesk
    fontSize: 20px
    fontWeight: '600'
    lineHeight: 28px
  body-lg:
    fontFamily: Hanken Grotesk
    fontSize: 18px
    fontWeight: '400'
    lineHeight: 28px
  body-md:
    fontFamily: Hanken Grotesk
    fontSize: 16px
    fontWeight: '400'
    lineHeight: 24px
  label-sm:
    fontFamily: JetBrains Mono
    fontSize: 12px
    fontWeight: '500'
    lineHeight: 16px
    letterSpacing: 0.05em
rounded:
  sm: 0.25rem
  DEFAULT: 0.5rem
  md: 0.75rem
  lg: 1rem
  xl: 1.5rem
  full: 9999px
spacing:
  unit: 4px
  xs: 4px
  sm: 8px
  md: 16px
  lg: 24px
  xl: 48px
  xxl: 80px
  gutter: 24px
  margin-mobile: 16px
  margin-desktop: 64px
---

## Brand & Style
The brand personality is curated, authoritative, and sophisticated—blending the utility of a social utility with the prestige of a high-end film journal. The design system prioritizes content over container, using an airy, minimalist aesthetic to allow high-quality film posters to act as the primary visual drivers.

The style is **Editorial Minimalism**. It utilizes expansive whitespace, a restricted "Police" color palette, and precise typographic hierarchies. The goal is to evoke the feeling of a clean, physical gallery space where the user's film collection is the exhibition.

## Colors
The palette is rooted in a "Police" aesthetic: crisp, high-contrast, and clinical.
- **Primary (Electric Blue):** Used for primary actions, active states, and verified indicators. It provides a digital "spark" against the white canvas.
- **Secondary (Alert Red):** Reserved for destructive actions, live indicators, or curated "hot" ratings.
- **Neutrals:** A range of grays from `#1A1A1A` (Ink) for text to `#F2F2F7` (Slate White) for subtle background shifts.
- **Background:** Pure `#FFFFFF` is the default to maintain maximum airiness and "breathability" between posters.

## Typography
This design system employs **Hanken Grotesk** for its sharp, contemporary geometry, providing a neutral yet premium feel. For technical metadata (runtimes, aspect ratios, dates), **JetBrains Mono** is used to introduce a "production-desk" technicality that balances the editorial feel.

- **Headlines:** Use tight letter-spacing and heavy weights to create "anchor points" on the page.
- **Labels:** Always set in JetBrains Mono and often uppercase to distinguish meta-information from narrative content.
- **Body:** Generous line-height is mandatory to maintain the "airy" feel during long reviews or film synopses.

## Layout & Spacing
The layout follows a **Fluid Grid** model with significant outer margins to mimic the layout of an art book.

- **Desktop:** A 12-column grid with wide 64px margins. Content is often centered with large "gutters of white" on either side to maintain focus.
- **Mobile:** A 4-column grid. Film posters should often bleed slightly or be presented in asymmetric pairs to keep the visual interest high.
- **Rhythm:** Use the `xl` (48px) and `xxl` (80px) spacing tokens between major sections to prevent the UI from feeling "crowded"—the whitespace is as important as the content.

## Elevation & Depth
Depth is handled through **Tonal Layers** and **Ambient Shadows** rather than heavy borders.

1.  **Level 0 (Base):** Pure white background.
2.  **Level 1 (Cards/Posters):** Uses a very soft, diffused shadow (`0px 4px 20px rgba(0,0,0,0.04)`) to lift posters off the page slightly.
3.  **Level 2 (Modals/Overlays):** Semi-transparent white backdrop blurs (Glassmorphism) are used to maintain the "bright" feeling even when layers are stacked.
4.  **Hairlines:** Use 0.5px or 1px borders in Slate White (`#F2F2F7`) to define sections without breaking the visual flow.

## Shapes
The design system uses a **Rounded** philosophy (`roundedness: 2`). 
- Standard UI components (buttons, inputs) use a 0.5rem (8px) radius.
- Film posters and "Card" containers use a larger 1rem (16px) radius to feel more like physical objects.
- Avatars and status "pills" use the `rounded-xl` token to create a soft, approachable contrast against the structured grid.

## Components
- **Buttons:** Primary buttons are solid Blue or Red with white text. Secondary buttons are "Ghost" style—thin 1px borders with JetBrains Mono labels.
- **Film Posters:** The core component. Must include a subtle inner-stroke (1px overlay, 10% black) to ensure light posters don't disappear into the white background.
- **Input Fields:** Minimalist underlines or very light gray fills. Focus states should only be indicated by a change in the label color to the Primary Blue.
- **Chips/Tags:** Used for genres or cast members. Use a Slate White background with JetBrains Mono text; no borders.
- **Lists:** Reviews and comments are separated by wide whitespace and a single-pixel horizontal line. Avoid "boxing" every list item.
- **Progress Bars:** For "Watch Progress," use a 2px thin Primary Blue line. It should be discreet and technical.