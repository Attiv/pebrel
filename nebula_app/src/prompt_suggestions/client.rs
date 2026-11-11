//! Message-prediction payload policy over the shared bounded inference transport.
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::{INSTRUCTIONS, Message, context, decode};
pub use crate::ai_inference::Failure;
use crate::ai_inference::Lane;
#[cfg(test)]
use crate::ai_inference::response_text;
pub use crate::ai_inference::supported;
use crate::ai_providers::{AiProvider, ProviderKind};

/// A prepared message-prediction client; the owner replaces it on configuration changes.
pub struct PreparedClient {
    inference: crate::ai_inference::PreparedClient,
}

impl PreparedClient {
    #[cfg(test)]
    pub(super) fn test_with_key(provider: AiProvider) -> Self {
        Self::with_test_key(provider, b"synthetic-test-key")
    }

    #[cfg(test)]
    fn with_test_key(provider: AiProvider, key: &[u8]) -> Self {
        Self { inference: crate::ai_inference::PreparedClient::test_with_key(provider, key) }
    }

    pub fn prepare(provider: AiProvider) -> Result<Self, Failure> {
        Ok(Self { inference: crate::ai_inference::PreparedClient::prepare(provider)? })
    }

    /// Blocking; a missing prediction is optional, but malformed output is not an abstention.
    pub fn request(
        &self,
        language: &str,
        messages: &[Message],
        cancelled: &AtomicBool,
    ) -> Result<Option<String>, Failure> {
        if cancelled.load(Ordering::Acquire) {
            return Err(Failure::Cancelled);
        }
        if !messages.iter().any(|message| message.role == "user" && !message.text.trim().is_empty())
        {
            return Err(Failure::Unavailable);
        }
        let (url, body) =
            payload(self.inference.provider(), language, messages).ok_or(Failure::Unavailable)?;
        let text = self.inference.request(Lane::Background, &url, &body, cancelled)?;
        decode(&text).map_err(|_| Failure::InvalidResponse)
    }
}

fn payload(provider: &AiProvider, language: &str, messages: &[Message]) -> Option<(String, Value)> {
    if !supported(provider) {
        return None;
    }
    // Preserve actual message roles, then append the upstream user fragment.
    // Language is a product setting, not a learned or persisted voice policy.
    let locale = format!("Write the suggested user message in {language}.");
    let mut input: Vec<Value> = context(messages)
        .into_iter()
        .map(|message| json!({"role": message.role, "content": message.text}))
        .collect();
    input.push(json!({"role":"user", "content":format!(
        "<external_prompt_suggestion>\n{INSTRUCTIONS}\n</external_prompt_suggestion>"
    )}));
    let reasoning_model = provider.model.starts_with("gpt-5")
        || provider.model.starts_with("gpt-6")
        || provider.model.starts_with("o3")
        || provider.model.starts_with("o4");
    let (route, mut body) = if provider.kind == ProviderKind::Anthropic {
        (
            "messages",
            json!({"model":provider.model,"max_tokens":512,"system":locale,
            "messages":input}),
        )
    } else if provider.wire_api == "responses" {
        (
            "responses",
            json!({"model":provider.model,"store":false,"max_output_tokens":512,
            "instructions":locale,"input":input}),
        )
    } else {
        input.insert(0, json!({"role":"system", "content":locale}));
        let mut body = json!({"model":provider.model, "messages":input});
        body[if reasoning_model { "max_completion_tokens" } else { "max_tokens" }] = json!(512);
        ("chat/completions", body)
    };
    if reasoning_model && provider.kind != ProviderKind::Anthropic {
        if provider.wire_api == "responses" {
            body["reasoning"] = json!({"effort":"low"});
        } else {
            body["reasoning_effort"] = json!("low");
        }
    }
    let base = provider.base_url.trim().trim_end_matches('/');
    let url = if provider.full_url { base.to_owned() } else { format!("{base}/{route}") };
    Some((url, body))
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
