use async_trait::async_trait;
use tokenkiller::CacheUsage;

use crate::{
    extract_choice_text, normalize_base_url, output_tokens, ChatRequest, JsonHttpClient,
    LlmProvider, Pricing, ProviderResponse, Tag,
};

const TAGS: [Tag; 2] = [Tag::Caching, Tag::Cheap];
const PRICING: Pricing = Pricing::new(1, 4, 8);

pub struct DeepSeekProvider {
    http: JsonHttpClient,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl DeepSeekProvider {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            http: JsonHttpClient::new(),
            base_url: base_url.into(),
            api_key,
            model: "deepseek-chat".into(),
        }
    }
}

#[async_trait]
impl LlmProvider for DeepSeekProvider {
    fn name(&self) -> &'static str {
        "deepseek"
    }

    fn tags(&self) -> &[Tag] {
        &TAGS
    }

    async fn complete(&self, req: &ChatRequest) -> Result<ProviderResponse, String> {
        let body = JsonHttpClient::json_body(
            &req.stable_prefix,
            &req.tail,
            req.max_tokens,
            req.stream,
            &self.model,
        );
        let response = self
            .http
            .post_json(
                &normalize_base_url(&self.base_url, "/chat/completions"),
                self.api_key.as_deref(),
                &body,
            )
            .await?;
        let text = extract_choice_text(&response)?;
        let usage = CacheUsage::from_deepseek_usage(&response["usage"]);
        let out_tokens = output_tokens(&response, &text, "completion_tokens");
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
