# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/2.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- The AppImage starts on systems without PipeWire or JACK: it carries its own libmpv built without them.

## [0.7.0] - 2026-10-05

### Added

- Torrent rows show what the release title says as tags: season and episodes, resolution, HDR or Dolby Vision, source (WEB-DL, BDRip, Remux…), codec, IMAX or extended cut, voice-over kinds with how many tracks of each, original audio, subtitles, and the voice-over studios.

### Changed

- Torrent rows show the film or show name without the tags around it, in larger type; the full title stays in the tooltip. Search results come up faster.
- Torrent publication dates are written in the interface language.
- Seeders and leechers are called «Раздают» and «Качают» in Russian, «Роздають» and «Завантажують» in Ukrainian.
- The torrents page puts the year, title, ratings, and genres beside a larger poster, in the order of the title page.
- An episode opened at its very end starts over instead of skipping to the next one; the next one starts on its own only after the episode was actually watched.
- Torrent filter dropdowns look like the other dropdowns and stay open while you pick several values; Reset sits at the top, and seasons are spelled out.
- Back in a player settings submenu returns to the settings menu instead of closing it.
- With a remote:
  - back from the player, focus lands on the file it stopped on, even after switching episodes in the player;
  - the playlist opens on the episode playing, and player settings submenus on the current value;
  - closing the playlist or settings returns focus to its button, so the arrows no longer seek;
  - back from a settings category or player submenu, focus returns to the row it came from;
  - closing a dropdown in the torrent filters or Settings returns focus to it instead of the top of the panel;
  - Up from far along a long shelf moves to the shelf above instead of the search bar;
  - a held arrow stops at the edge of a list or shelf; a new press moves on to the bar or side menu beside it.

### Fixed

- Titles in Chinese, Japanese, Korean, Arabic, Hebrew, Devanagari, and Thai scripts show their letters instead of boxes.
- The torrent files list shows the progress just watched when going back from the player, and the release gets its started mark right away.
- After a release moves up the list once watched, focus stays on it instead of landing on another release.
- With a remote, Right past the end of a shelf no longer jumps to another row, which made a held arrow swing the page up and down.
- The translation filter no longer lists a studio whose name only appears inside a film's name.
- Shelves further down a page no longer show their scroll bars on the page's bottom edge.
- On Windows, the app opens without a console window.

## [0.6.0] - 2026-10-03

### Added

- Android TV app (Android 8.0+, ARM): remote control and voice search, with video through ExoPlayer, including 4K, HDR10, and Dolby Vision on the hardware decoder.

### Changed

- The setup wizard opens from the top of Settings.
- Back from the player returns to the torrent files or trailers list it was started from.
- Going back from the torrents page animates the title page back instead of jumping.
- Long torrent lists scroll without lag.
- Interface texts use sentence case and plainer wording.

### Fixed

- Escape closes the torrent files and trailers windows without leaving the page.
- Behind a DNS block, posters load even when one DNS-over-HTTPS provider points at an unreachable server: the answers of all providers are combined.
- A home row that failed to load once no longer stays failed on later starts.

### Security

- Logs no longer contain the TMDB API key.

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

[Unreleased]: https://github.com/dexsper/cinebox/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/dexsper/cinebox/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/dexsper/cinebox/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/dexsper/cinebox/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/dexsper/cinebox/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/dexsper/cinebox/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/dexsper/cinebox/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/dexsper/cinebox/releases/tag/v0.1.0
