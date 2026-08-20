/**
 * Reviews: who a comment belongs to, a review with no score, and a rating with
 * nothing written.
 *
 * Comment threads are shared content now. They used to be per-viewer, so the server
 * sent "You" as the author's name and "Just now" as the date, and the screen printed
 * both verbatim. It gets real names and real dates, so "You" is derived from `is_you`
 * here — otherwise a stranger's comment carries your label, or yours carries theirs.
 *
 * A review is a rating, or text, or both. So a bare score is a post: `body` is null
 * on the card, `paragraphs` is empty on the page, and both have to look deliberate
 * and stay likeable and repliable. That is the last group of cases here, and the card
 * is mounted on its own — three screens draw it, and this is the same component on
 * all of them.
 *
 * Stars are queried by their glyph text, because the icon font renders as text:
 * `star` for a full one and `star_half` for the half. No other glyph on these screens
 * is called either.
 */
import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { api } from '../api'
import { ReviewCard } from '../components/People'
import { Review } from '../pages/Review'
import { ReviewMobile } from '../pages/ReviewMobile'
import { aComment, aReply, aReview, aUserReview, renderScreen } from './support'

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

const desktop = { path: '/review/:id', at: '/review/elena-solaris' }
const mobile = { path: '/review-mobile/:id', at: '/review-mobile/elena-solaris' }

/** A review card on its own, at an address that isn't one of its links. */
const card = { path: '/movie/:id', at: '/movie/solaris' }

/**
 * Every `<p>` that was drawn, as text.
 *
 * An empty one is the gap this change removed: a card whose prose block rendered
 * with nothing in it.
 */
function paragraphs(container: HTMLElement): string[] {
  return [...container.querySelectorAll('p')].map((p) => p.textContent ?? '')
}

describe("a review's comment thread", () => {
  it('credits a comment and its reply to the people who wrote them', async () => {
    vi.mocked(api.reviewOrNewest).mockResolvedValue(
      aReview({ comments: [aComment({ replies: [aReply()] })] }),
    )

    renderScreen(<Review />, desktop)

    expect(await screen.findByText('Nadia Halim')).toBeInTheDocument()
    expect(screen.getByText('Theo Marchetti')).toBeInTheDocument()
    // The old bug in reverse: neither row is the viewer's, so neither says "You".
    expect(screen.queryByText('You')).not.toBeInTheDocument()
    // Their real date, not the constant "Just now" the per-viewer thread sent.
    expect(screen.getByText('August 20, 2026')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: '@nadia' })).toHaveAttribute('href', '/people/nadia')
  })

  it('says "You" on the viewer\'s own comment, and still links to their page', async () => {
    // `is_you` is the only difference. The name on the wire is always the real one.
    vi.mocked(api.reviewOrNewest).mockResolvedValue(
      aReview({ comments: [aComment({ is_you: true })] }),
    )

    renderScreen(<Review />, desktop)

    expect(await screen.findByText('You')).toBeInTheDocument()
    expect(screen.queryByText('Nadia Halim')).not.toBeInTheDocument()
    // The avatar and the handle are valid either way, so your own row is not a dead end.
    expect(screen.getByRole('link', { name: '@nadia' })).toHaveAttribute('href', '/people/nadia')
    expect(screen.getByAltText('Portrait of Nadia')).toBeInTheDocument()
  })

  it('shows how many people liked a comment, not whether the viewer did', async () => {
    vi.mocked(api.reviewOrNewest).mockResolvedValue(
      aReview({ comments: [aComment({ like_count: 2, liked: true })] }),
    )

    renderScreen(<Review />, desktop)

    // The heart's glyph is part of the button's name, because the icon font renders as
    // text. The total already counts the viewer's like; adding it again would read 3.
    const like = await screen.findByRole('button', { name: /^favorite2$/ })
    expect(like).toHaveAttribute('aria-pressed', 'true')
    expect(screen.queryByRole('button', { name: /^favorite3$/ })).not.toBeInTheDocument()
  })
})

describe('a review written without a score', () => {
  it('draws no stars on the desktop screen', async () => {
    vi.mocked(api.reviewOrNewest).mockResolvedValue(aReview({ rating_half_stars: null }))

    renderScreen(<Review />, desktop)

    await screen.findByText('Elena Vasquez')
    // Not five empty ones: a film logged without a score is not a film scored zero.
    expect(screen.queryAllByText('star')).toHaveLength(0)
    expect(screen.queryByText('star_half')).not.toBeInTheDocument()
  })

  it('draws them on the desktop screen when there is a score', async () => {
    vi.mocked(api.reviewOrNewest).mockResolvedValue(aReview({ rating_half_stars: 9 }))

    renderScreen(<Review />, desktop)

    await screen.findByText('Elena Vasquez')
    expect(screen.getAllByText('star')).toHaveLength(4)
    expect(screen.getByText('star_half')).toBeInTheDocument()
  })

  it('prints no score line on the mobile screen', async () => {
    vi.mocked(api.reviewOrNewest).mockResolvedValue(aReview({ rating_half_stars: null }))

    renderScreen(<ReviewMobile />, mobile)

    await screen.findByText('Elena Vasquez')
    // `null / 2` is 0 in JavaScript, so the unguarded form said "0 / 5" here.
    expect(screen.queryByText('0 / 5')).not.toBeInTheDocument()
    expect(screen.queryAllByText('star')).toHaveLength(0)
  })

  it('prints it on the mobile screen when there is a score', async () => {
    vi.mocked(api.reviewOrNewest).mockResolvedValue(aReview({ rating_half_stars: 9 }))

    renderScreen(<ReviewMobile />, mobile)

    expect(await screen.findByText('4.5 / 5')).toBeInTheDocument()
  })
})

describe('a review card for a rating with nothing written', () => {
  it('states the score in place of the prose, and draws no empty block', () => {
    const { container } = renderScreen(<ReviewCard review={aUserReview({ body: null })} />, card)

    // The stars are the score; the line says so again in words, so the space where
    // the prose would be is filled by something deliberate.
    expect(screen.getAllByText('star')).toHaveLength(4)
    expect(screen.getByText('star_half')).toBeInTheDocument()
    expect(paragraphs(container)).toEqual(['Rated 4.5 / 5 · nothing written'])
    // The clamped prose block is not rendered at all, empty or otherwise.
    expect(container.querySelector('.line-clamp-4')).toBeNull()
  })

  it('still links to the review page, where the likes and the replies are', () => {
    renderScreen(<ReviewCard review={aUserReview({ body: null })} />, card)

    // The whole point of the change: a score is a post you can engage with. So the
    // link stays — it just stops promising text nobody wrote.
    const link = screen.getByRole('link', { name: 'Like or reply →' })
    expect(link).toHaveAttribute('href', '/review/elena-solaris')
    expect(screen.queryByText('Read full review →')).not.toBeInTheDocument()
  })

  it('offers the whole text when there is some', () => {
    const { container } = renderScreen(<ReviewCard review={aUserReview()} />, card)

    expect(screen.getByText('A cold film that stays warm in the memory.')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Read full review →' })).toHaveAttribute(
      'href',
      '/review/elena-solaris',
    )
    expect(container.querySelector('.line-clamp-4')).not.toBeNull()
  })

  it('prints no date for a row that has none', () => {
    // Old rows have no stored date and the API sends "" for them.
    const { container } = renderScreen(
      <ReviewCard review={aUserReview({ written_on: '' })} />,
      card,
    )

    expect(screen.queryByText('12 November 2014')).not.toBeInTheDocument()
    // And nothing blank where the date used to be.
    const spans = [...container.querySelectorAll('span')].map((span) => span.textContent)
    expect(spans).not.toContain('')
  })
})

describe('the review page for a rating with nothing written', () => {
  const scoreOnly = aReview({ paragraphs: [] })

  it('shows the score as the content, and invents no prose', async () => {
    vi.mocked(api.reviewOrNewest).mockResolvedValue(scoreOnly)

    const { container } = renderScreen(<Review />, desktop)

    expect(await screen.findByText('Rated 4.5 / 5')).toBeInTheDocument()
    expect(screen.getByText('Nothing written. Like it or reply below.')).toBeInTheDocument()
    // No `<article>` at all rather than an empty one, and no sentence pretending to
    // be theirs.
    expect(container.querySelector('article')).toBeNull()
  })

  it('keeps the like button working', async () => {
    vi.mocked(api.reviewOrNewest).mockResolvedValue(scoreOnly)
    vi.mocked(api.likeReview).mockResolvedValue({
      id: 'elena-solaris',
      liked: true,
      like_count: 1,
    })

    renderScreen(<Review />, desktop)

    await userEvent.click(await screen.findByRole('button', { name: /LIKE REVIEW/ }))

    expect(api.likeReview).toHaveBeenCalledWith('elena-solaris')
    expect(await screen.findByRole('button', { name: /LIKED/ })).toBeInTheDocument()
    expect(screen.getByText('1 Likes')).toBeInTheDocument()
  })

  it('posts a comment on it', async () => {
    vi.mocked(api.reviewOrNewest).mockResolvedValue(scoreOnly)
    vi.mocked(api.postComment).mockResolvedValue(
      aReview({ paragraphs: [], comments: [aComment({ body: 'I liked this one too.' })] }),
    )

    renderScreen(<Review />, desktop)

    await userEvent.type(
      await screen.findByPlaceholderText('Add your thoughts...'),
      'I liked this one too.',
    )
    await userEvent.click(screen.getByRole('button', { name: 'POST' }))

    expect(api.postComment).toHaveBeenCalledWith('elena-solaris', 'I liked this one too.')
    expect(await screen.findByText('I liked this one too.')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Conversation (1)' })).toBeInTheDocument()
  })

  it('does the same on the mobile screen', async () => {
    vi.mocked(api.reviewOrNewest).mockResolvedValue(scoreOnly)
    vi.mocked(api.postComment).mockResolvedValue(
      aReview({ paragraphs: [], comments: [aComment({ body: 'Agreed.' })] }),
    )

    const { container } = renderScreen(<ReviewMobile />, mobile)

    expect(await screen.findByText('Rated 4.5 / 5')).toBeInTheDocument()
    expect(container.querySelector('article')).toBeNull()

    await userEvent.type(screen.getByPlaceholderText('Add a comment...'), 'Agreed.')
    await userEvent.click(screen.getByRole('button', { name: 'Post comment' }))

    expect(api.postComment).toHaveBeenCalledWith('elena-solaris', 'Agreed.')
    expect(await screen.findByText('Agreed.')).toBeInTheDocument()
  })
})
