//! IBM Plex Sans, with the Plex scripts it lacks as fallbacks. OFL see `assets/fonts/license.txt`.

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{Context, FontData, FontFamily, FontId};

const REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf");
const MEDIUM: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf");
const BOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Bold.ttf");

const FAMILY_REGULAR: &str = "ibm_plex_sans";
const FAMILY_MEDIUM: &str = "ibm_plex_medium";
const FAMILY_BOLD: &str = "ibm_plex_bold";

/// A Plex font for a script Plex Sans lacks.
struct Script {
    name: &'static str,
    data: &'static [u8],
    /// Fraction of the font size that moves the glyphs down onto Plex's
    /// baseline: egui centres a fallback's line box instead.
    baseline: f32,
}

/// In fallback order: a Han character comes from the first font that has it,
/// so Japanese kanji take the Simplified Chinese forms.
macro_rules! scripts {
    ($weight:literal) => {
        [
            script!("Arabic", $weight, 0.04),
            script!("Hebrew", $weight, 0.0),
            script!("Devanagari", $weight, 0.07),
            script!("Thai", $weight, 0.084),
            script!("SC", $weight, 0.495),
            script!("TC", $weight, 0.495),
            script!("JP", $weight, 0.495),
            script!("KR", $weight, 0.04),
        ]
    };
}

macro_rules! script {
    ($script:literal, $weight:literal, $baseline:literal) => {
        Script {
            name: concat!("ibm_plex_", $script, "_", $weight),
            data: include_bytes!(concat!(
                "../assets/fonts/IBMPlexSans",
                $script,
                "-",
                $weight,
                ".otf"
            )),
            baseline: $baseline,
        }
    };
}

const SCRIPTS_REGULAR: [Script; 8] = scripts!("Regular");
const SCRIPTS_MEDIUM: [Script; 8] = scripts!("Medium");
const SCRIPTS_BOLD: [Script; 8] = scripts!("Bold");

/// Register Plex ahead of the default proportional stack (emoji / icons stay fallbacks).
/// The other scripts go after the icons, so a CJK font cannot take an icon's code point.
pub fn install(ctx: &Context) {
    let ui = InsertFontFamily {
        family: FontFamily::Proportional,
        priority: FontPriority::Highest,
    };

    ctx.add_font(FontInsert::new(
        FAMILY_REGULAR,
        FontData::from_static(REGULAR),
        vec![ui],
    ));

    install_named(ctx, FAMILY_MEDIUM, MEDIUM);
    install_named(ctx, FAMILY_BOLD, BOLD);
    egui_material_icons::initialize(ctx);

    install_fallbacks(ctx, FontFamily::Proportional, &SCRIPTS_REGULAR);
    install_fallbacks(ctx, FontFamily::Name(FAMILY_MEDIUM.into()), &SCRIPTS_MEDIUM);
    install_fallbacks(ctx, FontFamily::Name(FAMILY_BOLD.into()), &SCRIPTS_BOLD);
}

/// Display / headings (Medium).
#[must_use]
pub fn title(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_MEDIUM.into()))
}

/// Primary actions (Bold).
#[must_use]
pub fn emphasis(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_BOLD.into()))
}

fn install_named(ctx: &Context, name: &'static str, ttf: &'static [u8]) {
    let family = InsertFontFamily {
        family: FontFamily::Name(name.into()),
        priority: FontPriority::Highest,
    };

    ctx.add_font(FontInsert::new(
        name,
        FontData::from_static(ttf),
        vec![family],
    ));
}

fn install_fallbacks(ctx: &Context, family: FontFamily, scripts: &[Script]) {
    for script in scripts {
        let fallback = InsertFontFamily {
            family: family.clone(),
            priority: FontPriority::Lowest,
        };

        ctx.add_font(FontInsert::new(
            script.name,
            script_data(script),
            vec![fallback],
        ));
    }
}

fn script_data(script: &Script) -> FontData {
    let mut data = FontData::from_static(script.data);
    data.tweak.y_offset_factor = script.baseline;

    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_weight_draws_titles_in_other_scripts() {
        let ctx = Context::default();
        install(&ctx);
        // Fonts are added on the next pass.
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();

        let titles = [
            "仙逆剧场版：弑仙之战",
            "進擊的巨人",
            "すずめの戸締まり",
            "기생충",
            "مرحبا",
            "שלום",
            "नमस्ते",
            "สวัสดี",
        ];
        let fonts = [FontId::proportional(14.0), title(14.0), emphasis(14.0)];

        for font in &fonts {
            for text in titles {
                let drawn = ctx.fonts_mut(|f| f.has_glyphs(font, text));
                assert!(drawn, "{font:?} has no glyphs for {text}");
            }
        }
    }

    /// One font per context: in the shared stack a character that several
    /// fonts have would only test the first of them.
    #[test]
    fn every_script_shares_the_latin_baseline() {
        const SIZE: f32 = 1000.0;
        let weights = [
            (REGULAR, &SCRIPTS_REGULAR),
            (MEDIUM, &SCRIPTS_MEDIUM),
            (BOLD, &SCRIPTS_BOLD),
        ];

        let samples = ['م', 'ש', 'न', 'ส', '仙', '擊', 'す', '기'];
        for (plex, scripts) in weights {
            for (script, sample) in scripts.iter().zip(samples) {
                let ctx = Context::default();
                let family = FontFamily::Proportional;
                let mut fonts = egui::FontDefinitions::empty();

                fonts
                    .font_data
                    .insert("plex".into(), FontData::from_static(plex).into());

                fonts
                    .font_data
                    .insert(script.name.into(), script_data(script).into());

                let stack = vec!["plex".to_owned(), script.name.to_owned()];
                fonts.families.insert(family.clone(), stack);
                ctx.set_fonts(fonts);

                let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
                output.textures_delta.clear();

                let font = FontId::new(SIZE, family);
                let text = format!("A{sample}");

                let galley = ctx.fonts_mut(|f| f.layout_no_wrap(text, font, egui::Color32::WHITE));
                let glyphs = &galley.rows[0].row.glyphs;
                let shown = glyphs[1].pos.y + script.baseline * SIZE;
                let off = (shown - glyphs[0].pos.y) / SIZE;

                assert!(
                    off.abs() < 0.002,
                    "{} is {off:.4} em off the baseline",
                    script.name
                );
            }
        }
    }
}
