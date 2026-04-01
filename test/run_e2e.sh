#!/usr/bin/env bash
# E2E tests for redis_hooks startup reconciliation.
#
# Usage: ./test/run_e2e.sh
#
# Requires: docker compose, redis-cli, python3
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REDIS_PORT=16379
REDIS_PASS=password
WEBHOOK_PORT=9999
WEBHOOK_LOG="/tmp/redis_hooks_test_webhook.log"
PASS=0
FAIL=0
TOTAL=0

cli() {
    redis-cli -p "${REDIS_PORT}" -a "${REDIS_PASS}" --no-auth-warning "$@"
}

assert_eq() {
    local test_name="$1" expected="$2" actual="$3"
    TOTAL=$((TOTAL + 1))
    if [ "${expected}" = "${actual}" ]; then
        echo "  ✓ ${test_name}"
        PASS=$((PASS + 1))
    else
        echo "  ✗ ${test_name}: expected '${expected}', got '${actual}'"
        FAIL=$((FAIL + 1))
    fi
}

assert_contains() {
    local test_name="$1" needle="$2" haystack="$3"
    TOTAL=$((TOTAL + 1))
    if echo "${haystack}" | grep -q "${needle}"; then
        echo "  ✓ ${test_name}"
        PASS=$((PASS + 1))
    else
        echo "  ✗ ${test_name}: '${needle}' not found in output"
        FAIL=$((FAIL + 1))
    fi
}

assert_line_count() {
    local test_name="$1" expected="$2" file="$3"
    TOTAL=$((TOTAL + 1))
    local actual
    if [ -f "${file}" ]; then
        actual=$(wc -l < "${file}")
    else
        actual=0
    fi
    if [ "${expected}" -eq "${actual}" ]; then
        echo "  ✓ ${test_name}"
        PASS=$((PASS + 1))
    else
        echo "  ✗ ${test_name}: expected ${expected} lines, got ${actual}"
        FAIL=$((FAIL + 1))
    fi
}

cleanup() {
    echo "Cleaning up..."
    kill "${WEBHOOK_PID}" 2>/dev/null || true
    cd "${REPO_DIR}" && docker compose -f docker-compose.test.yaml down -v 2>/dev/null || true
    rm -f "${WEBHOOK_LOG}"
}
trap cleanup EXIT

# ── Setup ────────────────────────────────────────────────────

echo "=== Building and starting Redis with module ==="
cd "${REPO_DIR}"
docker compose -f docker-compose.test.yaml build --quiet
docker compose -f docker-compose.test.yaml up -d

echo "Starting mock webhook server on :${WEBHOOK_PORT}..."
rm -f "${WEBHOOK_LOG}"
python3 "${SCRIPT_DIR}/mock_webhook.py" "${WEBHOOK_PORT}" "${WEBHOOK_LOG}" &
WEBHOOK_PID=$!

echo "Waiting for Redis..."
for i in $(seq 1 30); do
    if cli PING 2>/dev/null | grep -q PONG; then
        break
    fi
    sleep 0.5
done
cli PING | grep -q PONG || { echo "Redis failed to start"; exit 1; }

# Wait for the auto-reconcile timer to fire (2s after module load).
# At this point the DB is empty so it should reconcile 0 keys.
sleep 3

echo ""

# ── Test 1: Empty DB returns 0 ──────────────────────────────

echo "Test 1: RECONCILE on empty DB"
RESULT=$(cli HOOKS.RECONCILE)
assert_eq "returns 0" "0" "${RESULT}"
assert_line_count "no webhooks dispatched" 0 "${WEBHOOK_LOG}"

# ── Test 2: Key with positive TTL is skipped ────────────────

echo "Test 2: Key with TTL > 0 is skipped"
cli SET "iExpire:turn:game-live/t1" 0 EX 300 > /dev/null
RESULT=$(cli HOOKS.RECONCILE)
assert_eq "returns 0" "0" "${RESULT}"
EXISTS=$(cli EXISTS "iExpire:turn:game-live/t1")
assert_eq "key still exists" "1" "${EXISTS}"
assert_line_count "no webhooks dispatched" 0 "${WEBHOOK_LOG}"
cli DEL "iExpire:turn:game-live/t1" > /dev/null

# ── Test 3: Key with no TTL (TTL=-1) is reconciled ─────────

echo "Test 3: Key with no TTL is reconciled"
cli SET "iExpire:turn:game-stale/t5" 0 > /dev/null
RESULT=$(cli HOOKS.RECONCILE)
assert_eq "returns 1" "1" "${RESULT}"
EXISTS=$(cli EXISTS "iExpire:turn:game-stale/t5")
assert_eq "key was deleted" "0" "${EXISTS}"
assert_line_count "1 webhook dispatched" 1 "${WEBHOOK_LOG}"

# Verify webhook payload
PAYLOAD=$(tail -1 "${WEBHOOK_LOG}")
assert_contains "type is turn" '"type": "turn"' "${PAYLOAD}"
assert_contains "id is game-stale/t5" '"id": "game-stale/t5"' "${PAYLOAD}"
assert_contains "auth header sent" '"auth": "test-auth-token"' "${PAYLOAD}"

# ── Test 4: Non-matching key is ignored ─────────────────────

echo "Test 4: Non-matching key is ignored"
rm -f "${WEBHOOK_LOG}"
cli SET "otherPrefix:something" 0 > /dev/null
RESULT=$(cli HOOKS.RECONCILE)
assert_eq "returns 0" "0" "${RESULT}"
EXISTS=$(cli EXISTS "otherPrefix:something")
assert_eq "non-matching key untouched" "1" "${EXISTS}"
assert_line_count "no webhooks dispatched" 0 "${WEBHOOK_LOG}"
cli DEL "otherPrefix:something" > /dev/null

# ── Test 5: Multiple expired keys reconciled ────────────────

echo "Test 5: Multiple expired keys"
rm -f "${WEBHOOK_LOG}"
cli SET "iExpire:turn:game-a/t1" 0 > /dev/null
cli SET "iExpire:phase:game-b/t2:reinforce" 0 > /dev/null
cli SET "iExpire:invite:game-c/player1" 0 > /dev/null
# Also add a live key that should NOT be reconciled
cli SET "iExpire:turn:game-d/t3" 0 EX 600 > /dev/null
RESULT=$(cli HOOKS.RECONCILE)
assert_eq "returns 3" "3" "${RESULT}"
assert_line_count "3 webhooks dispatched" 3 "${WEBHOOK_LOG}"
# Live key should still exist
EXISTS=$(cli EXISTS "iExpire:turn:game-d/t3")
assert_eq "live key untouched" "1" "${EXISTS}"
cli DEL "iExpire:turn:game-d/t3" > /dev/null

# ── Test 6: Auto-reconcile on startup ───────────────────────

echo "Test 6: Auto-reconcile on startup"
rm -f "${WEBHOOK_LOG}"
# Set a stale key, then restart Redis — the init timer should reconcile it
cli SET "iExpire:turn:game-restart/t9" 0 > /dev/null
docker compose -f docker-compose.test.yaml restart redis_test
echo "  Waiting for Redis restart + auto-reconcile..."
for i in $(seq 1 30); do
    if cli PING 2>/dev/null | grep -q PONG; then
        break
    fi
    sleep 0.5
done
# Wait for the 2-second auto-reconcile timer
sleep 4
EXISTS=$(cli EXISTS "iExpire:turn:game-restart/t9")
assert_eq "key auto-reconciled after restart" "0" "${EXISTS}"
assert_line_count "webhook dispatched on startup" 1 "${WEBHOOK_LOG}"

# ── Summary ─────────────────────────────────────────────────

echo ""
echo "=== Results: ${PASS}/${TOTAL} passed, ${FAIL} failed ==="
if [ "${FAIL}" -gt 0 ]; then
    exit 1
fi
