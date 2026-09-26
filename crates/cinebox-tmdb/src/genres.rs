//! TMDB genre, keyword, and company ids used by section shelves and discover filters.

use cinebox_core::{MediaKind, Section};

/// Genre ids from `/genre/movie/list` and `/genre/tv/list`.
pub mod genre {
    pub const ACTION: u32 = 28;
    pub const ADVENTURE: u32 = 12;
    pub const ANIMATION: u32 = 16;
    pub const COMEDY: u32 = 35;
    pub const CRIME: u32 = 80;
    pub const DOCUMENTARY: u32 = 99;
    pub const DRAMA: u32 = 18;
    pub const FAMILY: u32 = 10751;
    pub const FANTASY: u32 = 14;
    pub const HISTORY: u32 = 36;
    pub const HORROR: u32 = 27;
    pub const MUSIC: u32 = 10402;
    pub const MYSTERY: u32 = 9648;
    pub const ROMANCE: u32 = 10749;
    pub const SCI_FI: u32 = 878;
    pub const THRILLER: u32 = 53;
    pub const WAR: u32 = 10752;
    pub const WESTERN: u32 = 37;
    pub const ACTION_ADVENTURE: u32 = 10759;
    pub const KIDS: u32 = 10762;
    pub const REALITY: u32 = 10764;
    pub const SCI_FI_FANTASY: u32 = 10765;
    pub const WAR_POLITICS: u32 = 10768;
}

/// Keyword ids from `/search/keyword`.
pub mod keyword {
    pub const ANIME: u32 = 210_024;
    pub const VAMPIRE: u32 = 3133;
    pub const ROBOT: u32 = 14544;
    pub const ISEKAI: u32 = 237_451;
    pub const ROMANCE: u32 = 9840;
    pub const TIME_TRAVEL: u32 = 4379;
    pub const SUPERHERO: u32 = 9715;
}

/// Company ids from `/search/company`.
pub mod company {
    pub const PIXAR: u32 = 3;
    pub const DREAMWORKS_ANIMATION: u32 = 521;
    pub const DISNEY_ANIMATION: u32 = 6125;
    pub const GHIBLI: u32 = 10342;
}

const MOVIE_GENRES: &[u32] = &[
    genre::ACTION,
    genre::ADVENTURE,
    genre::COMEDY,
    genre::DRAMA,
    genre::THRILLER,
    genre::HORROR,
    genre::SCI_FI,
    genre::FANTASY,
    genre::ROMANCE,
    genre::CRIME,
    genre::MYSTERY,
    genre::FAMILY,
    genre::HISTORY,
    genre::WAR,
    genre::WESTERN,
    genre::MUSIC,
    genre::DOCUMENTARY,
];

const TV_GENRES: &[u32] = &[
    genre::ACTION_ADVENTURE,
    genre::COMEDY,
    genre::DRAMA,
    genre::CRIME,
    genre::SCI_FI_FANTASY,
    genre::MYSTERY,
    genre::FAMILY,
    genre::KIDS,
    genre::WAR_POLITICS,
    genre::WESTERN,
    genre::REALITY,
    genre::DOCUMENTARY,
];

/// Genres a user can filter `kind` by, in display order. Animation is never offered:
/// it is implied by the cartoon and anime sections and excluded from the others.
#[must_use]
pub const fn genres_for(kind: MediaKind) -> &'static [u32] {
    match kind {
        MediaKind::Tv => TV_GENRES,
        MediaKind::Movie | MediaKind::Person => MOVIE_GENRES,
    }
}

/// Section a title belongs to, from its TMDB genres and original language.
#[must_use]
pub fn classify(kind: MediaKind, genre_ids: &[u32], original_language: Option<&str>) -> Section {
    if !genre_ids.contains(&genre::ANIMATION) {
        return Section::fallback(kind);
    }

    if original_language == Some("ja") {
        return Section::Anime;
    }

    Section::Cartoons
}

#[cfg(test)]
mod tests {
    use super::*;

    mod classify {
        use super::*;

        #[test]
        fn japanese_animation_is_anime() {
            let section = classify(MediaKind::Tv, &[genre::ANIMATION], Some("ja"));

            assert_eq!(section, Section::Anime);
        }

        #[test]
        fn non_japanese_animation_is_cartoons() {
            let section = classify(MediaKind::Tv, &[genre::ANIMATION], Some("en"));

            assert_eq!(section, Section::Cartoons);
        }

        #[test]
        fn japanese_live_action_movie_is_movies() {
            let section = classify(MediaKind::Movie, &[genre::ACTION], Some("ja"));

            assert_eq!(section, Section::Movies);
        }

        #[test]
        fn live_action_series_is_tv() {
            let section = classify(MediaKind::Tv, &[genre::DRAMA], None);

            assert_eq!(section, Section::Tv);
        }
    }
}
