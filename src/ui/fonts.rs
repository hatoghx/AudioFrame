use crate::core::loc::Lang;
use eframe::egui::{self, FontData, FontFamily};

const IBM_PLEX_NAME: &str = "ui_ibm_plex_sans_light";
const IBM_PLEX_JP_NAME: &str = "ui_ibm_plex_sans_jp_light";
const IBM_PLEX_SC_NAME: &str = "ui_ibm_plex_sans_sc_light";
const IBM_PLEX_LIGHT: &[u8] = include_bytes!("../fonts/IBMPlexSans-Light.ttf");
const IBM_PLEX_JP_LIGHT: &[u8] = include_bytes!("../fonts/IBMPlexSansJP-Light.ttf");
const IBM_PLEX_SC_LIGHT: &[u8] = include_bytes!("../fonts/IBMPlexSansSC-Light.ttf");

pub fn language_text(lang: Lang) -> egui::RichText {
    egui::RichText::new(lang.label()).font(egui::FontId::new(12.0, FontFamily::Proportional))
}

/// The active language's face comes first so every glyph on a line is drawn from
/// one font; mixing faces makes glyphs sit at different heights.
fn font_order(lang: Lang) -> [&'static str; 3] {
    match lang {
        Lang::Ja => [IBM_PLEX_JP_NAME, IBM_PLEX_NAME, IBM_PLEX_SC_NAME],
        Lang::Zh => [IBM_PLEX_SC_NAME, IBM_PLEX_NAME, IBM_PLEX_JP_NAME],
        Lang::En => [IBM_PLEX_NAME, IBM_PLEX_JP_NAME, IBM_PLEX_SC_NAME],
    }
}

pub fn apply(ctx: &egui::Context, lang: Lang) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, data) in [
        (IBM_PLEX_NAME, IBM_PLEX_LIGHT),
        (IBM_PLEX_JP_NAME, IBM_PLEX_JP_LIGHT),
        (IBM_PLEX_SC_NAME, IBM_PLEX_SC_LIGHT),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_static(data).into());
    }

    let order = font_order(lang);
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        if let Some(stack) = fonts.families.get_mut(&family) {
            for name in order.iter().rev() {
                stack.insert(0, (*name).to_owned());
            }
        }
    }
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ascent of the face that actually drew each character of `text`.
    fn glyph_ascents(lang: Lang, text: &str) -> Vec<f32> {
        let ctx = egui::Context::default();
        apply(&ctx, lang);
        let _ = ctx.run(Default::default(), |_| {});
        ctx.fonts(|fonts| {
            fonts
                .layout_no_wrap(
                    text.to_owned(),
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                )
                .rows
                .iter()
                .flat_map(|row| row.glyphs.iter().map(|g| g.font_ascent))
                .collect()
        })
    }

    /// The language's own face must lead the family, or its text is drawn from
    /// two fonts at once and the glyphs stop lining up.
    #[test]
    fn active_language_face_leads_the_family() {
        assert_eq!(font_order(Lang::Ja)[0], IBM_PLEX_JP_NAME);
        assert_eq!(font_order(Lang::Zh)[0], IBM_PLEX_SC_NAME);
        assert_eq!(font_order(Lang::En)[0], IBM_PLEX_NAME);
    }

    /// Every glyph on a line reports the same ascent only while one face covers it.
    #[test]
    fn glyphs_share_one_ascent() {
        for lang in [Lang::Ja, Lang::En, Lang::Zh] {
            let ascents = glyph_ascents(lang, "AudioFrame 123");
            let first = ascents[0];
            assert!(ascents.iter().all(|a| *a == first), "{lang:?}: {ascents:?}");
        }
    }
}
