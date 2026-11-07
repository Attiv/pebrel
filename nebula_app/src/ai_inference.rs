//! Shared bounded JSON inference transport; feature prompts and output schemas stay with callers.
use std::sync::{
    LazyLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use serde_json::Value;
use zeroize::Zeroizing;

use crate::ai_providers::{AiProvider, ProviderKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Failure {
    Cancelled,
    Busy,
    Unavailable,
    Network,
    InvalidResponse,
}

fn agent(local: bool) -> ureq::Agent {
    let config = ureq::config::Config::builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .timeout_connect(Some(Duration::from_millis(1500)))
        .max_redirects(0);
    (if local { config.proxy(None) } else { config }).build().new_agent()
}

static CLIENT: LazyLock<ureq::Agent> = LazyLock::new(|| agent(false));
static LOCAL_CLIENT: LazyLock<ureq::Agent> = LazyLock::new(|| agent(true));
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

struct Slot;
impl Slot {
    fn acquire() -> Option<Self> {
        ACTIVE
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < 2).then_some(n + 1))
            .ok()?;
        Some(Self)
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}

pub fn supported(provider: &AiProvider) -> bool {
    provider.enabled
        && !provider.model.trim().is_empty()
        && !matches!(provider.kind, ProviderKind::Google | ProviderKind::AzureOpenAi)
        && matches!(provider.wire_api.as_str(), "" | "responses" | "chat" | "chat_completions")
}

/// Immutable provider/credential state. Prepare off the UI thread and reuse it.
pub(crate) struct PreparedClient {
    provider: AiProvider,
    key: Zeroizing<Vec<u8>>,
}

impl PreparedClient {
    pub(crate) fn prepare(provider: AiProvider) -> Result<Self, Failure> {
        if !supported(&provider) {
            return Err(Failure::Unavailable);
        }
        let key = Zeroizing::new(
            crate::ai_providers::load_api_key(&provider.id)
                .map_err(|_| Failure::Unavailable)?
                .unwrap_or_default(),
        );
        Self::with_key(provider, key)
    }

    fn with_key(provider: AiProvider, key: Zeroizing<Vec<u8>>) -> Result<Self, Failure> {
        std::str::from_utf8(&key).map_err(|_| Failure::Unavailable)?;
        if key.is_empty() && provider.kind.requires_api_key() {
            return Err(Failure::Unavailable);
        }
        Ok(Self { provider, key })
    }

    #[cfg(test)]
    pub(crate) fn test_with_key(provider: AiProvider, key: &[u8]) -> Self {
        Self::with_key(provider, Zeroizing::new(key.to_vec())).unwrap()
    }

    pub(crate) fn provider(&self) -> &AiProvider {
        &self.provider
    }

    /// Blocking; callers own task lifetime and their purpose-specific freshness checks.
    pub(crate) fn request(
        &self,
        url: &str,
        body: &Value,
        cancelled: &AtomicBool,
    ) -> Result<String, Failure> {
        if cancelled.load(Ordering::Acquire) {
            return Err(Failure::Cancelled);
        }
        let _slot = Slot::acquire().ok_or(Failure::Busy)?;
        let uri = url.parse::<ureq::http::Uri>().map_err(|_| Failure::Unavailable)?;
        if !matches!(uri.scheme_str(), Some("http" | "https"))
            || uri.authority().ok_or(Failure::Unavailable)?.as_str().contains('@')
        {
            return Err(Failure::Unavailable);
        }
        let local = uri.host().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        let client = if local { &*LOCAL_CLIENT } else { &*CLIENT };
        let key = std::str::from_utf8(&self.key).map_err(|_| Failure::Unavailable)?;
        let bearer = Zeroizing::new(format!("Bearer {key}"));
        let mut request = client.post(url);
        request = match self.provider.kind {
            ProviderKind::Anthropic => {
                request.header("x-api-key", key).header("anthropic-version", "2023-06-01")
            },
            _ if key.is_empty() => request,
            _ => request.header("Authorization", bearer.as_str()),
        };
        // shortcut: blocking HTTP can only suppress publication until its deadline; use abortable I/O if measured slot contention requires it.
        let mut response = request.send_json(body).map_err(|_| Failure::Network)?;
        if !response.status().is_success() {
            return Err(Failure::Network);
        }
        let value: Value = response
            .body_mut()
            .with_config()
            .limit(64 * 1024)
            .read_json()
            .map_err(|_| Failure::InvalidResponse)?;
        if cancelled.load(Ordering::Acquire) {
            return Err(Failure::Cancelled);
        }
        response_text(&self.provider, &value).ok_or(Failure::InvalidResponse)
    }
}

pub(crate) fn response_text(provider: &AiProvider, value: &Value) -> Option<String> {
    match provider.kind {
        ProviderKind::Anthropic => {
            if value["stop_reason"] != "end_turn" {
                return None;
            }
            let content = value["content"].as_array()?;
            let mut text = content.iter().filter(|v| v["type"] == "text");
            let result = text.next()?["text"].as_str()?.to_owned();
            text.next().is_none().then_some(result)
        },
        _ if provider.wire_api == "responses" => {
            if value["status"] != "completed" {
                return None;
            }
            let output = value["output"].as_array()?;
            let mut messages = output.iter().filter(|v| v["type"] == "message");
            let message = messages.next()?;
            if messages.next().is_some() {
                return None;
            }
            let content = message["content"].as_array()?;
            if content.len() != 1 || content[0]["type"] != "output_text" {
                return None;
            }
            Some(content[0]["text"].as_str()?.to_owned())
        },
        _ => {
            let choices = value["choices"].as_array()?;
            if choices.len() != 1
                || choices[0]["finish_reason"] != "stop"
                || !choices[0]["message"]["refusal"].is_null()
            {
                return None;
            }
            Some(choices[0]["message"]["content"].as_str()?.to_owned())
        },
    }
}

#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static NETWORK_FIXTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());
    NETWORK_FIXTURES.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;
