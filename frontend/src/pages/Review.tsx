/**
 * Friend Review — Desktop. Ported from `reference/cine-journal/review.html`.
 *
 * Serves the `dune-part-two` review: faded backdrop, sticky poster column, and
 * the comment thread with one nested reply.
 *
 * All four actions the export drew as buttons now work — LIKE REVIEW, REPLY,
 * POST, and the per-comment heart. Posting returns the whole review, so the
 * thread and the "Conversation (n)" heading update from one response.
 *
 * The export's REPLY button sat beside LIKE REVIEW with no target; here it
 * focuses the composer, and each comment gets its own inline Reply.
 */
import { useRef, useState } from 'react'
import { Link } from 'react-router-dom'

import type { Comment, Review as ReviewData } from '../api'
import { api } from '../api'
import { useApi } from '../useApi'
import { useAction } from '../useAction'
import { ActionError, BottomNavBar, ErrorNote, Loading, TopAppBar } from '../components/Chrome'
import { StarRating } from '../components/StarRating'

const REVIEW_ID = 'dune-part-two'

function CommentBlock({
  comment,
  onLike,
  likeBusy,
  onReply,
  replyBusy,
}: {
  comment: Comment
  onLike: (id: string) => void
  likeBusy: boolean
  onReply: (id: string, body: string) => Promise<void>
  replyBusy: boolean
}) {
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState('')

  const submit = async () => {
    if (!draft.trim()) return
    await onReply(comment.id, draft)
    // Only clear once the post resolved, so a failure doesn't lose the text.
    setDraft('')
    setOpen(false)
  }

  return (
    <div className="flex space-x-4">
      <img
        className="w-10 h-10 rounded-full object-cover shadow-sm border border-[#F2F2F7] flex-shrink-0"
        alt={comment.author_avatar.alt}
        src={comment.author_avatar.src}
      />
      <div className="flex-grow">
        <div className="flex items-baseline space-x-2 mb-1">
          <span className="font-label-sm text-label-sm font-bold text-on-background">
            {comment.author_name}
          </span>
          <span className="font-label-sm text-label-sm text-on-surface-variant">
            {comment.timestamp}
          </span>
        </div>
        <p className="font-body-md text-body-md text-on-background mb-2">{comment.body}</p>

        <div className="flex items-center space-x-4">
          <button
            onClick={() => onLike(comment.id)}
            disabled={likeBusy}
            aria-pressed={comment.liked}
            className={`font-label-sm text-label-sm transition-colors flex items-center space-x-1 disabled:cursor-wait ${
              comment.liked ? 'text-primary' : 'text-on-surface-variant hover:text-primary'
            }`}
          >
            <span
              className="material-symbols-outlined text-sm"
              style={comment.liked ? { fontVariationSettings: "'FILL' 1" } : undefined}
            >
              favorite
            </span>
            {/* The export hid the count on comments that had none; a liked
                comment always has at least one, so it shows then. */}
            {comment.like_count !== null && <span>{comment.like_count}</span>}
          </button>
          <button
            onClick={() => setOpen((was) => !was)}
            className="font-label-sm text-label-sm text-on-surface-variant hover:text-primary transition-colors flex items-center space-x-1"
          >
            <span className="material-symbols-outlined text-sm">reply</span>
            <span>Reply</span>
          </button>
        </div>

        {open && (
          <div className="mt-3 flex space-x-2">
            <input
              autoFocus
              className="flex-grow bg-surface-container-low border-none rounded-lg px-3 py-2 font-body-md text-body-md text-on-background focus:ring-1 focus:ring-primary placeholder:text-on-surface-variant"
              placeholder={`Reply to ${comment.author_name}…`}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void submit()
                if (e.key === 'Escape') setOpen(false)
              }}
            />
            <button
              onClick={() => void submit()}
              disabled={replyBusy || !draft.trim()}
              className="px-4 py-2 rounded bg-primary text-white font-label-sm text-label-sm hover:bg-primary/90 transition-colors disabled:opacity-40 disabled:cursor-default"
            >
              {replyBusy ? 'POSTING…' : 'POST'}
            </button>
          </div>
        )}

        {comment.replies.map((reply) => (
          <div key={reply.id} className="mt-4 flex space-x-4">
            <img
              className="w-8 h-8 rounded-full object-cover shadow-sm border border-[#F2F2F7] flex-shrink-0"
              alt={reply.author_avatar.alt}
              src={reply.author_avatar.src}
            />
            <div>
              <div className="flex items-baseline space-x-2 mb-1">
                <span className="font-label-sm text-label-sm font-bold text-on-background">
                  {reply.author_name}
                </span>
              </div>
              <p className="font-body-md text-body-md text-on-background">{reply.body}</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

export function Review() {
  const { data, error, loading, update, replace } = useApi(() => api.review(REVIEW_ID))

  const [draft, setDraft] = useState('')
  const composer = useRef<HTMLTextAreaElement>(null)

  // The export's heading reads "Conversation (3)": two top-level comments plus
  // the nested reply, so replies count toward the total.
  const conversationCount = data
    ? data.comments.reduce((n, c) => n + 1 + c.replies.length, 0)
    : 0

  const likeReview = useAction(async () => {
    const previous = { liked: data?.liked ?? false, count: data?.like_count ?? null }
    const setLike = (liked: boolean, count: number | null) => (current: ReviewData) => ({
      ...current,
      liked,
      like_count: count,
    })

    // Optimistic: the count moves with the button, then reconciles. A review
    // that showed no count reads 1 once liked and none again once unliked,
    // matching how the backend hydrates it.
    update(
      setLike(
        !previous.liked,
        previous.liked
          ? previous.count === 1
            ? null
            : (previous.count ?? 1) - 1
          : (previous.count ?? 0) + 1,
      ),
    )
    try {
      const state = await api.likeReview(REVIEW_ID)
      update(setLike(state.liked, state.like_count))
    } catch (cause) {
      update(setLike(previous.liked, previous.count))
      throw cause
    }
  })

  const likeComment = useAction(async (commentId: string) => {
    const target = data?.comments.find((c) => c.id === commentId)
    const previous = { liked: target?.liked ?? false, count: target?.like_count ?? null }
    const setLike = (liked: boolean, count: number | null) => (current: ReviewData) => ({
      ...current,
      comments: current.comments.map((c) =>
        c.id === commentId ? { ...c, liked, like_count: count } : c,
      ),
    })

    // A comment with no count shows 1 once liked, and none again once unliked —
    // matching how the backend hydrates it.
    update(
      setLike(
        !previous.liked,
        previous.liked ? (previous.count === 1 ? null : (previous.count ?? 1) - 1) : (previous.count ?? 0) + 1,
      ),
    )
    try {
      const state = await api.likeComment(REVIEW_ID, commentId)
      update(setLike(state.liked, state.like_count))
    } catch (cause) {
      update(setLike(previous.liked, previous.count))
      throw cause
    }
  })

  // Posting returns the whole review rather than a patch, so there is nothing to
  // apply optimistically — the server's copy replaces ours.
  const postComment = useAction(async () => {
    replace(await api.postComment(REVIEW_ID, draft))
    setDraft('')
  })

  const postReply = useAction(async (commentId: string, body: string) => {
    replace(await api.postReply(REVIEW_ID, commentId, body))
  })

  // A reply's own error surfaces at the top with the others; `run` never throws,
  // so awaiting it inside `CommentBlock` is safe.
  const replyTo = async (commentId: string, body: string) => {
    await postReply.run(commentId, body)
  }

  const actionError =
    likeReview.error ?? likeComment.error ?? postComment.error ?? postReply.error
  const clearActionError = () => {
    likeReview.clearError()
    likeComment.clearError()
    postComment.clearError()
    postReply.clearError()
  }

  return (
    <div className="bg-background text-on-background font-body-md min-h-screen relative overflow-x-hidden">
      <TopAppBar active="friends" showSearch />

      {data?.backdrop && (
        <div
          className="absolute inset-0 w-full h-[60vh] -z-10 bg-cover bg-center fade-backdrop pointer-events-none"
          style={{ backgroundImage: `url('${data.backdrop.src}')` }}
        />
      )}

      {loading && <Loading />}
      {error && <ErrorNote error={error} />}

      {data && (
        <main className="max-w-7xl mx-auto px-margin-mobile md:px-margin-desktop pt-xl pb-xxl relative z-10">
          <Link
            className="inline-flex items-center space-x-2 text-on-surface-variant hover:text-primary transition-colors mb-lg font-label-sm text-label-sm uppercase tracking-widest"
            to="/"
          >
            <span className="material-symbols-outlined text-lg">arrow_back</span>
            <span>Back to Feed</span>
          </Link>

          <div className="grid grid-cols-1 md:grid-cols-12 gap-gutter md:gap-margin-desktop">
            {/* Movie context */}
            <div className="md:col-span-4 hidden md:block">
              <div className="sticky top-xxl">
                <Link
                  to={`/movie/${data.movie.id}`}
                  className="block relative rounded-lg overflow-hidden shadow-[0px_4px_20px_rgba(0,0,0,0.04)] bg-white mb-lg group"
                >
                  <div className="absolute inset-0 border border-black/10 rounded-lg pointer-events-none z-10"></div>
                  <img
                    className="w-full h-auto object-cover aspect-[2/3] transition-transform duration-500 group-hover:scale-105"
                    alt={data.movie.poster.alt}
                    src={data.movie.poster.src}
                  />
                </Link>
                <Link to={`/movie/${data.movie.id}`}>
                  <h3 className="font-headline-md text-headline-md text-on-background mb-xs hover:text-primary transition-colors">
                    {data.movie.title}
                  </h3>
                </Link>
                <p className="font-body-md text-body-md text-on-surface-variant mb-md">
                  {data.director && `Directed by ${data.director} · `}
                  {data.movie.year}
                </p>
                <div className="flex flex-wrap gap-2">
                  {data.genres.map((genre) => (
                    <Link
                      key={genre}
                      to={`/search?genre=${encodeURIComponent(genre)}`}
                      className="px-3 py-1 bg-[#F2F2F7] rounded-full font-label-sm text-label-sm text-on-surface-variant hover:text-primary transition-colors"
                    >
                      {genre}
                    </Link>
                  ))}
                </div>
              </div>
            </div>

            {/* Review + comments */}
            <div className="md:col-span-8">
              {/* Mobile movie context */}
              <Link to={`/movie/${data.movie.id}`} className="md:hidden flex items-center space-x-4 mb-lg">
                <div className="w-16 h-24 rounded shadow-sm relative flex-shrink-0">
                  <div className="absolute inset-0 border border-black/10 rounded pointer-events-none z-10"></div>
                  <img
                    className="w-full h-full object-cover rounded"
                    alt={data.movie.poster.alt}
                    src={data.movie.poster.src}
                  />
                </div>
                <div>
                  <h3 className="font-headline-md text-headline-md text-on-background mb-xs">
                    {data.movie.title}
                  </h3>
                  <p className="font-label-sm text-label-sm text-on-surface-variant">
                    {data.movie.year} · {data.genres[0]}
                  </p>
                </div>
              </Link>

              {/* Reviewer header */}
              <div className="flex items-center justify-between mb-xl border-b border-[#F2F2F7] pb-lg">
                <div className="flex items-center space-x-4">
                  <img
                    className="w-12 h-12 rounded-full object-cover shadow-sm border border-[#F2F2F7]"
                    alt={data.author_avatar.alt}
                    src={data.author_avatar.src}
                  />
                  <div>
                    <h2 className="font-headline-md text-headline-md text-on-background">
                      {data.author_name}
                    </h2>
                    <p className="font-label-sm text-label-sm text-on-surface-variant">
                      {data.watched_on}
                    </p>
                  </div>
                </div>
                <StarRating
                  halfStars={data.rating_half_stars}
                  color="text-primary"
                  showEmpty={false}
                  className="gap-0"
                />
              </div>

              <article className="font-body-lg text-body-lg text-on-background space-y-6 mb-xl leading-relaxed">
                {data.paragraphs.map((para, i) => (
                  <p key={i}>{para}</p>
                ))}
              </article>

              {actionError && (
                <div className="mb-lg">
                  <ActionError message={actionError} onDismiss={clearActionError} />
                </div>
              )}

              {/* Actions */}
              <div className="flex items-center space-x-4 mb-xxl">
                <button
                  onClick={() => likeReview.run()}
                  disabled={likeReview.busy}
                  aria-pressed={data.liked}
                  className={`flex items-center space-x-2 px-6 py-3 rounded font-label-sm text-label-sm transition-colors disabled:cursor-wait ${
                    data.liked
                      ? 'bg-surface-container border border-primary text-primary'
                      : 'bg-primary text-white hover:bg-primary/90'
                  }`}
                >
                  <span
                    className="material-symbols-outlined text-lg"
                    style={data.liked ? { fontVariationSettings: "'FILL' 1" } : undefined}
                  >
                    thumb_up
                  </span>
                  <span>{data.liked ? 'LIKED' : 'LIKE REVIEW'}</span>
                </button>
                <button
                  onClick={() => composer.current?.focus()}
                  className="flex items-center space-x-2 px-6 py-3 rounded border border-outline-variant text-on-background font-label-sm text-label-sm hover:bg-surface-container transition-colors"
                >
                  <span className="material-symbols-outlined text-lg">chat_bubble</span>
                  <span>REPLY</span>
                </button>
                {data.like_count !== null && (
                  <span className="ml-auto font-label-sm text-label-sm text-on-surface-variant">
                    {data.like_count} Likes
                  </span>
                )}
              </div>

              {/* Conversation */}
              <section>
                <h3 className="font-headline-md text-headline-md mb-lg border-b border-[#F2F2F7] pb-sm">
                  Conversation ({conversationCount})
                </h3>

                <div className="flex space-x-4 mb-xl">
                  <div className="w-10 h-10 rounded-full bg-surface-container flex-shrink-0 flex items-center justify-center text-primary font-headline-md">
                    ME
                  </div>
                  <div className="flex-grow">
                    <textarea
                      ref={composer}
                      className="w-full bg-surface-container-low border-none rounded-lg p-4 font-body-md text-on-background focus:ring-1 focus:ring-primary placeholder:text-on-surface-variant resize-none"
                      placeholder="Add your thoughts..."
                      rows={3}
                      value={draft}
                      onChange={(e) => setDraft(e.target.value)}
                      onKeyDown={(e) => {
                        // Enter posts; Shift+Enter keeps the newline.
                        if (e.key === 'Enter' && !e.shiftKey && draft.trim()) {
                          e.preventDefault()
                          void postComment.run()
                        }
                      }}
                    />
                    <div className="flex justify-end mt-2">
                      <button
                        onClick={() => void postComment.run()}
                        disabled={postComment.busy || !draft.trim()}
                        className="px-4 py-2 rounded bg-primary text-white font-label-sm text-label-sm hover:bg-primary/90 transition-colors disabled:opacity-40 disabled:cursor-default"
                      >
                        {postComment.busy ? 'POSTING…' : 'POST'}
                      </button>
                    </div>
                  </div>
                </div>

                <div className="space-y-lg">
                  {data.comments.map((comment, i) => (
                    <div key={comment.id} className="space-y-lg">
                      <CommentBlock
                        comment={comment}
                        onLike={likeComment.run}
                        likeBusy={likeComment.busy}
                        onReply={replyTo}
                        replyBusy={postReply.busy}
                      />
                      {i < data.comments.length - 1 && <hr className="border-[#F2F2F7]" />}
                    </div>
                  ))}
                </div>
              </section>
            </div>
          </div>
        </main>
      )}

      <BottomNavBar active="friends" />
    </div>
  )
}
