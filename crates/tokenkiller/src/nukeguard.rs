#[derive(Debug, Clone)]
pub struct Budgets {
    pub max_bytes: usize,
    pub max_line_bytes: usize,
    pub max_fenced_lines: usize,
    pub max_base64_run: usize,
    pub max_json_depth: usize,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024,
            max_line_bytes: 4 * 1024,
            max_fenced_lines: 64,
            max_base64_run: 2 * 1024,
            max_json_depth: 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trip {
    Bytes,
    Line,
    FencedBlock,
    Base64Run,
    JsonDepth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Continue,
    Abort(Trip),
}

#[derive(Debug)]
pub struct NukeGuard {
    budgets: Budgets,
    total: usize,
    line: usize,
    fenced: bool,
    fence_ticks: u8,
    at_line_start: bool,
    fenced_lines: usize,
    base64_run: usize,
    json_depth: i64,
    in_string: bool,
    escaped: bool,
    tripped: Option<Trip>,
}

impl NukeGuard {
    pub fn new(budgets: Budgets) -> Self {
        Self {
            budgets,
            total: 0,
            line: 0,
            fenced: false,
            fence_ticks: 0,
            at_line_start: true,
            fenced_lines: 0,
            base64_run: 0,
            json_depth: 0,
            in_string: false,
            escaped: false,
            tripped: None,
        }
    }

    pub fn tripped(&self) -> Option<Trip> {
        self.tripped
    }

    pub fn bytes_seen(&self) -> usize {
        self.total
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Verdict {
        if let Some(trip) = self.tripped {
            return Verdict::Abort(trip);
        }

        for &byte in chunk {
            self.total += 1;
            if self.total > self.budgets.max_bytes {
                return self.trip(Trip::Bytes);
            }

            if byte == b'\n' {
                if self.fenced {
                    self.fenced_lines += 1;
                    if self.fenced_lines > self.budgets.max_fenced_lines {
                        return self.trip(Trip::FencedBlock);
                    }
                }
                self.line = 0;
                self.at_line_start = true;
                self.fence_ticks = 0;
            } else {
                self.line += 1;
                if self.line > self.budgets.max_line_bytes {
                    return self.trip(Trip::Line);
                }

                if self.at_line_start || self.fence_ticks > 0 {
                    if byte == b'`' {
                        self.fence_ticks += 1;
                        if self.fence_ticks == 3 {
                            self.fenced = !self.fenced;
                            if self.fenced {
                                self.fenced_lines = 0;
                            }
                            self.fence_ticks = 0;
                        }
                    } else {
                        self.fence_ticks = 0;
                    }
                }
                self.at_line_start = false;
            }

            if byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=') {
                self.base64_run += 1;
                if self.base64_run > self.budgets.max_base64_run {
                    return self.trip(Trip::Base64Run);
                }
            } else {
                self.base64_run = 0;
            }

            if self.in_string {
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'"' {
                    self.in_string = false;
                }
            } else {
                match byte {
                    b'"' => self.in_string = true,
                    b'{' | b'[' => {
                        self.json_depth += 1;
                        if self.json_depth as usize > self.budgets.max_json_depth {
                            return self.trip(Trip::JsonDepth);
                        }
                    }
                    b'}' | b']' => {
                        self.json_depth = (self.json_depth - 1).max(0);
                    }
                    _ => {}
                }
            }
        }

        Verdict::Continue
    }

    fn trip(&mut self, trip: Trip) -> Verdict {
        self.tripped = Some(trip);
        Verdict::Abort(trip)
    }
}

pub fn repair_tail(contract_summary: &str, max_bytes: usize) -> String {
    format!(
        "SYSTEM REPAIR NOTICE: your previous output exceeded the size contract and was discarded.\n\
Return ONLY {contract_summary}. Hard limit: {max_bytes} bytes. No code fences, no full documents, no base64."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard_with(
        max_bytes: usize,
        max_line_bytes: usize,
        max_fenced_lines: usize,
        max_base64_run: usize,
        max_json_depth: usize,
    ) -> NukeGuard {
        NukeGuard::new(Budgets {
            max_bytes,
            max_line_bytes,
            max_fenced_lines,
            max_base64_run,
            max_json_depth,
        })
    }

    #[test]
    fn tk_trip_bytes() {
        let mut guard = guard_with(100, 256, 3, 256, 4);
        assert_eq!(guard.feed(&[b'a'; 101]), Verdict::Abort(Trip::Bytes));
        assert_eq!(guard.feed(b"more"), Verdict::Abort(Trip::Bytes));
    }

    #[test]
    fn tk_trip_line() {
        let mut guard = guard_with(256, 20, 3, 256, 4);
        assert_eq!(guard.feed(&[b'x'; 21]), Verdict::Abort(Trip::Line));
    }

    #[test]
    fn tk_trip_fenced_block() {
        let mut guard = guard_with(256, 256, 3, 256, 4);
        assert_eq!(
            guard.feed(b"```\n1\n2\n3\n4\n"),
            Verdict::Abort(Trip::FencedBlock)
        );
    }

    #[test]
    fn tk_trip_base64_run() {
        let mut guard = guard_with(256, 256, 3, 16, 4);
        assert_eq!(
            guard.feed(b"QUFBQUFBQUFBQUFBQUFBQUFBQUFB"),
            Verdict::Abort(Trip::Base64Run)
        );
    }

    #[test]
    fn tk_trip_json_depth_string_aware() {
        let mut guard = guard_with(256, 256, 3, 256, 4);
        assert_eq!(
            guard.feed(br#"note: "{{{{{{" is prose "#),
            Verdict::Continue
        );
        assert_eq!(guard.feed(b"[[[[["), Verdict::Abort(Trip::JsonDepth));
    }

    #[test]
    fn tk_clean_small_output_passes() {
        let mut guard = guard_with(256, 256, 3, 256, 4);
        assert_eq!(
            guard.feed(br#"{"action":"move","deal":42}"#),
            Verdict::Continue
        );
        assert!(guard.tripped().is_none());
    }
}
