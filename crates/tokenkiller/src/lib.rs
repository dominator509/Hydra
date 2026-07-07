//! layer L3 TOKENKILLER core: canonical prompt assembly, output containment,
//! route contracts, cache-ledger math, and the only allowed LLM call path.

pub mod canon;
pub mod contracts;
pub mod ledger;
pub mod nukeguard;
pub mod prefix;
pub mod session;

pub use contracts::{validate as validate_contract, Contract, ContractError};
pub use ledger::{
    hit_ratio, month_to_date_cents, CacheUsage, LedgerError, LedgerRow, LedgerSink, StoreLedgerSink,
};
pub use nukeguard::{repair_tail, Budgets, NukeGuard, Trip, Verdict};
pub use prefix::{
    assemble, debug_assert_stable, ApproxTokenizer, PrefixError, Prompt, Segment, Stability,
    Tokenizer, Transcript, BLOCK_TOKENS,
};
pub use session::{
    Clock, CompletionRequest, CompletionResponse, Contracted, ProviderTag, RouteCfg, Router,
    RouterError, Segments, Session, SystemClock, Tail, TkError,
};
