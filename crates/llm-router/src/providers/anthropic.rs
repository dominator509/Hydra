use async_trait::async_trait;

use crate::{
    extract_anthropic_text, non_caching_usage, normalize_base_url, output_tokens, ChatRequest,
    JsonHttpClient, LlmProvider, Pricing, ProviderResponse, Tag,
};

const TAGS: [Tag; 1] = [Tag::Frontier];
const PRICING: Pricing = Pricing::new(8, 16, 24);

pub struct AnthropicProvider {
    http: JsonHttpClient,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl AnthropicProvider {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            http: JsonHttpClient::new(),
            base_url: base_url.into(),
            api_key,
            model: "claude-3-5-sonnet".into(),
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn tags(&self) -> &[Tag] {
        &TAGS
    }

    async fn complete(&self, req: &ChatRequest) -> Result<ProviderResponse, String> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "system": req.stable_prefix,
            "messages": [{ "role": "user", "content": req.tail }]
        });
        let response = self
            .http
            .post_json(
                &normalize_base_url(&self.base_url, "/v1/messages"),
                self.api_key.as_deref(),
                &body,
            )
            .await?;
        let text = extract_anthropic_text(&response)?;
        let usage = non_caching_usage(&response, req, "input_tokens");
        let out_tokens = output_tokens(&response, &text, "output_tokens");
        let cost_cents = PRICING.estimate_cents(usage.hit_tokens, usage.miss_tokens, out_tokens);

        Ok(ProviderResponse {
            chunks: vec![text],
            usage,
            out_tokens,
            cost_cents,
            provider: self.name(),
        })
    }
}
