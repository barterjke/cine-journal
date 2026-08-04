/**
 * Clickable five-star rating, in half-star steps.
 *
 * Each star is two buttons — a left half and a right half — because the display
 * vocabulary is half-stars and a whole-star-only picker couldn't express the
 * ratings the export itself draws (Elena's review is 9 halves). The two halves
 * are absolutely positioned over one glyph so the row still measures and aligns
 * exactly like `StarRating`.
 *
 * Clicking the value that is already set clears the rating, which is the only
 * way back to "unrated" without a separate control.
 */
import { useState } from 'react'

const FILLED = { fontVariationSettings: "'FILL' 1" }

interface RatePickerProps {
  /** Current rating in half-stars (0..=10). 0 renders an empty row. */
  value: number
  /** Called with the new half-star count; 0 means "cleared". */
  onRate: (halfStars: number) => void
  /** Disabled while a rating request is in flight. */
  busy?: boolean
  size?: string
  className?: string
}

export function RatePicker({
  value,
  onRate,
  busy = false,
  size = 'text-[28px]',
  className = '',
}: RatePickerProps) {
  // Preview follows the pointer; falls back to the committed value on leave.
  const [hover, setHover] = useState<number | null>(null)
  const shown = hover ?? value

  return (
    <div
      className={`flex items-center ${className}`.trim()}
      onMouseLeave={() => setHover(null)}
      role="group"
      aria-label="Your rating"
    >
      {[0, 1, 2, 3, 4].map((index) => {
        const halves = index * 2
        const glyph =
          shown >= halves + 2 ? 'star' : shown === halves + 1 ? 'star_half' : 'star_outline'

        return (
          <span key={index} className="relative inline-flex">
            <span
              className={`material-symbols-outlined ${size} ${
                shown > halves ? 'text-primary' : 'text-surface-variant'
              } transition-colors select-none`}
              style={glyph === 'star' ? FILLED : undefined}
              aria-hidden="true"
            >
              {glyph}
            </span>
            {/* Two invisible halves over the glyph: left sets x.5, right sets x+1. */}
            {[halves + 1, halves + 2].map((target, half) => (
              <button
                key={target}
                type="button"
                disabled={busy}
                onMouseEnter={() => setHover(target)}
                onFocus={() => setHover(target)}
                onClick={() => onRate(value === target ? 0 : target)}
                aria-label={`Rate ${target / 2} out of 5`}
                aria-pressed={value === target}
                className={`absolute top-0 h-full w-1/2 ${
                  half === 0 ? 'left-0' : 'right-0'
                } cursor-pointer disabled:cursor-wait focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary`}
              />
            ))}
          </span>
        )
      })}
    </div>
  )
}
