//! reference/tokenkiller/nukeguard.rs — streaming output containment (SPEC-009 TK5).
//!
//! A "nuke" is a model output that floods the pipe: whole-file dumps, base64 blobs,
//! runaway JSON. Post-hoc truncation still PAYS for every token — the guard must run
//! DURING streaming and abort the HTTP body early. Feed it each SSE text delta.
//! Pure state machine: no IO, trivially unit-testable (trip-table tests below).

#[derive(Debug, Clone)]
pub struct Budgets {
    pub max_bytes: usize,          // route budget; default TK_OUTPUT_BUDGET_BYTES=16384
    pub max_line_bytes: usize,     // 4096
    pub max_fenced_lines: usize,   // 64 consecutive lines inside ``` fences
    pub max_base64_run: usize,     // 2048 contiguous base64-alphabet bytes
    pub max_json_depth: usize,     // 32
}

impl Default for Budgets {
    fn default() -> Self {
        Self { max_bytes: 16 * 1024, max_line_bytes: 4 * 1024,
               max_fenced_lines: 64, max_base64_run: 2 * 1024, max_json_depth: 32 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trip { Bytes, Line, FencedBlock, Base64Run, JsonDepth }

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict { Continue, Abort(Trip) }

#[derive(Debug)]
pub struct NukeGuard {
    b: Budgets,
    total: usize,
    line: usize,
    fenced: bool,
    fence_ticks: u8,       // counts consecutive backticks to detect ``` on a line edge
    at_line_start: bool,
    fenced_lines: usize,
    b64_run: usize,
    json_depth: i64,       // brackets/braces net depth; strings excluded
    in_str: bool,
    esc: bool,
    tripped: Option<Trip>,
}

impl NukeGuard {
    pub fn new(b: Budgets) -> Self {
        Self { b, total: 0, line: 0, fenced: false, fence_ticks: 0, at_line_start: true,
               fenced_lines: 0, b64_run: 0, json_depth: 0, in_str: false, esc: false, tripped: None }
    }

    pub fn tripped(&self) -> Option<Trip> { self.tripped }
    pub fn bytes_seen(&self) -> usize { self.total }

    /// Feed one streamed chunk. First Abort verdict is sticky.
    pub fn feed(&mut self, chunk: &[u8]) -> Verdict {
        if let Some(t) = self.tripped { return Verdict::Abort(t); }
        for &c in chunk {
            self.total += 1;
            if self.total > self.b.max_bytes { return self.trip(Trip::Bytes); }

            // line accounting
            if c == b'\n' {
                if self.fenced { self.fenced_lines += 1;
                    if self.fenced_lines > self.b.max_fenced_lines { return self.trip(Trip::FencedBlock); } }
                self.line = 0; self.at_line_start = true; self.fence_ticks = 0;
            } else {
                self.line += 1;
                if self.line > self.b.max_line_bytes { return self.trip(Trip::Line); }
                // fence detection: ``` at line start toggles fenced mode
                if self.at_line_start || self.fence_ticks > 0 {
                    if c == b'`' {
                        self.fence_ticks += 1;
                        if self.fence_ticks == 3 {
                            self.fenced = !self.fenced;
                            if self.fenced { self.fenced_lines = 0; }
                            self.fence_ticks = 0;
                        }
                    } else { self.fence_ticks = 0; }
                }
                self.at_line_start = false;
            }

            // base64 run detection (dump smell): contiguous [A-Za-z0-9+/=]
            if c.is_ascii_alphanumeric() || c == b'+' || c == b'/' || c == b'=' {
                self.b64_run += 1;
                if self.b64_run > self.b.max_base64_run { return self.trip(Trip::Base64Run); }
            } else { self.b64_run = 0; }

            // JSON depth (string-aware so `"{"` in prose doesn't count)
            if self.in_str {
                if self.esc { self.esc = false; }
                else if c == b'\\' { self.esc = true; }
                else if c == b'"' { self.in_str = false; }
            } else {
                match c {
                    b'"' => self.in_str = true,
                    b'{' | b'[' => {
                        self.json_depth += 1;
                        if self.json_depth as usize > self.b.max_json_depth {
                            return self.trip(Trip::JsonDepth);
                        }
                    }
                    b'}' | b']' => self.json_depth = (self.json_depth - 1).max(0),
                    _ => {}
                }
            }
        }
        Verdict::Continue
    }

    fn trip(&mut self, t: Trip) -> Verdict { self.tripped = Some(t); Verdict::Abort(t) }
}

/// Session-side policy (SPEC-009 TK5): on Abort ⇒ drop the HTTP stream, record
/// tk_nuke_aborts_total, retry ONCE with the repair tail appended as a NEW frozen turn:
pub fn repair_tail(contract_summary: &str, max_bytes: usize) -> String {
    format!(
        "SYSTEM REPAIR NOTICE: your previous output exceeded the size contract and was discarded.\n\
         Return ONLY {contract_summary}. Hard limit: {max_bytes} bytes. No code fences, no full documents, no base64."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn guard() -> NukeGuard { NukeGuard::new(Budgets { max_bytes: 100, max_line_bytes: 20,
        max_fenced_lines: 3, max_base64_run: 16, max_json_depth: 4 }) }

    #[test] fn tk_trip_bytes() {
        let mut g = guard();
        assert_eq!(g.feed(&[b'a'; 101]), Verdict::Abort(Trip::Bytes));
        assert_eq!(g.feed(b"more"), Verdict::Abort(Trip::Bytes)); // sticky
    }
    #[test] fn tk_trip_line() {
        let mut g = guard();
        assert_eq!(g.feed(&[b'x'; 21]), Verdict::Abort(Trip::Line));
    }
    #[test] fn tk_trip_fenced_block() {
        let mut g = guard();
        assert_eq!(g.feed(b"```\n1\n2\n3\n4\n"), Verdict::Abort(Trip::FencedBlock));
    }
    #[test] fn tk_trip_base64_run() {
        let mut g = guard();
        assert_eq!(g.feed(b"QUFB QUFBQUFBQUFBQUFBQUFBQUFB"), Verdict::Abort(Trip::Base64Run));
    }
    #[test] fn tk_trip_json_depth_string_aware() {
        let mut g = guard();
        assert_eq!(g.feed(br#"note: "{{{{{{" is prose "#), Verdict::Continue);
        assert_eq!(g.feed(b"[[[[["), Verdict::Abort(Trip::JsonDepth));
    }
    #[test] fn tk_clean_small_output_passes() {
        let mut g = guard();
        assert_eq!(g.feed(b"{\"action\":\"move\",\"deal\":42}\n"), Verdict::Continue);
        assert!(g.tripped().is_none());
    }
}
