use super::*;

fn provider(wire_api: &str) -> AiProvider {
    let mut provider = AiProvider::preset(ProviderKind::Custom, "command-fixture");
    provider.base_url = "https://example.invalid/v1".into();
    provider.model = "fixture-model".into();
    provider.wire_api = wire_api.into();
    provider
}

#[test]
fn requests_have_only_the_approved_context_for_each_wire_protocol() {
    for wire in ["chat", "responses", "anthropic"] {
        let mut target = provider(wire);
        if wire == "anthropic" {
            target.kind = ProviderKind::Anthropic;
            target.wire_api.clear();
        }
        let (url, body) =
            payload(&target, Purpose::Search, "找占用端口的进程", "linux", "bash").unwrap();
        let input = if wire == "responses" {
            &body["input"][0]["content"]
        } else if wire == "anthropic" {
            &body["messages"][0]["content"]
        } else {
            &body["messages"][1]["content"]
        };
        let input: Value = serde_json::from_str(input.as_str().unwrap()).unwrap();
        assert_eq!(input, json!({"input":"找占用端口的进程","os":"linux","shell":"bash"}));
        assert!(body.get("tools").is_none());
        assert!(body.get("previous_response_id").is_none());
        assert!(url.starts_with("https://example.invalid/v1/"));
        if wire == "responses" {
            assert_eq!(body["store"], false);
        }
    }
    let mut reasoning = provider("responses");
    reasoning.model = "gpt-6-fixture".into();
    let (_, body) = payload(&reasoning, Purpose::Complete, "git ", "macos", "zsh").unwrap();
    assert_eq!(body["reasoning"]["effort"], "low");
    assert_eq!(body["max_output_tokens"], 512);
}

#[test]
fn input_and_metadata_are_bounded_before_any_model_request() {
    let target = provider("chat");
    for input in [
        "",
        "  ",
        "line\nnext",
        "line\rnext",
        "line\u{1b}",
        "line\u{2028}",
        &"a".repeat(MAX_INPUT_BYTES + 1),
    ] {
        assert_eq!(
            payload(&target, Purpose::Search, input, "linux", "bash"),
            Err(Failure::Unavailable)
        );
    }
    assert!(payload(&target, Purpose::Search, "query", "linux", "/bin/bash").is_err());
    assert!(payload(&target, Purpose::Search, "query", "host.example", "bash").is_err());
    assert!(
        payload(&target, Purpose::Search, &"a".repeat(MAX_INPUT_BYTES), "windows", "pwsh").is_ok()
    );
    let mut full = target;
    full.full_url = true;
    full.base_url = "https://example.invalid/custom".into();
    assert_eq!(
        payload(&full, Purpose::Search, "query", "windows", "powershell").unwrap().0,
        full.base_url
    );
}

#[test]
fn search_results_are_strict_single_line_json_or_explicit_abstention() {
    assert_eq!(
        parse_search(r#"{"command":"printf '中文'","explanation":"输出文字"}"#).unwrap(),
        Some(SearchResult {
            command: "printf '中文'".into(), explanation: "输出文字".into()
        })
    );
    assert_eq!(parse_search(r#"{"command":null,"explanation":""}"#), Ok(None));
    assert_eq!(parse_search(r#"{"command":"  ","explanation":""}"#), Ok(None));
    for text in [
        "not JSON",
        r#"{"explanation":"missing command"}"#,
        r#"{"command":"pwd"}"#,
        r#"{"command":"pwd","explanation":"ok","execute":true}"#,
        r#"{"command":"pwd\nwhoami","explanation":"ok"}"#,
        r#"{"command":"pwd\r","explanation":"ok"}"#,
        r#"{"command":"pwd\u001b","explanation":"ok"}"#,
        r#"{"command":"pwd\u2029","explanation":"ok"}"#,
        r#"{"command":"pwd","explanation":"line\nnext"}"#,
    ] {
        assert_eq!(parse_search(text), Err(Failure::InvalidResponse), "{text}");
    }
    assert_eq!(
        parse_search(
            &json!({"command":"a".repeat(MAX_INPUT_BYTES + 1),"explanation":""}).to_string()
        ),
        Err(Failure::InvalidResponse)
    );
    assert_eq!(
        parse_search(
            &json!({"command":"pwd","explanation":"中".repeat(MAX_EXPLANATION_CHARS + 1)})
                .to_string()
        ),
        Err(Failure::InvalidResponse)
    );
}

#[test]
fn completion_preserves_exact_unicode_quoting_and_spaces() {
    let prefix = "  printf '中文 ";
    assert_eq!(
        parse_completion(&json!({"command":format!("{prefix}完成'")}).to_string(), prefix),
        Ok(Some("完成'".into()))
    );
    assert_eq!(parse_completion(r#"{"command":null}"#, prefix), Ok(None));
    assert_eq!(parse_completion(&json!({"command":prefix}).to_string(), prefix), Ok(None));
    for value in [
        json!({"command":"printf '中文 完成'"}),
        json!({"command":format!("{prefix}完成'\n")}),
        json!({"command":"anything","explanation":"unexpected"}),
        json!({}),
    ] {
        assert_eq!(parse_completion(&value.to_string(), prefix), Err(Failure::InvalidResponse));
    }
}

#[test]
fn preparation_retries_failures_but_reuses_success_without_loading_again() {
    let mut cache = None;
    assert!(matches!(
        prepare_cached(&mut cache, 1, || Err(Failure::Unavailable)),
        Err(Failure::Unavailable)
    ));
    assert!(cache.is_none());
    let first = prepare_cached(&mut cache, 1, || {
        let mut prepared = Prepared::test_with_provider(provider("chat"));
        prepared.revision = 1;
        Ok(prepared)
    })
    .unwrap();
    let reused =
        prepare_cached(&mut cache, 1, || panic!("stable snapshot reloaded credentials")).unwrap();
    assert!(Arc::ptr_eq(&first, &reused));
    assert!(matches!(
        prepare_cached(&mut cache, 2, || Err(Failure::Unavailable)),
        Err(Failure::Unavailable)
    ));
    assert!(cache.is_none(), "the prior revision cannot survive a failed refresh");
    assert!(
        prepare_cached(&mut cache, 2, || {
            let mut prepared = Prepared::test_with_provider(provider("chat"));
            prepared.revision = 2;
            Ok(prepared)
        })
        .is_ok()
    );
}

#[test]
fn prepared_requests_use_the_real_shared_transport_and_reject_stale_snapshots() {
    let _guard = ai_inference::test_guard();
    let value = json!({"choices":[{"finish_reason":"stop", "message":{"content":
        json!({"command":"printf 'AI_QA'", "explanation":"输出固定文本"}).to_string()}}]});
    let (url, server) = ai_inference::test_support::reply(value, "200 OK", || {});
    let mut target = provider("chat");
    target.base_url = url;
    let client = Prepared::test_with_provider(target);
    let result =
        client.search("输出测试文字", "macos", "zsh", &AtomicBool::new(false)).unwrap().unwrap();
    assert_eq!(result.command, "printf 'AI_QA'");
    let captured = server.join().unwrap();
    assert_eq!(captured.request_line.trim(), "POST /chat/completions HTTP/1.1");
    let content: Value =
        serde_json::from_str(captured.body["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(content, json!({"input":"输出测试文字","os":"macos","shell":"zsh"}));
    let mut stale = client;
    stale.revision = revision() - 1;
    assert_eq!(
        stale.search("do not send", "macos", "zsh", &AtomicBool::new(false)),
        Err(Failure::Cancelled)
    );
}
