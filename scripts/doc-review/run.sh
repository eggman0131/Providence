#!/bin/sh
# scripts/doc-review/run.sh — orchestrator for the advisory doc-drift review
# (ADR 0013). Deterministic detector → LLM advisor → deduped GitHub issue.
# Advisory only: it never gates and (via the pre-push hook) runs detached, so it
# can never block or fail a push.
#
# The advisor is a pluggable agent (--agent): the default is the local **Qwen
# via Cline** (offline, I7); an **online Claude Haiku** agent is available as an
# explicit escalation when the local review comes back inconclusive.
#
# Usage:
#   scripts/doc-review/run.sh [--dry-run] [--agent cline|claude] \
#                             [--since <ref> | --range <a..b>]
# Env:
#   PROVIDENCE_SKIP_DOC_REVIEW=1          disable entirely
#   PROVIDENCE_DOC_REVIEW_AGENT=<name>    cline (default) | claude
#   PROVIDENCE_DOC_REVIEW_MODEL=<id>      local model (default qwen3.6:35b-mlx)
#   PROVIDENCE_DOC_REVIEW_PROVIDER=<id>   Cline provider id (default: Cline's default)
#   PROVIDENCE_DOC_REVIEW_TIMEOUT=<s>     Cline timeout seconds (default 900)
#   PROVIDENCE_DOC_REVIEW_CLAUDE_MODEL=<m> online model (default haiku)

if [ "${PROVIDENCE_SKIP_DOC_REVIEW:-0}" = "1" ]; then
    echo "[doc-review] skipped (PROVIDENCE_SKIP_DOC_REVIEW=1)"
    exit 0
fi

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
cd "$ROOT" || { echo "[doc-review] cannot cd to repo root"; exit 0; }

# A detached git hook inherits a minimal PATH, so cargo/gh/claude/ollama may be
# missing even when they are installed. Prepend the standard macOS locations
# (ADR 0005) plus mise shims; a still-missing agent degrades to the escalation
# message rather than a silent skip.
PATH="$HOME/.cargo/bin:$HOME/.local/bin:$HOME/.local/share/mise/shims:/opt/homebrew/bin:/usr/local/bin:$PATH"
export PATH

# Skip during rebase / merge / cherry-pick (mirrors the graphify hook).
GIT_DIR=$(git rev-parse --git-dir 2>/dev/null || echo .git)
if [ -d "$GIT_DIR/rebase-merge" ] || [ -d "$GIT_DIR/rebase-apply" ] ||
    [ -f "$GIT_DIR/MERGE_HEAD" ] || [ -f "$GIT_DIR/CHERRY_PICK_HEAD" ]; then
    echo "[doc-review] skipped (mid rebase/merge/cherry-pick)"
    exit 0
fi

# --- arguments ---
DRY_RUN=0
SINCE=""
RANGE=""
AGENT="${PROVIDENCE_DOC_REVIEW_AGENT:-cline}"
while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --agent) shift; AGENT="${1:-}" ;;
        --since) shift; SINCE="${1:-}" ;;
        --range) shift; RANGE="${1:-}" ;;
        *) echo "[doc-review] unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

case "$AGENT" in
    cline) AGENT_BIN="cline" ;;
    claude) AGENT_BIN="claude" ;;
    *) echo "[doc-review] unknown --agent '$AGENT' (use cline|claude)" >&2; exit 2 ;;
esac

# --- core tools (never block: skip cleanly if absent). The review agent binary
#     is checked later so a missing agent escalates instead of silently skipping. ---
for tool in cargo gh; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "[doc-review] '$tool' not found — skipping review"
        exit 0
    fi
done

# --- resolve the base ref (detector diffs <base>..HEAD) and the display range ---
if [ -n "$RANGE" ]; then
    BASE=${RANGE%%..*}
elif [ -n "$SINCE" ]; then
    BASE="$SINCE"
    RANGE="$SINCE..HEAD"
elif git rev-parse --verify --quiet origin/main >/dev/null 2>&1; then
    BASE="origin/main"
    RANGE="origin/main..HEAD"
else
    BASE="HEAD~1"
    RANGE="HEAD~1..HEAD"
fi
echo "[doc-review] range: $RANGE · agent: $AGENT · dry-run: $DRY_RUN"

CANDFILE=$(mktemp "${TMPDIR:-/tmp}/doc-review-cand.XXXXXX") || exit 0
REVIEW_OUT=$(mktemp "${TMPDIR:-/tmp}/doc-review-out.XXXXXX") || exit 0
trap 'rm -f "$CANDFILE" "$REVIEW_OUT"' EXIT INT TERM

# --- 1. deterministic candidate detection ---
if ! cargo xtask doc-review --since "$BASE" --json >"$CANDFILE" 2>/dev/null; then
    echo "[doc-review] detector failed for base '$BASE' — skipping"
    exit 0
fi
if [ ! -s "$CANDFILE" ] || head -n1 "$CANDFILE" | grep -q '^\[\]'; then
    echo "[doc-review] no drift candidates — done"
    exit 0
fi
NCAND=$(grep -c '"doc"' "$CANDFILE" 2>/dev/null || echo "?")
echo "[doc-review] $NCAND candidate(s) — reviewing with '$AGENT'…"

# --- 2. build the review prompt (shared across agents) ---
# The `.clinerules/workflows/doc-drift-review.md` workflow is the single source
# of the review contract (and drives the interactive `/doc-drift-review`); its
# text is fed straight into the prompt so a headless run does not depend on
# slash-command expansion. The diff is inlined so the agent needn't run git.
WORKFLOW="$ROOT/.clinerules/workflows/doc-drift-review.md"
DIFF=$(git diff "$RANGE" 2>/dev/null | head -c "${PROVIDENCE_DOC_REVIEW_DIFF_BYTES:-40000}")
PROMPT="Follow these instructions exactly. Do not ask questions. Do not edit any files.

$(cat "$WORKFLOW" 2>/dev/null)

--- INPUTS FOR THIS RUN ---
candidates=$CANDFILE
range=$RANGE

The unified diff for this range is included below, so you do NOT need to run git.
You MAY read the candidate doc files to check their current wording.
--- BEGIN DIFF ($RANGE) ---
$DIFF
--- END DIFF ---

Review every candidate now. Your FINAL message must contain ONLY the verdict
array wrapped in ===DOC_DRIFT_JSON_BEGIN=== and ===DOC_DRIFT_JSON_END=== — no
prose, no code fences, nothing before or after the block. Emit one object per
candidate, including drift:false ones."

# --- 3. run the advisor (advisor only, no side effects) ---
if command -v "$AGENT_BIN" >/dev/null 2>&1; then
    if [ "$AGENT" = "cline" ]; then
        MODEL="${PROVIDENCE_DOC_REVIEW_MODEL:-qwen3.6:35b-mlx}"
        PROVIDER="${PROVIDENCE_DOC_REVIEW_PROVIDER:-}"
        TIMEOUT="${PROVIDENCE_DOC_REVIEW_TIMEOUT:-900}"
        set -- --thinking low --auto-approve true -t "$TIMEOUT"
        [ -n "$PROVIDER" ] && set -- "$@" -P "$PROVIDER"
        [ -n "$MODEL" ] && set -- "$@" -m "$MODEL"
        if ! cline "$@" "$PROMPT" >"$REVIEW_OUT" 2>&1; then
            echo "[doc-review] cline exited non-zero — will still try to parse its output"
        fi
    else
        # Prompt via stdin: `--allowedTools` is variadic and would otherwise
        # swallow the positional prompt argument.
        CLAUDE_MODEL="${PROVIDENCE_DOC_REVIEW_CLAUDE_MODEL:-haiku}"
        if ! printf '%s' "$PROMPT" | claude -p --model "$CLAUDE_MODEL" \
            --output-format text --allowedTools Read >"$REVIEW_OUT" 2>&1; then
            echo "[doc-review] claude exited non-zero — will still try to parse its output"
        fi
    fi
else
    echo "[doc-review] agent '$AGENT' ($AGENT_BIN) is not installed"
    : >"$REVIEW_OUT" # empty output → treated as inconclusive → escalation below
fi

# --- 4. parse the verdict, post deduped issues, or escalate (python: robust JSON) ---
PYTHON=$(command -v python3 || command -v python || true)
if [ -z "$PYTHON" ]; then
    echo "[doc-review] python3 not found — cannot parse the verdict; skipping"
    exit 0
fi

DRY_RUN="$DRY_RUN" RANGE="$RANGE" CANDFILE="$CANDFILE" REVIEW_OUT="$REVIEW_OUT" \
    PDR_AGENT="$AGENT" "$PYTHON" "$SCRIPT_DIR/post_issues.py"
exit 0
