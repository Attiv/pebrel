//! Ignored native GPU capture of the real TerminalView -> TerminalElement path.
//! Run with isolated PEBREL_LABEL_QA_DIR and PEBREL_CONFIG_DIR under workspace tmp.

use std::{path::PathBuf, sync::Arc, time::Duration};

use gpui::{
    AppContext as _, Context, Entity, HeadlessAppContext, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px, size,
};
use nebula_settings::{RawSettings, RuntimeSettings, ThemeName};
use nebula_terminal::{
    index::{Column, Line, Point, Side},
    selection::{Selection, SelectionType},
};

use super::super::{LabelPalette, rgb_from_rgba};
use super::identity_tests::feed;
use crate::gpui_shell::{
    assets::NebulaAssets,
    config::Settings,
    terminal::view::{TerminalLaunch, TerminalView},
};

struct BadgeQaSurface(Vec<Entity<TerminalView>>);

impl Render for BadgeQaSurface {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = &cx.global::<Settings>().palette;
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.background)
            .text_color(palette.foreground)
            .children(self.0.iter().zip(["Logs", "Codex", "Claude Code"]).map(|(view, title)| {
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(div().h(px(24.0)).px(px(12.0)).child(title))
                    .child(div().flex_1().min_h_0().child(view.clone()))
            }))
    }
}

fn draw(cx: &mut HeadlessAppContext, window: gpui::AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[test]
#[ignore = "macOS Metal QA; requires fresh isolated PEBREL_LABEL_QA_DIR/config under tmp"]
fn native_headless_terminal_badges_capture_dark_light_and_selection() {
    let output = PathBuf::from(std::env::var_os("PEBREL_LABEL_QA_DIR").expect("QA output"));
    let workspace_tmp = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("tmp");
    assert!(output.is_absolute() && output.starts_with(workspace_tmp));
    assert_eq!(
        std::env::var_os("PEBREL_CONFIG_DIR").map(PathBuf::from),
        Some(output.join("config")),
        "never use the live application's configuration"
    );
    assert!(!output.join("dark-enabled.png").exists(), "use a fresh QA directory");
    std::fs::create_dir_all(output.join("config")).unwrap();

    let platform = gpui_platform::current_platform(true);
    let mut cx = HeadlessAppContext::with_platform(
        platform.text_system(),
        Arc::new(NebulaAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        crate::gpui_shell::register_bundled_fonts(cx);
        gpui_component::init(cx);
        crate::gpui_shell::scientific_render::init(cx);
        let mut runtime = RuntimeSettings::from_raw(&RawSettings::from_text(""));
        runtime.font_size_px = Some(17.0);
        runtime.cursor_blink = Some(false);
        runtime.animations = false;
        runtime.ghost = false;
        cx.set_global(Settings::load_with_runtime(ThemeName::Nord, runtime));
    });
    let mut terminals = Vec::new();
    let mut receivers = Vec::new();
    let window = cx.open_window(size(px(900.0), px(570.0)), |window, cx| {
        for (index, provider) in [None, Some("codex"), Some("claude")].into_iter().enumerate() {
            let pane = 91_000 + index as u64;
            let view = cx.new(|cx| TerminalView::new(
                pane,
                (80, 24),
                TerminalLaunch::Local {
                    cwd: Some(output.clone()),
                    shell: Some(nebula_terminal::tty::Shell::new(
                        "pebrel-test-missing-shell-executable".into(), vec![],
                    )),
                    shell_name: None,
                },
                window,
                cx,
            ));
            receivers.push(view.update(cx, |view, cx| {
                let receiver = view.install_completion_test_session();
                if let Some(provider) = provider {
                    let payload = format!(
                        "nebula-hook/1 source={provider} pane={pane} codex_hooks=full\n{{\"hook_event_name\":\"SessionStart\",\"session_id\":\"badge-qa-{provider}\"}}"
                    );
                    let hook = crate::ai_hook::parse_remote_envelope(payload.as_bytes(), Some(pane)).unwrap();
                    assert!(view.handle_ai_hook(&hook, cx));
                    assert_eq!(view.runtime_agent().unwrap().kind, provider);
                } else {
                    assert!(view.runtime_agent().is_none());
                }
                receiver
            }));
            terminals.push(view);
        }
        let surface = cx.new(|_| BadgeQaSurface(terminals.clone()));
        cx.new(|cx| gpui_component::Root::new(surface, window, cx))
    }).unwrap();
    draw(&mut cx, window.into());
    cx.advance_clock(Duration::from_millis(500));
    draw(&mut cx, window.into());

    let fixtures = [
        "[INFO] Listening on 127.0.0.1:3000\r\n[WARN] Retrying optional service\r\n[ERROR] Background job failed\r\nINFO is a word, not a structured label\r\n\x1b[41m[INFO] Explicit ANSI background stays native\x1b[0m",
        "• Ran cargo test --lib\r\n• Explored src/gpui_shell/terminal\r\n• Failed (exit 1) cargo fmt --check\r\n  • Ran quoted output stays native\r\nordinary body text remains unchanged",
        "⏺ Read(src/main.rs)\r\n⏺ Bash(cargo test)\r\n⏺ Update(src/main.rs)\r\n  ⏺ Read(quoted output stays native)\r\nordinary body text remains unchanged",
    ];
    cx.update(|cx| {
        for (view, fixture) in terminals.iter().zip(fixtures) {
            view.update(cx, |view, cx| {
                feed(view, format!("\x1b[2J\x1b[H{fixture}\r\n\x1b[?25l").as_bytes());
                cx.notify();
            });
        }
    });
    let mut report = Vec::new();
    for (name, theme) in [("dark", ThemeName::Nord), ("light", ThemeName::CatppuccinLatte)] {
        cx.update(|cx| {
            let mut runtime = RuntimeSettings::from_raw(&RawSettings::from_text(""));
            runtime.font_size_px = Some(17.0);
            runtime.cursor_blink = Some(false);
            runtime.animations = false;
            runtime.ghost = false;
            cx.set_global(Settings::load_with_runtime(theme, runtime));
            for view in &terminals {
                view.update(cx, |view, cx| view.apply_settings(cx));
            }
        });
        let mut disabled = None;
        for enabled in [false, true] {
            cx.update(|cx| cx.global_mut::<Settings>().terminal_label_badges = enabled);
            draw(&mut cx, window.into());
            let image = cx.capture_screenshot(window.into()).expect("native Metal screenshot");
            image
                .save(
                    output.join(format!(
                        "{name}-{}.png",
                        if enabled { "enabled" } else { "disabled" }
                    )),
                )
                .unwrap();
            if enabled {
                assert_ne!(disabled.as_ref().unwrap(), &image, "the real paint path must change");
                let neutral = cx.update(|cx| {
                    *rgb_from_rgba(
                        LabelPalette::new(&cx.global::<Settings>().palette)
                            .colors(super::super::LabelKind::Info)
                            .background,
                    )
                });
                let count = image
                    .pixels()
                    .filter(|pixel| pixel.0[..3] == [neutral.r, neutral.g, neutral.b])
                    .count();
                assert!(count > 100, "capsule backdrops must be visible in the real screenshot");
                report.push(serde_json::json!({"theme": name, "dimensions": image.dimensions(), "neutral_badge_pixels": count}));
            } else {
                disabled = Some(image);
            }
        }
        cx.update(|cx| {
            terminals[0].update(cx, |view, cx| {
                let mut term = view.session.as_ref().unwrap().term.lock();
                let mut selection = Selection::new(
                    SelectionType::Simple,
                    Point::new(Line(0), Column(1)),
                    Side::Left,
                );
                selection.update(Point::new(Line(0), Column(5)), Side::Left);
                term.selection = Some(selection);
                assert_eq!(term.selection_to_string().as_deref(), Some("INFO"));
                drop(term);
                cx.notify();
            })
        });
        draw(&mut cx, window.into());
        cx.capture_screenshot(window.into())
            .unwrap()
            .save(output.join(format!("{name}-selection.png")))
            .unwrap();
        cx.update(|cx| {
            terminals[0].update(cx, |view, cx| {
                view.session.as_ref().unwrap().term.lock().selection = None;
                cx.notify();
            })
        });
    }
    std::fs::write(output.join("capture-report.json"), serde_json::to_vec_pretty(&report).unwrap())
        .unwrap();
    drop(receivers);
}
