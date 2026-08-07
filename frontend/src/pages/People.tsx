/**
 * Friends: search by nickname, plus who you follow and who follows you.
 *
 * The Friends tab had no screen — it pointed at `/review`, a single review — and
 * the profile's "Following" list was a set of faces you couldn't click. This is
 * the screen behind that tab.
 *
 * No export mock exists for it, so the layout borrows the profile's: the same
 * bordered `surface-container-low` panels, the same row treatment, the same
 * headings. Inventing a new visual language for one screen would be the more
 * obvious change.
 *
 * All three lists come from one request, so following someone from the search
 * results can't leave the Following panel beside it disagreeing.
 */
import { useState } from 'react'

import type { PeopleResponse, PersonCard } from '../api'
import { api } from '../api'
import { useApi } from '../useApi'
import {
  BottomNavBar,
  DemoBanner,
  ErrorNote,
  Loading,
  TopAppBar,
} from '../components/Chrome'
import { PersonRow } from '../components/People'

/** A titled panel with a count, matching the profile's section headings. */
function Panel({
  title,
  count,
  children,
}: {
  title: string
  count?: number
  children: React.ReactNode
}) {
  return (
    <div className="flex flex-col gap-md">
      <div className="flex items-baseline justify-between">
        <h2 className="font-headline-lg-mobile md:font-headline-lg text-headline-lg-mobile md:text-headline-lg text-on-background">
          {title}
        </h2>
        {count !== undefined && (
          <span className="font-label-sm text-label-sm text-outline">{count}</span>
        )}
      </div>
      <div className="bg-surface-container-low rounded-xl p-lg border border-surface-variant flex flex-col gap-md">
        {children}
      </div>
    </div>
  )
}

/** Rows with a divider between them, as the profile's Following list draws them. */
function Rows({
  people,
  empty,
  onFollowChange,
}: {
  people: PersonCard[]
  empty: string
  onFollowChange: (id: string, following: boolean) => void
}) {
  if (!people.length) {
    return <p className="font-body-md text-body-md text-on-surface-variant">{empty}</p>
  }

  return (
    <>
      {people.map((person, index) => (
        <div key={person.id} className="flex flex-col gap-md">
          {index > 0 && <hr className="border-t border-surface-variant w-full" />}
          <PersonRow
            person={person}
            onFollowChange={(following) => onFollowChange(person.id, following)}
          />
        </div>
      ))}
    </>
  )
}

export function People() {
  // The submitted term, not the keystrokes: the input owns its own draft so
  // typing doesn't fire a request per character.
  const [query, setQuery] = useState('')
  const [draft, setDraft] = useState('')
  const { data, error, loading, update } = useApi(() => api.people(query), [query])

  /**
   * Patch every list at once.
   *
   * A person can appear in all three — searched, followed, and following you — and
   * the same row in two panels showing opposite states would be worse than a
   * refetch. Cheaper than one too: no round trip, no loading flash.
   *
   * `following` is also inserted into or removed from its own list, since that
   * panel is defined by the flag rather than merely displaying it.
   */
  const patch = (id: string, following: boolean) => {
    update((current: PeopleResponse) => {
      const apply = (list: PersonCard[]) =>
        list.map((person) => (person.id === id ? { ...person, following } : person))

      const person = [...current.results, ...current.following, ...current.followers].find(
        (candidate) => candidate.id === id,
      )
      const followingList = following
        ? current.following.some((p) => p.id === id)
          ? apply(current.following)
          : // Newest follow first, matching the API's own ordering.
            [...(person ? [{ ...person, following }] : []), ...apply(current.following)]
        : current.following.filter((p) => p.id !== id)

      return {
        ...current,
        results: apply(current.results),
        followers: apply(current.followers),
        following: followingList,
      }
    })
  }

  return (
    <div className="bg-background text-on-background min-h-screen font-body-md text-body-md overflow-x-hidden pb-24 md:pb-0 selection:bg-primary-container selection:text-on-primary-container">
      <TopAppBar active="friends" />
      <DemoBanner />

      <main className="max-w-[1440px] mx-auto px-margin-mobile md:px-margin-desktop py-xl md:py-xxl flex flex-col gap-xl">
        <section className="flex flex-col gap-md">
          <h1 className="font-headline-md text-headline-md text-on-background">Friends</h1>
          <form
            className="relative max-w-xl"
            onSubmit={(event) => {
              event.preventDefault()
              setQuery(draft.trim())
            }}
          >
            <span
              className="material-symbols-outlined absolute left-md top-1/2 -translate-y-1/2 text-outline"
              style={{ fontSize: '20px' }}
            >
              search
            </span>
            <input
              className="w-full bg-surface-container-low border border-surface-variant rounded-full py-3 pl-xxl pr-md font-body-md text-body-md text-on-surface focus:ring-1 focus:ring-primary focus:outline-none"
              placeholder="Search by nickname or name…"
              type="search"
              value={draft}
              aria-label="Search people by nickname"
              onChange={(event) => {
                setDraft(event.target.value)
                // Clearing the box returns to the full directory without a submit,
                // which is what the little ⓧ in a search input implies it does.
                if (event.target.value === '') setQuery('')
              }}
            />
          </form>
        </section>

        {loading && <Loading />}
        {error && <ErrorNote error={error} />}

        {data && (
          <div className="grid grid-cols-1 md:grid-cols-12 gap-gutter">
            <div className="md:col-span-7 flex flex-col gap-md">
              <Panel
                title={data.query ? `Results for "${data.query}"` : 'Everyone'}
                count={data.results.length}
              >
                <Rows
                  people={data.results}
                  empty={
                    data.query
                      ? `Nobody matches "${data.query}". Nicknames are the surest way to find someone.`
                      : 'No people yet. The graph is seeded at startup — check the server log.'
                  }
                  onFollowChange={patch}
                />
              </Panel>
            </div>

            <div className="md:col-span-5 flex flex-col gap-xl">
              <Panel title="Following" count={data.following.length}>
                <Rows
                  people={data.following}
                  empty="You don't follow anyone yet. Follow someone and their reviews rise to the top of every film's page."
                  onFollowChange={patch}
                />
              </Panel>

              <Panel title="Followers" count={data.followers.length}>
                <Rows
                  people={data.followers}
                  empty="Nobody follows you yet."
                  onFollowChange={patch}
                />
              </Panel>
            </div>
          </div>
        )}
      </main>

      <BottomNavBar active="friends" />
    </div>
  )
}
