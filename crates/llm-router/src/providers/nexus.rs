use async_trait::async_trait;

use crate::{
    extract_choice_text, non_caching_usage, normalize_base_url, output_tokens, provider_provenance,
    ChatRequest, JsonHttpClient, LlmProvider, Pricing, ProviderResponse, Tag,
};

const PRICING: Pricing = Pricing::new(4, 8, 12);

/// OpenAI-compatible adapter for the optional Nexus Model Gateway.
///
/// This type is only a router provider. TOKENKILLER remains responsible for
/// prompt assembly, output containment, contract validation, retries, and the
/// ledger call path.
pub struct NexusModelProvider {
    http: JsonHttpClient,
    base_url: String,
    token: Option<String>,
    model: String,
    tags: Vec<Tag>,
}

impl NexusModelProvider {
    pub fn new(
        base_url: impl Into<String>,
        token: Option<String>,
        model: impl Into<String>,
        private: bool,
    ) -> Self {
        Self {
            http: JsonHttpClient::new(),
            base_url: base_url.into(),
            token,
            model: model.into(),
            tags: if private {
                vec![Tag::Private]
            } else {
                Vec::new()
            },
        }
    }

    pub fn new_with_proxy(
        base_url: impl Into<String>,
        token: Option<String>,
        model: impl Into<String>,
        private: bool,
        proxy_url: Option<&str>,
    ) -> Result<Self, String> {
        Ok(Self {
            http: JsonHttpClient::new_with_proxy(proxy_url)?,
            base_url: base_url.into(),
            token,
            model: model.into(),
            tags: if private {
                vec![Tag::Private]
            } else {
                Vec::new()
            },
        })
    }
}

#[async_trait]
impl LlmProvider for NexusModelProvider {
    fn name(&self) -> &'static str {
        "nexus"
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
                self.token.as_deref(),
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
            provenance: provider_provenance(
                self.name(),
                &self.model,
                "nexus-model-gateway",
                if self.tags.contains(&Tag::Private) {
                    tokenkiller::ProviderPrivacy::Private
                } else {
                    tokenkiller::ProviderPrivacy::Unknown
                },
                req,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use tokenkiller::{
        ApproxTokenizer, Contract, LedgerError, LedgerRow, LedgerSink, ProviderTag, RouteCfg,
        Segment, Session, Stability,
    };
    use uuid::Uuid;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[derive(Default)]
    struct MemoryLedger(Mutex<Vec<LedgerRow>>);

    #[async_trait]
    impl LedgerSink for MemoryLedger {
        async fn record(&self, row: &LedgerRow) -> Result<(), LedgerError> {
            self.0
                .lock()
                .expect("test ledger lock should not be poisoned")
                .push(row.clone());
            Ok(())
        }
    }

    fn response() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "gateway answer"}}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 4}
        }))
    }

    #[tokio::test]
    async fn nexus_provider_contract_and_provenance() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer gateway-token"))
            .respond_with(response())
            .mount(&server)
            .await;

        let provider = NexusModelProvider::new(
            server.uri(),
            Some("gateway-token".to_owned()),
            "nexus-model",
            true,
        );
        let request = ChatRequest {
            stable_prefix: "stable".to_owned(),
            tail: "tail".to_owned(),
            max_tokens: 37,
            output_budget_bytes: 4096,
            stream: true,
        };
        let result = provider
            .complete(&request)
            .await
            .expect("Nexus gateway response");

        assert_eq!(result.provider, "nexus");
        assert_eq!(result.chunks, vec!["gateway answer"]);
        assert_eq!(result.provenance.provider, "nexus");
        assert_eq!(result.provenance.model, "nexus-model");
        assert_eq!(result.provenance.gateway, "nexus-model-gateway");
        assert_eq!(
            result.provenance.privacy,
            tokenkiller::ProviderPrivacy::Private
        );
        assert_eq!(result.provenance.requested_max_tokens, 37);
        assert_eq!(result.provenance.output_budget_bytes, 4096);
    }

    #[tokio::test]
    async fn nexus_provider_failure_redacts_upstream_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_string("secret prompt and bearer gateway-token"),
            )
            .mount(&server)
            .await;

        let provider = NexusModelProvider::new(
            server.uri(),
            Some("gateway-token".to_owned()),
            "nexus-model",
            false,
        );
        let request = ChatRequest {
            stable_prefix: "stable".to_owned(),
            tail: "tail".to_owned(),
            max_tokens: 8,
            output_budget_bytes: 128,
            stream: false,
        };
        let error = provider
            .complete(&request)
            .await
            .expect_err("failed gateway response");

        assert!(error.contains("upstream status 500"));
        assert!(!error.contains("secret prompt"));
        assert!(!error.contains("gateway-token"));
    }

    #[tokio::test]
    async fn nexus_provider_tokenkiller_owns_call_and_receives_provenance() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(response())
            .mount(&server)
            .await;

        let router = crate::Router::new(
            std::collections::HashMap::from([(
                "nexus".to_owned(),
                crate::RouteCfg {
                    name: "nexus".to_owned(),
                    pii: true,
                    max_tokens: 64,
                    output_budget_bytes: 512,
                    providers: vec!["nexus".to_owned()],
                    tk_exempt: false,
                },
            )]),
            vec![Box::new(NexusModelProvider::new(
                server.uri(),
                Some("gateway-token".to_owned()),
                "nexus-model",
                true,
            ))],
        );
        let routes = std::collections::HashMap::from([(
            "nexus".to_owned(),
            RouteCfg {
                provider: "nexus".to_owned(),
                provider_tags: vec![ProviderTag::Private],
                max_tokens: 64,
                output_budget_bytes: 512,
                contract: Contract::PlainAnswer,
                pii: true,
            },
        )]);
        let session = Session::new(
            Uuid::nil(),
            routes,
            Box::new(router),
            Box::new(MemoryLedger::default()),
            Box::new(ApproxTokenizer),
            Box::new(tokenkiller::SystemClock),
        );

        let contracted = session
            .complete(
                "nexus",
                vec![Segment {
                    stability: Stability::S0,
                    text: "private system context".to_owned(),
                    version: 1,
                }],
                "private question".to_owned(),
            )
            .await
            .expect("TOKENKILLER should accept the governed response");

        assert_eq!(contracted.raw, "gateway answer");
        assert_eq!(contracted.provenance.provider, "nexus");
        assert_eq!(
            contracted.provenance.privacy,
            tokenkiller::ProviderPrivacy::Private
        );
        assert_eq!(contracted.provenance.output_budget_bytes, 512);
    }
}
