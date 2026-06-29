//! Outbound HTTP seam (BE-0010): one pooled `reqwest::Client` with per-call timeout tiers. Every
//! outbound call goes through here — never `reqwest::Client::new()` per call site, which has no
//! connection reuse and no timeout (a hung upstream would hang the request). Lives in `AppState`;
//! clone freely (the client is `Arc`-backed).

use std::time::Duration;

/// Timeout tier per call class.
#[derive(Debug, Clone, Copy)]
pub enum Tier {
    /// Fast JSON APIs (auth, webhooks, small third-party calls).
    Fast,
    /// One-shot LLM / slow third-party calls.
    Llm,
    /// Long-lived streams (use `raw()` for the body; this tier for the connect).
    Stream,
}

impl Tier {
    fn timeout(self) -> Duration {
        match self {
            Tier::Fast => Duration::from_secs(10),
            Tier::Llm => Duration::from_secs(60),
            Tier::Stream => Duration::from_secs(300),
        }
    }
}

/// Cheap-to-clone shared HTTP client.
#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .expect("build shared reqwest client");
        Http { client }
    }

    /// The raw client — for streaming bodies, or callers managing their own retry/timeout.
    pub fn raw(&self) -> &reqwest::Client {
        &self.client
    }

    /// Send a request built by `build`, applying the tier's timeout. `build` is a closure so a retry
    /// (TODO: add exponential backoff on 429/5xx) gets a fresh request each attempt.
    pub async fn send<F>(&self, tier: Tier, build: F) -> Result<reqwest::Response, reqwest::Error>
    where
        F: Fn(&reqwest::Client) -> reqwest::RequestBuilder,
    {
        build(&self.client).timeout(tier.timeout()).send().await
    }
}
