//! JS `Date` / `Date.parse` / `Date.UTC` / `Date.now`.

use chrono::{FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};

use super::value::JsValue;

struct DateParts {
    y: i32,
    month: u32,
    day: u32,
    hour: u32,
    min: u32,
    sec: u32,
    ms: f64,
}

pub(super) fn now_ms() -> f64 {
    Utc::now().timestamp_millis() as f64
}

pub(super) fn parse(date_str: &str) -> Option<f64> {
    let s = strip_weekday(date_str);
    let s = collapse_ws(&s);

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s.replace(' ', "T")) {
        return Some(dt.timestamp_millis() as f64);
    }

    if let Some(ms) = parse_iso(&s) {
        return Some(ms);
    }

    let (body, offset_sec) = split_tz(&s);
    let body = body.trim();

    let formats = [
        "%d %B %Y %H:%M:%S",
        "%B %d %Y %H:%M:%S",
        "%d %B %Y",
        "%B %d %Y",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d",
    ];

    for fmt in formats {
        if let Ok(naive) = NaiveDateTime::parse_from_str(body, fmt) {
            return Some(naive_to_ms(naive, offset_sec));
        }

        if let Ok(date) = NaiveDate::parse_from_str(body, fmt) {
            let naive = date.and_hms_opt(0, 0, 0)?;
            return Some(naive_to_ms(naive, offset_sec));
        }
    }

    None
}

pub(super) fn utc_ms(args: &[JsValue]) -> f64 {
    let Some(parts) = date_parts(args) else {
        return f64::NAN;
    };

    let Some(date) = NaiveDate::from_ymd_opt(parts.y, parts.month, parts.day) else {
        return f64::NAN;
    };

    let Some(naive) = date.and_hms_opt(parts.hour, parts.min, parts.sec) else {
        return f64::NAN;
    };

    naive.and_utc().timestamp_millis() as f64 + parts.ms
}

pub(super) fn local_ms(args: &[JsValue]) -> f64 {
    use chrono::Local;

    let Some(parts) = date_parts(args) else {
        return f64::NAN;
    };

    let Some(date) = NaiveDate::from_ymd_opt(parts.y, parts.month, parts.day) else {
        return f64::NAN;
    };

    let Some(naive) = date.and_hms_opt(parts.hour, parts.min, parts.sec) else {
        return f64::NAN;
    };

    match Local.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            dt.timestamp_millis() as f64 + parts.ms
        }
        chrono::LocalResult::None => f64::NAN,
    }
}

pub(super) fn construct(args: &[JsValue]) -> f64 {
    if args.is_empty() {
        return now_ms();
    }

    if args.len() != 1 {
        return local_ms(args);
    }

    match &args[0] {
        JsValue::Undefined => f64::NAN,
        JsValue::String(s) => parse(s).unwrap_or(f64::NAN),
        JsValue::Number(n) if n.is_nan() => f64::NAN,
        JsValue::Number(n) => n.trunc(),
        other => number_or_parse(other),
    }
}

fn number_or_parse(v: &JsValue) -> f64 {
    let n = v.to_number();

    if n.is_nan() {
        return parse(&v.to_js_string()).unwrap_or(f64::NAN);
    }

    n.trunc()
}

fn date_parts(args: &[JsValue]) -> Option<DateParts> {
    Some(DateParts {
        y: num(args, 0, 1970.0) as i32,
        month: num(args, 1, 0.0) as u32 + 1,
        day: num(args, 2, 1.0) as u32,
        hour: num(args, 3, 0.0) as u32,
        min: num(args, 4, 0.0) as u32,
        sec: num(args, 5, 0.0) as u32,
        ms: num(args, 6, 0.0),
    })
}

fn num(args: &[JsValue], i: usize, default: f64) -> f64 {
    args.get(i).map(JsValue::to_number).unwrap_or(default)
}

fn naive_to_ms(naive: NaiveDateTime, offset_sec: i32) -> f64 {
    let offset = FixedOffset::east_opt(offset_sec).or_else(|| FixedOffset::east_opt(0));
    let Some(offset) = offset else {
        return f64::NAN;
    };

    let local = offset.from_local_datetime(&naive);
    let Some(dt) = local.single().or(local.earliest()) else {
        return f64::NAN;
    };

    dt.with_timezone(&Utc).timestamp_millis() as f64
}

fn parse_iso(s: &str) -> Option<f64> {
    let s = s.replace(' ', "T");

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
        return Some(dt.timestamp_millis() as f64);
    }

    if let Ok(naive) = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.3f") {
        return Some(naive.and_utc().timestamp_millis() as f64);
    }

    if let Ok(naive) = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S") {
        return Some(naive.and_utc().timestamp_millis() as f64);
    }

    None
}

fn strip_weekday(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let days = [
        "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday", "mon",
        "tue", "tues", "wed", "thu", "thur", "thurs", "fri", "sat", "sun",
    ];

    for day in days {
        if lower.starts_with(day) {
            return s[day.len()..].trim().to_owned();
        }
    }

    s.trim().to_owned()
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;

    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }

            prev_space = true;
            continue;
        }

        prev_space = false;
        out.push(c);
    }

    out
}

fn split_tz(s: &str) -> (String, i32) {
    let Some((body, tz)) = s.rsplit_once(' ') else {
        return (s.to_owned(), 0);
    };

    let Some(off) = tz_offset(tz) else {
        return (s.to_owned(), 0);
    };

    (body.to_owned(), off)
}

fn tz_offset(name: &str) -> Option<i32> {
    let hour = match name {
        "UTC" | "GMT" | "Z" => 0,
        "MDT" => -6,
        "MST" => -7,
        "PDT" => -7,
        "PST" => -8,
        "EDT" => -4,
        "EST" => -5,
        "CDT" => -5,
        "CST" => -6,
        _ => return None,
    };

    Some(hour * 3600)
}
