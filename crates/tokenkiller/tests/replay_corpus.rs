use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use llm_router::providers::deepseek::DeepSeekProvider;
use llm_router::Router as LlmRouter;
use serde::Deserialize;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tokenkiller::{
    ApproxTokenizer, Clock, Contract, LedgerError, LedgerRow, LedgerSink, ProviderTag, RouteCfg,
    Segment, Session, Stability, Tokenizer, Transcript, BLOCK_TOKENS,
};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[derive(Debug, Deserialize)]
struct CorpusFixture {
    route: String,
    contract: String,
    max_tokens: u32,
    output_budget_bytes: usize,
    response: String,
    segments: Vec<SegmentFixture>,
    turns: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SegmentFixture {
    stability: String,
    version: u32,
    text: String,
    repeat: usize,
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

struct ArcLedger(Arc<MemoryLedger>);

#[async_trait]
impl LedgerSink for ArcLedger {
    async fn record(&self, row: &LedgerRow) -> Result<(), LedgerError> {
        self.0.record(row).await
    }
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }
}

#[derive(Clone)]
struct DeepSeekCacheFake {
    prompts: Arc<Mutex<Vec<String>>>,
    response_body: Arc<String>,
}

impl DeepSeekCacheFake {
    fn new(response_body: String) -> Self {
        Self {
            prompts: Arc::new(Mutex::new(Vec::new())),
            response_body: Arc::new(response_body),
        }
    }
}

impl Respond for DeepSeekCacheFake {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value =
            serde_json::from_slice(&request.body).expect("wiremock request body should be json");
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
                "message": { "content": self.response_body.as_ref() }
            }],
            "usage": {
                "prompt_cache_hit_tokens": hit_tokens,
                "prompt_cache_miss_tokens": miss_tokens,
                "completion_tokens": 12
            }
        }))
    }
}

struct RouteStats {
    hit_tokens: u64,
    miss_tokens: u64,
}

impl CorpusFixture {
    fn contract(&self) -> Contract {
        match self.contract.as_str() {
            "PlainAnswer" => Contract::PlainAnswer,
            "UnifiedDiff" => Contract::UnifiedDiff,
            "MappingYaml" => Contract::MappingYaml,
            "EnvelopeProposal" => Contract::EnvelopeProposal,
            other => panic!("unknown corpus contract '{other}'"),
        }
    }

    fn segments(&self) -> Vec<Segment> {
        self.segments
            .iter()
            .map(|segment| Segment {
                stability: match segment.stability.as_str() {
                    "S0" => Stability::S0,
                    "S1" => Stability::S1,
                    "S2" => Stability::S2,
                    "S3" => Stability::S3,
                    other => panic!("unknown segment stability '{other}'"),
                },
                text: segment.text.repeat(segment.repeat),
                version: segment.version,
            })
            .collect()
    }

    fn session_route_cfg(&self) -> RouteCfg {
        RouteCfg {
            provider: "deepseek".into(),
            provider_tags: Vec::<ProviderTag>::new(),
            max_tokens: self.max_tokens,
            output_budget_bytes: self.output_budget_bytes,
            contract: self.contract(),
            pii: false,
        }
    }

    fn router_routes_yaml(&self) -> String {
        format!(
            "routes:\n  - name: {}\n    pii: false\n    max_tokens: {}\n    output_budget_bytes: {}\n    providers: [deepseek]\n",
            self.route, self.max_tokens, self.output_budget_bytes
        )
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

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).expect("usize to u64 conversion is infallible on supported targets")
}

async fn run_fixture(fixture: CorpusFixture) -> Result<RouteStats, Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(DeepSeekCacheFake::new(fixture.response.clone()))
        .mount(&server)
        .await;

    let router = LlmRouter::from_yaml_str(
        &fixture.router_routes_yaml(),
        vec![Box::new(DeepSeekProvider::new(server.uri(), None))],
    )?;
    let ledger = Arc::new(MemoryLedger::default());
    let session = Session::new(
        Uuid::nil(),
        HashMap::from([(fixture.route.clone(), fixture.session_route_cfg())]),
        Box::new(router),
        Box::new(ArcLedger(ledger.clone())),
        Box::new(ApproxTokenizer),
        Box::new(FixedClock),
    );

    let segments = fixture.segments();
    let mut transcript = Transcript::default();

    for (index, turn) in fixture.turns.iter().enumerate() {
        transcript.push(format!("user: {turn}"));
        let result = session
            .complete(&fixture.route, segments.clone(), transcript.render_full())
            .await?;
        assert!(
            !result.repaired,
            "corpus replay should stay inside contracts/budgets on the first try",
        );
        assert!(
            !result.ledger_row.aborted,
            "corpus replay must not produce nuked outputs",
        );
        transcript.push(format!("assistant: {}", result.raw));

        if index == 0 {
            continue;
        }

        println!(
            "tk-corpus route={} call={} prefix_sha={} hit={} miss={}",
            fixture.route,
            index + 1,
            result.ledger_row.prefix_sha,
            result.ledger_row.usage.hit_tokens,
            result.ledger_row.usage.miss_tokens
        );
    }

    let rows = ledger.rows();
    assert_eq!(
        rows.len(),
        fixture.turns.len(),
        "every corpus turn, including the warm-up call, should record a ledger row",
    );
    let measured = &rows[1..];
    let first_prefix = &measured[0].prefix_sha;
    assert!(
        measured.iter().all(|row| row.prefix_sha == *first_prefix),
        "stable S0-S2 bytes must keep a single prefix_sha across the measured corpus",
    );

    Ok(RouteStats {
        hit_tokens: measured.iter().map(|row| row.usage.hit_tokens).sum(),
        miss_tokens: measured.iter().map(|row| row.usage.miss_tokens).sum(),
    })
}

#[tokio::test]
async fn replay_corpus() -> Result<(), Box<dyn std::error::Error>> {
    let fixtures = [
        include_str!("../../../tests/fixtures/tk-corpus/concierge.json"),
        include_str!("../../../tests/fixtures/tk-corpus/bridge_codegen.json"),
        include_str!("../../../tests/fixtures/tk-corpus/bridge_mapping.json"),
    ];

    let mut hit_tokens = 0u64;
    let mut miss_tokens = 0u64;

    for fixture in fixtures {
        let stats = run_fixture(serde_json::from_str::<CorpusFixture>(fixture)?).await?;
        hit_tokens += stats.hit_tokens;
        miss_tokens += stats.miss_tokens;
    }

    let ratio = hit_tokens as f64 / (hit_tokens + miss_tokens) as f64;
    println!("tk-corpus ratio: {:.4}", ratio);

    assert!(ratio >= 0.97, "corpus ratio should meet the TK target");
    Ok(())
}
