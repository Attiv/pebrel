use super::*;
use crate::gpui_shell::settings_fixture::{SettingsBytesGuard, lock_theme_studio};

#[gpui::test]
fn command_ai_save_switch_and_clear_use_rendered_controls(cx: &mut gpui::TestAppContext) {
    let _lock = lock_theme_studio();
    let _settings = SettingsBytesGuard::capture();
    nebula_settings::persist_keys(&CommandAi::default().updates().unwrap()).unwrap();
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.set_reduce_motion(true);
        cx.set_global(crate::gpui_shell::config::Settings::load(ThemeName::Nord));
    });
    let mut pane_out = None;
    let (_, window) = cx.add_window_view(|window, cx| {
        let pane = cx.new(|cx| SettingsPane::new(window, cx));
        pane.update(cx, |pane, _| {
            pane.active_section = 3;
            pane.provider_store.providers = vec![AiProvider::preset(ProviderKind::Ollama, "local")];
        });
        pane_out = Some(pane.clone());
        gpui_component::Root::new(pane, window, cx)
    });
    let pane = pane_out.unwrap();
    window.simulate_resize(gpui::size(px(1400.0), px(1500.0)));
    window.update(|window, cx| window.draw(cx).clear(cx));
    pane.read_with(window, |pane, _| {
        assert!(pane.provider_ui.commands.as_ref().unwrap().provider_id.is_empty());
        assert!(!pane.runtime.command_ai.prediction_enabled);
    });
    window.update(|window, cx| {
        pane.update(cx, |pane, cx| {
            let ui = pane.provider_ui.commands.as_mut().unwrap();
            ui.provider_id = "local".into();
            ui.model.update(cx, |input, cx| input.set_value("fixture-model", window, cx));
            cx.notify();
        });
        window.draw(cx).clear(cx);
    });
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
    let save = window.debug_bounds("command-ai-save").expect("visible command AI Save control");
    window.simulate_click(save.center(), gpui::Modifiers::default());
    window.run_until_parked();
    let expected = CommandAi {
        provider_id: "local".into(),
        model: "fixture-model".into(),
        prediction_enabled: false,
        ..Default::default()
    };
    assert_eq!(nebula_settings::RuntimeSettings::load().command_ai, expected);
    pane.read_with(window, |pane, cx| {
        assert_eq!(pane.runtime.command_ai, expected);
        assert_eq!(cx.global::<crate::gpui_shell::config::Settings>().command_ai, expected);
        assert!(pane.provider_ui.commands.as_ref().unwrap().saved);
    });
    for (selector, keys) in
        [("command-ai-tab-source", "down down enter"), ("command-ai-right-source", "down enter")]
    {
        window.update(|window, cx| window.draw(cx).clear(cx));
        let trigger = window.debug_bounds(selector).expect("visible acceptance-source dropdown");
        window.simulate_click(trigger.center(), gpui::Modifiers::default());
        window.run_until_parked();
        window.simulate_keystrokes(keys);
        window.run_until_parked();
    }
    window.update(|window, cx| window.draw(cx).clear(cx));
    let save = window.debug_bounds("command-ai-save").unwrap();
    window.simulate_click(save.center(), gpui::Modifiers::default());
    window.run_until_parked();
    let expected = CommandAi {
        tab_source: CommandAiSource::History,
        right_source: CommandAiSource::Ai,
        ..expected
    };
    assert_eq!(nebula_settings::RuntimeSettings::load().command_ai, expected);
    pane.read_with(window, |pane, cx| {
        assert_eq!(pane.runtime.command_ai, expected);
        assert_eq!(cx.global::<crate::gpui_shell::config::Settings>().command_ai, expected);
    });
    for enabled in [true, false] {
        window.update(|window, cx| window.draw(cx).clear(cx));
        let switch = window
            .debug_bounds("nebula-switch-command-ai-prediction")
            .expect("visible command prediction switch");
        window.simulate_click(switch.center(), gpui::Modifiers::default());
        window.run_until_parked();
        assert_eq!(nebula_settings::RuntimeSettings::load().command_ai.prediction_enabled, enabled);
    }
    window.update(|window, cx| {
        pane.update(cx, |pane, cx| {
            pane.provider_store.providers.clear();
            let ui = pane.provider_ui.commands.as_mut().unwrap();
            ui.provider_id.clear();
            ui.model.update(cx, |input, cx| input.set_value("", window, cx));
            cx.notify();
        });
        window.draw(cx).clear(cx);
    });
    window.run_until_parked();
    window.update(|window, cx| window.draw(cx).clear(cx));
    let save = window.debug_bounds("command-ai-save").expect("clear selection remains saveable");
    window.simulate_click(save.center(), gpui::Modifiers::default());
    window.run_until_parked();
    assert_eq!(
        nebula_settings::RuntimeSettings::load().command_ai,
        CommandAi {
            tab_source: CommandAiSource::History,
            right_source: CommandAiSource::Ai,
            ..CommandAi::default()
        }
    );
}
