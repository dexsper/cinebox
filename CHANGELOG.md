# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/2.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/dexsper/cinebox/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/dexsper/cinebox/releases/tag/v0.1.0
