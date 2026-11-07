//! Same real GPUI/zsh fixture; bounded loopback responses, never a paid provider.
use super::*;
use crate::display::CompletionStyle;
use lifecycle::Server;

fn configure(server: &Server) -> Result<(), String> {
    let mut provider = crate::ai_providers::AiProvider::preset(
        crate::ai_providers::ProviderKind::Ollama,
        "native-loopback",
    );
    provider.base_url = server.url.clone();
    crate::ai_providers::save(&crate::ai_providers::ProviderStore {
        providers: vec![provider],
        ..Default::default()
    })
    .map_err(|e| e.to_string())
}

async fn draft(
    cx: &mut gpui::AsyncApp,
    window: gpui::AnyWindowHandle,
    view: &Entity<TerminalView>,
    line: &str,
) -> Result<(), String> {
    wait_for(cx, window, view, |view, _| view.ai_search_identity().is_some()).await?;
    cx.update_window(window, |_, window, cx| {
        window.focus(&view.read(cx).focus_handle.clone(), cx);
        key(window, cx, "ctrl-u");
        view.update(cx, |view, cx| view.replace_text_in_range(None, line, window, cx));
    })
    .map_err(|e| e.to_string())?;
    probe(cx, window, view, line, line.len()).await
}

async fn search(
    cx: &mut gpui::AsyncApp,
    window: gpui::AnyWindowHandle,
    view: &Entity<TerminalView>,
) -> Result<(), String> {
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.toggle_ai_search(window, cx))
    })
    .map_err(|e| e.to_string())?;
    wait_for(cx, window, view, |view, _| {
        view.ai_search.as_ref().is_some_and(|s| s.original.is_some() && s.seed.is_none())
    })
    .await?;
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.ai_search
                .as_ref()
                .unwrap()
                .input
                .update(cx, |input, cx| input.replace_all("manual request", window, cx));
        })
    })
    .map_err(|e| e.to_string())?;
    // Give the actual Input Change subscription its own event turn before Enter.
    wait_for(cx, window, view, |view, cx| {
        view.ai_search
            .as_ref()
            .is_some_and(|s| s.input.read(cx).value().as_ref() == "manual request")
    })
    .await?;
    cx.update_window(window, |_, window, cx| key(window, cx, "enter")).map_err(|e| e.to_string())
}

fn inputs(captured: Vec<serde_json::Value>) -> Vec<String> {
    captured
        .iter()
        .map(|body| {
            let input: serde_json::Value =
                serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
            assert_eq!(input.as_object().unwrap().len(), 3);
            assert_eq!(input["os"], std::env::consts::OS);
            assert_eq!(input["shell"], "zsh");
            input["input"].as_str().unwrap().to_owned()
        })
        .collect()
}

pub(super) async fn run(
    cx: &mut gpui::AsyncApp,
    window: gpui::AnyWindowHandle,
    view: &Entity<TerminalView>,
    output: &std::path::Path,
) -> Result<Vec<serde_json::Value>, String> {
    cx.update_window(window, |_, window, cx| {
        cx.global_mut::<Settings>().command_ai.prediction_enabled = true;
        cx.global_mut::<Settings>().ghost = true;
        view.update(cx, |view, cx| view.apply_settings(cx));
        window.focus(&view.read(cx).focus_handle.clone(), cx);
    })
    .map_err(|e| e.to_string())?;
    let mut report = Vec::new();
    for (index, mode) in [CompletionStyle::Inline, CompletionStyle::Popup, CompletionStyle::Hybrid]
        .into_iter()
        .enumerate()
    {
        let prefix = format!("printf '预测{index}😀");
        let command = format!("{prefix}完成' > prediction-{index}");
        let server = Server::respond(vec![serde_json::json!({"command":command})]);
        server.release.store(true, Ordering::Release);
        configure(&server)?;
        cx.update_window(window, |_, _, cx| {
            view.update(cx, |view, _| view.completion_style = mode)
        })
        .map_err(|e| e.to_string())?;
        let start = std::time::Instant::now();
        draft(cx, window, view, &prefix).await?;
        wait_for(cx, window, view, |view, _| view.has_ai_prediction()).await?;
        let elapsed = start.elapsed().as_millis();
        probe(cx, window, view, &prefix, prefix.len()).await?;
        assert!(!output.join(format!("prediction-{index}")).exists());
        cx.update_window(window, |_, window, cx| {
            key(window, cx, "tab");
            if mode == CompletionStyle::Hybrid {
                key(window, cx, "right");
            }
        })
        .map_err(|e| e.to_string())?;
        wait_for(cx, window, view, |view, _| !view.has_ai_prediction()).await?;
        probe(cx, window, view, &command, command.len()).await?;
        assert!(
            !output.join(format!("prediction-{index}")).exists(),
            "accepting a suffix must not submit"
        );
        cx.update_window(window, |_, window, cx| key(window, cx, "enter"))
            .map_err(|e| e.to_string())?;
        wait_for(cx, window, view, |_, _| {
            std::fs::read(output.join(format!("prediction-{index}")))
                .is_ok_and(|bytes| bytes == format!("预测{index}😀完成").as_bytes())
        })
        .await?;
        assert_eq!(inputs(server.finish()), [prefix.clone()]);
        report.push(serde_json::json!({"mode":format!("{mode:?}"),"prefix":prefix,"command":command,"loopback_including_idle_ms":elapsed,"request_count":1,"no_submit":true}));
    }

    // A Shell-side edit invisible to the view still cannot accept a stale prefix.
    let prefix = "printf 'stale😀";
    let server = Server::respond(vec![serde_json::json!({"command":format!("{prefix}完成'")})]);
    server.release.store(true, Ordering::Release);
    configure(&server)?;
    draft(cx, window, view, prefix).await?;
    wait_for(cx, window, view, |view, _| view.has_ai_prediction()).await?;
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, _| view.write_bytes(b"x".to_vec()));
        key(window, cx, "right");
    })
    .map_err(|e| e.to_string())?;
    wait_for(cx, window, view, |view, _| !view.has_ai_prediction()).await?;
    probe(cx, window, view, &format!("{prefix}x"), prefix.len() + 1).await?;
    assert_eq!(inputs(server.finish()), [prefix]);
    report.push(serde_json::json!({"fresh_native_recheck_rejected_stale_prefix":true}));

    // Local candidates discovered while HTTP is blocked also beat its eventual reply.
    let prefix = "cat prediction-race-f";
    let server = Server::respond(vec![serde_json::json!({"command":"cat prediction-race-future"})]);
    configure(&server)?;
    draft(cx, window, view, prefix).await?;
    wait_for(cx, window, view, |_, _| server.received.load(Ordering::Acquire) == 1).await?;
    std::fs::write(output.join("prediction-race-file.txt"), "owned fixture")
        .map_err(|e| e.to_string())?;
    cx.update_window(window, |_, _, cx| {
        view.update(cx, |view, cx| {
            view.invalidate_completion_history(cx);
            view.refresh_suggestion_from_snapshot(Some(prefix.into()), view.suggest_anchor, cx);
        })
    })
    .map_err(|e| e.to_string())?;
    wait_for(cx, window, view, |view, _| {
        !view.suggest.suggestion.is_empty() || !view.suggest.completion_items.is_empty()
    })
    .await?;
    server.release.store(true, Ordering::Release);
    wait_for(cx, window, view, |view, _| view.ai_search_job.is_none()).await?;
    cx.update_window(window, |_, _, cx| assert!(!view.read(cx).has_ai_prediction()))
        .map_err(|e| e.to_string())?;
    probe(cx, window, view, prefix, prefix.len()).await?;
    assert_eq!(inputs(server.finish()), [prefix]);
    report.push(serde_json::json!({"late_local_result_wins":true}));

    // Real local filesystem candidates win without connecting to the provider at all.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut provider = crate::ai_providers::AiProvider::preset(
        crate::ai_providers::ProviderKind::Ollama,
        "native-loopback",
    );
    provider.base_url = format!("http://{}", listener.local_addr().unwrap());
    crate::ai_providers::save(&crate::ai_providers::ProviderStore {
        providers: vec![provider],
        ..Default::default()
    })
    .map_err(|e| e.to_string())?;
    std::fs::write(output.join("prediction-local-file.txt"), "owned fixture")
        .map_err(|e| e.to_string())?;
    draft(cx, window, view, "cat prediction-local-f").await?;
    wait_for(cx, window, view, |view, _| {
        !view.suggest.suggestion.is_empty() || !view.suggest.completion_items.is_empty()
    })
    .await?;
    cx.background_executor().timer(Duration::from_millis(700)).await;
    assert!(listener.accept().is_err(), "useful local candidates prohibit model requests");
    report.push(serde_json::json!({"local_first":true,"requests":0}));

    // Opposite ownership handoffs share one actual job, never parallel command POSTs.
    for prediction_first in [true, false] {
        let prefix = if prediction_first { "printf 'lane😀" } else { "printf 'latest😀" };
        let complete = format!("{prefix}'");
        let responses = if prediction_first {
            vec![
                serde_json::json!({"command":complete}),
                serde_json::json!({"command":"echo manual","explanation":"fixture"}),
            ]
        } else {
            vec![
                serde_json::json!({"command":"echo old","explanation":"fixture"}),
                serde_json::json!({"command":complete}),
            ]
        };
        let server = Server::respond(responses);
        configure(&server)?;
        cx.update_window(window, |_, _, cx| {
            view.update(cx, |view, _| view.completion_style = CompletionStyle::Inline)
        })
        .map_err(|e| e.to_string())?;
        draft(cx, window, view, if prediction_first { prefix } else { "printf 'before😀" }).await?;
        if !prediction_first {
            search(cx, window, view).await?;
        }
        wait_for(cx, window, view, |_, _| server.received.load(Ordering::Acquire) == 1).await?;
        if prediction_first {
            search(cx, window, view).await?;
        } else {
            cx.update_window(window, |_, window, cx| key(window, cx, "escape"))
                .map_err(|e| e.to_string())?;
            draft(cx, window, view, prefix).await?;
            wait_for(cx, window, view, |view, _| view.prediction_queued_for_test()).await?;
        }
        cx.update_window(window, |_, _, cx| {
            let job = view.read(cx).ai_search_job.as_ref().unwrap();
            assert_eq!(
                job.prediction, prediction_first,
                "the cancelled actual job must retain its lane"
            );
            assert!(job.cancelled.load(Ordering::Acquire));
        })
        .map_err(|e| e.to_string())?;
        assert_eq!(server.received.load(Ordering::Acquire), 1);
        server.release.store(true, Ordering::Release);
        wait_for(cx, window, view, |view, _| {
            view.ai_search_job.is_none()
                && if prediction_first {
                    view.ai_search.as_ref().is_some_and(|s| s.result.is_some())
                } else {
                    view.has_ai_prediction()
                }
        })
        .await?;
        assert_eq!(
            inputs(server.finish()),
            if prediction_first {
                vec![prefix, "manual request"]
            } else {
                vec!["manual request", prefix]
            }
        );
        cx.update_window(window, |_, window, cx| key(window, cx, "escape"))
            .map_err(|e| e.to_string())?;
        probe(cx, window, view, prefix, prefix.len()).await?;
        cx.background_executor().timer(Duration::from_millis(600)).await;
        cx.update_window(window, |_, _, cx| assert!(!view.read(cx).has_ai_prediction()))
            .map_err(|e| e.to_string())?;
        report.push(serde_json::json!({"prediction_first":prediction_first,"requests":2,"single_actual_lane":true,"escape_preserves_prefix":true}));
    }
    cx.update_window(window, |_, _, cx| {
        cx.global_mut::<Settings>().command_ai.prediction_enabled = false;
        view.update(cx, |view, cx| view.apply_settings(cx));
    })
    .map_err(|e| e.to_string())?;
    Ok(report)
}
