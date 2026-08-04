//! The demo dataset, transcribed verbatim from the four reference screens in
//! `reference/cine-journal/`.
//!
//! Titles, years, blurbs, timestamps, review prose, comment bodies, ratings and
//! image alt text are byte-for-byte what the static HTML rendered — the point of
//! this crate is that the React frontend looks identical to the export. If you
//! change a string here, the fidelity claim in the README stops holding.
//!
//! Ratings use half-star units (see `models`): 10 = five filled stars,
//! 9 = four filled + one half, 6 = three filled, and so on.

use crate::models::*;

/// The desktop feed: `reference/cine-journal/index.html`.
pub fn feed() -> Feed {
    Feed { live: live_discussions(), recent: recent_entries(), friend_activity: friend_activity() }
}

/// The mobile feed: `reference/cine-journal/feed-mobile.html`.
pub fn mobile_feed() -> MobileFeed {
    MobileFeed { stories: stories(), items: mobile_items() }
}

/// Both review screens, desktop first.
pub fn reviews() -> Vec<Review> {
    vec![dune_review(), architecture_review()]
}

pub fn review_by_id(id: &str) -> Option<Review> {
    reviews().into_iter().find(|r| r.id == id)
}

/// Every movie that has a detail page — which is all of them, since every id
/// resolves (see `movie_detail_by_id`).
pub fn movie_details() -> Vec<MovieDetail> {
    catalogue().iter().map(|entry| detail_for(&entry.id, &entry.title)).collect()
}

/// The detail page for any id at all.
///
/// Only Neon Reverie was actually designed, so every film borrows its synopsis,
/// cast, gallery and credits; the id and title are the only things that vary. An
/// unknown id gets a title guessed from the slug rather than a 404, so links
/// from the feed and search always land somewhere.
pub fn movie_detail_by_id(id: &str) -> MovieDetail {
    let title = catalogue()
        .into_iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.title)
        .unwrap_or_else(|| title_from_slug(id));
    detail_for(id, &title)
}

/// "the-silence-of-space" -> "The Silence Of Space".
///
/// Only reached for ids that aren't in the catalogue, so it never overrides a
/// real title — it just keeps hand-typed URLs from looking broken.
fn title_from_slug(id: &str) -> String {
    let words: Vec<String> = id
        .split(['-', '_', '/'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();

    if words.is_empty() {
        "Untitled".into()
    } else {
        words.join(" ")
    }
}

/// How many cards one page of search results holds. Eight fills the export's
/// four-column grid exactly twice.
const PAGE_SIZE: usize = 8;

/// The search screen: `reference/stitch_lumi_cinema_social 2/movie_search_desktop/`.
///
/// This filters for real, which means the screen no longer opens on the frozen
/// state the export drew ("Showing 12 results for Space Exploration", Sci-Fi and
/// 2010s pre-selected, four cards). That state isn't self-consistent — three of
/// its four cards fall outside the 2010s and one is rated 0.0 against a 3-star
/// minimum — so it can't survive a filter that actually runs. The default view
/// is now the unfiltered catalogue instead, and the export's state is reachable
/// by setting those filters by hand.
pub fn search(query: &SearchQuery) -> SearchResponse {
    let text = query.q.as_deref().unwrap_or("").trim().to_lowercase();
    let genre = query.genre.as_deref().filter(|g| !g.is_empty());
    let year = query.year.as_deref().filter(|y| !y.is_empty());
    let min_rating = query.min_rating.unwrap_or(0);

    let matched: Vec<CatalogueEntry> = catalogue()
        .into_iter()
        .filter(|entry| entry.matches_text(&text))
        .filter(|entry| genre.is_none_or(|g| entry.has_genre(g)))
        .filter(|entry| year.is_none_or(|y| entry.in_decade(y)))
        .filter(|entry| entry.meets_minimum(min_rating))
        .collect();

    // At least one page, so the paginator still renders when nothing matched.
    let page_count = matched.len().div_ceil(PAGE_SIZE).max(1);
    let page = query.page.unwrap_or(1).clamp(1, page_count as u32);
    let start = (page as usize - 1) * PAGE_SIZE;

    let results = matched
        .iter()
        .skip(start)
        .take(PAGE_SIZE)
        .map(CatalogueEntry::to_search_result)
        .collect();

    SearchResponse {
        query: query.q.clone().unwrap_or_default(),
        total_results: matched.len() as u32,
        results,
        filters: facets(&text, genre, year, min_rating),
        page,
        page_count: page_count as u32,
    }
}

/// The sidebar's chips, with a count beside each.
///
/// Counts are leave-one-out: a genre chip's count ignores the current genre
/// selection but honours the query, the decade and the rating floor. Without
/// that, every unselected chip would read 0 as soon as one was picked.
fn facets(text: &str, genre: Option<&str>, year: Option<&str>, min_rating: u8) -> SearchFilters {
    let pool: Vec<CatalogueEntry> = catalogue()
        .into_iter()
        .filter(|entry| entry.matches_text(text))
        .filter(|entry| entry.meets_minimum(min_rating))
        .collect();

    let genres = GENRE_FACETS
        .iter()
        .map(|label| GenreFacet {
            label: (*label).into(),
            selected: genre == Some(label),
            count: pool
                .iter()
                .filter(|entry| entry.has_genre(label))
                .filter(|entry| year.is_none_or(|y| entry.in_decade(y)))
                .count() as u32,
        })
        .collect();

    let years = YEAR_FACETS
        .iter()
        .map(|label| YearFacet {
            label: (*label).into(),
            selected: year == Some(label),
            count: pool
                .iter()
                .filter(|entry| entry.in_decade(label))
                .filter(|entry| genre.is_none_or(|g| entry.has_genre(g)))
                .count() as u32,
        })
        .collect();

    SearchFilters { genres, years, minimum_rating_stars: min_rating }
}

// --- Desktop feed: "Live Now" -------------------------------------------------

fn live_discussions() -> Vec<LiveDiscussion> {
    vec![
        LiveDiscussion {
            id: "live-silence-of-space".into(),
            movie: Movie {
                id: "silence-of-space".into(),
                title: "The Silence of Space".into(),
                year: Some(2024),
                poster: poster_of("silence-of-space"),
            },
            rating_half_stars: 9,
            blurb: "Join the discussion room. 142 members currently debating the ambiguous ending."
                .into(),
            participants: vec![
                Image::new(
                    "img/avatar-live-1.jpg",
                    "A black and white studio portrait of a woman looking thoughtfully off-camera.",
                ),
                Image::new(
                    "img/avatar-live-2.jpg",
                    "A black and white studio portrait of a man with glasses looking directly at the camera.",
                ),
            ],
            overflow_count: Some(14),
        },
        LiveDiscussion {
            id: "live-morning-haze".into(),
            movie: Movie {
                id: "morning-haze".into(),
                title: "Morning Haze".into(),
                year: Some(1998),
                poster: poster_of("morning-haze"),
            },
            rating_half_stars: 10,
            blurb: "Live watch party starting in 10 minutes. Grab your coffee.".into(),
            participants: vec![Image::new(
                "img/avatar-live-3.jpg",
                "A black and white studio portrait of a woman laughing.",
            )],
            overflow_count: None,
        },
    ]
}

// --- Desktop feed: "Recent Entries" ------------------------------------------

fn recent_entries() -> Vec<FeedEntry> {
    // `on_watchlist` is false here for every tile; `hydrate` sets it per request.
    vec![
        FeedEntry {
            id: "entry-le-souffle".into(),
            movie: Movie {
                id: "le-souffle".into(),
                title: "Le Souffle".into(),
                year: Some(1960),
                poster: poster_of("le-souffle"),
            },
            rating_half_stars: 10,
            on_watchlist: false,
        },
        FeedEntry {
            id: "entry-the-drop".into(),
            movie: Movie {
                id: "the-drop".into(),
                title: "The Drop".into(),
                year: Some(2023),
                poster: poster_of("the-drop"),
            },
            rating_half_stars: 6,
            on_watchlist: false,
        },
        FeedEntry {
            id: "entry-estate-of-mind".into(),
            movie: Movie {
                id: "estate-of-mind".into(),
                title: "Estate of Mind".into(),
                year: Some(2019),
                poster: poster_of("estate-of-mind"),
            },
            rating_half_stars: 10,
            on_watchlist: false,
        },
        FeedEntry {
            id: "entry-blue-notes".into(),
            movie: Movie {
                id: "blue-notes".into(),
                title: "Blue Notes".into(),
                year: Some(2021),
                poster: poster_of("blue-notes"),
            },
            rating_half_stars: 6,
            on_watchlist: false,
        },
    ]
}

// --- Desktop feed: "Friends Activity" sidebar --------------------------------

fn friend_activity() -> Vec<FriendActivity> {
    vec![
        FriendActivity {
            id: "activity-alex".into(),
            author_name: "Alex M.".into(),
            author_avatar: Image::new(
                "img/avatar-alex-m.jpg",
                "A bright, airy profile photo of a young man smiling outdoors in soft sunlight.",
            ),
            timestamp: "2h ago".into(),
            kind: ActivityKind::Watched,
            movie_id: "silence-of-space".into(),
            movie_title: "The Silence of Space".into(),
            rating_half_stars: Some(10),
            quote: Some(
                "\"A masterpiece of visual storytelling. The silence is deafening in the best way possible.\""
                    .into(),
            ),
        },
        FriendActivity {
            id: "activity-sarah".into(),
            author_name: "Sarah K.".into(),
            author_avatar: Image::new(
                "img/avatar-sarah-k.jpg",
                "A bright, high-key studio portrait of a woman with short hair against a crisp white background.",
            ),
            timestamp: "5h ago".into(),
            kind: ActivityKind::AddedToWatchlist,
            movie_id: "morning-haze".into(),
            movie_title: "Morning Haze".into(),
            rating_half_stars: None,
            quote: None,
        },
        FriendActivity {
            id: "activity-david".into(),
            author_name: "David P.".into(),
            author_avatar: Image::new(
                "img/avatar-david-p.jpg",
                "A black and white portrait photo of a man looking off to the side with cinematic lighting.",
            ),
            timestamp: "Yesterday".into(),
            kind: ActivityKind::Watched,
            movie_id: "le-souffle".into(),
            movie_title: "Le Souffle".into(),
            rating_half_stars: Some(6),
            quote: Some("\"Style over substance, perhaps. But what style it is.\"".into()),
        },
    ]
}

// --- Mobile feed: stories rail ----------------------------------------------

fn stories() -> Vec<Story> {
    vec![
        Story {
            id: "story-elena".into(),
            name: "Elena".into(),
            avatar: Image::new(
                "img/avatar-story-elena.jpg",
                "A close up portrait of a young woman with short dark hair against a stark white background.",
            ),
            unseen: true,
        },
        Story {
            id: "story-marcus".into(),
            name: "Marcus".into(),
            avatar: Image::new(
                "img/avatar-story-marcus.jpg",
                "A black and white portrait of a man looking thoughtfully off-camera, wearing a simple dark turtleneck.",
            ),
            unseen: true,
        },
        Story {
            id: "story-sarah".into(),
            name: "Sarah".into(),
            avatar: Image::new(
                "img/avatar-story-sarah.jpg",
                "A casual portrait of a person with glasses, brightly lit in an airy, minimalist space.",
            ),
            unseen: false,
        },
        Story {
            id: "story-david".into(),
            name: "David".into(),
            avatar: Image::new(
                "img/avatar-story-david.jpg",
                "A profile picture showing a silhouette of a person against a bright window.",
            ),
            unseen: false,
        },
        Story {
            id: "story-anna".into(),
            name: "Anna".into(),
            avatar: Image::new(
                "img/avatar-story-anna.jpg",
                "A minimalist abstract avatar featuring simple geometric shapes in primary blue and slate white.",
            ),
            unseen: false,
        },
    ]
}

// --- Mobile feed: poster grid -----------------------------------------------

fn mobile_items() -> Vec<MobileFeedItem> {
    vec![
        MobileFeedItem {
            id: "mobile-the-horizon".into(),
            movie: Movie {
                id: "the-horizon".into(),
                title: "The Horizon".into(),
                year: None,
                poster: poster_of("the-horizon"),
            },
            subtitle: "Elena watched • 4h ago".into(),
            rating_half_stars: Some(8),
            on_watchlist: false,
        },
        MobileFeedItem {
            id: "mobile-fractured".into(),
            movie: Movie {
                id: "fractured".into(),
                title: "Fractured".into(),
                year: None,
                poster: poster_of("fractured"),
            },
            subtitle: "Marcus rated • 5h ago".into(),
            rating_half_stars: Some(6),
            on_watchlist: false,
        },
        MobileFeedItem {
            id: "mobile-red-shift".into(),
            movie: Movie {
                id: "red-shift".into(),
                title: "Red Shift".into(),
                year: None,
                poster: poster_of("red-shift"),
            },
            subtitle: "Anna added to watchlist".into(),
            rating_half_stars: None,
            on_watchlist: false,
        },
        MobileFeedItem {
            id: "mobile-endless".into(),
            movie: Movie {
                id: "endless".into(),
                title: "Endless".into(),
                year: None,
                poster: poster_of("endless"),
            },
            subtitle: "David wrote a review".into(),
            rating_half_stars: Some(10),
            on_watchlist: false,
        },
    ]
}

// --- Reviews ----------------------------------------------------------------

/// `reference/cine-journal/review.html` — Elena Rostova on Dune: Part Two.
///
/// Note the prose uses typographic apostrophes and an em dash exactly as the
/// export did (he's, doesn't, "be—the thumpers").
fn dune_review() -> Review {
    let elena_avatar_alt = "A close-up portrait of a young woman with a sharp, modern haircut wearing minimalist geometric glasses against a pure white background.";

    Review {
        id: "dune-part-two".into(),
        movie: Movie {
            id: "dune-part-two".into(),
            title: "Dune: Part Two".into(),
            year: Some(2024),
            poster: poster_of("dune-part-two"),
        },
        backdrop: Some(Image::new("img/backdrop-dune.jpg", "")),
        director: Some("Denis Villeneuve".into()),
        genres: vec!["Sci-Fi".into(), "Adventure".into()],
        author_name: "Elena Rostova".into(),
        author_avatar: Image::new("img/avatar-elena-rostova.jpg", elena_avatar_alt),
        watched_on: "Watched on March 15, 2024".into(),
        rating_half_stars: 9,
        paragraphs: vec![
            "Villeneuve has managed something nearly impossible here: he\u{2019}s taken the dense, almost impenetrable lore of Herbert\u{2019}s universe and rendered it not just coherent, but profoundly emotional. The sheer scale of Arrakis is felt in every frame, thanks to Fraser\u{2019}s monolithic cinematography.".into(),
            "What struck me most on this viewing was the sound design. It\u{2019}s oppressive when it needs to be\u{2014}the thumpers echoing through the floorboards of the theater\u{2014}but incredibly delicate in moments of intimacy between Paul and Chani. The score doesn't just accompany the film; it acts as a geological force within it.".into(),
            "While the pacing sags slightly in the second act during the sietch politics, the convergence of all these massive thematic plates in the finale is breathtaking. A true cinematic spectacle that demands to be seen on the largest screen possible.".into(),
        ],
        like_count: Some(24),
        comments: vec![
            Comment {
                id: "comment-marcus".into(),
                author_name: "Marcus".into(),
                author_avatar: Image::new(
                    "img/avatar-marcus.jpg",
                    "A candid profile photo of a young man wearing a simple black t-shirt against a clean white background.",
                ),
                timestamp: "2 hours ago".into(),
                body: "Completely agree about the sound design. The Harkonnen arena sequence shook my bones."
                    .into(),
                like_count: Some(2),
                replies: vec![],
                liked: false,
            },
            Comment {
                id: "comment-sarah-j".into(),
                author_name: "Sarah J.".into(),
                author_avatar: Image::new(
                    "img/avatar-sarah-j.jpg",
                    "An artistic, high-key portrait of a person with silver-dyed hair wearing a pristine white mock-neck sweater.",
                ),
                timestamp: "5 hours ago".into(),
                body: "I actually liked the sietch politics! Felt it gave necessary weight to Paul's integration. But yes, overall masterpiece."
                    .into(),
                like_count: None,
                replies: vec![Reply {
                    id: "reply-elena".into(),
                    author_name: "Elena (Author)".into(),
                    author_avatar: Image::new(
                        "img/avatar-elena-rostova-sm.jpg",
                        elena_avatar_alt,
                    ),
                    body: "Fair point! I just felt the pacing dragged a tiny bit compared to the breathless first act."
                        .into(),
                }],
                liked: false,
            },
        ],
        hashtags: vec![],
        liked: false,
    }
}

/// `reference/cine-journal/review-mobile.html` — Alex Mercer on The Architecture
/// of Silence. The heading says "Conversation (3)" on the desktop screen; this
/// one ships no comment thread, only the sticky composer.
fn architecture_review() -> Review {
    Review {
        id: "architecture-of-silence".into(),
        movie: Movie {
            id: "architecture-of-silence".into(),
            title: "The Architecture of Silence".into(),
            year: Some(2023),
            poster: poster_of("architecture-of-silence"),
        },
        backdrop: None,
        director: None,
        genres: vec!["Documentary".into()],
        author_name: "Alex Mercer".into(),
        author_avatar: Image::new(
            "img/avatar-alex-mercer.jpg",
            "A portrait of a young person in a brightly lit, modern setting wearing stylish minimalist clothing.",
        ),
        watched_on: "Reviewed yesterday".into(),
        rating_half_stars: 9,
        paragraphs: vec![
            "An absolute masterclass in visual storytelling. The director manages to convey profound isolation without uttering a single line of dialogue for the first forty minutes.".into(),
            "I was particularly struck by the framing—every shot feels like it could be hung in a gallery. The use of negative space in the urban environments perfectly mirrors the protagonist's internal emptiness. It's not a fast-paced film, but if you let it wash over you, it's incredibly moving.".into(),
        ],
        like_count: None,
        comments: vec![],
        hashtags: vec!["#Cinematography".into(), "#MustWatch".into()],
        liked: false,
    }
}

// --- Movie detail -------------------------------------------------------------

/// The demo film, laid out for `reference/movie page/code.html`.
///
/// Alt text is the export's `data-alt` prompts verbatim, and everything invented
/// here belongs to Neon Reverie — the one film the Stitch export actually
/// designed. Every other film is served this same page under its own id and
/// title; where the catalogue knows a film's real year, genres and poster those
/// are used, and the rest (synopsis, cast, credits, score, runtime) is Neon
/// Reverie's.
///
/// The invented numbers are chosen to exercise the layout rather than to
/// flatter it: a score with a decimal that half-stars would round away, a vote
/// count that needs thousands separators, and a certification, so the metadata
/// line renders all three of its segments in demo mode too.
fn detail_for(id: &str, title: &str) -> MovieDetail {
    let listed = catalogue().into_iter().find(|entry| entry.id == id);

    MovieDetail {
        id: id.into(),
        title: title.into(),
        year: listed.as_ref().map_or(2024, |entry| entry.year),
        certification: Some("R".into()),
        runtime: "1h 58m".into(),
        genres: listed
            .as_ref()
            .filter(|entry| !entry.genres.is_empty())
            .map(|entry| entry.genres.clone())
            .unwrap_or_else(|| vec!["Sci-Fi".into(), "Noir".into(), "Thriller".into()]),
        poster: listed.and_then(|entry| entry.poster).unwrap_or_else(|| {
            Image::new(
                "img/poster-neon-reverie.jpg",
                "A minimalist, high-contrast poster design for a neo-noir film. The poster features strong vertical lines and a stark, two-tone color palette of deep blue and crisp white. A solitary silhouette is placed asymmetrically in the lower third, surrounded by vast, empty space. The typography, if imagined, is clean and sans-serif. The style is modern, editorial, and sophisticated, avoiding any cluttered blockbuster tropes.",
            )
        }),
        backdrop: Image::new(
            "img/backdrop-neon-reverie.jpg",
            "A sweeping, cinematic still frame from a high-end neo-noir film. A lone figure stands in a rain-slicked city street at night, illuminated by the soft, diffused glow of distant neon signs. The composition is expansive and minimalist, dominated by deep, inky blacks and cool blues, with a single striking red accent in the neon light. The overall mood is moody, atmospheric, and highly stylized, fitting a premium editorial film journal aesthetic.",
        ),
        synopsis: "In a sprawling, rain-drenched metropolis where memories can be synthesized and sold, a disgraced detective is hired to locate a missing archivalist. His investigation leads him into the labyrinthine underground of memory-smugglers, forcing him to confront the fabricated reality of his own past. \"Neon Reverie\" is a slow-burn exploration of identity, truth, and the cost of forgetting, presented with breathtaking visual restraint.".into(),
        cast: vec![
            CastMember {
                id: "julian-black".into(),
                name: "Julian Black".into(),
                role: "Det. Corvis".into(),
                portrait: Image::new(
                    "img/cast-julian-black.jpg",
                    "A stark, high-contrast black and white studio portrait of a male actor in his late 40s. The lighting is dramatic, highlighting the texture of his skin and the intensity in his eyes. The background is pure white, adhering to the minimalist editorial aesthetic. The composition is focused and intimate, reminiscent of a high-end magazine photoshoot.",
                ),
            },
            CastMember {
                id: "maya-lin".into(),
                name: "Maya Lin".into(),
                role: "The Archivalist".into(),
                portrait: Image::new(
                    "img/cast-maya-lin.jpg",
                    "A clean, minimalist studio portrait of a female actress in her 30s. Soft, diffused lighting creates a gentle, almost ethereal mood against a pristine white background. Her expression is enigmatic. The image is rendered in black and white, maintaining the sophisticated, gallery-like visual style of the platform. The focus is entirely on her face.",
                ),
            },
            CastMember {
                id: "arthur-vance".into(),
                name: "Arthur Vance".into(),
                role: "Synthesizer".into(),
                portrait: Image::new(
                    "img/cast-arthur-vance.jpg",
                    "A high-contrast, editorial-style headshot of a mature male actor with a weathered face. Shot against a bright white backdrop, the black and white image emphasizes the lines and character of his features. The lighting is direct but controlled, fitting a premium, minimalist design system. He looks slightly off-camera.",
                ),
            },
            CastMember {
                id: "leo-thorne".into(),
                name: "Leo Thorne".into(),
                role: "Runner".into(),
                portrait: Image::new(
                    "img/cast-leo-thorne.jpg",
                    "A minimalist, bright studio portrait of a young male actor. The aesthetic is clean and modern, utilizing a pure white background and soft, even lighting. The black and white treatment gives it a timeless, sophisticated editorial feel. His expression is serious and intense.",
                ),
            },
        ],
        score: 7.8,
        vote_count: 12_450,
        details: vec![
            DetailFact { label: "Director".into(), value: "Elara Vance".into() },
            DetailFact {
                label: "Writers".into(),
                value: "Elara Vance, Idris Okonkwo".into(),
            },
            DetailFact { label: "Cinematography".into(), value: "Sarah Chen".into() },
            DetailFact { label: "Music".into(), value: "Trent Reznor".into() },
            DetailFact { label: "Production".into(), value: "Aether Films".into() },
        ],
        // No video was ever invented for the demo film, and a play button over a
        // still that plays nothing is the kind of dead end the export's mocks
        // could afford and an SPA can't. `None` hides the Media block, which is
        // the same thing that happens for a real film TMDB has no trailer for.
        trailer: None,
        watch_options: vec![
            WatchOption {
                provider: "Aether Stream".into(),
                kind: "Stream".into(),
                // No logo files exist for the invented services, which also
                // exercises the frontend's generic-glyph fallback in demo mode.
                logo: None,
            },
            WatchOption { provider: "Kino Rental".into(), kind: "Rent".into(), logo: None },
        ],
        // Nowhere to link: TMDB's watch page is the only permitted destination
        // and these two services don't exist.
        watch_link: None,
        // Both come from the store — `hydrate` fills them per request.
        on_watchlist: false,
        your_rating_half_stars: None,
    }
}

// --- The catalogue ------------------------------------------------------------

/// The sidebar's genre chips, in the export's order.
///
/// Public because TMDB mode counts the same chips over its own candidate window
/// (`content::facets`) — the sidebar's vocabulary is one list either way.
pub const GENRE_FACETS: [&str; 5] = ["Drama", "Sci-Fi", "Thriller", "Romance", "Documentary"];

/// The sidebar's decade radios, in the export's order.
///
/// The export offered exactly these three, so films outside them (Le Souffle,
/// 1960; Morning Haze, 1998) are only reachable with no decade selected. Adding
/// decades would mean redrawing a sidebar the reference fixed at three rows.
pub const YEAR_FACETS: [&str; 3] = ["2020s", "2010s", "2000s"];

/// One searchable film.
///
/// This is the only list of films in the crate — the feed, mobile feed and search
/// screens all draw their posters from here via `poster_of`, so each alt string
/// is transcribed once.
#[derive(Debug, Clone)]
pub struct CatalogueEntry {
    pub id: String,
    pub title: String,
    /// Used for the decade filter and the year printed on result cards.
    ///
    /// The mobile feed's own cards still show no year (`Movie.year` is `None`
    /// there, as the export drew it); these are only for search. Where the
    /// export never stated a year — the four mobile-feed films — one is assigned
    /// here so the film is reachable by decade rather than invisible.
    pub year: u16,
    /// 0.0–5.0 crowd average. Derived from the half-star rating the export drew
    /// where there was one (9 halves -> 4.5), assigned otherwise.
    pub star_rating: f32,
    pub poster: Option<Image>,
    pub grayscale: bool,
    /// Mostly taken from the export — either an explicit genre chip or the
    /// wording of the poster's alt text ("a modern thriller", "an indie drama") —
    /// and assigned where it said nothing. Genres outside `GENRE_FACETS`
    /// (Adventure, Noir) are kept: they show on the detail page even though no
    /// sidebar chip filters by them.
    pub genres: Vec<String>,
}

impl CatalogueEntry {
    /// Case-insensitive substring match over title and genres. An empty query
    /// matches everything.
    pub fn matches_text(&self, lowercased: &str) -> bool {
        if lowercased.is_empty() {
            return true;
        }
        self.title.to_lowercase().contains(lowercased)
            || self.genres.iter().any(|g| g.to_lowercase().contains(lowercased))
    }

    pub fn has_genre(&self, genre: &str) -> bool {
        self.genres.iter().any(|g| g == genre)
    }

    /// Whether the film's year falls in a decade label like "2010s". An
    /// unparseable label matches nothing rather than everything.
    pub fn in_decade(&self, label: &str) -> bool {
        match label.trim_end_matches('s').parse::<u16>() {
            Ok(start) => self.year >= start && self.year < start + 10,
            Err(_) => false,
        }
    }

    /// `min` is whole stars out of 5; 0 lets everything through.
    pub fn meets_minimum(&self, min: u8) -> bool {
        self.star_rating >= f32::from(min)
    }

    /// `on_watchlist` is left false here — `hydrate` fills it from the store.
    pub fn to_search_result(&self) -> SearchResult {
        SearchResult {
            id: self.id.clone(),
            title: self.title.clone(),
            year: self.year,
            star_rating: self.star_rating,
            poster: self.poster.clone(),
            grayscale: self.grayscale,
            genres: self.genres.clone(),
            on_watchlist: false,
        }
    }
}

/// Terse constructor so the catalogue below reads as a table.
fn entry(
    id: &str,
    title: &str,
    year: u16,
    star_rating: f32,
    genres: &[&str],
    poster: Option<Image>,
) -> CatalogueEntry {
    CatalogueEntry {
        id: id.into(),
        title: title.into(),
        year,
        star_rating,
        poster,
        grayscale: false,
        genres: genres.iter().map(|g| (*g).to_string()).collect(),
    }
}

/// Every film in the demo, drawn from all six screens.
pub fn catalogue() -> Vec<CatalogueEntry> {
    let mut films = vec![
        // Desktop feed — "Live Now".
        entry("silence-of-space", "The Silence of Space", 2024, 4.5, &["Sci-Fi", "Drama"], Some(Image::new(
            "img/poster-silence-of-space.jpg",
            "A striking minimalist movie poster for a sci-fi film featuring a solitary astronaut on a desolate, bright white landscape.",
        ))),
        entry("morning-haze", "Morning Haze", 1998, 5.0, &["Drama", "Romance"], Some(Image::new(
            "img/poster-morning-haze.jpg",
            "An abstract, minimalist movie poster featuring a blurred, motion-heavy shot of a city street at dawn.",
        ))),
        // Desktop feed — "Recent Entries".
        entry("le-souffle", "Le Souffle", 1960, 5.0, &["Drama", "Romance"], Some(Image::new(
            "img/poster-le-souffle.jpg",
            "A vintage-inspired movie poster for a French New Wave film, a stark black and white photograph of a woman smoking in a cafe overlaid with bold primary blue geometric shapes.",
        ))),
        entry("the-drop", "The Drop", 2023, 3.0, &["Thriller"], Some(Image::new(
            "img/poster-the-drop.jpg",
            "A hyper-minimalist poster for a modern thriller: an off-white cream canvas with a tiny, hyper-detailed photograph of a dropped set of keys casting a long, sharp black shadow.",
        ))),
        entry("estate-of-mind", "Estate of Mind", 2019, 5.0, &["Drama"], Some(Image::new(
            "img/poster-estate-of-mind.jpg",
            "An atmospheric, moody poster for a period drama: a soft-focus, painterly image of a grand estate shrouded in thick, pale morning fog.",
        ))),
        entry("blue-notes", "Blue Notes", 2021, 3.0, &["Documentary"], Some(Image::new(
            "img/poster-blue-notes.jpg",
            "A striking documentary poster: a high-contrast black and white portrait of a jazz musician playing a saxophone, with bold brutalist bright red typography cutting across the frame.",
        ))),
        // Mobile feed. The export printed no years for these four.
        entry("the-horizon", "The Horizon", 2022, 4.0, &["Sci-Fi"], Some(Image::new(
            "img/poster-the-horizon.jpg",
            "A minimalist film poster for a sci-fi movie: a lone figure on a vast, bright white dune under a massive, perfectly spherical black sun.",
        ))),
        entry("fractured", "Fractured", 2016, 3.0, &["Drama"], Some(Image::new(
            "img/poster-fractured.jpg",
            "An elegant, editorial-style movie poster for an indie drama: a tight close-up of a shattered teacup on a pristine white marble table.",
        ))),
        entry("red-shift", "Red Shift", 2008, 3.4, &["Sci-Fi"], Some(Image::new(
            "img/poster-red-shift.jpg",
            "A striking graphic film poster: a bright red abstract shape cutting across a vast white background with text aligned bottom-right.",
        ))),
        entry("endless", "Endless", 2005, 5.0, &["Drama", "Thriller"], Some(Image::new(
            "img/poster-endless.jpg",
            "A high-contrast black and white photography-based movie poster: a long, empty highway stretching into a flat, bright white horizon.",
        ))),
        // The two reviewed films.
        entry("dune-part-two", "Dune: Part Two", 2024, 4.5, &["Sci-Fi", "Adventure"], Some(Image::new(
            "img/poster-dune-part-two.jpg",
            "A minimalist, high-end film poster for a science fiction epic featuring stark typography and a solitary figure against an expansive pale sky.",
        ))),
        entry("architecture-of-silence", "The Architecture of Silence", 2023, 4.5, &["Documentary"], Some(Image::new(
            "img/poster-architecture-of-silence.jpg",
            "A striking, minimalist movie poster featuring abstract geometric shapes and a stark contrast between pure white and vibrant primary blue, with a single figure lost in a vast, empty architectural space.",
        ))),
        // The detailed film.
        entry("neon-reverie", "Neon Reverie", 2024, 4.4, &["Sci-Fi", "Noir", "Thriller"], Some(Image::new(
            "img/poster-neon-reverie.jpg",
            "A minimalist, high-contrast poster design for a neo-noir film. The poster features strong vertical lines and a stark, two-tone color palette of deep blue and crisp white. A solitary silhouette is placed asymmetrically in the lower third, surrounded by vast, empty space. The typography, if imagined, is clean and sans-serif. The style is modern, editorial, and sophisticated, avoiding any cluttered blockbuster tropes.",
        ))),
        // The four cards the export's search screen drew.
        entry("event-horizon-echoes", "Event Horizon Echoes", 2018, 4.2, &["Sci-Fi", "Thriller"], Some(Image::new(
            "img/poster-event-horizon-echoes.jpg",
            "A striking minimalist movie poster for a sci-fi film. The image shows a solitary figure standing on a barren, bright white alien landscape, looking up at a massive, glowing blue monolithic structure. The lighting is high-key, creating a sterile yet beautiful light-mode aesthetic. A sophisticated color palette of pristine whites, deep blacks, and sharp accents of primary blue dominates. The mood is awe-inspiring and slightly melancholic, reflecting high-end editorial gallery photography.",
        ))),
        entry("solitude-of-orbits", "The Solitude of Orbits", 2021, 3.9, &["Sci-Fi", "Drama"], Some(Image::new(
            "img/poster-solitude-of-orbits.jpg",
            "A minimalist, editorial-style movie poster for an indie science fiction drama. The visual features a close-up profile of a woman looking through an intricately frosted glass window. The lighting is soft and diffused, creating a serene light-mode aesthetic. The color palette relies heavily on slate whites, soft greys, and a singular, vibrant streak of primary blue light cutting across her face. The mood is contemplative and quiet.",
        ))),
        entry("void-geometry", "Void Geometry", 2015, 4.5, &["Sci-Fi", "Thriller"], Some(Image::new(
            "img/poster-void-geometry.jpg",
            "A high-contrast minimalist poster design for a space thriller. The composition showcases a vast, empty white room with a single, perfectly spherical black object hovering in the center. The room is brightly lit with harsh, directional light, typical of a bright light-mode gallery aesthetic. The palette is stark black and white, accented by a tiny, sharp red detail on the sphere. The overall tone is tense and analytical.",
        ))),
        // No poster on purpose — this is the "Poster Missing" placeholder state,
        // and its 0.0 rating means any rating floor above 0 filters it out.
        entry("project-kepler", "Project: Kepler", 2024, 0.0, &["Sci-Fi"], None),
    ];

    // The export renders Void Geometry's poster desaturated. A per-item art
    // direction choice, not a rule the grid can infer, so it is carried as data.
    if let Some(film) = films.iter_mut().find(|f| f.id == "void-geometry") {
        film.grayscale = true;
    }

    films
}

/// The poster the export used for `id`.
///
/// Panics for an id that isn't in the catalogue or has no poster. Every call site
/// passes a literal id from this module, so a panic means the catalogue and a
/// screen have drifted apart — the tests below call every screen to catch that.
fn poster_of(id: &str) -> Image {
    catalogue()
        .into_iter()
        .find(|entry| entry.id == id)
        .unwrap_or_else(|| panic!("no catalogue entry '{id}'"))
        .poster
        .unwrap_or_else(|| panic!("catalogue entry '{id}' has no poster"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every screen builds without panicking, which is what proves the ids the
    /// screens pass to `poster_of` all exist in the catalogue.
    #[test]
    fn every_screen_builds() {
        feed();
        mobile_feed();
        reviews();
        movie_details();
        search(&SearchQuery::default());
    }

    #[test]
    fn catalogue_ids_are_unique() {
        let mut ids: Vec<String> = catalogue().into_iter().map(|e| e.id).collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate id in the catalogue");
    }

    #[test]
    fn unfiltered_search_returns_everything() {
        let response = search(&SearchQuery::default());
        assert_eq!(response.total_results as usize, catalogue().len());
        assert!(response.results.len() <= PAGE_SIZE);
        assert_eq!(response.page, 1);
    }

    #[test]
    fn text_query_matches_title_case_insensitively() {
        let query = SearchQuery { q: Some("VOID".into()), ..Default::default() };
        let response = search(&query);
        assert_eq!(response.total_results, 1);
        assert_eq!(response.results[0].title, "Void Geometry");
    }

    #[test]
    fn genre_and_decade_filters_compose() {
        let query = SearchQuery {
            genre: Some("Sci-Fi".into()),
            year: Some("2010s".into()),
            ..Default::default()
        };
        let response = search(&query);
        assert!(response.total_results > 0);
        for result in &response.results {
            assert!(result.genres.contains(&"Sci-Fi".to_string()));
            assert!((2010..2020).contains(&result.year), "{} is not a 2010s film", result.title);
        }
    }

    #[test]
    fn rating_floor_drops_the_unrated_film() {
        let query = SearchQuery { min_rating: Some(1), ..Default::default() };
        let response = search(&query);
        assert!(!response.results.iter().any(|r| r.id == "project-kepler"));
    }

    /// A chip's count ignores the current genre selection, so picking one genre
    /// doesn't zero out every other chip.
    #[test]
    fn genre_counts_are_leave_one_out() {
        let query = SearchQuery { genre: Some("Sci-Fi".into()), ..Default::default() };
        let response = search(&query);
        let drama = response.filters.genres.iter().find(|g| g.label == "Drama").unwrap();
        assert!(!drama.selected);
        assert!(drama.count > 0, "Drama's count collapsed when Sci-Fi was selected");
    }

    /// Every count a chip advertises is a count you can actually reach by
    /// clicking it.
    #[test]
    fn genre_counts_match_what_selecting_them_returns() {
        let base = search(&SearchQuery::default());
        for facet in &base.filters.genres {
            let query = SearchQuery { genre: Some(facet.label.clone()), ..Default::default() };
            assert_eq!(
                search(&query).total_results,
                facet.count,
                "the {} chip advertises a count it doesn't deliver",
                facet.label
            );
        }
    }

    #[test]
    fn no_matches_still_leaves_one_page() {
        let query = SearchQuery { q: Some("nothing matches this".into()), ..Default::default() };
        let response = search(&query);
        assert_eq!(response.total_results, 0);
        assert!(response.results.is_empty());
        assert_eq!(response.page_count, 1);
        assert_eq!(response.page, 1);
    }

    #[test]
    fn out_of_range_pages_clamp() {
        let query = SearchQuery { page: Some(999), ..Default::default() };
        let response = search(&query);
        assert_eq!(response.page, response.page_count);
        assert!(!response.results.is_empty());
    }

    /// Paging covers every match exactly once.
    #[test]
    fn pages_partition_the_matches() {
        let mut seen: Vec<String> = Vec::new();
        let page_count = search(&SearchQuery::default()).page_count;
        for page in 1..=page_count {
            let query = SearchQuery { page: Some(page), ..Default::default() };
            seen.extend(search(&query).results.into_iter().map(|r| r.id));
        }
        assert_eq!(seen.len(), catalogue().len());
        let unique = {
            let mut ids = seen.clone();
            ids.sort();
            ids.dedup();
            ids.len()
        };
        assert_eq!(unique, seen.len(), "a film appeared on two pages");
    }

    /// Any id resolves, so a link from the feed can never 404.
    #[test]
    fn unknown_ids_still_get_a_detail_page() {
        let detail = movie_detail_by_id("some-unlisted-film");
        assert_eq!(detail.id, "some-unlisted-film");
        assert_eq!(detail.title, "Some Unlisted Film");
        assert!(!detail.cast.is_empty());
    }

    #[test]
    fn known_ids_keep_their_real_title() {
        assert_eq!(movie_detail_by_id("le-souffle").title, "Le Souffle");
        assert_eq!(movie_detail_by_id("dune-part-two").title, "Dune: Part Two");
    }

    #[test]
    fn slug_titles_degrade_gracefully() {
        assert_eq!(title_from_slug(""), "Untitled");
        assert_eq!(title_from_slug("---"), "Untitled");
        assert_eq!(title_from_slug("a"), "A");
    }

    #[test]
    fn unparseable_decade_matches_nothing() {
        let query = SearchQuery { year: Some("whenever".into()), ..Default::default() };
        assert_eq!(search(&query).total_results, 0);
    }
}
