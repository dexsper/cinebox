# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/2.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-09-30

### Added

- First-run wizard for the language, TMDB key, parser, and TorrServer.

### Changed

- Settings fields no longer reload the catalog on every keystroke.
- Error messages are translated and show what to fix.

### Fixed

- On Linux, buttons and banners react to hover right after dragging the window, without an extra click.

## [0.4.0] - 2026-09-29

### Added

- Linux x86_64 builds: Flatpak, .deb (Debian 12+, Ubuntu 24.04+), and AppImage.
- The AppImage updates in place through Gear Lever or AppImageUpdate.
- System proxy support on Linux, including the Flatpak build.
- Hardware video decoding, on by default; it can be turned off in Player settings.

## [0.3.0] - 2026-09-27

### Added

- Release/ongoing status pill next to the TMDB rating on the title page (e.g. "Released", "Ongoing").
- Side rail with Home, Movies, Cartoons, Series, Anime, and My lists; it widens with labels on hover.
- Section hubs with themed shelves: now playing, last year, worth a rewatch, highly rated, genres, decades, studios, and more.
- Discover screen: pick genres, decade, minimum rating, and sort order from any section.
- Personal lists: Watching, Plan to watch, Completed, Dropped, plus an independent Liked flag, set from the title page.
- Starting playback puts a title into Watching unless it already has a status.
- List status and Liked badges on posters; Watching and Plan to watch shelves on Home.
- General settings: choose which rows appear on Home.

### Changed

- Bigger title, tagline, year/country line, and "In Detail" heading on the title page.
- Poster and cast/crew cards scale up on wider windows instead of staying a fixed size.
- Backdrop artwork is darkened a bit more toward the center so text stays readable over bright images.
- Trailers and the new list button on the title page are compact icons that expand with a label on hover.

### Fixed

- Filter chips (discover, torrent filters, settings) wrap to the next row instead of running off the edge.

## [0.2.0] - 2026-09-14

### Added

- Skip intro, recap, credits, and preview segments via [TheIntroDB](https://theintrodb.org). When playback enters a skippable zone, Cancel and Skip buttons appear in the bottom-right corner above the seek bar.
- Auto-skip: after you press Skip once, the next time that segment type appears a fill animation counts down 8 s and skips automatically. Cancel during the countdown disarms it.
- Per-type toggles in Player settings (intro / recap / credits / preview).
- Seeking back into an already-skipped segment during the same session does not re-show the buttons.
- Skipping credits at the end of a file when auto-next is on goes straight to the next episode.

## [0.1.0] - 2026-09-05

First public release

### Added

- Home screen with recently watched, now playing, trending (day and week), popular, and top-rated shelves. Opening a shelf shows a paginated poster grid.
- Search for movies, series, and people, with recent query history.
- Title pages with overview, runtime, rating, budget, countries, directors, cast, collection, recommendations, and similar titles.
- Actor and director pages.
- Watched markers on posters, and local watch history: resume from the last position per title or episode.
- Optional TorrServer timecode sync (off by default): the same progress is also stored on the server. Local history wins when both exist.
- Built-in libmpv player, so MKV, HEVC, HDR, and other torrent-typical codecs do not need a second player.
- Jackett and Prowlarr search, with filters for quality, HDR, Dolby Vision, subtitles, year, translation and voice, and language. Sort by popularity, seeders, or size.
- Streaming through TorrServer, with optional preload wait, a file list inside the torrent, and play next automatically.
- Audio and subtitle tracks, subtitle size and delay, speed 0.5x–2x, video scale, volume, and loudness normalization.
- Fullscreen playback. Controls hide when the mouse is idle. Seek 10 seconds with keys or by clicking the left or right third of the screen.
- YouTube trailers in the same player.
- UI languages: English, Russian, and Ukrainian. The TMDB catalog language follows the UI.
- System proxy for TMDB and the parser. TorrServer always connects directly.
- DNS-block bypass via DNS-over-HTTPS for TMDB and the parser.
- Settings checks for the TMDB key, the parser, and TorrServer, plus a speed test and cache clear.
- Portable data: `settings.json` and `cinebox.sqlite` sit next to the executable.

[Unreleased]: https://github.com/dexsper/cinebox/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/dexsper/cinebox/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/dexsper/cinebox/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/dexsper/cinebox/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dexsper/cinebox/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dexsper/cinebox/releases/tag/v0.1.0
