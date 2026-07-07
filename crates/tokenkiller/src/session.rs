use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::contracts::{validate, Contract, ContractError};
use crate::ledger::{CacheUsage, LedgerError, LedgerRow, LedgerSink};
use crate::nukeguard::{repair_tail, Budgets, NukeGuard, Trip, Verdict};
use crate::prefix::{assemble, debug_assert_stable, PrefixError, Prompt, Segment, Tokenizer};

pub type Segments = Vec<Segment>;
pub type Tail = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTag {
    Private,
}

#[derive(Debug, Clone)]
pub struct RouteCfg {
    pub provider: String,
    pub provider_tags: Vec<ProviderTag>,
    pub max_tokens: u32,
    pub output_budget_bytes: usize,
    pub contract: Contract,
    pub pii: bool,
}

impl RouteCfg {
    fn budgets(&self) -> Budgets {
        Budgets {
            max_bytes: self.output_budget_bytes,
            ..Budgets::default()
        }
    }

    fn is_private(&self) -> bool {
        self.provider_tags.contains(&ProviderTag::Private)
    }
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub route: String,
    pub provider: String,
    pub prompt: Prompt,
    pub max_tokens: u32,
    pub pii: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionResponse {
    pub provider: String,
    pub chunks: Vec<String>,
    pub usage: CacheUsage,
    pub out_tokens: u64,
    pub cost_cents: u32,
}

#[derive(Debug, thiserror::Error)]
#[error("llm provider error: {message}")]
pub struct RouterError {
    pub message: String,
}

#[async_trait]
pub trait Router: Send + Sync {
    async fn complete(&self, request: CompletionRequest)
        -> Result<CompletionResponse, RouterError>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[derive(Debug, Clone)]
pub struct Contracted {
    pub value: Value,
    pub raw: String,
    pub repaired: bool,
    pub prompt: Prompt,
    pub ledger_row: LedgerRow,
}

#[derive(Debug, thiserror::Error)]
pub enum TkError {
    #[error("unknown route '{0}'")]
    UnknownRoute(String),
    #[error("route '{route}' cannot send pii to non-private provider '{provider}'")]
    PiiRouteBlocked { route: String, provider: String },
    #[error("router call failed: {0}")]
    Router(#[from] RouterError),
    #[error("prefix assembly failed: {0}")]
    Prefix(#[from] PrefixError),
    #[error("ledger write failed: {0}")]
    Ledger(#[from] LedgerError),
    #[error("contract validation failed: {0}")]
    Contract(#[from] ContractError),
    #[error("tk_output_nuked: {0:?}")]
    OutputNuked(Trip),
}

pub struct Session {
    tenant_id: Uuid,
    routes: HashMap<String, RouteCfg>,
    router: Box<dyn Router>,
    ledger: Box<dyn LedgerSink>,
    tokenizer: Box<dyn Tokenizer>,
    clock: Box<dyn Clock>,
}

impl Session {
    pub fn new(
        tenant_id: Uuid,
        routes: HashMap<String, RouteCfg>,
        router: Box<dyn Router>,
        ledger: Box<dyn LedgerSink>,
        tokenizer: Box<dyn Tokenizer>,
        clock: Box<dyn Clock>,
    ) -> Self {
        Self {
            tenant_id,
            routes,
            router,
            ledger,
            tokenizer,
            clock,
        }
    }

    pub async fn complete(
        &self,
        route: &str,
        segments: Segments,
        tail: Tail,
    ) -> Result<Contracted, TkError> {
        let cfg = self
            .routes
            .get(route)
            .ok_or_else(|| TkError::UnknownRoute(route.to_owned()))?;

        if cfg.pii && !cfg.is_private() {
            return Err(TkError::PiiRouteBlocked {
                route: route.to_owned(),
                provider: cfg.provider.clone(),
            });
        }

        debug_assert_stable(&segments, &tail, self.tokenizer.as_ref());

        let mut current_tail = tail;
        let mut repaired = false;

        for attempt in 0..=1 {
            let prompt = assemble(&segments, &current_tail, self.tokenizer.as_ref())?;
            let response = self
                .router
                .complete(CompletionRequest {
                    route: route.to_owned(),
                    provider: cfg.provider.clone(),
                    prompt: prompt.clone(),
                    max_tokens: cfg.max_tokens,
                    pii: cfg.pii,
                })
                .await?;

            let mut guard = NukeGuard::new(cfg.budgets());
            let mut raw = String::new();
            let mut tripped = None;

            for chunk in &response.chunks {
                match guard.feed(chunk.as_bytes()) {
                    Verdict::Continue => raw.push_str(chunk),
                    Verdict::Abort(trip) => {
                        tripped = Some(trip);
                        break;
                    }
                }
            }

            if let Some(trip) = tripped {
                let row = self.build_row(
                    route,
                    cfg,
                    &prompt,
                    &response,
                    usize_to_u64(guard.bytes_seen()),
                    true,
                )?;
                self.ledger.record(&row).await?;

                if attempt == 0 {
                    repaired = true;
                    current_tail =
                        append_repair_tail(current_tail, cfg.contract, cfg.output_budget_bytes);
                    continue;
                }

                return Err(TkError::OutputNuked(trip));
            }

            let value = match validate(cfg.contract, &raw) {
                Ok(value) => value,
                Err(error) => {
                    let row = self.build_row(
                        route,
                        cfg,
                        &prompt,
                        &response,
                        usize_to_u64(raw.len()),
                        false,
                    )?;
                    self.ledger.record(&row).await?;

                    if attempt == 0 {
                        repaired = true;
                        current_tail =
                            append_repair_tail(current_tail, cfg.contract, cfg.output_budget_bytes);
                        continue;
                    }

                    return Err(TkError::Contract(error));
                }
            };

            let row = self.build_row(
                route,
                cfg,
                &prompt,
                &response,
                usize_to_u64(raw.len()),
                false,
            )?;
            self.ledger.record(&row).await?;

            return Ok(Contracted {
                value,
                raw,
                repaired,
                prompt,
                ledger_row: row,
            });
        }

        Err(TkError::OutputNuked(Trip::Bytes))
    }

    fn build_row(
        &self,
        route: &str,
        cfg: &RouteCfg,
        prompt: &Prompt,
        response: &CompletionResponse,
        out_bytes: u64,
        aborted: bool,
    ) -> Result<LedgerRow, TkError> {
        Ok(LedgerRow {
            ts: self.clock.now(),
            tenant_id: self.tenant_id,
            route: route.to_owned(),
            provider: if response.provider.is_empty() {
                cfg.provider.clone()
            } else {
                response.provider.clone()
            },
            prefix_sha: prefix_sha_hex(&prompt.prefix_sha),
            usage: response.usage,
            out_tokens: response.out_tokens,
            out_bytes,
            aborted,
            cost_cents: response.cost_cents,
        })
    }
}

fn append_repair_tail(tail: String, contract: Contract, max_bytes: usize) -> String {
    let repair = repair_tail(contract.summary(), max_bytes);
    if tail.is_empty() {
        repair
    } else {
        format!("{tail}\n{repair}")
    }
}

fn prefix_sha_hex(prefix_sha: &[u8; 32]) -> String {
    let mut out = String::with_capacity(prefix_sha.len() * 2);
    for byte in prefix_sha {
        out.push(nibble_to_hex(byte >> 4));
        out.push(nibble_to_hex(byte & 0x0f));
    }
    out
}

fn nibble_to_hex(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        10..=15 => char::from(b'a' + (nibble - 10)),
        _ => unreachable!("nibbles are always <= 15"),
    }
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).expect("usize to u64 conversion is infallible on supported targets")
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;
    use crate::contracts::Contract;
    use crate::ledger::CacheUsage;
    use crate::prefix::{ApproxTokenizer, Segment, Stability};

    struct FixedClock(OffsetDateTime);

    impl Clock for FixedClock {
        fn now(&self) -> OffsetDateTime {
            self.0
        }
    }

    #[derive(Default)]
    struct MemoryLedger {
        rows: Mutex<Vec<LedgerRow>>,
    }

    impl MemoryLedger {
        fn rows(&self) -> Vec<LedgerRow> {
            self.rows
                .lock()
                .expect("memory ledger lock should not be poisoned")
                .clone()
        }
    }

    #[async_trait]
    impl LedgerSink for MemoryLedger {
        async fn record(&self, row: &LedgerRow) -> Result<(), LedgerError> {
            self.rows
                .lock()
                .expect("memory ledger lock should not be poisoned")
                .push(row.clone());
            Ok(())
        }
    }

    struct FakeRouter {
        responses: Mutex<VecDeque<Result<CompletionResponse, RouterError>>>,
        requests: Mutex<Vec<CompletionRequest>>,
    }

    impl FakeRouter {
        fn new(responses: Vec<Result<CompletionResponse, RouterError>>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<CompletionRequest> {
            self.requests
                .lock()
                .expect("router request log should not be poisoned")
                .clone()
        }
    }

    #[async_trait]
    impl Router for FakeRouter {
        async fn complete(
            &self,
            request: CompletionRequest,
        ) -> Result<CompletionResponse, RouterError> {
            self.requests
                .lock()
                .expect("router request log should not be poisoned")
                .push(request);

            self.responses
                .lock()
                .expect("router response queue should not be poisoned")
                .pop_front()
                .expect("test router should have a queued response")
        }
    }

    fn route_cfg(provider_private: bool, pii: bool, output_budget_bytes: usize) -> RouteCfg {
        RouteCfg {
            provider: "deepseek".into(),
            provider_tags: if provider_private {
                vec![ProviderTag::Private]
            } else {
                Vec::new()
            },
            max_tokens: 256,
            output_budget_bytes,
            contract: Contract::EnvelopeProposal,
            pii,
        }
    }

    fn session(
        router: FakeRouter,
        ledger: MemoryLedger,
        cfg: RouteCfg,
    ) -> (
        Session,
        std::sync::Arc<MemoryLedger>,
        std::sync::Arc<FakeRouter>,
    ) {
        let ledger = std::sync::Arc::new(ledger);
        let router = std::sync::Arc::new(router);
        let routes = HashMap::from([("concierge".to_owned(), cfg)]);
        let session = Session::new(
            Uuid::nil(),
            routes,
            Box::new(ArcRouter(router.clone())),
            Box::new(ArcLedger(ledger.clone())),
            Box::new(ApproxTokenizer),
            Box::new(FixedClock(OffsetDateTime::UNIX_EPOCH)),
        );
        (session, ledger, router)
    }

    #[tokio::test]
    async fn tk_session_repairs_after_nuke_once() -> Result<(), Box<dyn std::error::Error>> {
        let first = CompletionResponse {
            provider: "deepseek".into(),
            chunks: vec!["x".repeat(1024)],
            usage: CacheUsage {
                hit_tokens: 10,
                miss_tokens: 5,
            },
            out_tokens: 42,
            cost_cents: 7,
        };
        let second = CompletionResponse {
            provider: "deepseek".into(),
            chunks: vec![r#"{"domain":"pipeline","action":"move_stage","targets":["d1"],"payload":{"stage":"won"},"rationale":"90d idle","reversal":"Compensating","blast":{"entities":1,"external_sends":0,"money_cents":0,"pii_egress":false}}"#.into()],
            usage: CacheUsage {
                hit_tokens: 30,
                miss_tokens: 2,
            },
            out_tokens: 32,
            cost_cents: 3,
        };
        let (session, ledger, router) = session(
            FakeRouter::new(vec![Ok(first), Ok(second)]),
            MemoryLedger::default(),
            route_cfg(true, false, 256),
        );

        let result = session
            .complete(
                "concierge",
                vec![Segment {
                    stability: Stability::S0,
                    text: "You are HYDRA.".into(),
                    version: 1,
                }],
                "task: propose a move".into(),
            )
            .await?;

        assert!(
            result.repaired,
            "nuked output should trigger one repair retry"
        );
        assert_eq!(result.value["domain"], "pipeline");

        let rows = ledger.rows();
        assert_eq!(rows.len(), 2, "both attempts should land in the ledger");
        assert!(rows[0].aborted, "first row should record the nuke");
        assert!(!rows[1].aborted, "repair attempt should succeed cleanly");

        let requests = router.requests();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[1]
                .prompt
                .tail_bytes
                .contains("SYSTEM REPAIR NOTICE"),
            "repair attempt should append the repair tail"
        );

        Ok(())
    }

    #[tokio::test]
    async fn tk_session_repairs_contract_violation_once() -> Result<(), Box<dyn std::error::Error>>
    {
        let first = CompletionResponse {
            provider: "deepseek".into(),
            chunks: vec!["not json".into()],
            usage: CacheUsage {
                hit_tokens: 1,
                miss_tokens: 4,
            },
            out_tokens: 8,
            cost_cents: 1,
        };
        let second = CompletionResponse {
            provider: "deepseek".into(),
            chunks: vec![r#"{"domain":"pipeline","action":"move_stage","targets":["d1"],"payload":{"stage":"won"},"rationale":"90d idle","reversal":"Compensating","blast":{"entities":1,"external_sends":0,"money_cents":0,"pii_egress":false}}"#.into()],
            usage: CacheUsage {
                hit_tokens: 2,
                miss_tokens: 2,
            },
            out_tokens: 16,
            cost_cents: 2,
        };
        let (session, ledger, _) = session(
            FakeRouter::new(vec![Ok(first), Ok(second)]),
            MemoryLedger::default(),
            route_cfg(true, false, 2048),
        );

        let result = session
            .complete(
                "concierge",
                vec![Segment {
                    stability: Stability::S0,
                    text: "You are HYDRA.".into(),
                    version: 1,
                }],
                "task: propose a move".into(),
            )
            .await?;

        assert!(
            result.repaired,
            "contract violation should use the repair-once path"
        );
        assert_eq!(ledger.rows().len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn tk_session_blocks_pii_on_non_private_provider() {
        let (session, ledger, router) = session(
            FakeRouter::new(Vec::new()),
            MemoryLedger::default(),
            route_cfg(false, true, 2048),
        );

        let error = session
            .complete(
                "concierge",
                vec![Segment {
                    stability: Stability::S0,
                    text: "You are HYDRA.".into(),
                    version: 1,
                }],
                "task: pii".into(),
            )
            .await
            .expect_err("pii route should be blocked");

        assert!(matches!(error, TkError::PiiRouteBlocked { .. }));
        assert!(
            ledger.rows().is_empty(),
            "blocked routes should not write ledger rows"
        );
        assert!(
            router.requests().is_empty(),
            "blocked routes should not call the router"
        );
    }

    struct ArcLedger(std::sync::Arc<MemoryLedger>);

    #[async_trait]
    impl LedgerSink for ArcLedger {
        async fn record(&self, row: &LedgerRow) -> Result<(), LedgerError> {
            self.0.record(row).await
        }
    }

    struct ArcRouter(std::sync::Arc<FakeRouter>);

    #[async_trait]
    impl Router for ArcRouter {
        async fn complete(
            &self,
            request: CompletionRequest,
        ) -> Result<CompletionResponse, RouterError> {
            self.0.complete(request).await
        }
    }
}
