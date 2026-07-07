use sha2::{Digest, Sha256};

pub const BLOCK_TOKENS: usize = 64;
type StabilityHint = (&'static str, fn(&str) -> bool);

pub trait Tokenizer: Send + Sync {
    fn count(&self, text: &str) -> usize;
}

pub struct ApproxTokenizer;

impl Tokenizer for ApproxTokenizer {
    fn count(&self, text: &str) -> usize {
        text.split_whitespace()
            .map(|word| 1 + word.chars().filter(|ch| ch.is_ascii_punctuation()).count() / 3)
            .sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stability {
    S0,
    S1,
    S2,
    S3,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub stability: Stability,
    pub text: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub stable_bytes: String,
    pub tail_bytes: String,
    pub prefix_sha: [u8; 32],
    pub stable_tokens: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum PrefixError {
    #[error("dynamic-looking content in {stability:?} segment: {hint}")]
    UnstableContent { stability: Stability, hint: String },
}

pub fn assemble(
    segments: &[Segment],
    tail: &str,
    tokenizer: &dyn Tokenizer,
) -> Result<Prompt, PrefixError> {
    let mut ordered = segments.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|segment| segment.stability);

    let mut stable = String::new();
    for segment in ordered {
        debug_assert!(
            segment.stability < Stability::S3,
            "S3 content belongs in the dynamic tail"
        );
        lint_stability(segment)?;
        stable.push_str(&format!(
            "<seg v{}>\n{}\n</seg>\n",
            segment.version, segment.text
        ));
    }

    let mut count = tokenizer.count(&stable);
    while !count.is_multiple_of(BLOCK_TOKENS) {
        stable.push('\n');
        let next = tokenizer.count(&stable);
        if next == count {
            stable.push_str("pad\n");
        }
        count = tokenizer.count(&stable);
    }

    let mut hasher = Sha256::new();
    hasher.update(stable.as_bytes());
    let prefix_sha: [u8; 32] = hasher.finalize().into();

    Ok(Prompt {
        stable_bytes: stable,
        tail_bytes: tail.to_owned(),
        prefix_sha,
        stable_tokens: count,
    })
}

fn lint_stability(segment: &Segment) -> Result<(), PrefixError> {
    if segment.stability == Stability::S3 {
        return Ok(());
    }

    let hints: [StabilityHint; 4] = [
        ("ISO timestamp", |text| {
            text.as_bytes().windows(20).any(|window| {
                window.len() == 20
                    && window[4] == b'-'
                    && window[7] == b'-'
                    && window[10] == b'T'
                    && window[..4].iter().all(u8::is_ascii_digit)
            })
        }),
        ("UUID", |text| {
            text.as_bytes().windows(36).any(|window| {
                window[8] == b'-'
                    && window[13] == b'-'
                    && window[18] == b'-'
                    && window[23] == b'-'
                    && window.iter().enumerate().all(|(index, byte)| {
                        matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_hexdigit()
                    })
            })
        }),
        ("epoch millis", |text| {
            text.split(|ch: char| !ch.is_ascii_digit())
                .any(|digits| digits.len() == 13)
        }),
        ("request-id marker", |text| {
            text.contains("request_id") || text.contains("trace_id")
        }),
    ];

    for (hint, detector) in hints {
        if detector(&segment.text) {
            return Err(PrefixError::UnstableContent {
                stability: segment.stability,
                hint: hint.into(),
            });
        }
    }

    Ok(())
}

pub fn debug_assert_stable(segments: &[Segment], tail: &str, tokenizer: &dyn Tokenizer) {
    #[cfg(debug_assertions)]
    {
        let first = assemble(segments, tail, tokenizer).expect("assemble a");
        let second = assemble(segments, tail, tokenizer).expect("assemble b");
        assert_eq!(
            first.prefix_sha, second.prefix_sha,
            "TK-2 violation: prefix bytes drifted between identical assemblies"
        );
    }
    #[cfg(not(debug_assertions))]
    let _ = (segments, tail, tokenizer);
}

#[derive(Default, Debug, Clone)]
pub struct Transcript {
    frozen: Vec<String>,
    summarized_upto: usize,
}

impl Transcript {
    pub fn push(&mut self, turn: impl Into<String>) {
        self.frozen.push(turn.into());
    }

    pub fn summarize_head(&mut self, upto: usize, summary: String) {
        assert!(
            upto <= self.frozen.len() && upto >= self.summarized_upto,
            "TK-4: summaries move forward only"
        );
        self.frozen
            .push(format!("<summary upto={upto}>\n{summary}\n</summary>"));
        self.summarized_upto = upto;
    }

    pub fn render_full(&self) -> String {
        self.frozen.join("\n")
    }

    pub fn len(&self) -> usize {
        self.frozen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frozen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tk_alignment_lands_on_block_boundary() {
        let segments = [
            Segment {
                stability: Stability::S0,
                text: "You are HYDRA. Obey the constitution.".into(),
                version: 1,
            },
            Segment {
                stability: Stability::S1,
                text: "{\"tool\":\"propose_envelope\"}".into(),
                version: 3,
            },
        ];
        let prompt = assemble(&segments, "task: score deal 42", &ApproxTokenizer)
            .expect("prompt should assemble");
        assert_eq!(prompt.stable_tokens % BLOCK_TOKENS, 0);
        assert!(prompt.tail_bytes.starts_with("task:"));
    }

    #[test]
    fn tk_lint_rejects_timestamp_in_s2() {
        let bad = Segment {
            stability: Stability::S2,
            text: "policy snapshot generated 2026-07-06T22:00:00Z".into(),
            version: 1,
        };
        assert!(matches!(
            assemble(&[bad], "", &ApproxTokenizer),
            Err(PrefixError::UnstableContent { .. })
        ));
    }

    #[test]
    fn tk_transcript_append_only() {
        let mut transcript = Transcript::default();
        transcript.push("u: hi");
        transcript.push("a: hello");
        let before = transcript.render_full();
        transcript.summarize_head(2, "greeting exchange".into());
        assert!(
            transcript.render_full().starts_with(&before),
            "history bytes must remain a prefix"
        );
    }
}
