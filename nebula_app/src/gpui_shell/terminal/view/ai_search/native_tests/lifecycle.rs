//! Delayed real HTTP against production GPUI job ownership, sharing one QA window.
use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::AtomicUsize;

pub(super) struct Server {
    pub(super) url: String,
    pub(super) received: Arc<AtomicUsize>,
    pub(super) release: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Vec<serde_json::Value>>>,
}

impl Server {
    fn new(requests: usize) -> Self {
        Self::respond((0..requests).map(|i| serde_json::json!({"command":if i == 0 {"echo stale"} else {"echo latest"},"explanation":"fixture"})).collect())
    }

    pub(super) fn respond(responses: Vec<serde_json::Value>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let received = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(AtomicBool::new(false));
        let (seen, gate) = (received.clone(), release.clone());
        let thread = std::thread::spawn(move || {
            let mut captured = Vec::new();
            for (index, response) in responses.into_iter().enumerate() {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    if let Ok((stream, _)) = listener.accept() {
                        break stream;
                    }
                    assert!(std::time::Instant::now() < deadline, "expected HTTP request {index}");
                    std::thread::sleep(Duration::from_millis(1));
                };
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                captured.push(serde_json::from_slice(&body).unwrap());
                seen.store(index + 1, Ordering::Release);
                while !gate.load(Ordering::Acquire) {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "HTTP response gate was not released"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                let body = serde_json::json!({"choices":[{"finish_reason":"stop","message":{"content":response.to_string()}}]}).to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
            captured
        });
        Self { url, received, release, thread: Some(thread) }
    }

    pub(super) fn finish(mut self) -> Vec<serde_json::Value> {
        self.release.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.release.store(true, Ordering::Release);
    }
}

fn enter(
    view: &mut TerminalView,
    query: &str,
    window: &mut Window,
    cx: &mut Context<TerminalView>,
) {
    view.toggle_ai_search(window, cx);
    view.handle_completion_editor_report("pebrel_editor", "zsh:fixture\nutf8\n5\ndraft 中文", cx);
    view.ai_search_chrome(window, cx);
    view.ai_search
        .as_ref()
        .unwrap()
        .input
        .update(cx, |input, cx| input.replace_all(query, window, cx));
}

pub(super) async fn run(
    cx: &mut gpui::AsyncApp,
    window: gpui::AnyWindowHandle,
    surface: &Entity<Surface>,
    native: &Entity<TerminalView>,
) -> Result<Vec<&'static str>, String> {
    let mut passed = Vec::new();
    for case in ["query", "reopen", "focus", "config", "cwd", "destroy", "latest"] {
        crate::gpui_shell::try_write_stderr(format_args!("AI delayed HTTP case: {case}"));
        let server = Server::new(if case == "latest" { 2 } else { 1 });
        let mut provider = crate::ai_providers::AiProvider::preset(
            crate::ai_providers::ProviderKind::Ollama,
            "native-loopback",
        );
        provider.base_url = server.url.clone();
        crate::ai_providers::save(&crate::ai_providers::ProviderStore {
            providers: vec![provider],
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        let mut view = Some(
            cx.update_window(window, |_, window, cx| {
                let view = cx.new(|cx| {
                    TerminalView::new(
                        9200,
                        (80, 24),
                        TerminalLaunch::Local {
                            cwd: None,
                            shell: Some(nebula_terminal::tty::Shell::new(
                                "pebrel-test-missing-shell-executable".into(),
                                vec![],
                            )),
                            shell_name: None,
                        },
                        window,
                        cx,
                    )
                });
                view.update(cx, |view, cx| {
                    let _receiver = view.install_completion_test_session();
                    view.suggest.suggest_env = crate::display::SuggestEnv::Local;
                    view.suggest.completion_shell_report(
                        crate::completion_context::SHELL_VAR,
                        "zsh:fixture",
                    );
                    view.completion_editor.advertise("zsh:fixture");
                    view.session.as_ref().unwrap().term.lock().nebula_add_prompt_mark();
                    view.cursor_window_active = true;
                    enter(view, "first", window, cx);
                });
                surface.update(cx, |surface, cx| {
                    surface.0 = view.clone();
                    cx.notify();
                });
                window.draw(cx).clear(cx);
                view
            })
            .map_err(|e| e.to_string())?,
        );
        let probe = view.as_ref().unwrap().downgrade();
        cx.update_window(window, |_, _, cx| {
            view.as_ref().unwrap().update(cx, |view, cx| {
                view.generate_ai_search(cx);
                view.generate_ai_search(cx);
                assert!(!view.ai_search.as_ref().unwrap().queued, "pending Enter must deduplicate");
            })
        })
        .map_err(|e| e.to_string())?;
        wait_for(cx, window, view.as_ref().unwrap(), |_, _| {
            server.received.load(Ordering::Acquire) == 1
        })
        .await?;
        let (before, cancelled) = cx
            .update_window(window, |_, _, cx| {
                let job = view.as_ref().unwrap().read(cx).ai_search_job.as_ref().unwrap();
                (job.revision, job.cancelled.clone())
            })
            .map_err(|e| e.to_string())?;
        cx.update_window(window, |_, window, cx| {
            if case == "destroy" {
                surface.update(cx, |surface, cx| {
                    surface.0 = native.clone();
                    cx.notify();
                });
                window.draw(cx).clear(cx);
                view.take();
                return;
            }
            view.as_ref().unwrap().update(cx, |view, cx| match case {
                "query" | "latest" => {
                    let input = view.ai_search.as_ref().unwrap().input.clone();
                    input.update(cx, |input, cx| input.replace_all("second", window, cx));
                },
                "reopen" => {
                    view.close_ai_search(cx);
                    enter(view, "reopened", window, cx);
                },
                "focus" => {
                    let other = cx.focus_handle();
                    window.focus(&other, cx);
                },
                "config" => {
                    ai_commands::invalidate_config();
                    view.validate_ai_search(cx);
                },
                "cwd" => {
                    view.cwd.push_str("/changed");
                    view.validate_ai_search(cx);
                },
                _ => unreachable!(),
            });
        })
        .map_err(|e| e.to_string())?;
        // Flush query Change/Blur before delivering the first response.
        if let Some(view) = &view {
            wait_for(cx, window, view, |_, _| cancelled.load(Ordering::Acquire)).await?;
        }
        if case == "latest" {
            cx.update_window(window, |_, window, cx| {
                view.as_ref().unwrap().update(cx, |view, cx| {
                    view.generate_ai_search(cx);
                    view.ai_search
                        .as_ref()
                        .unwrap()
                        .input
                        .update(cx, |input, cx| input.replace_all("latest", window, cx));
                })
            })
            .map_err(|e| e.to_string())?;
            wait_for(cx, window, view.as_ref().unwrap(), |view, _| {
                !view.ai_search.as_ref().unwrap().queued
            })
            .await?;
            cx.update_window(window, |_, _, cx| {
                view.as_ref().unwrap().update(cx, |view, cx| {
                    view.generate_ai_search(cx);
                    view.generate_ai_search(cx);
                    assert!(view.ai_search.as_ref().unwrap().queued);
                    assert_eq!(
                        view.ai_search.as_ref().unwrap().status,
                        Some(Message::AiCommandSearchLoading)
                    );
                    assert_eq!(
                        view.ai_search_job.as_ref().unwrap().revision,
                        before,
                        "keep the one actual request until it finishes"
                    );
                })
            })
            .map_err(|e| e.to_string())?;
            assert_eq!(server.received.load(Ordering::Acquire), 1);
        }
        server.release.store(true, Ordering::Release);
        if let Some(view) = &view {
            wait_for(cx, window, view, |view, _| view.ai_search_job.is_none()).await?;
            cx.update_window(window, |_, _, cx| {
                let search = &view.read(cx).ai_search;
                if case == "latest" {
                    assert_eq!(
                        search.as_ref().unwrap().result.as_ref().unwrap().command,
                        "echo latest"
                    );
                } else {
                    assert!(
                        search.as_ref().is_none_or(|search| search.result.is_none()),
                        "{case}: stale result was published"
                    );
                }
            })
            .map_err(|e| e.to_string())?;
        } else {
            cx.update(|cx| assert!(probe.upgrade().is_none(), "destroyed owner must not survive"));
        }
        let captured = server.finish();
        let queries: Vec<String> = captured
            .iter()
            .map(|body| {
                let content: serde_json::Value =
                    serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
                assert_eq!(content.as_object().unwrap().len(), 3);
                content["input"].as_str().unwrap().to_owned()
            })
            .collect();
        assert_eq!(queries, if case == "latest" { vec!["first", "latest"] } else { vec!["first"] });
        cx.update_window(window, |_, _, cx| {
            surface.update(cx, |surface, cx| {
                surface.0 = native.clone();
                cx.notify();
            })
        })
        .map_err(|e| e.to_string())?;
        passed.push(case);
        crate::gpui_shell::try_write_stderr(format_args!("AI delayed HTTP passed: {case}"));
    }
    Ok(passed)
}
