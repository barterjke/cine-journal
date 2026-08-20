/**
 * The review screen: who a comment belongs to, and a review with no score.
 *
 * Comment threads are shared content now. They used to be per-viewer, so the server
 * sent "You" as the author's name and "Just now" as the date, and the screen printed
 * both verbatim. It gets real names and real dates, so "You" is derived from `is_you`
 * here — otherwise a stranger's comment carries your label, or yours carries theirs.
 *
 * Stars are queried by their glyph text, because the icon font renders as text:
 * `star` for a full one and `star_half` for the half. No other glyph on these screens
 * is called either.
 */
import { screen } from '@testing-library/react'

import { api } from '../api'
import { Review } from '../pages/Review'
import { ReviewMobile } from '../pages/ReviewMobile'
import { aComment, aReply, aReview, renderScreen } from './support'

vi.mock('../api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api')>()
  const { stubApi } = await import('./api-stub')
  return { ...actual, api: stubApi(actual.api) }
})

const desktop = { path: '/review/:id', at: '/review/elena-solaris' }
const mobile = { path: '/review-mobile/:id', at: '/review-mobile/elena-solaris' }

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
