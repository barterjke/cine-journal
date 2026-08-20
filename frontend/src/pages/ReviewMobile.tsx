/**
 * Friend Review — Mobile. Ported from `reference/cine-journal/review-mobile.html`.
 *
 * Full-bleed poster and a sticky comment composer. The export served one invented
 * review; this screen shows any of our users' reviews, by id, exactly as the
 * desktop one does — `/review-mobile/:id`, or the newest with no id.
 *
 * NB: the export put `md:hidden` on <body>, so the original screen rendered
 * blank at >=768px. That quirk is NOT reproduced here — a blank route in an SPA
 * reads as a bug rather than a faithful detail. The reference file still has it.
 *
 * The composer posts for real now. The export drew no comment thread on this
 * screen at all, so posted comments are listed under the review — a send button
 * that swallows what you typed is worse than a slightly extended layout.
 *
 * The thread is shared, so each row carries its real author: face, name, handle,
 * date, all linking to that person's page. Your own rows say "You", from `is_you`.
 * Replies are shown but not composable here — that lives on the desktop screen.
 */
import { useState } from 'react'
import { Link, useParams } from 'react-router-dom'

import { api } from '../api'
import { useApi } from '../useApi'
import { useAction } from '../useAction'
import { ActionError, DemoBanner, ErrorNote, Loading } from '../components/Chrome'
import { authorLabel, personPath, RatingOnlyNote } from '../components/People'
import { Poster } from '../components/PosterTile'
import { StarRating } from '../components/StarRating'

export function ReviewMobile() {
  // Same two routes as the desktop screen — an id, or the newest review.
  const { id } = useParams<{ id: string }>()
  const { data, error, loading, replace, reload } = useApi(() => api.reviewOrNewest(id), [id])
  const [draft, setDraft] = useState('')

  const postComment = useAction(async () => {
    if (!data) return
    replace(await api.postComment(data.id, draft))
    setDraft('')
  })

  return (
    <div className="bg-surface text-on-surface font-body-md min-h-screen flex flex-col pb-24">
      <header className="sticky top-0 w-full bg-surface/90 backdrop-blur-md z-40 px-margin-mobile py-md flex items-center justify-between border-b border-surface-variant">
        <Link
          className="flex items-center text-on-surface-variant hover:text-primary transition-colors cursor-pointer active:opacity-70"
          to="/feed-mobile"
        >
          <span className="material-symbols-outlined mr-sm">arrow_back</span>
          <span className="font-label-sm text-label-sm">Back</span>
        </Link>
        {/* Home link, as on the desktop bar — see `TopAppBar`. */}
        <Link to="/" aria-label="CinéJournal home">
          <span className="font-headline-md text-headline-md font-bold text-primary hover:opacity-70 transition-opacity">
            CinéJournal
          </span>
        </Link>
        <div className="w-8"></div>
      </header>
      <DemoBanner />

      {loading && <Loading />}
      {error && <ErrorNote error={error} onRetry={reload} />}

      {data && (
        <main className="flex-grow flex flex-col items-center pt-lg px-margin-mobile w-full max-w-md mx-auto">
          {/* Friend context. Tapping it opens their page, as on the desktop screen. */}
          <Link to={personPath(data.author_handle)} className="flex items-center w-full mb-lg active:opacity-70">
            <div className="w-12 h-12 rounded-full overflow-hidden bg-surface-variant mr-md border border-surface-variant shrink-0">
              <img
                className="w-full h-full object-cover"
                alt={data.author_avatar.alt}
                src={data.author_avatar.src}
              />
            </div>
            <div className="min-w-0">
              <h2 className="font-headline-md text-headline-md text-on-surface truncate">
                {data.author_name}
              </h2>
              <p className="font-label-sm text-label-sm text-on-surface-variant truncate">
                {data.author_handle} · {data.watched_on}
              </p>
            </div>
          </Link>

          <Link
            to={`/movie/${data.movie.id}`}
            className="w-full aspect-[2/3] rounded-lg overflow-hidden poster-shadow inner-stroke mb-lg relative bg-surface-variant block"
          >
            <Poster
              image={data.movie.poster}
              className="w-full h-full object-cover absolute inset-0"
            />
          </Link>

          {/* Title & rating */}
          <div className="w-full flex justify-between items-start mb-md">
            <div>
              <Link to={`/movie/${data.movie.id}`}>
                <h1 className="font-headline-lg-mobile text-headline-lg-mobile text-on-surface mb-xs">
                  {data.movie.title}
                </h1>
              </Link>
              <div className="flex flex-wrap gap-sm">
                {data.genres.map((genre) => (
                  <Link
                    key={genre}
                    to={`/search?genre=${encodeURIComponent(genre)}`}
                    className="bg-surface-container-low text-on-surface font-label-sm text-label-sm px-2 py-1 rounded active:opacity-70"
                  >
                    {genre}
                  </Link>
                ))}
                {/* Badge and all, since an empty one would read as a chip with
                    nothing in it. */}
                {data.movie.year !== null && (
                  <span className="bg-surface-container-low text-on-surface font-label-sm text-label-sm px-2 py-1 rounded">
                    {data.movie.year}
                  </span>
                )}
              </div>
            </div>
            {/* No score, nothing here. `null / 2` is 0 in JavaScript, so the old form
                printed "0 / 5" at a review its author never rated. */}
            {data.rating_half_stars !== null && (
              <div className="flex flex-col items-end">
                <StarRating
                  halfStars={data.rating_half_stars}
                  color="text-tertiary"
                  showEmpty={false}
                  className="gap-0"
                />
                <span className="font-label-sm text-label-sm text-on-surface-variant mt-xs">
                  {data.rating_half_stars / 2} / 5
                </span>
              </div>
            )}
          </div>

          <hr className="w-full border-t border-surface-variant my-md" />

          {/* Nothing written means this post is the score. The composer below is
              fixed to the bottom of the screen and still posts, which is the point
              of a bare rating having a page at all. */}
          {data.paragraphs.length > 0 ? (
            <article className="w-full font-body-lg text-body-lg text-on-surface mb-xl">
              {data.paragraphs.map((para, i) => (
                <p key={i} className={i === 0 ? 'mb-md' : undefined}>
                  {para}
                </p>
              ))}
            </article>
          ) : (
            <div className="w-full mb-xl">
              <RatingOnlyNote halfStars={data.rating_half_stars} />
            </div>
          )}

          {/* Comments. Not in the export — see the file header. */}
          {data.comments.length > 0 && (
            <section className="w-full mb-xl">
              <h2 className="font-label-sm text-label-sm text-on-surface-variant uppercase tracking-widest mb-md">
                {data.comments.length === 1 ? '1 Comment' : `${data.comments.length} Comments`}
              </h2>
              <div className="flex flex-col gap-md">
                {data.comments.map((comment) => (
                  <div key={comment.id} className="flex gap-sm">
                    {/* The face leads to its owner's page, as the author's does above.
                        Yours too — the avatar and the handle are real either way. */}
                    <Link
                      to={personPath(comment.author_handle)}
                      className="shrink-0 active:opacity-70"
                    >
                      <img
                        className="w-8 h-8 rounded-full object-cover border border-surface-variant"
                        alt={comment.author_avatar.alt}
                        src={comment.author_avatar.src}
                      />
                    </Link>
                    <div className="min-w-0">
                      <div className="flex items-baseline gap-sm flex-wrap">
                        <Link
                          to={personPath(comment.author_handle)}
                          className="font-label-sm text-label-sm font-bold text-on-surface active:opacity-70"
                        >
                          {authorLabel(comment)}
                        </Link>
                        <Link
                          to={personPath(comment.author_handle)}
                          className="font-label-sm text-label-sm text-on-surface-variant active:opacity-70"
                        >
                          {comment.author_handle}
                        </Link>
                        <span className="font-label-sm text-label-sm text-outline">
                          {comment.timestamp}
                        </span>
                      </div>
                      <p className="font-body-md text-body-md text-on-surface">{comment.body}</p>
                      {/* Replies are read-only here; the desktop screen is where you
                          post one. Without them a thread lost half of itself. */}
                      {comment.replies.map((reply) => (
                        <div
                          key={reply.id}
                          className="mt-sm pl-md border-l border-surface-variant"
                        >
                          <div className="flex items-baseline gap-sm flex-wrap">
                            <Link
                              to={personPath(reply.author_handle)}
                              className="font-label-sm text-label-sm font-bold text-on-surface active:opacity-70"
                            >
                              {authorLabel(reply)}
                            </Link>
                            <Link
                              to={personPath(reply.author_handle)}
                              className="font-label-sm text-label-sm text-on-surface-variant active:opacity-70"
                            >
                              {reply.author_handle}
                            </Link>
                            <span className="font-label-sm text-label-sm text-outline">
                              {reply.timestamp}
                            </span>
                          </div>
                          <p className="font-body-md text-body-md text-on-surface">{reply.body}</p>
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </section>
          )}
        </main>
      )}

      {/* Sticky composer */}
      <div className="fixed bottom-0 left-0 w-full bg-surface/90 backdrop-blur-md border-t border-surface-variant p-margin-mobile z-50">
        {postComment.error && (
          <div className="max-w-md mx-auto w-full mb-sm">
            <ActionError
              message={postComment.error}
              onDismiss={postComment.clearError}
              signIn={postComment.signInRequired}
            />
          </div>
        )}
        <form
          className="flex items-center gap-md max-w-md mx-auto w-full"
          onSubmit={(e) => {
            e.preventDefault()
            void postComment.run()
          }}
        >
          <div className="flex-grow relative">
            <input
              className="w-full bg-surface-container-low border-none rounded-full py-3 px-4 font-body-md text-on-surface focus:ring-1 focus:ring-primary focus:outline-none placeholder-outline"
              placeholder="Add a comment..."
              type="text"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
            />
          </div>
          <button
            className="bg-primary text-on-primary rounded-full w-12 h-12 flex items-center justify-center hover:bg-primary-fixed-dim transition-colors flex-shrink-0 disabled:opacity-40"
            type="submit"
            aria-label="Post comment"
            disabled={postComment.busy || !draft.trim()}
          >
            <span className="material-symbols-outlined">send</span>
          </button>
        </form>
      </div>
    </div>
  )
}
