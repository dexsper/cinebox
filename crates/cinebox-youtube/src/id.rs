//! YouTube video id parsing.

use crate::error::Error;

const ID_LEN: usize = 11;

/// 11-character YouTube video id (TMDB `youtube_key` or a watch URL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoId(String);

impl VideoId {
    /// Parse `watch?v=`, `youtu.be/`, `/embed/`, `/shorts/`, or a raw 11-char id.
    ///
    /// # Errors
    ///
    /// Not a YouTube watch URL and not an 11-character id.
    pub fn parse(input: &str) -> Result<Self, Error> {
        let input = input.trim();

        if is_raw_id(input) {
            return Ok(Self(input.to_owned()));
        }

        let url = with_scheme(input);

        if let Some(id) = id_from_query(&url) {
            return Ok(Self(id));
        }

        if let Some(id) = id_from_host_path(&url, "youtu.be") {
            return Ok(Self(id));
        }

        if let Some(id) = id_from_prefix(&url, "/embed/") {
            return Ok(Self(id));
        }

        if let Some(id) = id_from_prefix(&url, "/shorts/") {
            return Ok(Self(id));
        }

        Err(Error::InvalidUrl)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_raw_id(s: &str) -> bool {
    s.len() == ID_LEN && s.chars().all(is_id_char)
}

fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn with_scheme(input: &str) -> String {
    if input.contains("://") {
        return input.to_owned();
    }

    let mut url = String::with_capacity(8 + input.len());
    url.push_str("https://");
    url.push_str(input);
    url
}

fn id_from_query(url: &str) -> Option<String> {
    if !is_youtube_watch_host(url) {
        return None;
    }

    let Some((_, query)) = url.split_once('?') else {
        return None;
    };

    let query = query.split('#').next().unwrap_or(query);

    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };

        if k != "v" {
            continue;
        }

        let id = v.split(&['&', '#', '/'][..]).next().unwrap_or(v);

        if is_raw_id(id) {
            return Some(id.to_owned());
        }
    }

    None
}

fn is_youtube_watch_host(url: &str) -> bool {
    let host = host_of(url);

    host == "youtube.com" || host.ends_with(".youtube.com")
}

fn path_of(url: &str) -> &str {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host_path = rest.split('?').next().unwrap_or(rest);
    let host_path = host_path.split('#').next().unwrap_or(host_path);

    host_path.find('/').map(|i| &host_path[i..]).unwrap_or("")
}

fn host_of(url: &str) -> &str {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);

    host.strip_prefix("www.").unwrap_or(host)
}

fn id_from_host_path(url: &str, host: &str) -> Option<String> {
    if host_of(url) != host {
        return None;
    }

    let path = path_of(url);
    let id = path.trim_start_matches('/').split('/').next().unwrap_or("");

    if is_raw_id(id) {
        return Some(id.to_owned());
    }

    None
}

fn id_from_prefix(url: &str, prefix: &str) -> Option<String> {
    let path = path_of(url);
    let Some(rest) = path.find(prefix).map(|i| &path[i + prefix.len()..]) else {
        return None;
    };

    let id = rest.split(['/', '?', '#']).next().unwrap_or(rest);

    if is_raw_id(id) {
        return Some(id.to_owned());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(input: &str) -> String {
        VideoId::parse(input)
            .map(|id| id.as_str().to_owned())
            .unwrap_or_else(|_| panic!("parse {input}"))
    }

    #[test]
    fn test_raw_id() {
        assert_eq!(parse_ok("abcdefghijk"), "abcdefghijk");
        assert_eq!(parse_ok("  dQw4w9WgXcQ  "), "dQw4w9WgXcQ");
    }

    #[test]
    fn test_watch_url() {
        assert_eq!(
            parse_ok("https://www.youtube.com/watch?v=abcdefghijk"),
            "abcdefghijk"
        );
        assert_eq!(
            parse_ok("https://youtube.com/watch?v=abcdefghijk&t=12"),
            "abcdefghijk"
        );
        assert_eq!(parse_ok("www.youtube.com/watch?v=abcdefghijk"), "abcdefghijk");
    }

    #[test]
    fn test_short_and_embed() {
        assert_eq!(parse_ok("https://youtu.be/abcdefghijk"), "abcdefghijk");
        assert_eq!(parse_ok("youtu.be/abcdefghijk?t=1"), "abcdefghijk");
        assert_eq!(
            parse_ok("https://www.youtube.com/embed/abcdefghijk"),
            "abcdefghijk"
        );
        assert_eq!(
            parse_ok("https://www.youtube.com/shorts/abcdefghijk"),
            "abcdefghijk"
        );
    }

    #[test]
    fn test_rejects_garbage() {
        assert!(matches!(VideoId::parse(""), Err(Error::InvalidUrl)));
        assert!(matches!(VideoId::parse("short"), Err(Error::InvalidUrl)));
        assert!(matches!(
            VideoId::parse("https://example.com/watch?v=abcdefghijk"),
            Err(Error::InvalidUrl)
        ));
    }
}
