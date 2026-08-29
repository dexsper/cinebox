//! JSON row → [`Hit`] mapping (no HTTP).

use serde_json::Value;

/// Normalized indexer row (Jackett + Prowlarr).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub title: String,
    pub tracker: String,
    pub size_bytes: u64,
    pub seeders: u32,
    pub peers: u32,
    pub magnet: String,
    pub published: String,
}

fn json_u64(value: &Value) -> u64 {
    match value {
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_i64().and_then(|v| u64::try_from(v).ok()))
            .or_else(|| n.as_f64().map(|v| v.max(0.0) as u64))
            .unwrap_or(0),
        Value::String(s) => parse_size_text(s),
        _ => 0,
    }
}

fn parse_size_text(raw: &str) -> u64 {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return 0;
    }

    if let Ok(n) = trimmed.parse::<u64>() {
        return n;
    }

    let compact: String = trimmed
        .chars()
        .filter(|ch| *ch != ',' && *ch != ' ' && *ch != '\u{00a0}')
        .collect();

    if let Ok(n) = compact.parse::<u64>() {
        return n;
    }

    if let Ok(n) = trimmed.replace(',', ".").parse::<f64>()
        && n.is_finite()
        && n >= 0.0
    {
        return n as u64;
    }
    parse_size_label(trimmed).unwrap_or(0)
}

fn parse_size_label(raw: &str) -> Option<u64> {
    let lower = raw.trim().to_lowercase().replace(',', ".");
    let split = lower
        .char_indices()
        .find(|(_, ch)| ch.is_alphabetic() || *ch == 'б' || *ch == 'Б');
    let (num_part, unit_part) = match split {
        Some((index, _)) => lower.split_at(index),
        None => return None,
    };
    let num: f64 = num_part.trim().parse().ok()?;
    if !num.is_finite() || num < 0.0 {
        return None;
    }
    let unit = unit_part.trim();
    let mul = if unit.starts_with("tib") || unit.starts_with("tb") || unit.starts_with('т') {
        1024.0 * 1024.0 * 1024.0 * 1024.0
    } else if unit.starts_with("gib") || unit.starts_with("gb") || unit.starts_with('г') {
        1024.0 * 1024.0 * 1024.0
    } else if unit.starts_with("mib") || unit.starts_with("mb") || unit.starts_with('м') {
        1024.0 * 1024.0
    } else if unit.starts_with("kib") || unit.starts_with("kb") || unit.starts_with('к') {
        1024.0
    } else {
        return None;
    };
    Some((num * mul).round() as u64)
}

fn row_size_bytes(raw: &Value) -> u64 {
    for key in ["Size", "size", "fileSize", "FileSize", "bytes"] {
        let n = json_u64(raw.get(key).unwrap_or(&Value::Null));
        if n > 0 {
            return n;
        }
    }
    0
}

fn json_u32(value: &Value) -> u32 {
    u32::try_from(json_u64(value)).unwrap_or(u32::MAX)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn format_publish_date(raw: &str) -> String {
    let Some(y) = raw.get(0..4).and_then(|part| part.parse::<u16>().ok()) else {
        return String::new();
    };

    let Some(month) = raw
        .get(5..7)
        .and_then(|part| part.parse::<usize>().ok())
        .filter(|month| (1..=12).contains(month))
    else {
        return String::new();
    };

    let Some(day) = raw
        .get(8..10)
        .and_then(|part| part.parse().ok())
        .filter(|day: &u8| *day > 0)
    else {
        return String::new();
    };

    let Some(name) = MONTHS.get(month.saturating_sub(1)) else {
        return String::new();
    };

    format!("{day} {name} {y}")
}

pub(crate) fn text_field(obj: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = obj.get(*key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.to_owned();
            }
        }
    }
    String::new()
}

pub(crate) fn hit_from_jackett(raw: &Value) -> Option<Hit> {
    let title = text_field(raw, &["Title", "title"]);
    if title.is_empty() {
        return None;
    }

    let magnet = text_field(raw, &["MagnetUri", "magnetUri", "Link", "link", "Guid"]);
    Some(Hit {
        title,
        tracker: text_field(raw, &["Tracker", "tracker", "TrackerId"]),
        size_bytes: row_size_bytes(raw),
        seeders: json_u32(
            raw.get("Seeders")
                .or_else(|| raw.get("seeders"))
                .unwrap_or(&Value::Null),
        ),
        peers: json_u32(
            raw.get("Peers")
                .or_else(|| raw.get("peers"))
                .or_else(|| raw.get("Leechers"))
                .unwrap_or(&Value::Null),
        ),
        magnet,
        published: format_publish_date(&text_field(
            raw,
            &["PublishDate", "publishDate", "Published"],
        )),
    })
}

pub(crate) fn hit_from_prowlarr(raw: &Value) -> Option<Hit> {
    let protocol = text_field(raw, &["protocol"]).to_ascii_lowercase();
    if !protocol.is_empty() && protocol != "torrent" {
        return None;
    }

    let title = text_field(raw, &["title", "Title"]);
    if title.is_empty() {
        return None;
    }

    let magnet = text_field(
        raw,
        &["magnetUrl", "MagnetUri", "downloadUrl", "guid", "Link"],
    );

    Some(Hit {
        title,
        tracker: text_field(raw, &["indexer", "Tracker"]),
        size_bytes: row_size_bytes(raw),
        seeders: json_u32(
            raw.get("seeders")
                .or_else(|| raw.get("Seeders"))
                .unwrap_or(&Value::Null),
        ),
        peers: json_u32(
            raw.get("leechers")
                .or_else(|| raw.get("Peers"))
                .unwrap_or(&Value::Null),
        ),
        magnet,
        published: format_publish_date(&text_field(
            raw,
            &["publishDate", "PublishDate", "published"],
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jackett_row_maps_magnet_or_link() {
        let raw = serde_json::json!({
            "Title": " Dune 2021 ",
            "Tracker": "rutor",
            "Size": 1234,
            "Seeders": 10,
            "Peers": "3",
            "Link": "magnet:?xt=urn:btih:ab",
            "PublishDate": "2021-10-22T12:00:00Z"
        });
        let hit = match hit_from_jackett(&raw) {
            Some(hit) => hit,
            None => panic!("hit"),
        };
        assert_eq!(hit.title, "Dune 2021");
        assert_eq!(hit.size_bytes, 1234);
        assert_eq!(hit.peers, 3);
        assert_eq!(hit.published, "22 Oct 2021");
        assert!(hit.magnet.starts_with("magnet:"));
    }

    #[test]
    fn prowlarr_skips_usenet() {
        let raw = serde_json::json!({
            "protocol": "usenet",
            "title": "Nope"
        });
        assert!(hit_from_prowlarr(&raw).is_none());
    }

    #[test]
    fn prowlarr_keeps_missing_protocol() {
        let raw = serde_json::json!({ "title": "Film", "indexer": "rutor" });
        let hit = match hit_from_prowlarr(&raw) {
            Some(hit) => hit,
            None => panic!("hit"),
        };
        assert_eq!(hit.title, "Film");
        assert_eq!(hit.tracker, "rutor");
    }

    #[test]
    fn size_parses_numeric_string_and_label() {
        let labeled = serde_json::json!({
            "Title": "A",
            "Size": "1.5 GB"
        });
        let hit = match hit_from_jackett(&labeled) {
            Some(hit) => hit,
            None => panic!("hit"),
        };
        assert_eq!(hit.size_bytes, 1_610_612_736);

        let dotted = serde_json::json!({
            "Title": "B",
            "Size": "1572864000.0"
        });
        let hit = match hit_from_jackett(&dotted) {
            Some(hit) => hit,
            None => panic!("hit"),
        };
        assert_eq!(hit.size_bytes, 1_572_864_000);
    }
}
