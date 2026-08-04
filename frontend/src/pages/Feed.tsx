/**
 * Movie Feed — Desktop. Ported from `reference/cine-journal/index.html`.
 *
 * Layout, class lists and copy match the export; the three sections are now fed
 * by `GET /api/feed` instead of being inlined in the markup.
 *
 * Every poster and every film name links to `/movie/:id`. The export's cards were
 * styled as clickable (`cursor-pointer`, hover lifts) but went nowhere.
 */
import { useState } from 'react'
import { Link } from 'react-router-dom'

import type { Feed as FeedData, FeedEntry, FriendActivity, LiveDiscussion } from '../api'
import { api } from '../api'
import { useApi } from '../useApi'
import { useAction } from '../useAction'
import {
  ActionError,
  BottomNavBar,
  DemoBanner,
  ErrorNote,
  Loading,
  TopAppBar,
} from '../components/Chrome'
import { StarRating } from '../components/StarRating'

function LiveCard({ item }: { item: LiveDiscussion }) {
  return (
    <Link
      to={`/movie/${item.movie.id}`}
      className="bg-surface-container-lowest rounded-xl p-md flex gap-md items-start soft-shadow border border-surface-variant group cursor-pointer transition-transform hover:-translate-y-1"
    >
      <div className="w-24 h-36 shrink-0 rounded-lg overflow-hidden inner-stroke relative">
        <img
          className="w-full h-full object-cover transition-transform duration-500 group-hover:scale-105"
          alt={item.movie.poster.alt}
          src={item.movie.poster.src}
        />
      </div>
      <div className="flex flex-col gap-xs flex-grow">
        <div className="flex justify-between items-start">
          <h3 className="font-headline-md text-headline-md text-on-background line-clamp-2 group-hover:text-primary transition-colors">
            {item.movie.title}
          </h3>
          <span className="bg-surface-container px-2 py-1 rounded font-label-sm text-label-sm text-on-surface-variant">
            {item.movie.year}
          </span>
        </div>
        <StarRating halfStars={item.rating_half_stars} size="text-[16px]" color="text-tertiary" />
        <p className="font-body-md text-body-md text-on-surface-variant mt-sm text-sm line-clamp-3">
          {item.blurb}
        </p>
        <div className="mt-auto pt-sm flex -space-x-2">
          {item.participants.map((p) => (
            <div
              key={p.src}
              className="w-6 h-6 rounded-full bg-surface-variant border-2 border-surface-container-lowest overflow-hidden"
            >
              <img className="w-full h-full object-cover" alt={p.alt} src={p.src} />
            </div>
          ))}
          {item.overflow_count !== null && (
            <div className="w-6 h-6 rounded-full bg-surface-container-high border-2 border-surface-container-lowest flex items-center justify-center font-label-sm text-label-sm text-[10px] text-on-surface">
              +{item.overflow_count}
            </div>
          )}
        </div>
      </div>
    </Link>
  )
}

function PosterTile({
  entry,
  onToggleWatchlist,
  busy,
}: {
  entry: FeedEntry
  onToggleWatchlist: (id: string) => void
  busy: boolean
}) {
  return (
    <div className="flex flex-col gap-sm group">
      <div className="aspect-[2/3] w-full rounded-lg overflow-hidden inner-stroke soft-shadow relative bg-surface-container-low">
        <Link to={`/movie/${entry.movie.id}`} aria-label={entry.movie.title}>
          <img
            className="w-full h-full object-cover transition-transform duration-700 group-hover:scale-105"
            alt={entry.movie.poster.alt}
            src={entry.movie.poster.src}
          />
        </Link>
        {/* Overlay ignores pointer events so it never eats the poster's click;
            only the button inside takes one. It stays up on a watchlisted film,
            otherwise the state would be invisible until you hover. */}
        <div
          className={`absolute inset-0 bg-black/40 transition-opacity duration-300 flex items-center justify-center backdrop-blur-sm pointer-events-none ${
            entry.on_watchlist
              ? 'opacity-100'
              : 'opacity-0 group-hover:opacity-100 focus-within:opacity-100'
          }`}
        >
          <button
            onClick={() => onToggleWatchlist(entry.movie.id)}
            disabled={busy}
            aria-pressed={entry.on_watchlist}
            aria-label={
              entry.on_watchlist
                ? `Remove ${entry.movie.title} from watchlist`
                : `Add ${entry.movie.title} to watchlist`
            }
            className={`pointer-events-auto w-12 h-12 rounded-full border flex items-center justify-center transition-colors disabled:cursor-wait ${
              entry.on_watchlist
                ? 'bg-white text-black border-white'
                : 'bg-white/20 border-white/50 text-white hover:bg-white hover:text-black'
            }`}
          >
            <span
              className="material-symbols-outlined"
              style={{ fontVariationSettings: "'FILL' 1" }}
            >
              {entry.on_watchlist ? 'check' : 'add'}
            </span>
          </button>
        </div>
      </div>
      <div className="flex flex-col">
        <div className="flex justify-between items-baseline gap-2">
          <Link to={`/movie/${entry.movie.id}`} className="truncate">
            <h3 className="font-headline-md text-[16px] leading-tight text-on-background font-bold truncate hover:text-primary transition-colors">
              {entry.movie.title}
            </h3>
          </Link>
          <span className="font-label-sm text-label-sm text-on-surface-variant shrink-0">
            {entry.movie.year}
          </span>
        </div>
        <StarRating
          halfStars={entry.rating_half_stars}
          size="text-[14px]"
          color="text-primary"
          className="mt-1"
        />
      </div>
    </div>
  )
}

function ActivityItem({ item }: { item: FriendActivity }) {
  return (
    <div className="flex gap-md group">
      <div className="w-10 h-10 shrink-0 rounded-full bg-surface-variant overflow-hidden inner-stroke">
        <img
          className="w-full h-full object-cover grayscale transition-all duration-300 group-hover:grayscale-0"
          alt={item.author_avatar.alt}
          src={item.author_avatar.src}
        />
      </div>
      <div className="flex flex-col gap-xs w-full">
        <div className="flex items-baseline gap-2 justify-between">
          <span className="font-headline-md text-[14px] font-bold text-on-background">
            {item.author_name}
          </span>
          <span className="font-label-sm text-[10px] text-outline">{item.timestamp}</span>
        </div>
        <div className="flex items-center gap-2">
          <span className="font-body-md text-sm text-on-surface-variant">
            {item.kind === 'watched' ? 'watched' : 'added'}
          </span>
          <Link
            to={`/movie/${item.movie_id}`}
            className="font-headline-md text-sm font-bold text-primary hover:underline"
          >
            {item.movie_title}
          </Link>
          {item.kind === 'added_to_watchlist' && (
            <span className="font-body-md text-sm text-on-surface-variant">to Watchlist</span>
          )}
        </div>
        {item.rating_half_stars !== null && (
          <StarRating
            halfStars={item.rating_half_stars}
            size="text-[12px]"
            color="text-tertiary"
            className="my-1"
          />
        )}
        {item.quote && (
          <p className="font-body-md text-sm text-on-surface-variant italic border-l-2 border-surface-variant pl-3 py-1">
            {item.quote}
          </p>
        )}
      </div>
    </div>
  )
}

export function Feed() {
  const { data, error, loading, update } = useApi(() => api.feed())
  const [gridView, setGridView] = useState(true)

  const watchlist = useAction(async (id: string) => {
    const target = !data?.recent.find((e) => e.movie.id === id)?.on_watchlist
    const setFlag = (on: boolean) => (current: FeedData) => ({
      ...current,
      recent: current.recent.map((e) => (e.movie.id === id ? { ...e, on_watchlist: on } : e)),
    })

    // Optimistic: flip immediately, then reconcile with what the server stored.
    update(setFlag(target))
    try {
      const state = await api.setWatchlist(id, target)
      update(setFlag(state.on_watchlist))
    } catch (cause) {
      update(setFlag(!target))
      throw cause
    }
  })

  return (
    <div className="bg-background text-on-background min-h-screen font-body-md text-body-md overflow-x-hidden selection:bg-primary-container selection:text-on-primary-container">
      <TopAppBar active="feed" />
      <DemoBanner />

      {loading && <Loading />}
      {error && <ErrorNote error={error} />}

      {data && (
        <main className="max-w-[1440px] mx-auto px-margin-mobile md:px-margin-desktop py-xl md:py-xxl grid grid-cols-1 md:grid-cols-12 gap-gutter">
          <section className="md:col-span-8 lg:col-span-9 flex flex-col gap-xxl">
            {/* Live Discussion */}
            <div className="flex flex-col gap-lg">
              <div className="flex items-center gap-sm">
                <span className="w-2 h-2 rounded-full bg-secondary animate-pulse"></span>
                <h2 className="font-label-sm text-label-sm text-secondary uppercase tracking-widest">
                  Live Now
                </h2>
              </div>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-gutter">
                {data.live.map((item) => (
                  <LiveCard key={item.id} item={item} />
                ))}
              </div>
            </div>

            {/* Recent Entries */}
            <div className="flex flex-col gap-lg">
              <div className="flex justify-between items-end border-b border-surface-variant pb-sm">
                <h2 className="font-headline-lg text-headline-lg text-on-background">
                  Recent Entries
                </h2>
                <div className="flex gap-sm">
                  <button
                    onClick={() => setGridView(true)}
                    aria-label="Grid view"
                    aria-pressed={gridView}
                    className={`p-xs hover:text-primary transition-colors ${
                      gridView ? 'text-primary' : 'text-outline'
                    }`}
                  >
                    <span
                      className="material-symbols-outlined"
                      style={gridView ? { fontVariationSettings: "'FILL' 1" } : undefined}
                    >
                      grid_view
                    </span>
                  </button>
                  <button
                    onClick={() => setGridView(false)}
                    aria-label="List view"
                    aria-pressed={!gridView}
                    className={`p-xs hover:text-primary transition-colors ${
                      gridView ? 'text-outline' : 'text-primary'
                    }`}
                  >
                    <span
                      className="material-symbols-outlined"
                      style={gridView ? undefined : { fontVariationSettings: "'FILL' 1" }}
                    >
                      view_list
                    </span>
                  </button>
                </div>
              </div>

              {watchlist.error && (
                <ActionError message={watchlist.error} onDismiss={watchlist.clearError} />
              )}

              <div
                className={
                  gridView
                    ? 'grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-x-gutter gap-y-xl'
                    : 'grid grid-cols-1 sm:grid-cols-2 gap-x-gutter gap-y-lg'
                }
              >
                {data.recent.map((entry) => (
                  <PosterTile
                    key={entry.id}
                    entry={entry}
                    onToggleWatchlist={watchlist.run}
                    busy={watchlist.busy}
                  />
                ))}
              </div>
            </div>
          </section>

          {/* Friends Activity */}
          <aside className="hidden md:flex md:col-span-4 lg:col-span-3 flex-col gap-xl">
            <div className="sticky top-32 flex flex-col gap-lg">
              <div className="flex items-center gap-sm border-b border-surface-variant pb-sm">
                <span className="material-symbols-outlined text-on-surface-variant">group</span>
                <h2 className="font-label-sm text-label-sm text-on-surface-variant uppercase tracking-widest">
                  Friends Activity
                </h2>
              </div>
              <div className="flex flex-col gap-lg">
                {data.friend_activity.map((item) => (
                  <ActivityItem key={item.id} item={item} />
                ))}
              </div>
              {/* The export named Elena here. Who wrote the featured review now
                  depends on what's trending, so the link can't name anyone. */}
              <Link
                to="/review"
                className="font-label-sm text-label-sm text-primary uppercase tracking-widest hover:opacity-70 transition-opacity"
              >
                Read the featured review →
              </Link>
            </div>
          </aside>
        </main>
      )}

      <BottomNavBar active="feed" />
    </div>
  )
}
