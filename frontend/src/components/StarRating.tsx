/**
 * Star row, driven by a half-star count (0..=10).
 *
 * The export styled stars differently per context, so the variation is exposed
 * rather than hardcoded:
 *  - `size`  — 12/14/16px on the feeds, inherited 24px on the review screens
 *  - `color` — `text-tertiary` on live cards and sidebars, `text-primary` on grids
 *  - `emptyClassName` — the mobile feed draws FILLed grey stars for the empty
 *    slots instead of outlines, which is why empties get their own class hook.
 */
interface StarRatingProps {
  /** Half-stars, 0..=10. 9 renders four filled plus a half. */
  halfStars: number
  /** Tailwind text-size class for the glyphs, e.g. `text-[16px]`. */
  size?: string
  /** Tailwind color class applied to the row. */
  color?: string
  /** Extra classes for empty stars (the mobile feed fills them grey). */
  emptyClassName?: string
  /** Render empty stars at all. The desktop review screen omits them. */
  showEmpty?: boolean
  className?: string
}

const FILLED = { fontVariationSettings: "'FILL' 1" }

export function StarRating({
  halfStars,
  size,
  color = 'text-primary',
  emptyClassName,
  showEmpty = true,
  className = '',
}: StarRatingProps) {
  const full = Math.floor(halfStars / 2)
  const half = halfStars % 2 === 1
  const empty = 5 - full - (half ? 1 : 0)

  const glyph = ['material-symbols-outlined', size].filter(Boolean).join(' ')

  return (
    <div className={`flex items-center gap-xs ${color} ${className}`.trim()}>
      {Array.from({ length: full }, (_, i) => (
        <span key={`f${i}`} className={glyph} style={FILLED}>
          star
        </span>
      ))}
      {half && (
        <span key="half" className={glyph}>
          star_half
        </span>
      )}
      {showEmpty &&
        Array.from({ length: empty }, (_, i) => (
          <span
            key={`e${i}`}
            className={[glyph, emptyClassName].filter(Boolean).join(' ')}
            style={emptyClassName ? FILLED : undefined}
          >
            star
          </span>
        ))}
    </div>
  )
}
