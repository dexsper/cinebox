use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::LazyLock;

use crate::engine::{Context, Typograf};
use crate::re;
use crate::PRIVATE;

static EXCEPTIONS: LazyLock<HashSet<u32>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    let items: &[Exception] = &[
        Exception::N(4162),
        Exception::N(416332),
        Exception::N(8512),
        Exception::N(851111),
        Exception::N(4722),
        Exception::N(4725),
        Exception::N(391379),
        Exception::N(8442),
        Exception::N(4732),
        Exception::N(4152),
        Exception::N(4154451),
        Exception::N(4154459),
        Exception::N(4154455),
        Exception::N(41544513),
        Exception::N(8142),
        Exception::N(8332),
        Exception::N(8612),
        Exception::N(8622),
        Exception::N(3525),
        Exception::N(812),
        Exception::N(8342),
        Exception::N(8152),
        Exception::N(3812),
        Exception::N(4862),
        Exception::N(3422),
        Exception::N(342633),
        Exception::N(8112),
        Exception::N(9142),
        Exception::N(8452),
        Exception::N(3432),
        Exception::N(3434),
        Exception::N(3435),
        Exception::N(4812),
        Exception::N(8432),
        Exception::N(8439),
        Exception::N(3822),
        Exception::N(4872),
        Exception::N(3412),
        Exception::N(3511),
        Exception::N(3512),
        Exception::N(3022),
        Exception::N(4112),
        Exception::N(4852),
        Exception::N(4855),
        Exception::N(3852),
        Exception::N(3854),
        Exception::N(8182),
        Exception::N(818),
        Exception::N(90),
        Exception::N(3472),
        Exception::N(4741),
        Exception::N(4764),
        Exception::N(4832),
        Exception::N(4922),
        Exception::N(8172),
        Exception::N(8202),
        Exception::N(8722),
        Exception::N(4932),
        Exception::N(493),
        Exception::N(3952),
        Exception::N(3951),
        Exception::N(3953),
        Exception::N(411533),
        Exception::N(4842),
        Exception::N(3842),
        Exception::N(3843),
        Exception::N(8212),
        Exception::N(4942),
        Exception::R(39131, 39179),
        Exception::R(39190, 39199),
        Exception::N(391),
        Exception::N(4712),
        Exception::N(4742),
        Exception::N(8362),
        Exception::N(495),
        Exception::N(499),
        Exception::N(4966),
        Exception::N(4964),
        Exception::N(4967),
        Exception::N(498),
        Exception::N(8312),
        Exception::N(8313),
        Exception::N(3832),
        Exception::N(383612),
        Exception::N(3532),
        Exception::N(8412),
        Exception::N(4232),
        Exception::N(423370),
        Exception::N(423630),
        Exception::N(8632),
        Exception::N(8642),
        Exception::N(8482),
        Exception::N(4242),
        Exception::N(8672),
        Exception::N(8652),
        Exception::N(4752),
        Exception::N(4822),
        Exception::N(482502),
        Exception::N(4826300),
        Exception::N(3452),
        Exception::N(8422),
        Exception::N(4212),
        Exception::N(3466),
        Exception::N(3462),
        Exception::N(8712),
        Exception::N(8352),
        Exception::N(800),
        Exception::R(901, 934),
        Exception::R(936, 939),
        Exception::R(950, 953),
        Exception::N(958),
        Exception::R(960, 969),
        Exception::R(977, 989),
        Exception::R(991, 997),
        Exception::N(999),
    ];

    for item in items {
        match *item {
            Exception::N(n) => {
                set.insert(n);
            }
            Exception::R(a, b) => {
                for n in a..=b {
                    set.insert(n);
                }
            }
        }
    }

    set
});

enum Exception {
    N(u32),
    R(u32, u32),
}

pub fn phone_number<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_m(&format!(
            "(^|,| |{PRIVATE})(\\+7[\\d\\(\\) \u{00A0}-]{{10,18}})(?=,|;|{PRIVATE}|$)"
        ))
    });
    
    static LABELED: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_i(
            "(^|[^а-яё])([☎☏✆📠📞📱]|т\\.|тел\\.|ф\\.|моб\\.|факс|сотовый|мобильный|телефон)(:?\\s*?)([+\\d(][\\d \u{00A0}\\-()]{3,}\\d)",
        )
    });

    let step = re::replace_all_fn(&RE, text, |caps| {
        let buf = clear_phone(&caps[2]);

        if buf.chars().count() == 12 {
            format!("{}{}", &caps[1], phone(&buf))
        } else {
            caps[0].to_string()
        }
    });

    re::chain_fn(&LABELED, step, |caps| {
        let buf = clear_phone(&caps[4]);

        if buf.chars().count() >= 5 {
            format!("{}{}{}{}", &caps[1], &caps[2], &caps[3], phone(&buf))
        } else {
            caps[0].to_string()
        }
    })
}

fn clear_phone(text: &str) -> String {
    text.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect()
}

fn phone(num: &str) -> String {
    let first = num.chars().next().unwrap_or(' ');
    let mut num = num.to_string();
    let mut has_plus = false;
    let mut has_eight = false;

    if num.len() < 8 {
        return phone_blocks(&num);
    }

    if num.len() > 10 {
        if first == '+' {
            if num.chars().nth(1) == Some('7') {
                has_plus = true;
                num = num.chars().skip(2).collect();
            } else {
                return num;
            }
        } else if first == '8' {
            has_eight = true;
            num = num.chars().skip(1).collect();
        }
    }

    let mut city_code = String::new();

    for city_len in (2..=8).rev() {
        if num.len() < city_len {
            continue;
        }

        let code: String = num.chars().take(city_len).collect();
        let Ok(n) = code.parse::<u32>() else {
            continue;
        };

        if EXCEPTIONS.contains(&n) {
            city_code = code;
            num = num.chars().skip(city_len).collect();
            break;
        }
    }

    if city_code.is_empty() {
        city_code = num.chars().take(5).collect();
        num = num.chars().skip(5).collect();
    }

    let mut out = String::new();

    if has_plus {
        out.push_str("+\u{00A0}");
        out.insert(1, '7');
    }

    if has_eight {
        out.push_str("8\u{00A0}");
    }

    out.push_str(&prepare_code(&city_code));
    out.push('\u{00A0}');
    out.push_str(&phone_blocks(&num));

    out
}

fn prepare_code(code: &str) -> String {
    let num_code: u32 = code.parse().unwrap_or(0);
    let len = code.chars().count();
    let parts: Vec<String> = match len {
        4 => vec![code.chars().take(2).collect(), code.chars().skip(2).take(2).collect()],
        5 => vec![code.chars().take(3).collect(), code.chars().skip(3).take(3).collect()],
        6 => vec![
            code.chars().take(2).collect(),
            code.chars().skip(2).take(2).collect(),
            code.chars().skip(4).take(2).collect(),
        ],
        _ => vec![code.to_string()],
    };
    let without_brackets = (num_code > 900 && num_code <= 999) || num_code == 495 || num_code == 499 || num_code == 800;
    let joined = parts.join("-");

    if without_brackets {
        joined
    } else {
        format!("({joined})")
    }
}

fn phone_blocks(num: &str) -> String {
    let mut add = String::new();
    let mut num = num.to_string();

    if num.len() % 2 == 1 {
        if let Some(first) = num.chars().next() {
            add.push(first);

            if num.len() <= 5 {
                add.push('-');
            }

            num = num.chars().skip(1).collect();
        }
    }

    let mut chunks = Vec::new();
    let bytes = num.as_bytes();
    let mut i = 0;

    while i + 2 <= bytes.len() {
        chunks.push(num[i..i + 2].to_string());
        i += 2;
    }

    format!("{add}{}", chunks.join("-"))
}
