use serde::Deserialize;
use cinebox_typograf::Typograf;

#[derive(Deserialize)]
struct Suite {
    name: String,
    locale: LocaleSpec,
    #[serde(default)]
    inner: bool,
    pairs: Vec<Pair>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LocaleSpec {
    One(String),
    Many(Vec<String>),
}

impl LocaleSpec {
    fn as_vec(&self) -> Vec<String> {
        match self {
            Self::One(s) => vec![s.clone()],
            Self::Many(v) => v.clone(),
        }
    }
}

#[derive(Deserialize)]
struct Pair {
    before: String,
    after: String,
    #[serde(default)]
    enable: Vec<String>,
}

#[test]
fn original_rule_fixtures() {
    let json = include_str!("fixtures/rules.json");
    let suites: Vec<Suite> = serde_json::from_str(json).expect("rules.json");
    let mut failed = 0usize;
    let mut report = String::new();

    for suite in &suites {
        if suite.pairs.is_empty() {
            continue;
        }

        let rule_loc = suite.name.split('/').next().unwrap_or("common");
        let mut locales = suite.locale.as_vec();

        if rule_loc != "common" && locales.first().map(String::as_str) != Some(rule_loc) {
            locales = vec![rule_loc.to_string()];
        }

        if locales.is_empty() {
            continue;
        }

        for pair in &suite.pairs {
            let mut extra = pair.enable.clone();

            if suite.name == "common/punctuation/quote" && pair.before.contains("&quot;") {
                extra.push("common/html/quot".to_string());
            }

            let got = run(&suite.name, &locales, suite.inner, &extra, &pair.before);

            if got != pair.after {
                failed += 1;

                if failed <= 25 {
                    report.push_str(&format!(
                        "\n--- {} {:?} inner={}\nbefore: {:?}\ngot:    {:?}\nwant:   {:?}\n",
                        suite.name, locales, suite.inner, pair.before, got, pair.after
                    ));
                }

                continue;
            }

            if suite.inner || !Typograf::rule_enabled_by_default(&suite.name) {
                continue;
            }

            let again = run(&suite.name, &locales, suite.inner, &extra, &got);

            if again != pair.after {
                failed += 1;

                if failed <= 25 {
                    report.push_str(&format!(
                        "\n--- {} {:?} not idempotent\nfirst: {:?}\nsecond {:?}\nwant: {:?}\n",
                        suite.name, locales, got, again, pair.after
                    ));
                }
            }
        }
    }

    assert_eq!(failed, 0, "{failed} fixture pair(s) failed:{report}");
}

fn run(name: &str, locales: &[String], inner: bool, extra: &[String], text: &str) -> String {
    let mut tp = Typograf::new(locales.iter().map(String::as_str));

    if inner {
        return tp.execute_inner(name, text);
    }

    tp.disable_rule("*");
    tp.enable_rule(name);

    for rule in extra {
        tp.enable_rule(rule);
    }

    tp.execute_prefs(text, Some(locales))
}
