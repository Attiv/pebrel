use super::*;
use serde_json::json;

fn provider(base_url: String) -> AiProvider {
    let mut provider = AiProvider::preset(ProviderKind::Custom, "transport-fixture");
    provider.model = "fixture-model".into();
    provider.base_url = base_url;
    provider
}

fn complete_response() -> Value {
    json!({"choices":[{"finish_reason":"stop", "message":{"content":"result"}}]})
}

#[test]
fn preserves_complete_envelopes_and_rejects_refused_ambiguous_or_partial_text() {
    let chat = provider("https://example.invalid".into());
    assert_eq!(response_text(&chat, &complete_response()).as_deref(), Some("result"));
    for value in [
        json!({"choices":[{"finish_reason":"length", "message":{"content":"x"}}]}),
        json!({"choices":[{"finish_reason":"stop", "message":{"content":"x","refusal":"no"}}]}),
        json!({"choices":[{"finish_reason":"stop","message":{"content":"x"}},
            {"finish_reason":"stop","message":{"content":"y"}}]}),
    ] {
        assert!(response_text(&chat, &value).is_none());
    }
    let mut anthropic = chat;
    anthropic.kind = ProviderKind::Anthropic;
    assert_eq!(
        response_text(
            &anthropic,
            &json!({"stop_reason":"end_turn", "content":[
            {"type":"text", "text":"one"}]})
        )
        .as_deref(),
        Some("one")
    );
    assert!(
        response_text(
            &anthropic,
            &json!({"stop_reason":"max_tokens", "content":[
            {"type":"text", "text":"one"}]})
        )
        .is_none()
    );
    assert!(
        response_text(
            &anthropic,
            &json!({"stop_reason":"end_turn", "content":[
            {"type":"text", "text":"one"},{"type":"text", "text":"two"}]})
        )
        .is_none()
    );
}

#[test]
fn real_http_keeps_auth_limits_and_no_redirect_contract() {
    let _guard = test_guard();
    let cancelled = AtomicBool::new(false);
    let (base, server) = test_support::reply(complete_response(), "200 OK", || {});
    let client = PreparedClient::test_with_key(provider(base.clone()), b"fixture-secret");
    assert_eq!(
        client
            .request(
                Lane::Explicit,
                &format!("{base}/chat/completions"),
                &json!({"input":"safe"}),
                &cancelled
            )
            .unwrap(),
        "result"
    );
    let captured = server.join().unwrap();
    assert_eq!(captured.request_line.trim(), "POST /chat/completions HTTP/1.1");
    assert_eq!(captured.authorization.as_deref(), Some("Bearer fixture-secret"));
    assert_eq!(captured.body, json!({"input":"safe"}));

    let (base, server) = test_support::reply(
        complete_response(),
        "307 Temporary Redirect\r\nLocation: http://127.0.0.1:1/leak",
        || {},
    );
    let client = PreparedClient::test_with_key(provider(base.clone()), b"fixture-secret");
    assert_eq!(
        client.request(Lane::Explicit, &base, &json!({}), &cancelled),
        Err(Failure::Network)
    );
    server.join().unwrap();

    let (base, server) =
        test_support::reply(json!({"padding":"x".repeat(64 * 1024)}), "200 OK", || {});
    let client = PreparedClient::test_with_key(provider(base.clone()), b"fixture-secret");
    assert_eq!(
        client.request(Lane::Explicit, &base, &json!({}), &cancelled),
        Err(Failure::InvalidResponse)
    );
    server.join().unwrap();
}

#[test]
fn cancellation_suppresses_results_and_explicit_slots_enforce_the_total_cap() {
    let _guard = test_guard();
    let cancelled = std::sync::Arc::new(AtomicBool::new(false));
    let on_request = cancelled.clone();
    let (base, server) = test_support::reply(complete_response(), "200 OK", move || {
        on_request.store(true, Ordering::Release);
    });
    let client = PreparedClient::test_with_key(provider(base.clone()), b"fixture-secret");
    assert_eq!(
        client.request(Lane::Explicit, &base, &json!({}), &cancelled),
        Err(Failure::Cancelled)
    );
    server.join().unwrap();
    assert_eq!(
        client.request(Lane::Explicit, &base, &json!({}), &cancelled),
        Err(Failure::Cancelled)
    );

    let first = Slot::acquire(Lane::Explicit).unwrap();
    let second = Slot::acquire(Lane::Explicit).unwrap();
    assert!(Slot::acquire(Lane::Explicit).is_none());
    assert_eq!(
        client.request(Lane::Explicit, &base, &json!({}), &AtomicBool::new(false)),
        Err(Failure::Busy)
    );
    drop(first);
    assert!(Slot::acquire(Lane::Explicit).is_some());
    drop(second);
    assert_eq!(ACTIVE.load(Ordering::Acquire), 0);
}

#[test]
fn background_predictions_never_displace_an_explicit_request() {
    let _guard = test_guard();
    let (base, server) = test_support::reply(complete_response(), "200 OK", || {});
    let client = PreparedClient::test_with_key(provider(base.clone()), b"fixture-secret");

    // One opportunistic prediction may be in flight, and a second one must not
    // take the remaining slot that belongs to an explicit user request.
    let background = Slot::acquire(Lane::Background).unwrap();
    assert!(Slot::acquire(Lane::Background).is_none());
    assert_eq!(ACTIVE.load(Ordering::Acquire), BACKGROUND_SLOTS);
    assert_eq!(
        client.request(Lane::Background, &base, &json!({}), &AtomicBool::new(false)),
        Err(Failure::Busy)
    );

    // The user-initiated search still reaches the network while the prediction
    // holds its slot; this is the regression that a flat two-slot cap allowed.
    assert_eq!(
        client.request(Lane::Explicit, &base, &json!({}), &AtomicBool::new(false)),
        Ok("result".to_owned())
    );
    server.join().unwrap();

    // A later background prediction yields to an in-flight explicit request.
    let explicit = Slot::acquire(Lane::Explicit).unwrap();
    assert!(Slot::acquire(Lane::Background).is_none());
    drop(explicit);
    drop(background);
    assert_eq!(ACTIVE.load(Ordering::Acquire), 0);
}

#[test]
fn invalid_url_never_sends_and_supported_provider_filter_is_unchanged() {
    let _guard = test_guard();
    let mut target = provider("https://example.invalid".into());
    assert!(supported(&target));
    target.kind = ProviderKind::AzureOpenAi;
    assert!(!supported(&target));
    target.kind = ProviderKind::Google;
    assert!(!supported(&target));
    let client = PreparedClient::test_with_key(provider("https://example.invalid".into()), b"key");
    for url in ["file:///tmp/not-a-request", "https://user:secret@example.invalid/v1"] {
        assert_eq!(
            client.request(Lane::Explicit, url, &json!({}), &AtomicBool::new(false)),
            Err(Failure::Unavailable)
        );
    }
}
