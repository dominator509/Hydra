#!/usr/bin/env sh
# TOKENKILLER replay gate (SPEC-009 TK8): corpus vs deepseek fake; ratio must be >= TK_HIT_RATIO_TARGET.
set -eu
[ -f AGENTS.md ] || { echo "cache-hit audit ERROR: run from repository root." >&2; exit 1; }
[ -d crates/tokenkiller ] || { echo "cache-hit audit ERROR: crates/tokenkiller missing. Execute EP-004 first." >&2; exit 1; }
[ -f crates/tokenkiller/tests/replay_corpus.rs ] || { echo "cache-hit audit ERROR: replay corpus test missing. Execute EP-004 first." >&2; exit 1; }
DATABASE_URL="${DATABASE_URL:-postgres://hydra:hydra@localhost:5432/hydra}"
TARGET="${TK_HIT_RATIO_TARGET:-0.97}"
OUT="$(DATABASE_URL="$DATABASE_URL" cargo test -p tokenkiller --test replay_corpus -- --nocapture 2>&1)" || { printf '%s\n' "$OUT" >&2; echo "cache-hit audit ERROR: replay_corpus test failed." >&2; exit 1; }
RATIO="$(printf '%s\n' "$OUT" | sed -n 's/.*tk-corpus ratio: \([0-9.]*\).*/\1/p' | tail -1)"
[ -n "$RATIO" ] || { printf '%s\n' "$OUT" >&2; echo "cache-hit audit ERROR: ratio line not found (expected 'tk-corpus ratio: 0.xxxx')." >&2; exit 1; }
awk -v r="$RATIO" -v t="$TARGET" 'BEGIN{exit !(r>=t)}' || { echo "cache-hit audit ERROR: ratio $RATIO < target $TARGET. See SPEC-009 M6 recovery (prefix_sha forensics)." >&2; exit 1; }
if printf '%s\n' "$OUT" | grep -q "tk_output_nuked"; then
  echo "cache-hit audit ERROR: a corpus call was nuked; contracts/budgets violated." >&2; exit 1
fi
echo "cache-hit audit: ok (ratio=$RATIO)"
