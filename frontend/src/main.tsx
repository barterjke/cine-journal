import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter, Route, Routes } from 'react-router-dom'

import './index.css'
import { Feed } from './pages/Feed'
import { FeedMobile } from './pages/FeedMobile'
import { MovieDetail } from './pages/MovieDetail'
import { Review } from './pages/Review'
import { ReviewMobile } from './pages/ReviewMobile'
import { Search } from './pages/Search'

// One route per screen in the export. The desktop screens are responsive and
// collapse on narrow viewports; the two `*-mobile` routes are the export's
// separate mobile-only designs, kept as distinct screens rather than merged.
//
// `/movie` with no id falls through to the one film the export detailed.
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Feed />} />
        <Route path="/review" element={<Review />} />
        <Route path="/search" element={<Search />} />
        <Route path="/movie" element={<MovieDetail />} />
        <Route path="/movie/:id" element={<MovieDetail />} />
        <Route path="/feed-mobile" element={<FeedMobile />} />
        <Route path="/review-mobile" element={<ReviewMobile />} />
      </Routes>
    </BrowserRouter>
  </StrictMode>,
)
