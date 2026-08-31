//! Locale character, quote, and short-word tables.

pub const COMMON_CHAR: &str = "a-z";
pub const COMMON_DASH: &str = "--?|‒|–|—";
pub const COMMON_QUOTE: &str = "«‹»›„“‟”\"";

pub const RU_DASH_BEFORE: &str = "(^| |\\n)";
pub const RU_DASH_AFTER: &str = "(?=[\u{00A0} ,.?:!]|$)";
pub const RU_DASH_AFTER_DE: &str = "(?=[,.?:!]|[\u{00A0} ][^А-ЯЁ]|$)";
pub const RU_MONTH: &str = "январь|февраль|март|апрель|май|июнь|июль|август|сентябрь|октябрь|ноябрь|декабрь";
pub const RU_MONTH_GEN: &str = "января|февраля|марта|апреля|мая|июня|июля|августа|сентября|октября|ноября|декабря";
pub const RU_MONTH_PRE: &str = "январе|феврале|марте|апреле|мае|июне|июле|августе|сентябре|октябре|ноябре|декабре";
pub const RU_SHORT_MONTH: &str = "янв|фев|мар|апр|ма[ейя]|июн|июл|авг|сен|окт|ноя|дек";
pub const RU_WEEKDAY: &str = "понедельник|вторник|среда|четверг|пятница|суббота|воскресенье";

#[derive(Clone, Copy, Debug)]
pub struct QuoteData {
    pub left: &'static str,
    pub right: &'static str,
    /// `None` = no spacing. `Some(n)` = first `n` levels. `Some(usize::MAX)` = all levels (`spacing: true`).
    pub spacing: Option<usize>,
    pub remove_duplicate_quotes: bool,
}

struct Locale {
    chars: &'static str,
    quote: QuoteData,
    short_word: Option<&'static str>,
}

fn find(code: &str) -> Option<&'static Locale> {
    Some(match code {
        "be" => &BE,
        "bg" => &BG,
        "ca" => &CA,
        "cs" => &CS,
        "da" => &DA,
        "de" => &DE,
        "el" => &EL,
        "en-GB" => &EN_GB,
        "en-US" => &EN_US,
        "eo" => &EO,
        "es" => &ES,
        "et" => &ET,
        "fi" => &FI,
        "fr" => &FR,
        "ga" => &GA,
        "hu" => &HU,
        "it" => &IT,
        "lv" => &LV,
        "nl" => &NL,
        "no" => &NO,
        "pl" => &PL,
        "ro" => &RO,
        "ru" => &RU,
        "sk" => &SK,
        "sl" => &SL,
        "sr" => &SR,
        "sv" => &SV,
        "tr" => &TR,
        "uk" => &UK,
        _ => return None,
    })
}

pub fn has_locale(code: &str) -> bool {
    code == "common" || find(code).is_some()
}

pub fn chars(code: &str) -> &'static str {
    if code == "common" {
        return COMMON_CHAR;
    }

    find(code).map(|l| l.chars).unwrap_or("")
}

pub fn short_word(code: &str) -> Option<&'static str> {
    find(code).and_then(|l| l.short_word)
}

pub fn quote(code: &str) -> Option<&'static QuoteData> {
    find(code).map(|l| &l.quote)
}

pub fn joined_chars(locales: &[String]) -> String {
    locales.iter().map(|loc| chars(loc)).collect()
}

macro_rules! loc {
    ($name:ident, $chars:expr, $left:expr, $right:expr, $sw:expr) => {
        static $name: Locale = Locale {
            chars: $chars,
            quote: QuoteData {
                left: $left,
                right: $right,
                spacing: None,
                remove_duplicate_quotes: false,
            },
            short_word: $sw,
        };
    };
}

loc!(BE, "абвгдежзйклмнопрстуфхцчшыьэюяёіўґ", "«„«", "»“»", None);
loc!(BG, "абвгдежзийклмнопрстуфхцчшщъьюя", "„", "“", Some("а|в|и|о|с|у"));
loc!(CA, "a-zàçèéíïòóúü", "«“‘", "»”’", None);
loc!(CS, "a-záéíóúýčďěňřšťůž", "„‚»", "“‘«", Some("k|s|v|z"));
loc!(DA, "a-zåæø", "»", "«", None);
loc!(DE, "a-zßẞäöü", "„‚", "“‘", Some("ab|aber|als|am|an|ans|auf|aufs|aus|bei|beim|bis|da|das|dass|dem|den|denn|der|des|die|doch|ein|eine|für|fürs|im|in|ins|mit|nach|ob|oder|ohne|seit|so|über|um|ums|und|vom|von|vor|weil|wenn|wie|zu|zum|zur"));
loc!(EL, "ΐάέήίΰαβγδεζηθικλμνξοπρςστυφχψωϊϋόύώϲάέήίόύώ", "«“‘", "»”’", None);
loc!(EN_GB, "a-z", "‘“‘", "’”’", Some("a|an|and|as|at|bar|but|by|for|if|in|nor|not|of|off|on|or|out|per|pro|so|the|to|up|via|yet"));
loc!(EN_US, "a-z", "“‘“", "”’”", Some("a|an|and|as|at|bar|but|by|for|if|in|nor|not|of|off|on|or|out|per|pro|so|the|to|up|via|yet"));
loc!(EO, "abcdefghijklmnoprstuvzĉĝĥĵŝŭ", "“‘“", "”’”", None);
loc!(ES, "a-záéíñóúü", "«“‘", "»”’", None);
loc!(ET, "a-zäõöüšž", "„", "“", None);
loc!(FI, "a-zšžåäö", "”’", "”’", None);
loc!(GA, "abcdefghilmnoprstuvwxyzáéíóú", "‘“‘", "’”’", None);
loc!(HU, "a-záäéíóöúüőű", "„»’", "”«’", Some("a|az"));
loc!(IT, "a-zàéèìòù", "«“‘", "»”’", Some("a|da|di|in|la|il|lo|e|o|se|su|che|come|ma|è|ho|ha|sa"));
loc!(LV, "a-zāčēģīķļņšūž", "“", "”", None);
loc!(NL, "a-záäçèéêëíîïñóöúûü", "‘“‘", "’”’", None);
loc!(NO, "a-zàåæèéêòóôø", "«‘«", "»’»", None);
loc!(PL, "abcdefghijklmnoprstuvwxyzóąćęłńśźż", "„«‘", "”»’", Some("a|i|o|u|w|z"));
loc!(RO, "a-zăâîșț", "„«„", "”»”", None);
loc!(SK, "a-záäéíóôúýčďĺľňŕšťž", "„‚»", "“‘«", Some("k|o|s|u|v|z"));
loc!(SL, "a-zčšž", "„‚", "“‘", None);
loc!(SR, "abcdefghijklmnoprstuvzćčđšž", "„’", "”’", None);
loc!(SV, "a-zàäåéö", "”’", "”’", None);
loc!(TR, "abcdefghijklmnoprstuvyzâçîöûüİğış", "“‘“", "”’”", None);
loc!(UK, "абвгдежзийклмнопрстуфхцчшщьюяєіїґ", "«„«", "»“»", Some("а|в|до|з|за|зі|зо|і|із|й|на|не|ні|о|об|од|по|та|у"));

static FR: Locale = Locale {
    chars: "a-zàâçèéêëîïôùûüÿœæ",
    quote: QuoteData {
        left: "«“‘",
        right: "»”’",
        spacing: Some(1),
        remove_duplicate_quotes: false,
    },
    short_word: Some("à|au|aux|avec|car|chez|dans|de|des|donc|du|en|et|hors|la|le|les|mais|ni|or|ou|par|pas|pour|que|sans|si|sous|sur|un|une|vers"),
};

static RU: Locale = Locale {
    chars: "а-яё",
    quote: QuoteData {
        left: "«„‚",
        right: "»“‘",
        spacing: None,
        remove_duplicate_quotes: true,
    },
    short_word: Some("а|без|в|во|если|да|до|для|за|и|или|из|к|ко|как|ли|на|но|не|ни|о|об|обо|от|по|про|при|под|с|со|то|у"),
};
