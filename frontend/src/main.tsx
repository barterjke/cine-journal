import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter, Route, Routes } from 'react-router-dom'

import './index.css'
import { Collection } from './pages/Collection'
import { Feed } from './pages/Feed'
import { FeedMobile } from './pages/FeedMobile'
import { MovieDetail } from './pages/MovieDetail'
import { People } from './pages/People'
import { Person } from './pages/Person'
import { Profile } from './pages/Profile'
import { Review } from './pages/Review'
import { ReviewMobile } from './pages/ReviewMobile'
import { Search } from './pages/Search'

// One route per screen in the export, plus the two friend screens the export had
// no design for. The desktop screens are responsive and collapse on narrow
// viewports; the two `*-mobile` routes are the export's separate mobile-only
// designs, kept as distinct screens rather than merged.
//
// `/movie` with no id falls through to the one film the export detailed.
// `/people/:handle` is keyed on the nickname, not the id, so the URL is the thing
// you'd search for.
//
// `/review/:id` is where a review card links: `<person_id>-<movie_id>`, which is a
// real address you can share. Bare `/review` opens the newest review instead, which
// is what the feed's "featured review" link means.
//
// `/collections/:slug` is where a profile tile goes — `favorites`, `watchlist` or
// `journal`, with an optional `?person=` for somebody else's. One route for all of
// them because it is one page: the server resolves the title and whose it is.
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Feed />} />
        <Route path="/review" element={<Review />} />
        <Route path="/review/:id" element={<Review />} />
        <Route path="/search" element={<Search />} />
        <Route path="/people" element={<People />} />
        <Route path="/people/:handle" element={<Person />} />
        <Route path="/profile" element={<Profile />} />
        <Route path="/collections/:slug" element={<Collection />} />
        <Route path="/movie" element={<MovieDetail />} />
        <Route path="/movie/:id" element={<MovieDetail />} />
        <Route path="/feed-mobile" element={<FeedMobile />} />
        <Route path="/review-mobile" element={<ReviewMobile />} />
        <Route path="/review-mobile/:id" element={<ReviewMobile />} />
      </Routes>
    </BrowserRouter>
  </StrictMode>,
)
