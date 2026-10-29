use std::borrow::Cow;
use std::sync::LazyLock;

use fancy_regex::Regex;

use crate::PRIVATE;
use crate::data::{self};
use crate::engine::{Context, Typograf};
use crate::re;

const NDASH: &str = "\u{2013}";

pub(super) fn dash_centuries<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("(X|I|V)[ |\u{00A0}]?({})[ |\u{00A0}]?(X|I|V)", data::COMMON_DASH);
        re::compile(&pattern)
    });

    re::replace_all(&RE, text, &format!("$1{NDASH}$3"))
}

pub(super) fn dash_days_month<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile(&format!("(^|\\s)([123]?\\d)({})([123]?\\d)[ \u{00A0}]({})", data::COMMON_DASH, data::RU_MONTH_GEN))
    });

    re::replace_all(&RE, text, &format!("$1$2{NDASH}$4\u{00A0}$5"))
}

pub(super) fn dash_de<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile(&format!("([a-яё]+) де{}", data::RU_DASH_AFTER_DE)));

    re::replace_all(&RE, text, "$1-де")
}

pub(super) fn dash_decade<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile(&format!(
            "(^|\\s)(\\d{{3}}|\\d)0({})(\\d{{3}}|\\d)0(-е[ \u{00A0}])(?=г\\.?[ \u{00A0}]?г|год)",
            data::COMMON_DASH
        ))
    });

    re::replace_all(&RE, text, &format!("$1$20{NDASH}$40$5"))
}

pub(super) fn dash_direct_speech<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE1: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("([\"»‘“,])[ |\u{00A0}]?({})[ |\u{00A0}]", data::COMMON_DASH);
        re::compile(&pattern)
    });

    static RE2: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("(^|{PRIVATE})({})( |\u{00A0})", data::COMMON_DASH);
        re::compile_m(&pattern)
    });

    static RE3: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("([.…?!])[ \u{00A0}]({})[ \u{00A0}]", data::COMMON_DASH);
        re::compile(&pattern)
    });

    let a = re::replace_all(&RE1, text, "$1\u{00A0}\u{2014} ");
    let b = re::chain(&RE2, a, "$1\u{2014}\u{00A0}");

    re::chain(&RE3, b, "$1 \u{2014}\u{00A0}")
}

pub(super) fn dash_izpod<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("{}(И|и)з под{}", data::RU_DASH_BEFORE, data::RU_DASH_AFTER);
        re::compile(&pattern)
    });

    re::replace_all(&RE, text, "$1$2з-под")
}

pub(super) fn dash_izza<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("{}(И|и)з за{}", data::RU_DASH_BEFORE, data::RU_DASH_AFTER);
        re::compile(&pattern)
    });

    re::replace_all(&RE, text, "$1$2з-за")
}

pub(super) fn dash_ka<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile(&format!("([a-яё]+) ка(сь)?{}", data::RU_DASH_AFTER)));

    re::replace_all(&RE, text, "$1-ка$2")
}

pub(super) fn dash_kakto<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("(^|[^А-ЯЁа-яё\\w])([Кк]ак) то{}", data::RU_DASH_AFTER);
        re::compile(&pattern)
    });

    re::replace_all(&RE, text, "$1$2-то")
}

pub(super) fn dash_koe<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile(&format!("{}([Кк]о[ей])\\s([а-яё]{{3,}}){}", data::RU_DASH_BEFORE, data::RU_DASH_AFTER))
    });

    re::replace_all(&RE, text, "$1$2-$3")
}

pub(super) fn dash_main<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("([ \u{00A0}])({})([ \u{00A0}\\n])", data::COMMON_DASH);
        re::compile(&pattern)
    });

    re::replace_all(&RE, text, "\u{00A0}\u{2014}$3")
}

pub(super) fn dash_month<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_i(&format!(
            "({months}) ?({dashes}) ?({months})",
            months = data::RU_MONTH,
            dashes = data::COMMON_DASH
        ))
    });

    static RE_PRE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_i(&format!(
            "({months}) ?({dashes}) ?({months})",
            months = data::RU_MONTH_PRE,
            dashes = data::COMMON_DASH
        ))
    });

    let repl = format!("$1{NDASH}$3");
    let step = re::replace_all(&RE, text, &repl);

    re::chain(&RE_PRE, step, &repl)
}

pub(super) fn dash_surname<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile("([А-ЯЁ][а-яё]+)\\s-([а-яё]{1,3})(?![^а-яё]|$)"));

    re::replace_all(&RE, text, "$1\u{00A0}\u{2014}$2")
}

pub(super) fn dash_taki<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile(&format!(
            "(верно|довольно|опять|прямо|так|вс[её]|действительно|неужели)\\s(таки){}",
            data::RU_DASH_AFTER
        ))
    });

    re::replace_all(&RE, text, "$1-$2")
}

pub(super) fn dash_time<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile(&format!(
            "{}(\\d?\\d:[0-5]\\d){}(\\d?\\d:[0-5]\\d){}",
            data::RU_DASH_BEFORE,
            data::COMMON_DASH,
            data::RU_DASH_AFTER
        ))
    });

    re::replace_all(&RE, text, &format!("$1$2{NDASH}$3"))
}

pub(super) fn dash_to<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let words = "[Оо]ткуда|[Кк]уда|[Гг]де|[Кк]огда|[Зз]ачем|[Пп]очему|[Кк]ак|[Кк]ако[ейм]|[Кк]акая|[Кк]аки[емх]|[Кк]акими|[Кк]акую|[Чч]то|[Чч]его|[Чч]е[йм]|[Чч]ьим?|[Кк]то|[Кк]ого|[Кк]ому|[Кк]ем";

        re::compile(&format!("(^|[^А-ЯЁа-яё\\w])({words})( | -|- )(то|либо|нибудь){}", data::RU_DASH_AFTER))
    });

    re::replace_all_fn(&RE, text, |caps| {
        let kakto = format!("{}{}{}", &caps[2], &caps[3], &caps[4]);

        if kakto == "как то" || kakto == "Как то" {
            return caps[0].to_string();
        }

        format!("{}{}-{}", &caps[1], &caps[2], &caps[4])
    })
}

pub(super) fn dash_weekday<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_i(&format!("({part}) ?({dashes}) ?({part})", part = data::RU_WEEKDAY, dashes = data::COMMON_DASH))
    });

    re::replace_all(&RE, text, &format!("$1{NDASH}$3"))
}

pub(super) fn dash_years<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile(&format!(
            "(\\D|^)(\\d{{4}})[ \u{00A0}]?({})[ \u{00A0}]?(\\d{{4}})(?=[ \u{00A0}]?г)",
            data::COMMON_DASH
        ))
    });

    re::replace_all_fn(&RE, text, |caps| {
        let a: i32 = caps[2].parse().unwrap_or(0);
        let b: i32 = caps[4].parse().unwrap_or(0);

        if a < b {
            return format!("{}{}{NDASH}{}", &caps[1], &caps[2], &caps[4]);
        }

        caps[0].to_string()
    })
}

pub(super) fn date_from_iso<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE1: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = "(^|\\D)(\\d{4})(-|\\.|/)(\\d{2})(-|\\.|/)(\\d{2})(\\D|$)";
        re::compile_i(pattern)
    });

    static RE2: LazyLock<Regex> = LazyLock::new(|| re::compile_i("(^|\\D)(\\d{2})(-|/)(\\d{2})(-|/)(\\d{4})(\\D|$)"));
    let step = re::replace_all(&RE1, text, "$1$6.$4.$2$7");

    re::chain(&RE2, step, "$1$4.$2.$6$7")
}

pub(super) fn date_weekday<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_i(&format!("(\\d)( |\u{00A0})({}),( |\u{00A0})({})", data::RU_MONTH_GEN, data::RU_WEEKDAY))
    });

    re::replace_all_fn(&RE, text, |caps| {
        format!("{}{}{},{}{}", &caps[1], &caps[2], caps[3].to_lowercase(), &caps[4], caps[5].to_lowercase())
    })
}

pub(super) fn money_currency<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    const CURRENCY: &str = "([$€¥Ұ£₤₽])";
    const SPACE: &str = "[ \u{00A0}\u{2009}\u{202F}]";
    const NUMBER: &str = "\\d+([.,]\\d+)?";

    static RE1: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_m(&format!(
            "(^|[\\D]{{2}}){CURRENCY} ?({NUMBER}({SPACE}\\d{{3}})*)({SPACE}?(тыс\\.|млн|млрд|трлн))?"
        ))
    });

    static RE2: LazyLock<Regex> = LazyLock::new(|| re::compile_m(&format!("(^|[\\D])({NUMBER}) ?{CURRENCY}")));

    let step = re::replace_all_fn(&RE1, text, |caps| {
        let suffix = caps.get(7).map_or("", |m| m.as_str());
        let mid = if suffix.is_empty() { String::new() } else { format!("\u{00A0}{suffix}") };

        format!("{}{}{mid}\u{00A0}{}", &caps[1], &caps[3], &caps[2])
    });

    re::chain(&RE2, step, "$1$2\u{00A0}$4")
}

pub(super) fn money_ruble<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    const COMMON: &str = "(\\d+)( |\u{00A0})?(р|руб)\\.";

    static RE1: LazyLock<Regex> = LazyLock::new(|| re::compile(&format!("^{COMMON}$")));
    static RE2: LazyLock<Regex> = LazyLock::new(|| re::compile(&format!("{COMMON}(?=[!?,:;])")));
    static RE3: LazyLock<Regex> = LazyLock::new(|| re::compile(&format!("{COMMON}(?=\\s+[A-ЯЁ])")));

    let a = re::replace_all(&RE1, text, "$1\u{00A0}₽");
    let b = re::chain(&RE2, a, "$1\u{00A0}₽");

    re::chain(&RE3, b, "$1\u{00A0}₽.")
}

pub(super) fn nbsp_abbr<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("(^|\\s|{PRIVATE})([а-яё]{{1,3}})\\. ?([а-яё]{{1,3}})\\.");
        re::compile(&pattern)
    });

    fn abbr(caps: &fancy_regex::Captures<'_, str>) -> String {
        if &caps[2] == "дд" && &caps[3] == "мм" {
            return caps[0].to_string();
        }

        if ["рф", "ру", "рус", "орг", "укр", "бг", "срб"].contains(&&caps[3]) {
            return caps[0].to_string();
        }

        format!("{}{}.\u{00A0}{}.", &caps[1], &caps[2], &caps[3])
    }

    let step = re::replace_all_fn(&RE, text, abbr);

    re::chain_fn(&RE, step, abbr)
}

pub(super) fn nbsp_addr<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static HOUSE: LazyLock<Regex> = LazyLock::new(|| re::compile_i("(\\s|^)(дом|д\\.|кв\\.|под\\.|п-д) *(\\d+)"));

    static MKRN: LazyLock<Regex> = LazyLock::new(|| re::compile_i("(\\s|^)(мкр-н|мк-н|мкр\\.|мкрн)\\s"));

    static FLOOR: LazyLock<Regex> = LazyLock::new(|| re::compile_i("(\\s|^)(эт\\.) *(-?\\d+)"));
    static FLOOR_WORD: LazyLock<Regex> = LazyLock::new(|| re::compile_i("(\\s|^)(\\d+) +этаж([^а-яё]|$)"));

    static LITER: LazyLock<Regex> = LazyLock::new(|| re::compile_i("(\\s|^)литер\\s([А-Я]|$)"));
    static STREET: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_i(
            "(\\s|^)(обл|кр|ст|пос|с|д|ул|пер|пр|пр-т|просп|пл|бул|б-р|наб|ш|туп|оф|комн?|уч|вл|влад|стр|кор)\\. *([а-яёa-z\\d]+)",
        )
    });

    static CITY: LazyLock<Regex> = LazyLock::new(|| re::compile_m("(\\D[ \u{00A0}]|^)г\\. ?([А-ЯЁ])"));

    let mut out = re::replace_all(&HOUSE, text, "$1$2\u{00A0}$3");
    out = re::chain(&MKRN, out, "$1$2\u{00A0}");
    out = re::chain(&FLOOR, out, "$1$2\u{00A0}$3");
    out = re::chain(&FLOOR_WORD, out, "$1$2\u{00A0}этаж$3");
    out = re::chain(&LITER, out, "$1литер\u{00A0}$2");
    out = re::chain(&STREET, out, "$1$2.\u{00A0}$3");

    re::chain(&CITY, out, "$1г.\u{00A0}$2")
}

pub(super) fn nbsp_after_number_sign<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile("№[ \u{00A0}\u{2009}]?(\\d|п/п)"));

    re::replace_all(&RE, text, "№\u{202F}$1")
}

pub(super) fn nbsp_before_particle<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    const PARTICLES: &str = "(ли|ль|же|ж|бы|б)";

    static RE1: LazyLock<Regex> = LazyLock::new(|| re::compile(&format!("([А-ЯЁа-яё]) {PARTICLES}(?=[,;:?!\"‘“»])")));

    static RE2: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("([А-ЯЁа-яё])[ \u{00A0}]{PARTICLES}[ \u{00A0}]");
        re::compile(&pattern)
    });

    let step = re::replace_all(&RE1, text, "$1\u{00A0}$2");

    re::chain(&RE2, step, "$1\u{00A0}$2 ")
}

pub(super) fn nbsp_centuries<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    const BEFORE: &str = "(^|\\s)([VIX]+)";
    const AFTER: &str = "(?=[,;:?!\"‘“»]|$)";

    static RE1: LazyLock<Regex> = LazyLock::new(|| re::compile_m(&format!("{BEFORE}[ \u{00A0}]?в\\.?{AFTER}")));

    static RE2: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_m(&format!("{BEFORE}({})([VIX]+)[ \u{00A0}]?в\\.?([ \u{00A0}]?в\\.?)?{AFTER}", data::COMMON_DASH))
    });

    let step = re::replace_all(&RE1, text, "$1$2\u{00A0}в.");
    re::chain(&RE2, step, "$1$2$3$4\u{00A0}вв.")
}

pub(super) fn nbsp_day_month<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile_i(&format!("(\\d{{1,2}}) ({})", data::RU_SHORT_MONTH)));

    re::replace_all(&RE, text, "$1\u{00A0}$2")
}

pub(super) fn nbsp_initials<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let spaces = "\u{00A0}\u{202F} ";
        let quote = data::quote("ru").map_or("«„‚", |q| q.left);

        re::compile_m(&format!(
            "(^|[({spaces}{quote}{PRIVATE}\"])([А-ЯЁ])\\.[{spaces}]?([А-ЯЁ])\\.[{spaces}]?([А-ЯЁ][а-яё]+)"
        ))
    });

    re::replace_all(&RE, text, "$1$2.\u{00A0}$3.\u{00A0}$4")
}

pub(super) fn nbsp_m<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_m(&format!(
            "(^|[\\s,.\\({PRIVATE}])(\\d+)[ \u{00A0}]?(мм?|см|км|дм|гм|mm?|km|cm|dm)([23²³])?([\\s\\).!?,;{PRIVATE}]|$)"
        ))
    });

    re::replace_all_fn(&RE, text, |caps| {
        let pow = match caps.get(4).map_or("", |m| m.as_str()) {
            "2" | "²" => "²",
            "3" | "³" => "³",
            _ => "",
        };
        let end = &caps[5];
        let end = if end == "\u{00A0}" { " " } else { end };

        format!("{}{}\u{00A0}{}{pow}{end}", &caps[1], &caps[2], &caps[3])
    })
}

pub(super) fn nbsp_mln<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile_i("(\\d) ?(тыс|млн|млрд|трлн)(\\.|\\s|$)"));

    re::replace_all(&RE, text, "$1\u{00a0}$2$3")
}

pub(super) fn nbsp_ooo<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile("(^|[^a-яёA-ЯЁ])(ООО|ОАО|ЗАО|НИИ|ПБОЮЛ) "));

    re::replace_all(&RE, text, "$1$2\u{00A0}")
}

pub(super) fn nbsp_page<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_im(&format!("(^|[)\\s{PRIVATE}])(стр|гл|рис|илл?|ст|п|c)\\. *(\\d+)([\\s.,?!;:]|$)"))
    });

    re::replace_all(&RE, text, "$1$2.\u{00A0}$3$4")
}

pub(super) fn nbsp_ps<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_im(&format!("(^|\\s|{PRIVATE})[pз]\\.[ \u{00A0}]?([pз]\\.[ \u{00A0}]?)?[sы]\\.:? "))
    });

    re::replace_all_fn(&RE, text, |caps| {
        let prefix = if caps.get(2).is_some_and(|m| !m.as_str().is_empty()) {
            "P.\u{00A0}P.\u{00A0}S. "
        } else {
            "P.\u{00A0}S. "
        };

        format!("{}{prefix}", &caps[1])
    })
}

pub(super) fn nbsp_ruble_kopek<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile("(\\d) ?(?=(руб|коп)\\.)"));

    re::replace_all(&RE, text, "$1\u{00A0}")
}

pub(super) fn nbsp_see<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_i(&format!("(^|\\s|{PRIVATE}|\\()(см|им)\\.[ \u{00A0}]?([а-яё0-9a-z]+)([\\s.,?!]|$)"))
    });

    re::replace_all_fn(&RE, text, |caps| {
        let lead = if &caps[1] == "\u{00A0}" { " " } else { &caps[1] };

        format!("{lead}{}.\u{00A0}{}{}", &caps[2], &caps[3], &caps[4])
    })
}

pub(super) fn nbsp_year<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile("(^|\\D)(\\d{4}) ?г([ ,;.\\n]|$)"));

    re::replace_all(&RE, text, "$1$2\u{00A0}г$3")
}

pub(super) fn nbsp_years<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re::compile_m(&format!(
            "(^|\\D)(\\d{{4}})({})(\\d{{4}})[ \u{00A0}]?г\\.?([ \u{00A0}]?г\\.)?(?=[,;:?!\"‘“»\\s]|$)",
            data::COMMON_DASH
        ))
    });

    re::replace_all(&RE, text, "$1$2$3$4\u{00A0}гг.")
}

pub(super) fn number_comma<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = "(^|\\s)(\\d+)\\.(\\d+[\u{00A0}\u{2009}\u{202F} ]*?[%‰°×x])";
        re::compile_im(pattern)
    });

    re::replace_all(&RE, text, "$1$2,$3")
}

pub(super) fn number_ordinals<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let char = ctx.chars();
    let re = re::cached(&format!("(\\d[%‰]?)-(ый|ой|ая|ое|ые|ым|ом|ых|ого|ому|ыми)(?![{char}])"));

    re::replace_all_fn(&re, text, |caps| {
        let part = match &caps[2] {
            "ой" | "ый" => "й",
            "ая" => "я",
            "ое" | "ые" => "е",
            "ым" | "ом" => "м",
            "ых" => "х",
            "ого" => "го",
            "ому" => "му",
            "ыми" => "ми",
            other => other,
        };

        format!("{}-{part}", &caps[1])
    })
}

pub(super) fn punct_ano<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        let pattern = format!("([^«„\\[(!?,:;\\-‒–—\\s{PRIVATE}])(\\s+)(а|но)(?= |\u{00A0}|\\n)");
        re::compile(&pattern)
    });

    re::replace_all(&RE, text, "$1,$2$3")
}

pub(super) fn punct_exclamation<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE2: LazyLock<Regex> = LazyLock::new(|| re::compile_m("(^|[^!])!{2}($|[^!])"));
    static RE4: LazyLock<Regex> = LazyLock::new(|| re::compile_m("(^|[^!])!{4}($|[^!])"));
    let step = re::replace_all(&RE2, text, "$1!$2");

    re::chain(&RE4, step, "$1!!!$2")
}

pub(super) fn punct_exclamation_question<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile("(^|[^!])!\\?([^?]|$)"));

    re::replace_all(&RE, text, "$1?!$2")
}

pub(super) fn punct_hellip_question<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE1: LazyLock<Regex> = LazyLock::new(|| re::compile("(^|[^.])(\\.\\.\\.|…),"));
    static RE2: LazyLock<Regex> = LazyLock::new(|| re::compile("(!|\\?)(\\.\\.\\.|…)(?=[^.]|$)"));
    let step = re::replace_all(&RE1, text, "$1…");

    re::chain(&RE2, step, "$1..")
}

pub(super) fn space_after_hellip<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE1: LazyLock<Regex> = LazyLock::new(|| re::compile("([а-яё])(\\.\\.\\.|…)([А-ЯЁ])"));
    static RE2: LazyLock<Regex> = LazyLock::new(|| re::compile_i("([?!]\\.\\.)([а-яёa-z])"));
    let step = re::replace_all(&RE1, text, "$1$2 $3");

    re::chain(&RE2, step, "$1 $2")
}

pub(super) fn space_year<'a>(_tp: &Typograf, text: &'a str, ctx: &Context<'_>) -> Cow<'a, str> {
    let char = ctx.chars();
    let re = re::cached(&format!("(^| |\u{00A0})(\\d{{3,4}})(год([ауе]|ом)?)([^{char}]|$)"));

    re::replace_all(&re, text, "$1$2 $3$5")
}

pub(super) fn symbols_nn<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    if !text.contains("№№") {
        return Cow::Borrowed(text);
    }

    Cow::Owned(text.replace("№№", "№"))
}

pub(super) fn other_accent<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile("([а-яё])([АЕЁИОУЫЭЮЯ])([^А-ЯЁA-Za-z0-9_]|$)"));

    re::replace_all_fn(&RE, text, |caps| format!("{}{}\u{0301}{}", &caps[1], caps[2].to_lowercase(), &caps[3]))
}

pub(super) fn switching_keyboard<'a>(_tp: &Typograf, text: &'a str, _ctx: &Context<'_>) -> Cow<'a, str> {
    static RE: LazyLock<Regex> = LazyLock::new(|| re::compile("([AaBEeKMHOoPpCcTyXx]{1,3})(?=[А-ЯЁа-яё]+?)"));

    re::replace_all_fn(&RE, text, |caps| {
        caps[1]
            .chars()
            .map(|ch| match ch {
                'A' => 'А',
                'a' => 'а',
                'B' => 'В',
                'E' => 'Е',
                'e' => 'е',
                'K' => 'К',
                'M' => 'М',
                'H' => 'Н',
                'O' => 'О',
                'o' => 'о',
                'P' => 'Р',
                'p' => 'р',
                'C' => 'С',
                'c' => 'с',
                'T' => 'Т',
                'y' => 'у',
                'X' => 'Х',
                'x' => 'х',
                other => other,
            })
            .collect()
    })
}
