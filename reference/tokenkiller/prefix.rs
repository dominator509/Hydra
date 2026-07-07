//! reference/tokenkiller/prefix.rs — stability-ordered prompt assembly (SPEC-009 TK2/TK3).
//!
//! WHY: DeepSeek context caching bills only the un-cached SUFFIX. The cache key is a
//! byte-prefix match in 64-token blocks. So we split every prompt into stability classes:
//!   S0 constitution/system   (changes ~never)
//!   S1 tool/contract schemas (changes per release)
//!   S2 tenant policy snapshot(changes per config version)
//!   S3 dynamic tail          (task, records, latest turns)
//! and guarantee S0..S2 are byte-identical across calls on the same route+tenant,
//! padded to a 64-token boundary so S3 churn never dirties a shared block.
//! Deps: sha2. Tokenizer is injected (provider-specific); tests use ApproxTokenizer.

use sha2::{Digest, Sha256};

pub const BLOCK_TOKENS: usize = 64; // ASSUMPTION A3; keep in one place.

pub trait Tokenizer: Send + Sync {
    fn count(&self, text: &str) -> usize;
}

/// Deterministic test tokenizer: whitespace words + isolated punctuation.
/// NOT accurate — only STABLE, which is all alignment math needs in CI.
pub struct ApproxTokenizer;
impl Tokenizer for ApproxTokenizer {
    fn count(&self, text: &str) -> usize {
        text.split_whitespace()
            .map(|w| 1 + w.chars().filter(|c| c.is_ascii_punctuation()).count() / 3)
            .sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stability { S0, S1, S2, S3 }

#[derive(Debug, Clone)]
pub struct Segment {
    pub stability: Stability,
    /// Canonical text: callers MUST produce this via canon::to_string (TK-2)
    /// for any structured content. Free prose allowed but frozen by version.
    pub text: String,
    /// Version participates in bytes ⇒ bumping it is an intentional cache reset.
    pub version: u32,
}

#[derive(Debug)]
pub struct Prompt {
    pub stable_bytes: String,  // S0..S2 + padding, ends exactly on a block boundary
    pub tail_bytes: String,    // S3
    pub prefix_sha: [u8; 32],  // sha256(stable_bytes) — the forensics key
    pub stable_tokens: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum PrefixError {
    #[error("dynamic-looking content in {stability:?} segment: {hint}")]
    UnstableContent { stability: Stability, hint: String },
}

/// Assemble segments into a cache-disciplined prompt.
pub fn assemble(segs: &[Segment], tail: &str, tok: &dyn Tokenizer) -> Result<Prompt, PrefixError> {
    let mut ordered: Vec<&Segment> = segs.iter().collect();
    ordered.sort_by_key(|s| s.stability); // stable sort keeps intra-class author order

    let mut stable = String::new();
    for s in &ordered {
        debug_assert!(s.stability < Stability::S3, "S3 content goes in `tail`, not segments");
        lint_stability(s)?;
        stable.push_str(&format!("<seg v{}>\n{}\n</seg>\n", s.version, s.text));
    }

    // Pad S0..S2 to a 64-token boundary so the S2/S3 seam sits ON a block edge.
    // Newlines are the padding unit: token-stable across real tokenizers we route to.
    let mut count = tok.count(&stable);
    while count % BLOCK_TOKENS != 0 {
        stable.push('\n');
        let next = tok.count(&stable);
        if next == count {
            // Tokenizer ignores bare newlines ⇒ pad with a sentinel word instead.
            stable.push_str("pad\n");
        }
        count = tok.count(&stable);
    }

    let mut hasher = Sha256::new();
    hasher.update(stable.as_bytes());
    let prefix_sha: [u8; 32] = hasher.finalize().into();

    Ok(Prompt { stable_tokens: count, stable_bytes: stable, tail_bytes: tail.to_string(), prefix_sha })
}

/// TK-2 tripwires: reject the classic cache-killers before they cost money.
/// Heuristic by design — the REAL guarantee is `debug_assert_stable` + the CI replay gate.
fn lint_stability(s: &Segment) -> Result<(), PrefixError> {
    if s.stability == Stability::S3 { return Ok(()); }
    let t = &s.text;
    let hints: [(&str, fn(&str) -> bool); 4] = [
        ("ISO timestamp", |t| {
            t.as_bytes().windows(20).any(|w| {
                w.len() == 20 && w[4] == b'-' && w[7] == b'-' && w[10] == b'T'
                    && w[..4].iter().all(u8::is_ascii_digit)
            })
        }),
        ("UUID", |t| {
            t.as_bytes().windows(36).any(|w| {
                w[8] == b'-' && w[13] == b'-' && w[18] == b'-' && w[23] == b'-'
                    && w.iter().enumerate().all(|(i, c)| matches!(i, 8|13|18|23) || c.is_ascii_hexdigit())
            })
        }),
        ("epoch millis", |t| t.split(|c: char| !c.is_ascii_digit()).any(|d| d.len() == 13)),
        ("request-id marker", |t| t.contains("request_id") || t.contains("trace_id")),
    ];
    for (hint, f) in hints {
        if f(t) {
            return Err(PrefixError::UnstableContent { stability: s.stability, hint: hint.into() });
        }
    }
    Ok(())
}

/// Debug-build law: re-assembling the same inputs must yield the same sha (TK-2).
/// Call at every Session boundary in dev/test builds.
pub fn debug_assert_stable(segs: &[Segment], tail: &str, tok: &dyn Tokenizer) {
    #[cfg(debug_assertions)]
    {
        let a = assemble(segs, tail, tok).expect("assemble a");
        let b = assemble(segs, tail, tok).expect("assemble b");
        assert_eq!(a.prefix_sha, b.prefix_sha, "TK-2 violation: prefix bytes drifted between identical assemblies");
    }
    #[cfg(not(debug_assertions))]
    let _ = (segs, tail, tok);
}

// ---------- Append-only transcript (TK-3/TK-4) ----------

/// Frozen turns: once pushed, bytes never change. Summarization APPENDS a summary
/// turn and advances `summarized_upto`; renderers include the marker line so earlier
/// bytes remain a shared prefix while superseded turns are skipped AFTER it.
#[derive(Default, Debug)]
pub struct Transcript {
    frozen: Vec<String>,
    summarized_upto: usize,
}

impl Transcript {
    pub fn push(&mut self, turn: impl Into<String>) { self.frozen.push(turn.into()); }

    pub fn summarize_head(&mut self, upto: usize, summary: String) {
        assert!(upto <= self.frozen.len() && upto >= self.summarized_upto,
            "TK-4: summaries move forward only");
        self.frozen.push(format!("<summary upto={upto}>\n{summary}\n</summary>"));
        self.summarized_upto = upto;
    }

    /// Render for the S3 tail. NOTE: turns [0, summarized_upto) are still in `frozen`
    /// (immutability), but a renderer MAY skip their bodies once the provider cache
    /// window ages out — see Session::render_tail policy in crates/tokenkiller.
    pub fn render_full(&self) -> String { self.frozen.join("\n") }
    pub fn len(&self) -> usize { self.frozen.len() }
    pub fn is_empty(&self) -> bool { self.frozen.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tk_alignment_lands_on_block_boundary() {
        let segs = [
            Segment { stability: Stability::S0, text: "You are HYDRA. Obey the constitution.".into(), version: 1 },
            Segment { stability: Stability::S1, text: "{\"tool\":\"propose_envelope\"}".into(), version: 3 },
        ];
        let p = assemble(&segs, "task: score deal 42", &ApproxTokenizer).unwrap();
        assert_eq!(p.stable_tokens % BLOCK_TOKENS, 0);
        assert!(p.tail_bytes.starts_with("task:"));
    }

    #[test]
    fn tk_lint_rejects_timestamp_in_s2() {
        let bad = Segment { stability: Stability::S2,
            text: "policy snapshot generated 2026-07-06T22:00:00Z".into(), version: 1 };
        assert!(matches!(assemble(&[bad], "", &ApproxTokenizer),
            Err(PrefixError::UnstableContent { .. })));
    }

    #[test]
    fn tk_transcript_append_only() {
        let mut t = Transcript::default();
        t.push("u: hi"); t.push("a: hello");
        let before = t.render_full();
        t.summarize_head(2, "greeting exchange".into());
        assert!(t.render_full().starts_with(&before), "history bytes must remain a prefix");
    }
}
