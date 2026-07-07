use async_trait::async_trait;

use crate::{
    extract_choice_text, non_caching_usage, normalize_base_url, output_tokens, ChatRequest,
    JsonHttpClient, LlmProvider, Pricing, ProviderResponse, Tag,
};

const PRICING: Pricing = Pricing::new(4, 8, 12);

pub struct OpenAiCompatProvider {
    http: JsonHttpClient,
    name: &'static str,
    base_url: String,
    api_key: Option<String>,
    model: String,
    tags: Vec<Tag>,
}

impl OpenAiCompatProvider {
    pub fn new(
        name: &'static str,
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
        tags: Vec<Tag>,
    ) -> Self {
        Self {
            http: JsonHttpClient::new(),
            name,
            base_url: base_url.into(),
            api_key,
            model: model.into(),
            tags,
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    fn tags(&self) -> &[Tag] {
        &self.tags
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
        let usage = non_caching_usage(&response, req, "prompt_tokens");
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
