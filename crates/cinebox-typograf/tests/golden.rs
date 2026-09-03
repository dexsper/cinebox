//! Golden tests: the full default pipeline (`typograph`, ru + en-US locales)
//! on realistic catalog texts, not isolated rules.

use cinebox_typograf::typograph;

const NBSP: char = '\u{a0}';

#[test]
fn russian_overview_gets_quotes_dash_nbsp_and_ellipsis() {
    let input = r#"Фильм "Начало" - шедевр Нолана, снятый в 2010 г. Бюджет - 160 млн. долларов..."#;

    let expected = format!(
        "Фильм «Начало»{NBSP}— шедевр Нолана, снятый в{NBSP}2010{NBSP}г. \
         Бюджет{NBSP}— 160{NBSP}млн. долларов…"
    );

    assert_eq!(typograph(input), expected);
}

#[test]
fn dialogue_dashes_and_short_prepositions() {
    let input = "- Кто здесь? - спросил он. Никто не ответил, и через 2-3 секунды дверь захлопнулась.";

    let expected = format!(
        "—{NBSP}Кто здесь? —{NBSP}спросил{NBSP}он. \
         Никто не{NBSP}ответил, и{NBSP}через 2-3 секунды дверь захлопнулась."
    );

    assert_eq!(typograph(input), expected);
}

#[test]
fn english_text_gets_smart_apostrophe_and_quotes() {
    let input = r#"He said "I'll be back" - and he was. It cost $5,000,000 (about 4 500 000 EUR)."#;

    let expected = format!(
        "He{NBSP}said «I’ll be{NBSP}back»{NBSP}— and he{NBSP}was. \
         It{NBSP}cost $5,000,000 (about 4 500 000 EUR)."
    );

    assert_eq!(typograph(input), expected);
}

#[test]
fn nested_quotes_and_year_range_dash() {
    let input = r#"Сериал "Тьма" (нем. "Dark") выходил в 2017-2020 гг. на Netflix."#;

    let expected = format!(
        "Сериал «Тьма» (нем. «Dark») выходил в{NBSP}2017–2020{NBSP}гг. на{NBSP}Netflix."
    );

    assert_eq!(typograph(input), expected);
}

#[test]
fn abbreviations_superscript_and_space_collapse() {
    let input = "Т. е. т. н. «умный дом» - это 100 м2 датчиков,   и т. д. и т. п.";

    let expected = format!(
        "Т. е.{NBSP}т.{NBSP}н. «умный дом»{NBSP}— это 100{NBSP}м² датчиков, \
         и{NBSP}т.{NBSP}д.{NBSP}и{NBSP}т.{NBSP}п."
    );

    assert_eq!(typograph(input), expected);
}

#[test]
fn pipeline_is_idempotent_on_golden_texts() {
    let inputs = [
        r#"Фильм "Начало" - шедевр Нолана, снятый в 2010 г. Бюджет - 160 млн. долларов..."#,
        r#"He said "I'll be back" - and he was. It cost $5,000,000 (about 4 500 000 EUR)."#,
        r#"Сериал "Тьма" (нем. "Dark") выходил в 2017-2020 гг. на Netflix."#,
    ];

    for input in inputs {
        let once = typograph(input);
        let twice = typograph(&once);

        assert_eq!(once, twice, "not idempotent for {input:?}");
    }
}
