use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use bridge_wit::adapter;
use bridge_wit::export_adapter;
use bridge_wit::types::{
    BridgeError, Capabilities, Change, ChangeOp, ChangePage, Descriptor, FieldSchema, Page,
    RawRecord,
};
use serde::Deserialize;
use serde_json::Value;

const DEFAULT_KIND: &str = "party";
const DESCRIPTOR_NAME: &str = "memcrm";
const DESCRIPTOR_VERSION: &str = "0.1.0";
const RATE_LIMIT_RETRY_AFTER_SECS: u32 = 2;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct ProbeConfig {
    kinds: Vec<String>,
    rate_limit_every: u32,
    seed: BTreeMap<String, Vec<SeedRecord>>,
}

#[derive(Debug, Clone, Deserialize)]
struct SeedRecord {
    id: String,
    #[serde(default)]
    etag: Option<String>,
    data: Value,
}

#[derive(Debug, Clone)]
struct StoredRecord {
    kind: String,
    id: String,
    etag: Option<String>,
    data: String,
}

#[derive(Debug, Clone)]
struct ChangeEntry {
    seq: u64,
    op: ChangeOp,
    rec: RawRecord,
}

#[derive(Debug, Default)]
struct AdapterState {
    config: ProbeConfig,
    records: BTreeMap<String, BTreeMap<String, StoredRecord>>,
    changes: Vec<ChangeEntry>,
    calls: u64,
    next_seq: u64,
    next_etag: u64,
}

struct MemCrm;

static STATE: OnceLock<Mutex<AdapterState>> = OnceLock::new();

fn state() -> &'static Mutex<AdapterState> {
    STATE.get_or_init(|| Mutex::new(AdapterState::default()))
}

fn lock_state() -> Result<MutexGuard<'static, AdapterState>, BridgeError> {
    state()
        .lock()
        .map_err(|_| BridgeError::Upstream("memcrm state poisoned".into()))
}

fn supported_kinds(config: &ProbeConfig) -> Vec<String> {
    if config.kinds.is_empty() {
        vec![DEFAULT_KIND.to_string()]
    } else {
        config.kinds.clone()
    }
}

fn descriptor_for(config: &ProbeConfig) -> Descriptor {
    Descriptor {
        name: DESCRIPTOR_NAME.to_string(),
        version: DESCRIPTOR_VERSION.to_string(),
        kinds: supported_kinds(config),
        caps: Capabilities {
            read: true,
            write: true,
            incremental_sync: true,
            etags: true,
            server_side_query: false,
        },
    }
}

fn ensure_supported_kind(config: &ProbeConfig, kind: &str) -> Result<(), BridgeError> {
    if supported_kinds(config)
        .iter()
        .any(|candidate| candidate == kind)
    {
        Ok(())
    } else {
        Err(BridgeError::Invalid(format!("unsupported kind: {kind}")))
    }
}

fn parse_cursor(cursor: &str) -> Result<u64, BridgeError> {
    if cursor.is_empty() {
        return Ok(0);
    }
    cursor
        .parse::<u64>()
        .map_err(|_| BridgeError::Invalid(format!("invalid cursor: {cursor}")))
}

fn normalize_data(data: &str) -> Result<String, BridgeError> {
    let value: Value = serde_json::from_str(data)
        .map_err(|error| BridgeError::Invalid(format!("invalid json payload: {error}")))?;
    if !value.is_object() {
        return Err(BridgeError::Invalid(
            "record data must be a JSON object".into(),
        ));
    }
    serde_json::to_string(&value)
        .map_err(|error| BridgeError::Invalid(format!("failed to serialize payload: {error}")))
}

fn next_etag(state: &mut AdapterState) -> String {
    state.next_etag += 1;
    format!("etag-{}", state.next_etag)
}

fn maybe_rate_limit(state: &mut AdapterState) -> Result<(), BridgeError> {
    if state.config.rate_limit_every == 0 {
        return Ok(());
    }

    state.calls += 1;
    if state.calls % u64::from(state.config.rate_limit_every) == 0 {
        return Err(BridgeError::RateLimited(RATE_LIMIT_RETRY_AFTER_SECS));
    }

    Ok(())
}

fn push_change(state: &mut AdapterState, op: ChangeOp, rec: RawRecord) {
    state.next_seq += 1;
    state.changes.push(ChangeEntry {
        seq: state.next_seq,
        op,
        rec,
    });
}

fn make_raw_record(record: &StoredRecord) -> RawRecord {
    RawRecord {
        kind: record.kind.clone(),
        id: record.id.clone(),
        etag: record.etag.clone(),
        data: record.data.clone(),
    }
}

fn default_schema(kind: &str) -> Result<Vec<FieldSchema>, BridgeError> {
    if kind != DEFAULT_KIND {
        return Err(BridgeError::Invalid(format!(
            "unsupported schema kind: {kind}"
        )));
    }

    Ok(vec![
        FieldSchema {
            name: "name".into(),
            ty: "string".into(),
            required: true,
            enum_values: Vec::new(),
        },
        FieldSchema {
            name: "email".into(),
            ty: "string".into(),
            required: true,
            enum_values: Vec::new(),
        },
        FieldSchema {
            name: "phone".into(),
            ty: "string".into(),
            required: false,
            enum_values: Vec::new(),
        },
        FieldSchema {
            name: "status".into(),
            ty: "string".into(),
            required: false,
            enum_values: vec!["lead".into(), "active".into(), "paused".into()],
        },
    ])
}

fn reset_state(config: ProbeConfig) -> Result<Descriptor, BridgeError> {
    let mut state = lock_state()?;
    state.config = config;
    state.records.clear();
    state.changes.clear();
    state.calls = 0;
    state.next_seq = 0;
    state.next_etag = 0;

    let kinds = supported_kinds(&state.config);
    for kind in &kinds {
        state.records.entry(kind.clone()).or_default();
    }

    let seed = state.config.seed.clone();
    for (kind, records) in seed {
        ensure_supported_kind(&state.config, &kind)?;
        for record in records {
            let data = serde_json::to_string(&record.data).map_err(|error| {
                BridgeError::Invalid(format!("invalid seed record for {kind}: {error}"))
            })?;
            let etag = record.etag.or_else(|| Some(next_etag(&mut state)));
            let stored = StoredRecord {
                kind: kind.clone(),
                id: record.id.clone(),
                etag,
                data,
            };
            let raw = make_raw_record(&stored);
            state
                .records
                .entry(kind.clone())
                .or_default()
                .insert(record.id, stored);
            push_change(&mut state, ChangeOp::Upserted, raw);
        }
    }

    Ok(descriptor_for(&state.config))
}

fn load_probe_config(config_json: String) -> Result<ProbeConfig, BridgeError> {
    if config_json.trim().is_empty() {
        return Ok(ProbeConfig::default());
    }

    serde_json::from_str::<ProbeConfig>(&config_json)
        .map_err(|error| BridgeError::Invalid(format!("invalid probe config: {error}")))
}

impl adapter::Guest for MemCrm {
    fn describe() -> Descriptor {
        match lock_state() {
            Ok(state) => descriptor_for(&state.config),
            Err(_) => descriptor_for(&ProbeConfig::default()),
        }
    }

    fn probe(config_json: String) -> Result<Descriptor, BridgeError> {
        let config = load_probe_config(config_json)?;
        reset_state(config)
    }

    fn introspect_schema(kind: String) -> Result<Vec<FieldSchema>, BridgeError> {
        let state = lock_state()?;
        ensure_supported_kind(&state.config, &kind)?;
        default_schema(&kind)
    }

    fn list(kind: String, cursor: Option<String>, limit: u32) -> Result<Page, BridgeError> {
        let mut state = lock_state()?;
        maybe_rate_limit(&mut state)?;
        ensure_supported_kind(&state.config, &kind)?;

        let start = match cursor {
            Some(value) => parse_cursor(&value)? as usize,
            None => 0,
        };
        let page_size = usize::try_from(limit.max(1)).unwrap_or(usize::MAX);

        let Some(kind_records) = state.records.get(&kind) else {
            return Ok(Page {
                records: Vec::new(),
                next_cursor: None,
            });
        };

        let records: Vec<RawRecord> = kind_records
            .values()
            .skip(start)
            .take(page_size)
            .map(make_raw_record)
            .collect();
        let next_cursor = if start + records.len() < kind_records.len() {
            Some((start + records.len()).to_string())
        } else {
            None
        };

        Ok(Page {
            records,
            next_cursor,
        })
    }

    fn get(kind: String, id: String) -> Result<RawRecord, BridgeError> {
        let mut state = lock_state()?;
        maybe_rate_limit(&mut state)?;
        ensure_supported_kind(&state.config, &kind)?;

        let record = state
            .records
            .get(&kind)
            .and_then(|records| records.get(&id))
            .ok_or_else(|| BridgeError::NotFound(format!("{kind}/{id}")))?;

        Ok(make_raw_record(record))
    }

    fn upsert(rec: RawRecord) -> Result<RawRecord, BridgeError> {
        let mut state = lock_state()?;
        maybe_rate_limit(&mut state)?;
        ensure_supported_kind(&state.config, &rec.kind)?;

        let normalized_data = normalize_data(&rec.data)?;

        {
            let records = state.records.entry(rec.kind.clone()).or_default();
            if let Some(existing) = records.get(&rec.id) {
                if let Some(ref provided_etag) = rec.etag {
                    if existing.etag.as_ref() != Some(provided_etag) {
                        return Err(BridgeError::Conflict(format!(
                            "etag mismatch for {}/{}",
                            rec.kind, rec.id
                        )));
                    }
                }
            }
        }

        let etag = Some(next_etag(&mut state));
        let stored = StoredRecord {
            kind: rec.kind.clone(),
            id: rec.id.clone(),
            etag,
            data: normalized_data,
        };
        let raw = make_raw_record(&stored);

        let records = state.records.entry(rec.kind.clone()).or_default();
        records.insert(rec.id, stored);
        push_change(&mut state, ChangeOp::Upserted, raw.clone());

        Ok(raw)
    }

    fn delete(kind: String, id: String) -> Result<(), BridgeError> {
        let mut state = lock_state()?;
        maybe_rate_limit(&mut state)?;
        ensure_supported_kind(&state.config, &kind)?;

        let deleted = state
            .records
            .get_mut(&kind)
            .and_then(|records| records.remove(&id))
            .ok_or_else(|| BridgeError::NotFound(format!("{kind}/{id}")))?;

        push_change(
            &mut state,
            ChangeOp::Deleted,
            RawRecord {
                kind: deleted.kind,
                id: deleted.id,
                etag: deleted.etag,
                data: "{}".into(),
            },
        );

        Ok(())
    }

    fn changes_since(cursor: String, limit: u32) -> Result<ChangePage, BridgeError> {
        let mut state = lock_state()?;
        maybe_rate_limit(&mut state)?;

        let cursor_value = parse_cursor(&cursor)?;
        let page_size = usize::try_from(limit.max(1)).unwrap_or(usize::MAX);
        let entries: Vec<&ChangeEntry> = state
            .changes
            .iter()
            .filter(|entry| entry.seq > cursor_value)
            .take(page_size)
            .collect();
        let next_cursor = entries
            .last()
            .map(|entry| entry.seq.to_string())
            .unwrap_or(cursor);
        let changes = entries
            .into_iter()
            .map(|entry| Change {
                op: entry.op,
                rec: entry.rec.clone(),
            })
            .collect();

        Ok(ChangePage {
            changes,
            next_cursor,
        })
    }
}

export_adapter!(MemCrm);
