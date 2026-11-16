use super::*;

#[test]
fn badge_colors_preserve_ansi_accents_and_contrast_on_light_dark_and_custom_themes() {
    let dark = Palette::default();
    let mut light = dark.clone();
    light.background = gpui::rgb(0xf8f9fb);
    light.foreground = gpui::rgb(0x21252b);
    let mut custom = dark.clone();
    custom.ansi[1] = gpui::rgb(0x35121c);
    custom.ansi[3] = gpui::rgb(0xcfd3dd);
    for theme in [dark, light, custom] {
        let palette = LabelPalette::new(&theme);
        for kind in [
            LabelKind::Info,
            LabelKind::Tool,
            LabelKind::Warn,
            LabelKind::Error,
            LabelKind::Failure,
        ] {
            let colors = palette.colors(kind);
            assert!(
                rgb_from_rgba(colors.foreground).contrast(*rgb_from_rgba(colors.background)) >= 4.5
            );
            assert_eq!(colors.background.a, 1.0, "wallpaper cannot dilute badge contrast");
        }
        assert_eq!(palette.colors(LabelKind::Warn).background, theme.ansi[3]);
        assert_eq!(palette.colors(LabelKind::Error).background, theme.ansi[1]);
        assert_eq!(palette.colors(LabelKind::Info).foreground, gpui::rgb(0xffffff));
    }
}
