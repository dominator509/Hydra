# reference/ — Copy-Adapt Implementations for the Hardest Tasks

STATUS: INFORMATIVE. Specs in .agent/specs/ are NORMATIVE. When they disagree, the spec wins and the delta goes in the active ExecPlan Decision Log.

Purpose: these files exist because lower-tier coding agents reliably fail at exactly these spots — component-model host plumbing, deterministic policy machines, cache-stable serialization, streaming abort logic. Do not re-derive them from memory (AGENTS.md §6). Copy the file into the target crate, adjust imports/paths, keep the tests.

| File | Adapted into | ExecPlan |
|---|---|---|
| governor.rs | crates/governor | EP-002 |
| router.rs | crates/llm-router | EP-004 M5 |
| bridge/hydra-bridge.wit | wit/hydra-bridge.wit (verbatim) | EP-004 M1 |
| bridge/host.rs | crates/bridge-host | EP-004 M2 |
| bridge/conformance.rs | crates/bridge-host/tests | EP-004 M3 |
| tokenkiller/canon.rs | crates/tokenkiller | EP-004 M4 |
| tokenkiller/prefix.rs | crates/tokenkiller | EP-004 M4 |
| tokenkiller/nukeguard.rs | crates/tokenkiller | EP-004 M4 |
| tokenkiller/contracts.rs | crates/tokenkiller | EP-004 M4 |
| tokenkiller/ledger.rs | crates/tokenkiller + crates/store | EP-003 M4 / EP-004 M4 |

Dependency notes (add via AGENTS §8 process): governor→{serde,serde_json,thiserror,uuid,time}; tokenkiller→{serde_json,sha2,ryu,unicode-normalization,thiserror}; host→{wasmtime(component-model,async),anyhow,tokio}; router→{reqwest,serde,tokio,thiserror}.
