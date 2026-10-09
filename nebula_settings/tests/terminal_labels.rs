use nebula_settings::{RawSettings, RuntimeSettings, apply_updates};

#[test]
fn terminal_label_badges_default_off_for_old_or_invalid_preferences() {
    for text in ["", "terminal_label_badges=", "terminal_label_badges=invalid"] {
        assert!(!RuntimeSettings::from_raw(&RawSettings::from_text(text)).terminal_label_badges);
    }
}

#[test]
fn terminal_label_badges_round_trip_without_changing_hook_authorization() {
    let original = "# keep\nai_hooks_codex=0\nai_hooks_claude=1\ncustom=untouched\n";
    for (value, expected) in [("1", true), ("on", true), ("0", false), ("off", false)] {
        let saved = apply_updates(original, &[("terminal_label_badges", value.into())]);
        let raw = RawSettings::from_text(&saved);
        assert_eq!(RuntimeSettings::from_raw(&raw).terminal_label_badges, expected);
        assert_eq!(raw.bool_on("ai_hooks_codex"), Some(false));
        assert_eq!(raw.bool_on("ai_hooks_claude"), Some(true));
        assert!(saved.contains("custom=untouched"));
    }
}
