mod en;
mod html;
mod nbsp;
mod number;
mod optalign;
mod other;
mod phone;
mod punct;
mod ru;
mod space;
mod symbols;

use std::borrow::Cow;

use crate::engine::{Context, Typograf};

pub type Handler = for<'a> fn(&Typograf, &'a str, &Context<'_>) -> Cow<'a, str>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Queue {
    Start,
    HideSafeTagsOwn,
    HideSafeTagsHtml,
    HideSafeTagsUrl,
    HideSafeTags,
    Utf,
    Default,
    HtmlEntities,
    ShowSafeTagsUrl,
    ShowSafeTagsHtml,
    ShowSafeTagsOwn,
    End,
}

pub const QUEUES: &[Queue] = &[
    Queue::Start,
    Queue::HideSafeTagsOwn,
    Queue::HideSafeTagsHtml,
    Queue::HideSafeTagsUrl,
    Queue::HideSafeTags,
    Queue::Utf,
    Queue::Default,
    Queue::HtmlEntities,
    Queue::ShowSafeTagsUrl,
    Queue::ShowSafeTagsHtml,
    Queue::ShowSafeTagsOwn,
    Queue::End,
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Live {
    Any,
    #[allow(dead_code)]
    On,
    Off,
}

pub struct Rule {
    pub name: &'static str,
    pub locale: &'static str,
    pub queue: Queue,
    pub index: i32,
    pub order: u16,
    pub enabled: bool,
    pub live: Live,
    pub html_attrs: bool,
    pub inner: bool,
    pub handler: Handler,
}

pub fn all() -> &'static [Rule] {
    RULES
}

#[allow(clippy::too_many_arguments)]
const fn r(
    order: u16,
    locale: &'static str,
    name: &'static str,
    queue: Queue,
    index: i32,
    enabled: bool,
    live: Live,
    html_attrs: bool,
    inner: bool,
    handler: Handler,
) -> Rule {
    Rule {
        name,
        locale,
        queue,
        index,
        order,
        enabled,
        live,
        html_attrs,
        inner,
        handler,
    }
}

const RULES: &[Rule] = &[
    // common/html
    r(0, "common", "common/html/e-mail", Queue::End, 1210, false, Live::Any, false, false, html::e_mail),
    r(1, "common", "common/html/escape", Queue::End, 1310, false, Live::Any, true, false, html::escape),
    r(2, "common", "common/html/nbr", Queue::End, 1220, false, Live::Any, false, false, html::nbr),
    r(3, "common", "common/html/p", Queue::End, 1215, false, Live::Any, false, false, html::p),
    r(4, "common", "common/html/processingAttrs", Queue::HideSafeTagsOwn, 1210, false, Live::Any, false, false, html::processing_attrs),
    r(5, "common", "common/html/quot", Queue::HideSafeTags, 1210, true, Live::Any, true, false, html::quot),
    r(6, "common", "common/html/stripTags", Queue::End, 1309, false, Live::Any, true, false, html::strip_tags),
    r(7, "common", "common/html/url", Queue::End, 1210, false, Live::Any, false, false, html::url),
    // common/nbsp
    r(8, "common", "common/nbsp/afterNumber", Queue::Default, 510, false, Live::Any, true, false, nbsp::after_number),
    r(9, "common", "common/nbsp/afterParagraphMark", Queue::Default, 510, true, Live::Any, true, false, nbsp::after_paragraph_mark),
    r(10, "common", "common/nbsp/afterSectionMark", Queue::Default, 510, true, Live::Any, true, false, nbsp::after_section_mark),
    r(11, "common", "common/nbsp/afterShortWord", Queue::Default, 510, true, Live::Any, true, false, nbsp::after_short_word),
    r(12, "common", "common/nbsp/afterShortWordByList", Queue::Default, 510, true, Live::Any, true, false, nbsp::after_short_word_by_list),
    r(13, "common", "common/nbsp/beforeShortLastNumber", Queue::Default, 510, true, Live::Off, true, false, nbsp::before_short_last_number),
    r(14, "common", "common/nbsp/beforeShortLastWord", Queue::Default, 510, true, Live::Any, true, false, nbsp::before_short_last_word),
    r(15, "common", "common/nbsp/dpi", Queue::Default, 510, true, Live::Any, true, false, nbsp::dpi),
    r(16, "common", "common/nbsp/nowrap", Queue::End, 510, true, Live::Any, true, false, nbsp::nowrap),
    r(17, "common", "common/nbsp/replaceNbsp", Queue::Utf, 510, false, Live::Off, true, false, nbsp::replace_nbsp),
    // common/number
    r(18, "common", "common/number/digitGrouping", Queue::Default, 460, false, Live::Any, true, false, number::digit_grouping),
    r(19, "common", "common/number/fraction", Queue::Default, 150, true, Live::Any, true, false, number::fraction),
    r(20, "common", "common/number/mathSigns", Queue::Default, 150, true, Live::Any, true, false, number::math_signs),
    r(21, "common", "common/number/times", Queue::Default, 150, true, Live::Any, true, false, number::times),
    // common/other
    r(22, "common", "common/other/delBOM", Queue::Start, -1, true, Live::Any, true, false, other::del_bom),
    r(23, "common", "common/other/repeatWord", Queue::Default, 910, false, Live::Any, true, false, other::repeat_word),
    // common/punctuation
    r(24, "common", "common/punctuation/apostrophe", Queue::Default, 410, true, Live::Any, true, false, punct::apostrophe),
    r(25, "common", "common/punctuation/delDoublePunctuation", Queue::Default, 410, true, Live::Any, true, false, punct::del_double),
    r(26, "common", "common/punctuation/hellip", Queue::Default, 410, true, Live::Any, true, false, punct::hellip),
    r(27, "common", "common/punctuation/quote", Queue::Default, 410, true, Live::Any, true, false, punct::quote_rule),
    r(28, "common", "common/punctuation/quoteLink", Queue::ShowSafeTagsHtml, 415, true, Live::Any, true, false, punct::quote_link),
    // common/space
    r(29, "common", "common/space/afterColon", Queue::Default, 210, true, Live::Any, true, false, space::after_colon),
    r(30, "common", "common/space/afterComma", Queue::Default, 210, true, Live::Any, true, false, space::after_comma),
    r(31, "common", "common/space/afterQuestionMark", Queue::Default, 210, true, Live::Any, true, false, space::after_question),
    r(32, "common", "common/space/afterExclamationMark", Queue::Default, 210, true, Live::Any, true, false, space::after_exclamation),
    r(33, "common", "common/space/afterSemicolon", Queue::Default, 210, true, Live::Any, true, false, space::after_semicolon),
    r(34, "common", "common/space/beforeBracket", Queue::Default, 210, true, Live::Any, true, false, space::before_bracket),
    r(35, "common", "common/space/bracket", Queue::Default, 210, true, Live::Any, true, false, space::bracket),
    r(36, "common", "common/space/delBeforeDot", Queue::Default, 210, true, Live::Any, true, false, space::del_before_dot),
    r(37, "common", "common/space/delBeforePercent", Queue::Default, 210, true, Live::Any, true, false, space::del_before_percent),
    r(38, "common", "common/space/delBeforePunctuation", Queue::Default, 210, true, Live::Any, true, false, space::del_before_punctuation),
    r(39, "common", "common/space/delBetweenExclamationMarks", Queue::Default, 210, true, Live::Any, true, false, space::del_between_exclamation),
    r(40, "common", "common/space/delLeadingBlanks", Queue::Default, 210, false, Live::Any, true, false, space::del_leading_blanks),
    r(41, "common", "common/space/delRepeatN", Queue::Default, 209, true, Live::Any, true, false, space::del_repeat_n),
    r(42, "common", "common/space/delRepeatSpace", Queue::Default, 209, true, Live::Any, true, false, space::del_repeat_space),
    r(43, "common", "common/space/delTrailingBlanks", Queue::Default, 207, true, Live::Any, true, false, space::del_trailing_blanks),
    r(44, "common", "common/space/insertFinalNewline", Queue::End, 210, false, Live::Off, true, false, space::insert_final_newline),
    r(45, "common", "common/space/replaceTab", Queue::Default, 205, true, Live::Any, true, false, space::replace_tab),
    r(46, "common", "common/space/squareBracket", Queue::Default, 210, true, Live::Any, true, false, space::square_bracket),
    r(47, "common", "common/space/trimLeft", Queue::Default, 206, true, Live::Any, true, false, space::trim_left),
    r(48, "common", "common/space/trimRight", Queue::Default, 207, true, Live::Off, true, false, space::trim_right),
    // common/symbols
    r(49, "common", "common/symbols/arrow", Queue::Default, 110, true, Live::Any, true, false, symbols::arrow),
    r(50, "common", "common/symbols/cf", Queue::Default, 110, true, Live::Any, true, false, symbols::cf),
    r(51, "common", "common/symbols/copy", Queue::Default, 110, true, Live::Any, true, false, symbols::copy),
    // en-US / en-GB
    r(52, "en-US", "en-US/dash/main", Queue::Default, 305, true, Live::Any, true, false, en::dash_en_us),
    r(53, "en-GB", "en-GB/dash/main", Queue::Default, 305, true, Live::Any, true, false, en::dash_en_gb),
    // ru/dash
    r(54, "ru", "ru/dash/centuries", Queue::Default, 310, true, Live::Any, true, false, ru::dash_centuries),
    r(55, "ru", "ru/dash/daysMonth", Queue::Default, 310, true, Live::Any, true, false, ru::dash_days_month),
    r(56, "ru", "ru/dash/de", Queue::Default, 310, false, Live::Any, true, false, ru::dash_de),
    r(57, "ru", "ru/dash/decade", Queue::Default, 310, true, Live::Any, true, false, ru::dash_decade),
    r(58, "ru", "ru/dash/directSpeech", Queue::Default, 310, true, Live::Any, true, false, ru::dash_direct_speech),
    r(59, "ru", "ru/dash/izpod", Queue::Default, 310, true, Live::Any, true, false, ru::dash_izpod),
    r(60, "ru", "ru/dash/izza", Queue::Default, 310, true, Live::Any, true, false, ru::dash_izza),
    r(61, "ru", "ru/dash/ka", Queue::Default, 310, true, Live::Any, true, false, ru::dash_ka),
    r(62, "ru", "ru/dash/koe", Queue::Default, 310, true, Live::Any, true, false, ru::dash_koe),
    r(63, "ru", "ru/dash/main", Queue::Default, 305, true, Live::Any, true, false, ru::dash_main),
    r(64, "ru", "ru/dash/month", Queue::Default, 310, true, Live::Any, true, false, ru::dash_month),
    r(65, "ru", "ru/dash/surname", Queue::Default, 310, true, Live::Any, true, false, ru::dash_surname),
    r(66, "ru", "ru/dash/taki", Queue::Default, 310, true, Live::Any, true, false, ru::dash_taki),
    r(67, "ru", "ru/dash/time", Queue::Default, 310, true, Live::Any, true, false, ru::dash_time),
    r(68, "ru", "ru/dash/to", Queue::Default, 310, true, Live::Any, true, false, ru::dash_to),
    r(69, "ru", "ru/dash/kakto", Queue::Default, 310, true, Live::Any, true, false, ru::dash_kakto),
    r(70, "ru", "ru/dash/weekday", Queue::Default, 310, true, Live::Any, true, false, ru::dash_weekday),
    r(71, "ru", "ru/dash/years", Queue::Default, 310, true, Live::Any, true, false, ru::dash_years),
    // ru/date
    r(72, "ru", "ru/date/fromISO", Queue::Default, 810, true, Live::Any, true, false, ru::date_from_iso),
    r(73, "ru", "ru/date/weekday", Queue::Default, 810, true, Live::Any, true, false, ru::date_weekday),
    // ru/money
    r(74, "ru", "ru/money/currency", Queue::Default, 710, false, Live::Any, true, false, ru::money_currency),
    r(75, "ru", "ru/money/ruble", Queue::Default, 710, false, Live::Any, true, false, ru::money_ruble),
    // ru/nbsp
    r(76, "ru", "ru/nbsp/abbr", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_abbr),
    r(77, "ru", "ru/nbsp/addr", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_addr),
    r(78, "ru", "ru/nbsp/afterNumberSign", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_after_number_sign),
    r(79, "ru", "ru/nbsp/beforeParticle", Queue::Default, 515, true, Live::Any, true, false, ru::nbsp_before_particle),
    r(80, "ru", "ru/nbsp/centuries", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_centuries),
    r(81, "ru", "ru/nbsp/dayMonth", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_day_month),
    r(82, "ru", "ru/nbsp/initials", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_initials),
    r(83, "ru", "ru/nbsp/m", Queue::Default, 515, true, Live::Any, true, false, ru::nbsp_m),
    r(84, "ru", "ru/nbsp/mln", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_mln),
    r(85, "ru", "ru/nbsp/ooo", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_ooo),
    r(86, "ru", "ru/nbsp/page", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_page),
    r(87, "ru", "ru/nbsp/ps", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_ps),
    r(88, "ru", "ru/nbsp/rubleKopek", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_ruble_kopek),
    r(89, "ru", "ru/nbsp/see", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_see),
    r(90, "ru", "ru/nbsp/year", Queue::Default, 510, true, Live::Any, true, false, ru::nbsp_year),
    r(91, "ru", "ru/nbsp/years", Queue::Default, 515, true, Live::Any, true, false, ru::nbsp_years),
    // ru/number
    r(92, "ru", "ru/number/comma", Queue::Default, 150, true, Live::Any, true, false, ru::number_comma),
    r(93, "ru", "ru/number/ordinals", Queue::Default, 150, true, Live::Any, true, false, ru::number_ordinals),
    // ru/optalign main + inner
    r(94, "ru", "ru/optalign/bracket", Queue::Default, 1010, false, Live::Any, false, false, optalign::bracket),
    r(95, "ru", "ru/optalign/comma", Queue::Default, 1010, false, Live::Any, false, false, optalign::comma),
    r(96, "ru", "ru/optalign/quote", Queue::Default, 1010, false, Live::Any, false, false, optalign::quote),
    r(97, "ru", "ru/optalign/bracket", Queue::Start, 1010, true, Live::Any, false, true, optalign::bracket_start),
    r(98, "ru", "ru/optalign/bracket", Queue::End, 1010, true, Live::Any, false, true, optalign::bracket_end),
    r(99, "ru", "ru/optalign/comma", Queue::Start, 1010, true, Live::Any, false, true, optalign::comma_start),
    r(100, "ru", "ru/optalign/comma", Queue::End, 1010, true, Live::Any, false, true, optalign::comma_end),
    r(101, "ru", "ru/optalign/quote", Queue::Start, 1010, true, Live::Any, false, true, optalign::quote_start),
    r(102, "ru", "ru/optalign/quote", Queue::End, 1010, true, Live::Any, false, true, optalign::quote_end),
    // ru/other
    r(103, "ru", "ru/other/accent", Queue::Default, 910, false, Live::Any, true, false, ru::other_accent),
    r(104, "ru", "ru/other/phone-number", Queue::Default, 910, true, Live::Off, true, false, phone::phone_number),
    // ru/punctuation
    r(105, "ru", "ru/punctuation/ano", Queue::Default, 410, true, Live::Any, true, false, ru::punct_ano),
    r(106, "ru", "ru/punctuation/exclamation", Queue::Default, 410, true, Live::Off, true, false, ru::punct_exclamation),
    r(107, "ru", "ru/punctuation/exclamationQuestion", Queue::Default, 415, true, Live::Any, true, false, ru::punct_exclamation_question),
    r(108, "ru", "ru/punctuation/hellipQuestion", Queue::Default, 410, true, Live::Any, true, false, ru::punct_hellip_question),
    // ru/space
    r(109, "ru", "ru/space/afterHellip", Queue::Default, 210, true, Live::Any, true, false, ru::space_after_hellip),
    r(110, "ru", "ru/space/year", Queue::Default, 210, true, Live::Any, true, false, ru::space_year),
    // ru/symbols + typo
    r(111, "ru", "ru/symbols/NN", Queue::Default, 110, true, Live::Any, true, false, ru::symbols_nn),
    r(112, "ru", "ru/typo/switchingKeyboardLayout", Queue::Default, 1110, true, Live::Any, true, false, ru::switching_keyboard),
];
