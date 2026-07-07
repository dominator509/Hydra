use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use serde_json::{json, Value};
use tokenkiller::{
    ApproxTokenizer, CacheUsage, CompletionRequest, CompletionResponse, Router as TkRouter,
    RouterError as TkRouterError, Tokenizer,
};

pub mod providers {
    pub mod anthropic;
    pub mod deepseek;
    pub mod openai_compat;
}
pub mod routes;

pub use routes::{load_routes_yaml, RouteCfg, Routes, RoutesError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Private,
    Caching,
    Cheap,
    Frontier,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub stable_prefix: String,
    pub tail: String,
    pub max_tokens: u32,
    pub stream: bool,
}

impl From<&CompletionRequest> for ChatRequest {
    fn from(request: &CompletionRequest) -> Self {
        Self {
            stable_prefix: request.prompt.stable_bytes.clone(),
            tail: request.prompt.tail_bytes.clone(),
            max_tokens: request.max_tokens,
            stream: true,
        }
    }
}

impl ChatRequest {
    pub fn prompt_tokens(&self) -> u64 {
        let tokenizer = ApproxTokenizer;
        usize_to_u64(tokenizer.count(&self.stable_prefix) + tokenizer.count(&self.tail))
    }
}

#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub chunks: Vec<String>,
    pub usage: CacheUsage,
    pub out_tokens: u64,
    pub cost_cents: u32,
    pub provider: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct Pricing {
    hit_input_milli_cents: u32,
    miss_input_milli_cents: u32,
    output_milli_cents: u32,
}

impl Pricing {
    pub const fn new(
        hit_input_milli_cents: u32,
        miss_input_milli_cents: u32,
        output_milli_cents: u32,
    ) -> Self {
        Self {
            hit_input_milli_cents,
            miss_input_milli_cents,
            output_milli_cents,
        }
    }

    pub fn estimate_cents(&self, hit_tokens: u64, miss_tokens: u64, out_tokens: u64) -> u32 {
        let total_milli = u64::from(self.hit_input_milli_cents) * hit_tokens
            + u64::from(self.miss_input_milli_cents) * miss_tokens
            + u64::from(self.output_milli_cents) * out_tokens;
        u32::try_from(total_milli.div_ceil(1_000))
            .expect("token pricing should stay within u32 cents for route-scale requests")
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn tags(&self) -> &[Tag];
    async fn complete(&self, req: &ChatRequest) -> Result<ProviderResponse, String>;
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RouterError {
    #[error("unknown llm route '{0}'")]
    UnknownRoute(String),
    #[error("tk_pii_route_blocked: route '{0}' is PII but no private provider is configured")]
    PiiBlocked(String),
    #[error("llm_provider_error: all providers in chain failed; last: {0}")]
    Exhausted(String),
}

#[derive(Clone, Default)]
pub struct JsonHttpClient {
    inner: reqwest::Client,
}

impl JsonHttpClient {
    pub fn new() -> Self {
        Self {
            inner: reqwest::Client::new(),
        }
    }

    pub async fn post_json(
        &self,
        url: &str,
        bearer_token: Option<&str>,
        body: &Value,
    ) -> Result<Value, String> {
        let mut request = self.inner.post(url).json(body);
        if let Some(token) = bearer_token {
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .await
            .map_err(|error| format!("request failed for {url}: {error}"))?;
        let status = response.status();
        let payload = response
            .text()
            .await
            .map_err(|error| format!("response read failed for {url}: {error}"))?;

        if !status.is_success() {
            return Err(format!(
                "upstream status {} for {url}: {}",
                status.as_u16(),
                payload
            ));
        }

        serde_json::from_str(&payload)
            .map_err(|error| format!("invalid json from {url}: {error}; payload={payload}"))
    }

    pub fn json_body(
        stable_prefix: &str,
        tail: &str,
        max_tokens: u32,
        stream: bool,
        model: &str,
    ) -> Value {
        json!({
            "model": model,
            "stream": stream,
            "max_tokens": max_tokens,
            "messages": [
                { "role": "system", "content": stable_prefix },
                { "role": "user", "content": tail }
            ]
        })
    }
}

pub struct Router {
    routes: Routes,
    providers: Vec<Box<dyn LlmProvider>>,
    pub degrade_to_private: AtomicBool,
}

impl Router {
    pub fn new(routes: Routes, providers: Vec<Box<dyn LlmProvider>>) -> Self {
        Self {
            routes,
            providers,
            degrade_to_private: AtomicBool::new(false),
        }
    }

    pub fn from_yaml_str(
        routes_yaml: &str,
        providers: Vec<Box<dyn LlmProvider>>,
    ) -> Result<Self, RoutesError> {
        Ok(Self::new(load_routes_yaml(routes_yaml)?, providers))
    }

    pub fn route(&self, name: &str) -> Option<&RouteCfg> {
        self.routes.get(name)
    }

    pub async fn complete_route(
        &self,
        route_name: &str,
        req: &ChatRequest,
    ) -> Result<ProviderResponse, RouterError> {
        let route = self
            .routes
            .get(route_name)
            .ok_or_else(|| RouterError::UnknownRoute(route_name.to_owned()))?;

        let mut chain = route
            .providers
            .iter()
            .filter_map(|name| {
                self.providers
                    .iter()
                    .find(|provider| provider.name() == name)
                    .map(|provider| provider.as_ref())
            })
            .collect::<Vec<_>>();

        if route.pii {
            chain.retain(|provider| provider.tags().contains(&Tag::Private));
            if chain.is_empty() {
                return Err(RouterError::PiiBlocked(route.name.clone()));
            }
        }

        if self.degrade_to_private.load(Ordering::Relaxed) {
            chain.sort_by_key(|provider| !provider.tags().contains(&Tag::Private));
        }

        let mut last = String::from("empty chain");
        for provider in chain {
            match provider.complete(req).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    tracing::warn!(
                        provider = provider.name(),
                        route = %route.name,
                        "provider failed: {error}"
                    );
                    last = error;
                }
            }
        }

        Err(RouterError::Exhausted(last))
    }
}

#[async_trait]
impl TkRouter for Router {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, TkRouterError> {
        let response = self
            .complete_route(&request.route, &ChatRequest::from(&request))
            .await
            .map_err(|error| TkRouterError {
                message: error.to_string(),
            })?;

        Ok(CompletionResponse {
            chunks: response.chunks,
            usage: response.usage,
            out_tokens: response.out_tokens,
            cost_cents: response.cost_cents,
            provider: response.provider.to_owned(),
        })
    }
}

pub(crate) fn extract_choice_text(value: &Value) -> Result<String, String> {
    value["choices"][0]["message"]["content"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| "missing choices[0].message.content".to_owned())
}

pub(crate) fn extract_anthropic_text(value: &Value) -> Result<String, String> {
    value["content"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["text"].as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "missing content[0].text".to_owned())
}

pub(crate) fn non_caching_usage(value: &Value, req: &ChatRequest, input_field: &str) -> CacheUsage {
    let miss_tokens = value["usage"][input_field]
        .as_u64()
        .unwrap_or_else(|| req.prompt_tokens());
    CacheUsage {
        hit_tokens: 0,
        miss_tokens,
    }
}

pub(crate) fn output_tokens(value: &Value, fallback_text: &str, usage_field: &str) -> u64 {
    value["usage"][usage_field]
        .as_u64()
        .unwrap_or_else(|| usize_to_u64(ApproxTokenizer.count(fallback_text)))
}

pub(crate) fn normalize_base_url(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

pub(crate) fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).expect("usize to u64 conversion is infallible on supported targets")
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::{json, Value};
    use tokenkiller::{ApproxTokenizer, BLOCK_TOKENS};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    use super::providers::deepseek::DeepSeekProvider;
    use super::*;

    #[derive(Clone)]
    struct FakeProvider {
        name: &'static str,
        tags: &'static [Tag],
        fail: bool,
    }

    #[async_trait]
    impl LlmProvider for FakeProvider {
        fn name(&self) -> &'static str {
            self.name
        }

        fn tags(&self) -> &[Tag] {
            self.tags
        }

        async fn complete(&self, _req: &ChatRequest) -> Result<ProviderResponse, String> {
            if self.fail {
                Err(format!("{} failed", self.name))
            } else {
                Ok(ProviderResponse {
                    chunks: vec!["ok".into()],
                    usage: CacheUsage::default(),
                    out_tokens: 1,
                    cost_cents: 1,
                    provider: self.name,
                })
            }
        }
    }

    fn req(stable: &str, tail: &str) -> ChatRequest {
        ChatRequest {
            stable_prefix: stable.into(),
            tail: tail.into(),
            max_tokens: 256,
            stream: false,
        }
    }

    #[tokio::test]
    async fn pii_gate_blocks() {
        let routes = load_routes_yaml(
            r#"
routes:
  - name: concierge
    pii: true
    max_tokens: 256
    output_budget_bytes: 2048
    providers: [deepseek]
"#,
        )
        .expect("route yaml should parse");
        let router = Router::new(
            routes,
            vec![Box::new(FakeProvider {
                name: "deepseek",
                tags: &[Tag::Caching, Tag::Cheap],
                fail: false,
            })],
        );

        let error = router
            .complete_route("concierge", &req("stable", "tail"))
            .await
            .expect_err("PII routes without a private provider should fail");

        assert!(matches!(error, RouterError::PiiBlocked(route) if route == "concierge"));
    }

    #[tokio::test]
    async fn fallback_chain() {
        let routes = load_routes_yaml(
            r#"
routes:
  - name: concierge
    pii: false
    max_tokens: 256
    output_budget_bytes: 2048
    providers: [broken, private]
"#,
        )
        .expect("route yaml should parse");
        let router = Router::new(
            routes,
            vec![
                Box::new(FakeProvider {
                    name: "broken",
                    tags: &[Tag::Cheap],
                    fail: true,
                }),
                Box::new(FakeProvider {
                    name: "private",
                    tags: &[Tag::Private],
                    fail: false,
                }),
            ],
        );

        let response = router
            .complete_route("concierge", &req("stable", "tail"))
            .await
            .expect("fallback chain should reach the second provider");

        assert_eq!(response.provider, "private");
    }

    #[derive(Clone, Default)]
    struct DeepSeekCacheFake {
        prompts: Arc<Mutex<Vec<String>>>,
    }

    impl Respond for DeepSeekCacheFake {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            let body: Value = serde_json::from_slice(&request.body)
                .expect("wiremock request body should be json");
            let stable = body["messages"][0]["content"]
                .as_str()
                .expect("system message should be present");
            let tail = body["messages"][1]["content"]
                .as_str()
                .expect("user message should be present");
            let prompt = format!("{stable}\n{tail}");
            let total_tokens = usize_to_u64(ApproxTokenizer.count(&prompt));

            let hit_tokens = {
                let prompts = self
                    .prompts
                    .lock()
                    .expect("cache fake state should not be poisoned");
                prompts
                    .iter()
                    .map(|prior| longest_prefix_hit_tokens(prior, &prompt))
                    .max()
                    .unwrap_or(0)
            };
            let miss_tokens = total_tokens.saturating_sub(hit_tokens);

            self.prompts
                .lock()
                .expect("cache fake state should not be poisoned")
                .push(prompt);

            ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": { "content": "{\"ok\":true}" }
                }],
                "usage": {
                    "prompt_cache_hit_tokens": hit_tokens,
                    "prompt_cache_miss_tokens": miss_tokens,
                    "completion_tokens": 12
                }
            }))
        }
    }

    fn longest_prefix_hit_tokens(prior: &str, current: &str) -> u64 {
        let mut prefix_len = 0usize;
        let mut prior_chars = prior.chars();
        let mut current_chars = current.chars();

        loop {
            match (prior_chars.next(), current_chars.next()) {
                (Some(left), Some(right)) if left == right => {
                    prefix_len += left.len_utf8();
                }
                _ => break,
            }
        }

        let common = &current[..prefix_len];
        let tokens = ApproxTokenizer.count(common);
        usize_to_u64(tokens - (tokens % BLOCK_TOKENS))
    }

    #[tokio::test]
    async fn deepseek_usage_fields() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(DeepSeekCacheFake::default())
            .mount(&server)
            .await;

        let routes = load_routes_yaml(
            r#"
routes:
  - name: concierge
    pii: false
    max_tokens: 256
    output_budget_bytes: 4096
    providers: [deepseek]
"#,
        )
        .expect("route yaml should parse");
        let router = Router::new(
            routes,
            vec![Box::new(DeepSeekProvider::new(server.uri(), None))],
        );

        let stable = "policy ".repeat(128);
        let first = router
            .complete_route("concierge", &req(&stable, "draft one"))
            .await
            .expect("first deepseek call should succeed");
        let second = router
            .complete_route("concierge", &req(&stable, "draft two"))
            .await
            .expect("second deepseek call should succeed");

        assert_eq!(first.usage.hit_tokens, 0);
        assert!(second.usage.hit_tokens >= 64);
        assert!(second.usage.miss_tokens < first.usage.miss_tokens);
        assert_eq!(second.provider, "deepseek");
    }
}
