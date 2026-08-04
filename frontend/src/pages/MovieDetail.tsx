/**
 * Movie Detail — Desktop. Ported from
 * `reference/stitch_lumi_cinema_social 2/movie_detail_desktop/code.html`.
 *
 * Fed by `GET /api/movies/:id`, which resolves *any* id: only Neon Reverie was
 * designed, so every film borrows its synopsis, cast, gallery and credits while
 * the title, year, genres and poster come from the catalogue. That is what makes
 * every poster in the app clickable.
 *
 * Things worth knowing:
 *  - the export's app bar here had no nav links and no search box, only the
 *    trailing icons. It gets the same `TopAppBar` as every other screen: a detail
 *    page is reached from all of them, and without the links there is no way back
 *    out of it.
 *  - the Gallery heading says "12 Stills" while the grid holds 4. That mismatch
 *    is the export's; `still_count` is carried separately so it is preserved
 *    rather than silently corrected to the array length.
 *  - the export drew no rating control at all. One is added under the hero,
 *    since "give a starred rating" needs somewhere to live and the Details
 *    sidebar is label/value rows.
 */
import { Link, useParams } from 'react-router-dom'

import type { CastMember, GalleryStill, StillShape } from '../api'
import { api } from '../api'
import { useApi } from '../useApi'
import { useAction } from '../useAction'
import { ActionError, BottomNavBar, ErrorNote, Loading, TopAppBar } from '../components/Chrome'
import { RatePicker } from '../components/RatePicker'

/** The export's detail page is Neon Reverie; `/movie` with no id lands there. */
const DEFAULT_MOVIE_ID = 'neon-reverie'

function CastCard({ member }: { member: CastMember }) {
  return (
    <div className="flex flex-col items-center group">
      <div className="w-full aspect-square rounded-full overflow-hidden bg-surface-variant mb-sm poster-shadow poster-border group-hover:scale-105 transition-transform duration-300">
        <img
          className="w-full h-full object-cover grayscale hover:grayscale-0 transition-all duration-300"
          alt={member.portrait.alt}
          src={member.portrait.src}
        />
      </div>
      <span className="font-body-md text-body-md font-medium text-center">{member.name}</span>
      <span className="font-label-sm text-label-sm text-outline text-center mt-xs">
        {member.role}
      </span>
    </div>
  )
}

/**
 * The export's four gallery slots, spelled out as literal class strings so
 * Tailwind's JIT actually emits them. The API sends a `shape` variant and this
 * map turns it into CSS — building the strings dynamically (or passing them from
 * the backend) yields no CSS at all and flattens the grid to uniform tiles.
 */
const SHAPE_CLASSES: Record<StillShape, string> = {
  hero: 'col-span-2 md:col-span-2 aspect-video',
  companion: 'col-span-2 md:col-span-1 aspect-video md:aspect-auto',
  compact: 'col-span-1 aspect-square md:aspect-video',
  panorama: 'col-span-1 md:col-span-2 aspect-square md:aspect-video',
}

function GalleryTile({ still }: { still: GalleryStill }) {
  return (
    <div
      className={`${SHAPE_CLASSES[still.shape]} bg-surface-variant rounded-lg overflow-hidden poster-border`}
    >
      <img
        className="w-full h-full object-cover hover:scale-105 transition-transform duration-500"
        alt={still.image.alt}
        src={still.image.src}
      />
    </div>
  )
}

export function MovieDetail() {
  const { id } = useParams()
  const movieId = id ?? DEFAULT_MOVIE_ID
  const { data, error, loading, update } = useApi(() => api.movie(movieId), [movieId])

  const watchlist = useAction(async () => {
    const target = !data?.on_watchlist
    // Optimistic: flip immediately, then reconcile with what the server stored.
    update((current) => ({ ...current, on_watchlist: target }))
    try {
      const state = await api.setWatchlist(movieId, target)
      update((current) => ({ ...current, on_watchlist: state.on_watchlist }))
    } catch (cause) {
      update((current) => ({ ...current, on_watchlist: !target }))
      throw cause
    }
  })

  const rating = useAction(async (halfStars: number) => {
    const previous = data?.your_rating_half_stars ?? null
    update((current) => ({
      ...current,
      your_rating_half_stars: halfStars === 0 ? null : halfStars,
    }))
    try {
      const state = await api.rate(movieId, halfStars)
      update((current) => ({
        ...current,
        your_rating_half_stars: state.your_rating_half_stars,
      }))
    } catch (cause) {
      update((current) => ({ ...current, your_rating_half_stars: previous }))
      throw cause
    }
  })

  const yourRating = data?.your_rating_half_stars ?? 0

  return (
    <div className="bg-background text-on-background font-body-md min-h-screen">
      <TopAppBar active="movies" />

      {loading && <Loading />}
      {error && <ErrorNote error={error} />}

      {data && (
        <main className="w-full max-w-7xl mx-auto pb-xxl md:pb-xxl">
          {/* Hero */}
          <section className="relative w-full h-[50vh] md:h-[60vh] lg:h-[70vh] bg-surface-container-high overflow-hidden">
            <div
              className="absolute inset-0 bg-cover bg-center w-full h-full"
              role="img"
              aria-label={data.backdrop.alt}
              style={{ backgroundImage: `url('${data.backdrop.src}')` }}
            ></div>
            <div className="absolute inset-0 bg-gradient-to-t from-background via-background/40 to-transparent"></div>
            <div className="absolute bottom-0 left-0 w-full px-margin-mobile md:px-margin-desktop pb-xl md:pb-xxl z-10">
              <div className="flex flex-col md:flex-row items-end gap-gutter md:gap-xl">
                <div className="relative w-32 md:w-48 lg:w-64 aspect-[2/3] rounded-lg overflow-hidden poster-shadow poster-border bg-surface-variant flex-shrink-0 -mb-8 md:-mb-16 z-20 hidden md:block">
                  <img
                    className="w-full h-full object-cover"
                    alt={data.poster.alt}
                    src={data.poster.src}
                  />
                </div>
                <div className="flex-1 pb-4">
                  <h1 className="font-display-lg text-display-lg text-on-background mb-sm">
                    {data.title}
                  </h1>
                  <div className="flex flex-wrap items-center gap-md font-label-sm text-label-sm text-outline mb-lg">
                    <span>{data.year}</span>
                    <span className="w-1 h-1 rounded-full bg-outline"></span>
                    <span>
                      Directed by <span className="text-on-background">{data.director}</span>
                    </span>
                    <span className="w-1 h-1 rounded-full bg-outline"></span>
                    <span>{data.runtime}</span>
                  </div>
                  <div className="flex flex-wrap gap-xs mb-lg">
                    {data.genres.map((genre) => (
                      /* Chips search by genre, so the detail page leads back
                         into the catalogue instead of being a dead end. */
                      <Link
                        key={genre}
                        to={`/search?genre=${encodeURIComponent(genre)}`}
                        className="px-3 py-1 bg-surface-container-lowest border border-outline-variant text-on-surface-variant font-label-sm text-label-sm rounded-full hover:border-primary hover:text-primary transition-colors"
                      >
                        {genre}
                      </Link>
                    ))}
                  </div>
                  <div className="flex flex-wrap items-center gap-md">
                    <button className="bg-primary text-on-primary px-6 py-3 rounded-DEFAULT font-label-sm text-label-sm uppercase tracking-wider hover:bg-primary/90 transition-colors flex items-center gap-sm">
                      <span className="material-symbols-outlined">play_arrow</span>
                      Trailer
                    </button>
                    <button
                      onClick={() => watchlist.run()}
                      disabled={watchlist.busy}
                      aria-pressed={data.on_watchlist}
                      className={`px-6 py-3 rounded-DEFAULT font-label-sm text-label-sm uppercase tracking-wider transition-colors flex items-center gap-sm disabled:cursor-wait ${
                        data.on_watchlist
                          ? 'bg-surface-variant border border-primary text-primary'
                          : 'bg-transparent border border-outline text-on-background hover:bg-surface-variant'
                      }`}
                    >
                      <span
                        className="material-symbols-outlined"
                        style={
                          data.on_watchlist ? { fontVariationSettings: "'FILL' 1" } : undefined
                        }
                      >
                        {data.on_watchlist ? 'check' : 'add'}
                      </span>
                      {data.on_watchlist ? 'On Watchlist' : 'Watchlist'}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </section>

          {/* Your rating. Not in the export — see the file header. */}
          <section className="px-margin-mobile md:px-margin-desktop mt-xl md:mt-xxl">
            <div className="flex flex-col sm:flex-row sm:items-center gap-sm sm:gap-lg bg-surface-container-low border border-surface-variant rounded-xl px-lg py-md">
              <div className="flex-grow">
                <span className="font-label-sm text-label-sm text-outline uppercase tracking-wider block">
                  Your Rating
                </span>
                <span className="font-body-md text-body-md text-on-surface-variant">
                  {yourRating > 0
                    ? `${yourRating / 2} out of 5 — click again to clear`
                    : 'Not rated yet'}
                </span>
              </div>
              <RatePicker value={yourRating} onRate={rating.run} busy={rating.busy} />
            </div>
            {rating.error && (
              <div className="mt-sm">
                <ActionError message={rating.error} onDismiss={rating.clearError} />
              </div>
            )}
            {watchlist.error && (
              <div className="mt-sm">
                <ActionError message={watchlist.error} onDismiss={watchlist.clearError} />
              </div>
            )}
          </section>

          <div className="px-margin-mobile md:px-margin-desktop mt-xl md:mt-xxl grid grid-cols-1 lg:grid-cols-12 gap-gutter md:gap-xl">
            {/* Left column */}
            <div className="lg:col-span-8">
              <section className="mb-xxl">
                <h2 className="font-headline-lg text-headline-lg md:hidden mb-md">Synopsis</h2>
                <p className="font-body-lg text-body-lg text-on-surface-variant leading-relaxed max-w-3xl">
                  {data.synopsis}
                </p>
              </section>

              <hr className="border-t border-surface-variant mb-xxl" />

              <section className="mb-xxl">
                <div className="flex items-baseline justify-between mb-lg">
                  <h2 className="font-headline-md text-headline-md text-on-background">Cast</h2>
                  <span className="font-label-sm text-label-sm text-primary uppercase cursor-pointer hover:underline">
                    View All
                  </span>
                </div>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-md">
                  {data.cast.map((member) => (
                    <CastCard key={member.id} member={member} />
                  ))}
                </div>
              </section>

              <hr className="border-t border-surface-variant mb-xxl" />

              <section className="mb-xxl">
                <div className="flex items-baseline justify-between mb-lg">
                  <h2 className="font-headline-md text-headline-md text-on-background">Gallery</h2>
                  <span className="font-label-sm text-label-sm text-primary uppercase cursor-pointer hover:underline">
                    {data.still_count} Stills
                  </span>
                </div>
                <div className="grid grid-cols-2 md:grid-cols-3 gap-md">
                  {data.gallery.map((still) => (
                    <GalleryTile key={still.id} still={still} />
                  ))}
                </div>
              </section>
            </div>

            {/* Right column — Details sidebar */}
            <div className="lg:col-span-4">
              <div className="sticky top-[100px] bg-surface-container-low p-lg rounded-xl poster-shadow border border-surface-variant">
                <h3 className="font-headline-md text-headline-md font-semibold mb-lg">Details</h3>
                <div className="space-y-md">
                  {data.details.map((fact, i) => (
                    <div key={fact.label}>
                      {i > 0 && <hr className="border-t border-surface-variant mb-md" />}
                      <span className="font-label-sm text-label-sm text-outline block mb-xs">
                        {fact.label}
                      </span>
                      <span className="font-body-md text-body-md">{fact.value}</span>
                    </div>
                  ))}
                </div>
                <div className="mt-xl">
                  <span className="font-label-sm text-label-sm text-outline block mb-md">
                    Watch Progress
                  </span>
                  <div className="w-full h-1 bg-surface-variant rounded-full overflow-hidden">
                    <div
                      className="h-full bg-primary"
                      style={{ width: `${data.watch_progress_percent}%` }}
                    ></div>
                  </div>
                  <div className="flex justify-between items-center mt-xs font-label-sm text-label-sm text-outline">
                    <span>{data.watch_progress_label}</span>
                    <span className="material-symbols-outlined text-sm cursor-pointer hover:text-primary transition-colors">
                      edit
                    </span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </main>
      )}

      <BottomNavBar active="movies" />
    </div>
  )
}
