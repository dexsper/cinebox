//! IBM Plex Sans. OFL — see `assets/fonts/license.txt`.

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{Context, FontData, FontFamily, FontId};

const REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf");
const MEDIUM: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf");
const BOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Bold.ttf");

const FAMILY_REGULAR: &str = "ibm_plex_sans";
const FAMILY_MEDIUM: &str = "ibm_plex_medium";
const FAMILY_BOLD: &str = "ibm_plex_bold";

/// Register Plex ahead of the default proportional stack (emoji / icons stay fallbacks).
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
