//! Opt-in real public zsh-autosuggestions widget, native PTY, GPUI keys and loopback inference.
use super::*;
use lifecycle::Server;
use nebula_settings::CommandAiSource;

pub(super) async fn run(
    cx: &mut gpui::AsyncApp,
    window: gpui::AnyWindowHandle,
    view: &Entity<TerminalView>,
    output: &std::path::Path,
) -> Result<serde_json::Value, String> {
    let seed = (0..3)
        .map(|i| format!("print -s -- \"printf '并列{i}历史' > coex-history-{i}\""))
        .collect::<Vec<_>>()
        .join("; ")
        + "; : > coex-seeded";
    prediction::draft(cx, window, view, &seed).await?;
    cx.update_window(window, |_, window, cx| key(window, cx, "enter"))
        .map_err(|e| e.to_string())?;
    wait_for(cx, window, view, |view, _| {
        output.join("coex-seeded").exists() && view.ai_search_identity().is_some()
    })
    .await?;
    cx.update_window(window, |_, window, cx| {
        cx.activate(true);
        window.activate_window();
        cx.global_mut::<Settings>().command_ai.prediction_enabled = true;
        cx.global_mut::<Settings>().ghost = true;
        view.update(cx, |view, cx| view.apply_settings(cx));
        window.focus(&view.read(cx).focus_handle.clone(), cx);
    })
    .map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for (index, key_name, history, manual) in
        [(0, "tab", false, false), (1, "tab", true, false), (2, "right", true, true)]
    {
        let prefix = format!("printf '并列{index}");
        let ai = format!("{prefix}AI' > coex-ai-{index}");
        let native = format!("{prefix}历史' > coex-history-{index}");
        let server = Server::respond(vec![serde_json::json!({"command":ai})]);
        prediction::configure(&server)?;
        server.release.store(true, Ordering::Release);
        cx.update_window(window, |_, _, cx| {
            let settings = &mut cx.global_mut::<Settings>().command_ai;
            settings.tab_source =
                if index == 1 { CommandAiSource::History } else { CommandAiSource::Ai };
            settings.right_source = CommandAiSource::Ai;
        })
        .map_err(|e| e.to_string())?;
        // Printable keys arrive through the native text-input channel, not a synthetic control KeyDown.
        prediction::draft(cx, window, view, &prefix[..prefix.len() - 1]).await?;
        cx.update_window(window, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.replace_text_in_range(None, &index.to_string(), window, cx)
            });
        })
        .map_err(|e| e.to_string())?;
        probe(cx, window, view, &prefix, prefix.len()).await?;
        wait_for(cx, window, view, |view, _| {
            view.has_ai_prediction() && view.suggest.completion_items.len() == 2
        })
        .await?;
        cx.update_window(window, |_, window, cx| {
            let state = view.read(cx);
            assert!(state.suggest.suggestion.is_empty(), "native gray text owns the input row");
            assert_eq!(state.suggest.completion_items[0].label, native);
            assert_eq!(state.suggest.completion_items[1].label, ai);
            if manual {
                key(window, cx, "down");
            }
        })
        .map_err(|e| e.to_string())?;
        probe(cx, window, view, &prefix, prefix.len()).await?;
        cx.update_window(window, |_, window, cx| key(window, cx, key_name))
            .map_err(|e| e.to_string())?;
        let expected = if history { &native } else { &ai };
        probe(cx, window, view, expected, expected.len()).await?;
        assert!(!output.join(format!("coex-ai-{index}")).exists());
        assert!(!output.join(format!("coex-history-{index}")).exists());
        cx.update_window(window, |_, window, cx| key(window, cx, "enter"))
            .map_err(|e| e.to_string())?;
        let marker =
            output.join(format!("coex-{}-{index}", if history { "history" } else { "ai" }));
        wait_for(cx, window, view, |_, _| marker.exists()).await?;
        let inputs = prediction::inputs(server.finish());
        assert_eq!(inputs, [prefix]);
        results.push(serde_json::json!({"key":key_name,"history":history,"manual":manual,"posts":inputs.len(),"no_submit_until_separate_enter":true}));
    }
    Ok(serde_json::json!({"native_zsh_autosuggestions_coexistence":results}))
}
