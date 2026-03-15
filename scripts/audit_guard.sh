#!/usr/bin/env bash
# audit_guard.sh — Lightweight CI check that fails if known insecure patterns
# reappear in the codebase.  Run as part of CI or pre-commit.
#
# Exit code 0 = clean, non-zero = regression detected.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ERRORS=0

check_pattern() {
  local label="$1"
  local pattern="$2"
  local scope="$3"

  if grep -rq "$pattern" "$scope" 2>/dev/null; then
    echo "FAIL: $label"
    grep -rn "$pattern" "$scope" 2>/dev/null || true
    ERRORS=$((ERRORS + 1))
  else
    echo "  OK: $label"
  fi
}

check_present() {
  local label="$1"
  local pattern="$2"
  local file="$3"

  if grep -q "$pattern" "$file" 2>/dev/null; then
    echo "  OK: $label"
  else
    echo "FAIL: $label"
    ERRORS=$((ERRORS + 1))
  fi
}

echo "=== 0-ads Audit Regression Guard ==="
echo ""

echo "--- Mock signature literals ---"
check_pattern \
  "No mock signature strings in backend" \
  "0xUniversalSignedProofOfIntent" \
  "$REPO_ROOT/backend/"

check_pattern \
  "No placeholder signatures in anti-sybil" \
  '"0x\.\.\."' \
  "$REPO_ROOT/backend/oracle_anti_sybil.py"

echo ""
echo "--- Fail-open verifier branches ---"
check_pattern \
  "No 'Defaulting to True' in backend" \
  "Defaulting to True" \
  "$REPO_ROOT/backend/"

echo ""
echo "--- Hardcoded oracle key defaults ---"
check_pattern \
  "No hardcoded fallback oracle key in Rust node" \
  "33be6fa714a02fc089f7fc2084da8260d081c210ed04989862db1ee8cf500808" \
  "$REPO_ROOT/src/"

echo ""
echo "--- Contract / relayer ABI compatibility ---"
check_present \
  "Relayer uses claimPayoutFor" \
  "claimPayoutFor" \
  "$REPO_ROOT/backend/gasless_relayer.py"

check_present \
  "AdEscrow exposes claimPayoutFor" \
  "function claimPayoutFor" \
  "$REPO_ROOT/contracts/evm/contracts/AdEscrow.sol"

echo ""
if [ "$ERRORS" -gt 0 ]; then
  echo "=== $ERRORS audit regression(s) detected ==="
  exit 1
else
  echo "=== All audit guards passed ==="
  exit 0
fi
