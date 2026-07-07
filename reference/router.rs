//! reference/router.rs — multi-provider LLM router with structural PII gate (EP-004 M5).
//! Agents NEVER see this crate (TK-1); tokenkiller::Session is the only caller.
//! Deps: reqwest (via fabric egress client), serde, tokio, thiserror.

use serde::Deserialize;
use crate::ledger::CacheUsage; // reference/tokenkiller/ledger.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag { Private, Caching, Cheap, Frontier }

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub stable_prefix: String, // from tokenkiller::prefix — DO NOT mutate here (TK-3)
    pub tail: String,
    pub max_tokens: u32,       // route-bound; unbounded max_tokens is a forbidden move
    pub stream: bool,
}

#[derive(Debug)]
pub struct ProviderResponse {
    pub text_stream: tokio::sync::mpsc::Receiver<String>, // Session wraps this in NukeGuard
    pub usage_rx: tokio::sync::oneshot::Receiver<CacheUsage>, // resolved at stream end
    pub provider: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("tk_pii_route_blocked: route '{0}' is PII but no private provider available")]
    PiiBlocked(String),
    #[error("llm_provider_error: all providers in chain failed; last: {0}")]
    Exhausted(String),
}

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn tags(&self) -> &[Tag];
    async fn complete(&self, req: &ChatRequest) -> Result<ProviderResponse, String>;
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouteCfg {
    pub name: String,            // "concierge" | "bridge_codegen" | "comms_draft" ...
    pub pii: bool,
    pub max_tokens: u32,
    pub output_budget_bytes: usize,
    pub providers: Vec<String>,  // ordered fallback chain by provider name
    #[serde(default)]
    pub tk_exempt: bool,         // SPEC-009: excluded from ratio SLO, never from NukeGuard
}

pub struct Router {
    providers: Vec<Box<dyn LlmProvider>>,
    /// Set by ledger watcher when month-to-date >= constitution cap (SPEC-009 TK9):
    /// degrade every route to the first Private provider (self-hosted) until month reset.
    pub degrade_to_private: std::sync::atomic::AtomicBool,
}

impl Router {
    /// The PII gate is STRUCTURAL: order of checks is normative (INV-4).
    /// 1. filter chain to providers that exist
    /// 2. if pii ⇒ retain ONLY Tag::Private (an empty result is an ERROR, never a fallback)
    /// 3. if degraded ⇒ move Private providers to front
    /// 4. walk chain; first success wins; carry last error
    pub async fn complete(&self, route: &RouteCfg, req: &ChatRequest)
        -> Result<ProviderResponse, RouterError>
    {
        let mut chain: Vec<&dyn LlmProvider> = route.providers.iter()
            .filter_map(|n| self.providers.iter().find(|p| p.name() == n).map(|b| b.as_ref()))
            .collect();

        if route.pii {
            chain.retain(|p| p.tags().contains(&Tag::Private));
            if chain.is_empty() {
                return Err(RouterError::PiiBlocked(route.name.clone())); // hard stop — no silent downgrade
            }
        }
        if self.degrade_to_private.load(std::sync::atomic::Ordering::Relaxed) {
            chain.sort_by_key(|p| !p.tags().contains(&Tag::Private)); // private first, stable
        }

        let mut last = String::from("empty chain");
        for p in chain {
            match p.complete(req).await {
                Ok(r) => return Ok(r),
                Err(e) => { tracing::warn!(provider = p.name(), route = %route.name, "provider failed: {e}"); last = e; }
            }
        }
        Err(RouterError::Exhausted(last))
    }
}

// ---- DeepSeek provider sketch: the two lines that pay for this whole subsystem ----
// After the SSE stream ends, the final chunk's `usage` object carries the cache split:
//
//   let usage = CacheUsage::from_deepseek_usage(&final_chunk["usage"]);
//   let _ = usage_tx.send(usage);   // Session joins this with prefix_sha into the LedgerRow
//
// Request body notes: messages = [system: stable_prefix, user: tail] — the prefix is ONE
// system message whose bytes never vary per (route, tenant, segment-versions). max_tokens
// from RouteCfg. stream: true always (NukeGuard needs deltas).

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake(&'static str, &'static [Tag], bool);
    #[async_trait::async_trait]
    impl LlmProvider for Fake {
        fn name(&self) -> &'static str { self.0 }
        fn tags(&self) -> &[Tag] { self.1 }
        async fn complete(&self, _r: &ChatRequest) -> Result<ProviderResponse, String> {
            if self.2 { Err("boom".into()) } else {
                let (_t, rx) = tokio::sync::mpsc::channel(1);
                let (utx, urx) = tokio::sync::oneshot::channel();
                let _ = utx.send(CacheUsage::default());
                Ok(ProviderResponse { text_stream: rx, usage_rx: urx, provider: self.0 })
            }
        }
    }
    fn router() -> Router {
        Router { providers: vec![
            Box::new(Fake("deepseek", &[Tag::Caching, Tag::Cheap], false)),
            Box::new(Fake("llama", &[Tag::Private], false)),
        ], degrade_to_private: false.into() }
    }
    fn req() -> ChatRequest { ChatRequest { stable_prefix: "s".into(), tail: "t".into(), max_tokens: 256, stream: true } }

    #[tokio::test]
    async fn pii_gate_blocks_when_no_private_in_chain() {
        let r = router();
        let route = RouteCfg { name: "x".into(), pii: true, max_tokens: 256,
            output_budget_bytes: 16384, providers: vec!["deepseek".into()], tk_exempt: false };
        assert!(matches!(r.complete(&route, &req()).await, Err(RouterError::PiiBlocked(_))));
    }

    #[tokio::test]
    async fn pii_retains_private_only() {
        let r = router();
        let route = RouteCfg { name: "x".into(), pii: true, max_tokens: 256,
            output_budget_bytes: 16384, providers: vec!["deepseek".into(), "llama".into()], tk_exempt: false };
        let resp = r.complete(&route, &req()).await.unwrap();
        assert_eq!(resp.provider, "llama");
    }

    #[tokio::test]
    async fn fallback_chain_walks_on_failure() {
        let r = Router { providers: vec![
            Box::new(Fake("a", &[Tag::Cheap], true)),
            Box::new(Fake("b", &[Tag::Cheap], false)),
        ], degrade_to_private: false.into() };
        let route = RouteCfg { name: "x".into(), pii: false, max_tokens: 1,
            output_budget_bytes: 1, providers: vec!["a".into(), "b".into()], tk_exempt: false };
        assert_eq!(r.complete(&route, &req()).await.unwrap().provider, "b");
    }
}
