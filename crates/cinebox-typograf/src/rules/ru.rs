use std::sync::LazyLock;

use crate::data::{self};
use crate::engine::{Context, Typograf};
use crate::re;
use crate::PRIVATE;

const NDASH: &str = "\u{2013}";

pub fn dash_centuries(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "(X|I|V)[ |\u{00A0}]?({})[ |\u{00A0}]?(X|I|V)",
        data::COMMON_DASH
    ));

    re::replace_all(&re, text, &format!("$1{NDASH}$3"))
}

pub fn dash_days_month(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "(^|\\s)([123]?\\d)({})([123]?\\d)[ \u{00A0}]({})",
        data::COMMON_DASH,
        data::RU_MONTH_GEN
    ));

    re::replace_all(&re, text, &format!("$1$2{NDASH}$4\u{00A0}$5"))
}

pub fn dash_de(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!("([a-яё]+) де{}", data::RU_DASH_AFTER_DE));

    re::replace_all(&re, text, "$1-де")
}

pub fn dash_decade(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "(^|\\s)(\\d{{3}}|\\d)0({})(\\d{{3}}|\\d)0(-е[ \u{00A0}])(?=г\\.?[ \u{00A0}]?г|год)",
        data::COMMON_DASH
    ));

    re::replace_all(&re, text, &format!("$1$20{NDASH}$40$5"))
}

pub fn dash_direct_speech(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let dashes = data::COMMON_DASH;
    let re1 = re::compile(&format!("([\"»‘“,])[ |\u{00A0}]?({dashes})[ |\u{00A0}]"));
    let re2 = re::compile_m(&format!("(^|{PRIVATE})({dashes})( |\u{00A0})"));
    let re3 = re::compile(&format!("([.…?!])[ \u{00A0}]({dashes})[ \u{00A0}]"));
    let a = re::replace_all(&re1, text, "$1\u{00A0}\u{2014} ");
    let b = re::replace_all(&re2, &a, "$1\u{2014}\u{00A0}");

    re::replace_all(&re3, &b, "$1 \u{2014}\u{00A0}")
}

pub fn dash_izpod(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "{}(И|и)з под{}",
        data::RU_DASH_BEFORE,
        data::RU_DASH_AFTER
    ));

    re::replace_all(&re, text, "$1$2з-под")
}

pub fn dash_izza(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "{}(И|и)з за{}",
        data::RU_DASH_BEFORE,
        data::RU_DASH_AFTER
    ));

    re::replace_all(&re, text, "$1$2з-за")
}

pub fn dash_ka(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!("([a-яё]+) ка(сь)?{}", data::RU_DASH_AFTER));

    re::replace_all(&re, text, "$1-ка$2")
}

pub fn dash_kakto(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "(^|[^А-ЯЁа-яё\\w])([Кк]ак) то{}",
        data::RU_DASH_AFTER
    ));

    re::replace_all(&re, text, "$1$2-то")
}

pub fn dash_koe(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "{}([Кк]о[ей])\\s([а-яё]{{3,}}){}",
        data::RU_DASH_BEFORE,
        data::RU_DASH_AFTER
    ));

    re::replace_all(&re, text, "$1$2-$3")
}

pub fn dash_main(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "([ \u{00A0}])({})([ \u{00A0}\\n])",
        data::COMMON_DASH
    ));

    re::replace_all(&re, text, "\u{00A0}\u{2014}$3")
}

pub fn dash_month(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let months = format!("({})", data::RU_MONTH);
    let months_pre = format!("({})", data::RU_MONTH_PRE);
    let dashes = data::COMMON_DASH;
    let re = re::compile_i(&format!("{months} ?({dashes}) ?{months}"));
    let re_pre = re::compile_i(&format!("{months_pre} ?({dashes}) ?{months_pre}"));
    let repl = format!("$1{NDASH}$3");
    let step = re::replace_all(&re, text, &repl);

    re::replace_all(&re_pre, &step, &repl)
}

pub fn dash_surname(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("([А-ЯЁ][а-яё]+)\\s-([а-яё]{1,3})(?![^а-яё]|$)")
    });

    re::replace_all(&RE, text, "$1\u{00A0}\u{2014}$2")
}

pub fn dash_taki(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "(верно|довольно|опять|прямо|так|вс[её]|действительно|неужели)\\s(таки){}",
        data::RU_DASH_AFTER
    ));

    re::replace_all(&re, text, "$1-$2")
}

pub fn dash_time(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "{}(\\d?\\d:[0-5]\\d){}(\\d?\\d:[0-5]\\d){}",
        data::RU_DASH_BEFORE,
        data::COMMON_DASH,
        data::RU_DASH_AFTER
    ));

    re::replace_all(&re, text, &format!("$1$2{NDASH}$3"))
}

pub fn dash_to(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let words = "[Оо]ткуда|[Кк]уда|[Гг]де|[Кк]огда|[Зз]ачем|[Пп]очему|[Кк]ак|[Кк]ако[ейм]|[Кк]акая|[Кк]аки[емх]|[Кк]акими|[Кк]акую|[Чч]то|[Чч]его|[Чч]е[йм]|[Чч]ьим?|[Кк]то|[Кк]ого|[Кк]ому|[Кк]ем";
    let re = re::compile(&format!(
        "(^|[^А-ЯЁа-яё\\w])({words})( | -|- )(то|либо|нибудь){}",
        data::RU_DASH_AFTER
    ));

    re::replace_all_fn(&re, text, |caps| {
        let kakto = format!("{}{}{}", &caps[2], &caps[3], &caps[4]);

        if kakto == "как то" || kakto == "Как то" {
            return caps[0].to_string();
        }

        format!("{}{}-{}", &caps[1], &caps[2], &caps[4])
    })
}

pub fn dash_weekday(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let part = format!("({})", data::RU_WEEKDAY);
    let re = re::compile_i(&format!("{part} ?({}) ?{part}", data::COMMON_DASH));

    re::replace_all(&re, text, &format!("$1{NDASH}$3"))
}

pub fn dash_years(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "(\\D|^)(\\d{{4}})[ \u{00A0}]?({})[ \u{00A0}]?(\\d{{4}})(?=[ \u{00A0}]?г)",
        data::COMMON_DASH
    ));

    re::replace_all_fn(&re, text, |caps| {
        let a: i32 = caps[2].parse().unwrap_or(0);
        let b: i32 = caps[4].parse().unwrap_or(0);

        if a < b {
            return format!("{}{}{NDASH}{}", &caps[1], &caps[2], &caps[4]);
        }

        caps[0].to_string()
    })
}

pub fn date_from_iso(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE1: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_i("(^|\\D)(\\d{4})(-|\\.|/)(\\d{2})(-|\\.|/)(\\d{2})(\\D|$)")
    });
    static RE2: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_i("(^|\\D)(\\d{2})(-|/)(\\d{2})(-|/)(\\d{4})(\\D|$)")
    });
    let step = re::replace_all(&RE1, text, "$1$6.$4.$2$7");

    re::replace_all(&RE2, &step, "$1$4.$2.$6$7")
}

pub fn date_weekday(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile_i(&format!(
        "(\\d)( |\u{00A0})({}),( |\u{00A0})({})",
        data::RU_MONTH_GEN,
        data::RU_WEEKDAY
    ));

    re::replace_all_fn(&re, text, |caps| {
        format!(
            "{}{}{},{}{}",
            &caps[1],
            &caps[2],
            caps[3].to_lowercase(),
            &caps[4],
            caps[5].to_lowercase()
        )
    })
}

pub fn money_currency(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let currency = "([$€¥Ұ£₤₽])";
    let space = "[ \u{00A0}\u{2009}\u{202F}]";
    let number = "\\d+([.,]\\d+)?";
    let re1 = re::compile_m(&format!(
        "(^|[\\D]{{2}}){currency} ?({number}({space}\\d{{3}})*)({space}?(тыс\\.|млн|млрд|трлн))?"
    ));
    let re2 = re::compile_m(&format!("(^|[\\D])({number}) ?{currency}"));
    let step = re::replace_all_fn(&re1, text, |caps| {
        let suffix = caps.get(7).map(|m| m.as_str()).unwrap_or("");
        let mid = if suffix.is_empty() {
            String::new()
        } else {
            format!("\u{00A0}{suffix}")
        };

        format!("{}{}{mid}\u{00A0}{}", &caps[1], &caps[3], &caps[2])
    });

    re::replace_all(&re2, &step, "$1$2\u{00A0}$4")
}

pub fn money_ruble(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let common = "(\\d+)( |\u{00A0})?(р|руб)\\.";
    let re1 = re::compile(&format!("^{common}$"));
    let re2 = re::compile(&format!("{common}(?=[!?,:;])"));
    let re3 = re::compile(&format!("{common}(?=\\s+[A-ЯЁ])"));
    let a = re::replace_all(&re1, text, "$1\u{00A0}₽");
    let b = re::replace_all(&re2, &a, "$1\u{00A0}₽");

    re::replace_all(&re3, &b, "$1\u{00A0}₽.")
}

pub fn nbsp_abbr(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "(^|\\s|{PRIVATE})([а-яё]{{1,3}})\\. ?([а-яё]{{1,3}})\\."
    ));

    fn abbr(caps: &fancy_regex::Captures<'_, str>) -> String {
        if &caps[2] == "дд" && &caps[3] == "мм" {
            return caps[0].to_string();
        }

        if ["рф", "ру", "рус", "орг", "укр", "бг", "срб"].contains(&&caps[3]) {
            return caps[0].to_string();
        }

        format!("{}{}.\u{00A0}{}.", &caps[1], &caps[2], &caps[3])
    }

    let step = re::replace_all_fn(&re, text, abbr);

    re::replace_all_fn(&re, &step, abbr)
}

pub fn nbsp_addr(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let mut out = re::replace_all(
        &re::compile_i("(\\s|^)(дом|д\\.|кв\\.|под\\.|п-д) *(\\d+)"),
        text,
        "$1$2\u{00A0}$3",
    );
    out = re::replace_all(
        &re::compile_i("(\\s|^)(мкр-н|мк-н|мкр\\.|мкрн)\\s"),
        &out,
        "$1$2\u{00A0}",
    );
    out = re::replace_all(
        &re::compile_i("(\\s|^)(эт\\.) *(-?\\d+)"),
        &out,
        "$1$2\u{00A0}$3",
    );
    out = re::replace_all(
        &re::compile_i("(\\s|^)(\\d+) +этаж([^а-яё]|$)"),
        &out,
        "$1$2\u{00A0}этаж$3",
    );
    out = re::replace_all(
        &re::compile_i("(\\s|^)литер\\s([А-Я]|$)"),
        &out,
        "$1литер\u{00A0}$2",
    );
    out = re::replace_all(
        &re::compile_i("(\\s|^)(обл|кр|ст|пос|с|д|ул|пер|пр|пр-т|просп|пл|бул|б-р|наб|ш|туп|оф|комн?|уч|вл|влад|стр|кор)\\. *([а-яёa-z\\d]+)"),
        &out,
        "$1$2.\u{00A0}$3",
    );

    re::replace_all(
        &re::compile_m("(\\D[ \u{00A0}]|^)г\\. ?([А-ЯЁ])"),
        &out,
        "$1г.\u{00A0}$2",
    )
}

pub fn nbsp_after_number_sign(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("№[ \u{00A0}\u{2009}]?(\\d|п/п)")
    });

    re::replace_all(&RE, text, "№\u{202F}$1")
}

pub fn nbsp_before_particle(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let particles = "(ли|ль|же|ж|бы|б)";
    let re1 = re::compile(&format!("([А-ЯЁа-яё]) {particles}(?=[,;:?!\"‘“»])"));
    let re2 = re::compile(&format!("([А-ЯЁа-яё])[ \u{00A0}]{particles}[ \u{00A0}]"));
    let step = re::replace_all(&re1, text, "$1\u{00A0}$2");

    re::replace_all(&re2, &step, "$1\u{00A0}$2 ")
}

pub fn nbsp_centuries(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let dashes = data::COMMON_DASH;
    let before = "(^|\\s)([VIX]+)";
    let after = "(?=[,;:?!\"‘“»]|$)";
    let re1 = re::compile_m(&format!("{before}[ \u{00A0}]?в\\.?{after}"));
    let re2 = re::compile_m(&format!(
        "{before}({dashes})([VIX]+)[ \u{00A0}]?в\\.?([ \u{00A0}]?в\\.?)?{after}"
    ));
    let step = re::replace_all(&re1, text, "$1$2\u{00A0}в.");

    re::replace_all(&re2, &step, "$1$2$3$4\u{00A0}вв.")
}

pub fn nbsp_day_month(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile_i(&format!("(\\d{{1,2}}) ({})", data::RU_SHORT_MONTH));

    re::replace_all(&re, text, "$1\u{00A0}$2")
}

pub fn nbsp_initials(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let spaces = "\u{00A0}\u{202F} ";
    let quote = data::quote("ru").map(|q| q.left).unwrap_or("«„‚");
    let re = re::compile_m(&format!(
        "(^|[({spaces}{quote}{PRIVATE}\"])([А-ЯЁ])\\.[{spaces}]?([А-ЯЁ])\\.[{spaces}]?([А-ЯЁ][а-яё]+)"
    ));

    re::replace_all(&re, text, "$1$2.\u{00A0}$3.\u{00A0}$4")
}

pub fn nbsp_m(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile_m(&format!(
        "(^|[\\s,.\\({PRIVATE}])(\\d+)[ \u{00A0}]?(мм?|см|км|дм|гм|mm?|km|cm|dm)([23²³])?([\\s\\).!?,;{PRIVATE}]|$)"
    ));

    re::replace_all_fn(&re, text, |caps| {
        let pow = match caps.get(4).map(|m| m.as_str()).unwrap_or("") {
            "2" | "²" => "²",
            "3" | "³" => "³",
            _ => "",
        };
        let end = &caps[5];
        let end = if end == "\u{00A0}" { " " } else { end };

        format!("{}{}\u{00A0}{}{pow}{end}", &caps[1], &caps[2], &caps[3])
    })
}

pub fn nbsp_mln(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_i("(\\d) ?(тыс|млн|млрд|трлн)(\\.|\\s|$)")
    });

    re::replace_all(&RE, text, "$1\u{00a0}$2$3")
}

pub fn nbsp_ooo(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(^|[^a-яёA-ЯЁ])(ООО|ОАО|ЗАО|НИИ|ПБОЮЛ) ")
    });

    re::replace_all(&RE, text, "$1$2\u{00A0}")
}

pub fn nbsp_page(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile_im(&format!(
        "(^|[)\\s{PRIVATE}])(стр|гл|рис|илл?|ст|п|c)\\. *(\\d+)([\\s.,?!;:]|$)"
    ));

    re::replace_all(&re, text, "$1$2.\u{00A0}$3$4")
}

pub fn nbsp_ps(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile_im(&format!(
        "(^|\\s|{PRIVATE})[pз]\\.[ \u{00A0}]?([pз]\\.[ \u{00A0}]?)?[sы]\\.:? "
    ));

    re::replace_all_fn(&re, text, |caps| {
        let prefix = if caps.get(2).is_some_and(|m| !m.as_str().is_empty()) {
            "P.\u{00A0}P.\u{00A0}S. "
        } else {
            "P.\u{00A0}S. "
        };

        format!("{}{prefix}", &caps[1])
    })
}

pub fn nbsp_ruble_kopek(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(\\d) ?(?=(руб|коп)\\.)"));

    re::replace_all(&RE, text, "$1\u{00A0}")
}

pub fn nbsp_see(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile_i(&format!(
        "(^|\\s|{PRIVATE}|\\()(см|им)\\.[ \u{00A0}]?([а-яё0-9a-z]+)([\\s.,?!]|$)"
    ));

    re::replace_all_fn(&re, text, |caps| {
        let lead = if &caps[1] == "\u{00A0}" { " " } else { &caps[1] };

        format!("{lead}{}.\u{00A0}{}{}", &caps[2], &caps[3], &caps[4])
    })
}

pub fn nbsp_year(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("(^|\\D)(\\d{4}) ?г([ ,;.\\n]|$)")
    });

    re::replace_all(&RE, text, "$1$2\u{00A0}г$3")
}

pub fn nbsp_years(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile_m(&format!(
        "(^|\\D)(\\d{{4}})({})(\\d{{4}})[ \u{00A0}]?г\\.?([ \u{00A0}]?г\\.)?(?=[,;:?!\"‘“»\\s]|$)",
        data::COMMON_DASH
    ));

    re::replace_all(&re, text, "$1$2$3$4\u{00A0}гг.")
}

pub fn number_comma(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile_im("(^|\\s)(\\d+)\\.(\\d+[\u{00A0}\u{2009}\u{202F} ]*?[%‰°×x])")
    });

    re::replace_all(&RE, text, "$1$2,$3")
}

pub fn number_ordinals(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    let char = ctx.chars();
    let re = re::compile(&format!(
        "(\\d[%‰]?)-(ый|ой|ая|ое|ые|ым|ом|ых|ого|ому|ыми)(?![{char}])"
    ));

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

pub fn punct_ano(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    let re = re::compile(&format!(
        "([^«„\\[(!?,:;\\-‒–—\\s{PRIVATE}])(\\s+)(а|но)(?= |\u{00A0}|\\n)"
    ));

    re::replace_all(&re, text, "$1,$2$3")
}

pub fn punct_exclamation(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE2: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile_m("(^|[^!])!{2}($|[^!])"));
    static RE4: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile_m("(^|[^!])!{4}($|[^!])"));
    let step = re::replace_all(&RE2, text, "$1!$2");

    re::replace_all(&RE4, &step, "$1!!!$2")
}

pub fn punct_exclamation_question(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^!])!\\?([^?]|$)"));

    re::replace_all(&RE, text, "$1?!$2")
}

pub fn punct_hellip_question(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE1: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(^|[^.])(\\.\\.\\.|…),"));
    static RE2: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("(!|\\?)(\\.\\.\\.|…)(?=[^.]|$)"));
    let step = re::replace_all(&RE1, text, "$1…");

    re::replace_all(&RE2, &step, "$1..")
}

pub fn space_after_hellip(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE1: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile("([а-яё])(\\.\\.\\.|…)([А-ЯЁ])"));
    static RE2: LazyLock<fancy_regex::Regex> = LazyLock::new(|| re::compile_i("([?!]\\.\\.)([а-яёa-z])"));
    let step = re::replace_all(&RE1, text, "$1$2 $3");

    re::replace_all(&RE2, &step, "$1 $2")
}

pub fn space_year(_tp: &Typograf, text: &str, ctx: &Context<'_>) -> String {
    let char = ctx.chars();
    let re = re::compile(&format!(
        "(^| |\u{00A0})(\\d{{3,4}})(год([ауе]|ом)?)([^{char}]|$)"
    ));

    re::replace_all(&re, text, "$1$2 $3$5")
}

pub fn symbols_nn(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    text.replace("№№", "№")
}

pub fn other_accent(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("([а-яё])([АЕЁИОУЫЭЮЯ])([^А-ЯЁA-Za-z0-9_]|$)")
    });

    re::replace_all_fn(&RE, text, |caps| {
        format!("{}{}\u{0301}{}", &caps[1], caps[2].to_lowercase(), &caps[3])
    })
}

pub fn switching_keyboard(_tp: &Typograf, text: &str, _ctx: &Context<'_>) -> String {
    static RE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        re::compile("([AaBEeKMHOoPpCcTyXx]{1,3})(?=[А-ЯЁа-яё]+?)")
    });

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
