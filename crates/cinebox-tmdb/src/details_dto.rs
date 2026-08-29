//! Serde DTOs for TMDB movie/TV/person JSON.

use serde::Deserialize;

use crate::catalog_map::CatalogListItem;

#[derive(Deserialize)]
pub(crate) struct Named {
    pub(crate) id: Option<u32>,
    pub(crate) name: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct Country {
    pub(crate) name: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct CollectionRef {
    pub(crate) id: Option<u32>,
}

#[derive(Deserialize)]
pub(crate) struct CreditsBlock {
    pub(crate) cast: Option<Vec<CreditRaw>>,
    pub(crate) crew: Option<Vec<CreditRaw>>,
}

#[derive(Deserialize)]
pub(crate) struct CreditRaw {
    pub(crate) id: Option<u32>,
    pub(crate) name: Option<String>,
    pub(crate) character: Option<String>,
    pub(crate) job: Option<String>,
    pub(crate) profile_path: Option<String>,
    pub(crate) order: Option<u32>,
}

#[derive(Deserialize)]
pub(crate) struct VideosBlock {
    pub(crate) results: Option<Vec<VideoRaw>>,
}

#[derive(Deserialize)]
pub(crate) struct VideoRaw {
    pub(crate) name: Option<String>,
    pub(crate) key: Option<String>,
    pub(crate) site: Option<String>,
    #[serde(rename = "type")]
    pub(crate) kind: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ListBlock {
    pub(crate) results: Option<Vec<CatalogListItem>>,
}

#[derive(Deserialize)]
pub(crate) struct CollectionBody {
    pub(crate) parts: Option<Vec<CatalogListItem>>,
}

#[derive(Deserialize)]
pub(crate) struct MediaBody {
    pub(crate) id: Option<u32>,
    pub(crate) title: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) original_title: Option<String>,
    pub(crate) original_name: Option<String>,
    pub(crate) original_language: Option<String>,
    pub(crate) tagline: Option<String>,
    pub(crate) overview: Option<String>,
    pub(crate) release_date: Option<String>,
    pub(crate) first_air_date: Option<String>,
    pub(crate) runtime: Option<u32>,
    pub(crate) episode_run_time: Option<Vec<u32>>,
    pub(crate) number_of_seasons: Option<u32>,
    pub(crate) number_of_episodes: Option<u32>,
    pub(crate) last_episode_to_air: Option<EpisodeAir>,
    pub(crate) next_episode_to_air: Option<EpisodeAir>,
    pub(crate) vote_average: Option<f32>,
    pub(crate) budget: Option<u64>,
    pub(crate) poster_path: Option<String>,
    pub(crate) backdrop_path: Option<String>,
    pub(crate) genres: Option<Vec<Named>>,
    pub(crate) production_countries: Option<Vec<Country>>,
    pub(crate) origin_country: Option<Vec<String>>,
    pub(crate) belongs_to_collection: Option<CollectionRef>,
    pub(crate) credits: Option<CreditsBlock>,
    pub(crate) videos: Option<VideosBlock>,
    pub(crate) recommendations: Option<ListBlock>,
    pub(crate) similar: Option<ListBlock>,
    pub(crate) release_dates: Option<ReleaseDatesBlock>,
    pub(crate) content_ratings: Option<ContentRatingsBlock>,
}

#[derive(Deserialize)]
pub(crate) struct ReleaseDatesBlock {
    pub(crate) results: Option<Vec<ReleaseCountry>>,
}

#[derive(Deserialize)]
pub(crate) struct ReleaseCountry {
    pub(crate) iso_3166_1: Option<String>,
    pub(crate) release_dates: Option<Vec<CertDate>>,
}

#[derive(Deserialize)]
pub(crate) struct CertDate {
    pub(crate) certification: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ContentRatingsBlock {
    pub(crate) results: Option<Vec<ContentRating>>,
}

#[derive(Deserialize)]
pub(crate) struct ContentRating {
    pub(crate) iso_3166_1: Option<String>,
    pub(crate) rating: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct EpisodeAir {
    pub(crate) runtime: Option<u32>,
}

#[derive(Deserialize)]
pub(crate) struct CombinedCredits {
    pub(crate) cast: Option<Vec<CatalogListItem>>,
    pub(crate) crew: Option<Vec<CatalogListItem>>,
}

#[derive(Deserialize)]
pub(crate) struct PersonBody {
    pub(crate) id: Option<u32>,
    pub(crate) name: Option<String>,
    pub(crate) biography: Option<String>,
    pub(crate) birthday: Option<String>,
    pub(crate) place_of_birth: Option<String>,
    pub(crate) profile_path: Option<String>,
    pub(crate) combined_credits: Option<CombinedCredits>,
}
