//! Opt-in real zsh/PTY/window proof; no user configuration or paid provider is used.
use super::*;
use gpui::{EntityInputHandler as _, WindowBounds, WindowOptions, size};
use std::{path::PathBuf, sync::Mutex, time::Duration};

mod lifecycle;
mod prediction;

struct Surface(Entity<TerminalView>);
impl Render for Surface {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(cx.theme().background).child(self.0.clone())
    }
}

async fn wait_for(
    cx: &mut gpui::AsyncApp,
    window: gpui::AnyWindowHandle,
    view: &Entity<TerminalView>,
    check: impl Fn(&TerminalView, &App) -> bool,
) -> Result<(), String> {
    for _ in 0..500 {
        let ready = cx
            .update_window(window, |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
                check(view.read(cx), cx)
            })
            .map_err(|e| e.to_string())?;
        if ready {
            return Ok(());
        }
        cx.background_executor().timer(Duration::from_millis(20)).await;
    }
    Err(cx
        .update_window(window, |_, _, cx| {
            let view = view.read(cx);
            format!(
                "AI native timeout: editor={:?}, query={:?}, status={:?}, error={:?}, exited={:?}, active={}, env={:?}, agent={:?}, prompt={}",
                view.completion_editor,
                view.ai_search.as_ref().map(|s| s.input.read(cx).value()),
                view.ai_search.as_ref().and_then(|s| s.status),
                view.error,
                view.exited,
                view.cursor_window_active,
                view.suggest.suggest_env,
                view.runtime_agent(),
                view.session.as_ref().is_some_and(|s| s.term.lock().nebula_prompt_active())
            )
        })
        .map_err(|e| e.to_string())?)
}

async fn probe(
    cx: &mut gpui::AsyncApp,
    window: gpui::AnyWindowHandle,
    view: &Entity<TerminalView>,
    line: &str,
    cursor: usize,
) -> Result<(), String> {
    cx.update_window(window, |_, _, cx| {
        view.update(cx, |view, _| {
            view.completion_editor.clear_report_for_test();
            view.write_bytes(view.completion_editor_query_bytes());
        })
    })
    .map_err(|e| e.to_string())?;
    wait_for(cx, window, view, |view, _| {
        view.completion_editor.reported_line_for_test() == Some(line)
            && view.completion_editor.reported_cursor_for_test() == Some(cursor)
    })
    .await
}

fn key(window: &mut Window, cx: &mut App, key: &str) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    window.dispatch_event(
        gpui::PlatformInput::KeyDown(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        }),
        cx,
    );
    window.dispatch_event(gpui::PlatformInput::KeyUp(gpui::KeyUpEvent { keystroke }), cx);
}

#[test]
#[ignore = "requires native desktop, zsh and fresh isolated PEBREL_AI_SEARCH_QA_DIR/config + ZDOTDIR"]
fn native_zsh_query_cancel_and_tab_never_execute() {
    let output = PathBuf::from(std::env::var_os("PEBREL_AI_SEARCH_QA_DIR").expect("QA directory"));
    assert!(output.is_absolute());
    assert_eq!(
        std::env::var_os("PEBREL_CONFIG_DIR").map(PathBuf::from),
        Some(output.join("config"))
    );
    assert_eq!(std::env::var_os("ZDOTDIR").map(PathBuf::from), Some(output.join("shell")));
    assert!(!output.join("result.json").exists(), "use a fresh QA directory");
    std::fs::create_dir_all(output.join("shell")).unwrap();
    std::fs::write(
        output.join("shell/.zshrc"),
        "PROMPT='QA> '\nRPROMPT=''\nHISTFILE=''\nbindkey -e\n",
    )
    .unwrap();
    let outcome = Arc::new(Mutex::new(None));
    let after = outcome.clone();
    gpui_platform::application().with_assets(crate::gpui_shell::assets::NebulaAssets).run(move |cx| {
        crate::gpui_shell::register_bundled_fonts(cx);
        gpui_component::init(cx);
        crate::gpui_shell::terminal::init(cx);
        crate::gpui_shell::scientific_render::init(cx);
        let settings = nebula_settings::CommandAi { provider_id: "native-loopback".into(), model: "fixture".into(), prediction_enabled: false };
        nebula_settings::persist_keys(&settings.updates().unwrap()).unwrap();
        cx.set_global(Settings::load(nebula_settings::ThemeName::Nord));
        crate::gpui_shell::theme::apply_chrome_theme(cx);
        cx.activate(true);
        let mut terminal = None;
        let mut surface_out = None;
        let cwd = output.clone();
        let window = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(80.0), px(80.0)), size(px(720.0), px(440.0))))),
            ..Default::default()
        }, |window, cx| {
            window.set_window_title("Pebrel AI Search Native QA");
            let view = cx.new(|cx| TerminalView::new(9101, (80, 24), TerminalLaunch::Local {
                cwd: Some(cwd), shell: Some(nebula_terminal::tty::Shell::new("/bin/zsh".into(), vec!["-i".into()])), shell_name: None,
            }, window, cx));
            window.focus(&view.read(cx).focus_handle.clone(), cx);
            window.activate_window();
            terminal = Some(view.clone());
            let surface = cx.new(|_| Surface(view));
            surface_out = Some(surface.clone());
            cx.new(|cx| gpui_component::Root::new(surface, window, cx))
        }).unwrap();
        let terminal = terminal.unwrap();
        let surface = surface_out.unwrap();
        cx.spawn(async move |cx| {
            let run = async {
                cx.update_window(window.into(), |_, window, cx| {
                    cx.activate(true);
                    window.activate_window();
                }).map_err(|e| e.to_string())?;
                wait_for(cx, window.into(), &terminal, |view, cx| view.ai_search_identity().is_some()).await?;
                let original = "printf '原稿😀保持'";
                let caret = "printf '原稿😀".len();
                cx.update_window(window.into(), |_, window, cx| {
                    terminal.update(cx, |view, cx| view.replace_text_in_range(None, original, window, cx));
                    for _ in 0..3 { key(window, cx, "left"); }
                }).map_err(|e| e.to_string())?;
                probe(cx, window.into(), &terminal, original, caret).await?;
                cx.update_window(window.into(), |_, window, cx| terminal.update(cx, |view, cx| view.toggle_ai_search(window, cx))).map_err(|e| e.to_string())?;
                wait_for(cx, window.into(), &terminal, |view, cx| view.ai_search.as_ref().is_some_and(|s| s.original.is_some() && s.seed.is_none())).await?;
                cx.update_window(window.into(), |_, window, cx| {
                    terminal.update(cx, |view, cx| {
                        let search = view.ai_search.as_ref().unwrap();
                        assert_eq!(search.input.read(cx).value().as_ref(), original);
                        assert_eq!(search.original.as_ref().unwrap().cursor, caret);
                        search.input.update(cx, |input, cx| input.replace_and_mark_text_in_range(Some(0..original.encode_utf16().count()), "查找进程", None, window, cx));
                    });
                    key(window, cx, "enter");
                }).map_err(|e| e.to_string())?;
                wait_for(cx, window.into(), &terminal, |view, cx| view.ai_search_job.is_none()).await?;
                // The native Shell still holds the original while its child input owns preedit.
                probe(cx, window.into(), &terminal, original, caret).await?;
                cx.update_window(window.into(), |_, window, cx| {
                    terminal.update(cx, |view, cx| view.ai_search.as_ref().unwrap().input.update(cx, |input, cx| input.unmark_text(window, cx)));
                    key(window, cx, "escape");
                }).map_err(|e| e.to_string())?;
                wait_for(cx, window.into(), &terminal, |view, cx| view.ai_search.is_none()).await?;
                probe(cx, window.into(), &terminal, original, caret).await?;
                let command = "printf 'executed' > ai-search-executed";
                let response = serde_json::json!({"choices":[{"finish_reason":"stop","message":{"content":serde_json::json!({"command":command,"explanation":"只在单独按 Enter 后写入测试标记"}).to_string()}}]});
                let (url, server) = crate::ai_inference::test_support::reply(response, "200 OK", || {});
                let mut provider = crate::ai_providers::AiProvider::preset(crate::ai_providers::ProviderKind::Ollama, "native-loopback");
                provider.base_url = url;
                crate::ai_providers::save(&crate::ai_providers::ProviderStore { providers: vec![provider], ..Default::default() }).map_err(|e| e.to_string())?;
                cx.update_window(window.into(), |_, window, cx| terminal.update(cx, |view, cx| view.toggle_ai_search(window, cx))).map_err(|e| e.to_string())?;
                wait_for(cx, window.into(), &terminal, |view, cx| view.ai_search.as_ref().is_some_and(|s| s.original.is_some() && s.seed.is_none())).await?;
                let query = "写入无害测试标记";
                cx.update_window(window.into(), |_, window, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(query.into()));
                    key(window, cx, if cfg!(target_os = "macos") { "cmd-a" } else { "ctrl-a" });
                    key(window, cx, if cfg!(target_os = "macos") { "cmd-v" } else { "ctrl-v" });
                }).map_err(|e| e.to_string())?;
                wait_for(cx, window.into(), &terminal, |view, cx| view.ai_search.as_ref().is_some_and(|s| s.input.read(cx).value().as_ref() == query)).await?;
                probe(cx, window.into(), &terminal, original, caret).await?;
                cx.update_window(window.into(), |_, window, cx| { key(window, cx, "enter"); key(window, cx, "enter"); }).map_err(|e| e.to_string())?;
                wait_for(cx, window.into(), &terminal, |view, cx| view.ai_search.as_ref().is_some_and(|s| s.result.is_some())).await?;
                let captured = server.join().map_err(|_| "mock server panicked")?;
                let content: serde_json::Value = serde_json::from_str(captured.body["messages"][1]["content"].as_str().unwrap()).unwrap();
                assert_eq!(content, serde_json::json!({"input":query,"os":std::env::consts::OS,"shell":"zsh"}));
                probe(cx, window.into(), &terminal, original, caret).await?;
                assert!(!output.join("ai-search-executed").exists());
                std::fs::write(output.join("review-ready"), b"result ready").map_err(|e| e.to_string())?;
                if std::env::var_os("PEBREL_AI_SEARCH_QA_HOLD").is_some() {
                    for _ in 0..6000 {
                        if output.join("continue").exists() { break; }
                        cx.background_executor().timer(Duration::from_millis(50)).await;
                    }
                    if !output.join("continue").exists() { return Err("visual review hold timed out".into()); }
                }
                cx.update_window(window.into(), |_, window, cx| key(window, cx, "tab")).map_err(|e| e.to_string())?;
                wait_for(cx, window.into(), &terminal, |view, cx| view.ai_search.is_none()).await?;
                probe(cx, window.into(), &terminal, command, command.len()).await?;
                assert!(!output.join("ai-search-executed").exists(), "Tab must not execute");
                cx.update_window(window.into(), |_, window, cx| key(window, cx, "enter")).map_err(|e| e.to_string())?;
                wait_for(cx, window.into(), &terminal, |_, _| std::fs::read(output.join("ai-search-executed")).is_ok_and(|b| b == b"executed")).await?;
                std::fs::write(output.join("native-edit-proof.json"), serde_json::to_vec_pretty(&serde_json::json!({"original":original,"cursor_utf8":caret,"query":query,"adopted":command,"tab_did_not_execute":true,"separate_enter_executed":true})).unwrap()).map_err(|e| e.to_string())?;
                let lifecycle = lifecycle::run(cx, window.into(), &surface, &terminal).await?;
                let prediction = prediction::run(cx, window.into(), &terminal, &output).await?;
                Ok::<_, String>(serde_json::json!({"shell":"zsh","original":original,"original_cursor_utf8":caret,"query":query,"adopted":command,"executed_only_after_separate_enter":true,"ime":"marked-text API, not OS candidate-window proof","delayed_http_lifecycle":lifecycle,"prediction":prediction}))
            }.await;
            std::fs::write(output.join("result.json"), serde_json::to_vec_pretty(&run).unwrap()).unwrap();
            *outcome.lock().unwrap() = Some(run);
            let _ = cx.update_window(window.into(), |_, window, cx| { terminal.update(cx, |view, _| view.shutdown()); window.remove_window(); });
            cx.update(|cx| cx.quit());
        }).detach();
    });
    let result = after.lock().unwrap().take();
    assert!(result.as_ref().is_some_and(Result::is_ok), "{result:?}");
}
