# startup-reconciliation
**Status:** DONE
**Type:** feature
**Created:** 2026-04-01
**Repo:** redis_hooks

## Goal

When Redis restarts and loads persisted data (RDB/AOF), keys whose TTL expired during downtime are silently discarded — no keyspace notification fires, so the `redis_hooks` module never dispatches the webhook. This leaves turns/phases/invites stuck until a client-side fallback timer notices.

Add a `HOOKS.RECONCILE` command that scans for `iExpire:*` keys with no remaining TTL and dispatches their webhooks as if they'd expired normally. Schedule it to auto-run shortly after module load so no entrypoint changes are needed.

## Background

- The `redis_hooks` module (Rust, `redis_module` crate v1.0 with `experimental-api`) listens for `@EXPIRED` and `@EVICTED` events on keys matching the configured `prefixMatch` (default: `iExpire`).
- Key format: `iExpire:{type}:{id}` (e.g., `iExpire:turn:gameId/turnId`).
- On match, it POSTs `{"type": "...", "id": "..."}` to the configured webhook URL with an `Authorization` header.
- Downstream handlers (`dispatchExpiry` in backchannel-srv) are **idempotent** — duplicate or late dispatch is safe.
- Current test coverage: **zero**. The `#[cfg(test)] mod tests {}` block is empty.

## Design

### Reconciliation logic

1. `SCAN 0 MATCH iExpire:* COUNT 100` (iterate full keyspace)
2. For each key found:
   - `TTL key` → if result is **-1** (no expiry) or **0** (about to expire): this key missed its expiry event
   - `TTL key` → if result is **> 0**: still alive, skip
   - For reconcilable keys: parse `{prefix}:{type}:{id}`, dispatch webhook, then `DEL key`
3. Return the count of reconciled keys

**Why TTL = -1?** After AOF replay of a `SET key val PXAT <absolute-time>` where the timestamp has already passed, Redis behavior varies by version — the key may exist with TTL -1 (no expiry) or be lazily expired on access. Checking for TTL ≤ 0 during SCAN covers both cases.

### Module changes

1. **New command: `HOOKS.RECONCILE`** — runs the scan/dispatch/delete cycle. Returns integer count.
2. **Init auto-schedule** — register an `init` function that uses `ctx.create_timer(Duration::from_secs(2), ...)` to fire reconciliation 2 seconds after module load (giving Redis time to finish AOF replay). If the timer API isn't available in this crate version, fall back to requiring manual invocation.
3. **Refactor `call_endpoint` and `on_match_prefix`** — extract shared dispatch logic so both the event handler path and the reconciliation path use the same code.

### Safety

- Downstream handlers are idempotent → double-dispatch is harmless.
- `DEL` after dispatch ensures the key doesn't trigger again via normal expiry.
- If the webhook call fails (3s timeout), log the error but still delete the key — the key was already expired, keeping it serves no purpose. The client-side fallback timer will catch it.

## Tasks

### T1: Unit tests for existing pure functions + reconciliation logic
- Write Rust unit tests (`cargo test`, no Redis needed) for:
  - `match_prefix()`: matching keys, non-matching keys, edge cases (empty string, prefix-only)
  - Key parsing in `on_match_prefix()`: valid `prefix:type:id`, missing parts, extra colons
  - `should_reconcile(ttl: i64) -> bool`: TTL -1 → true, TTL 0 → true, TTL > 0 → false
- Refactor existing code to make pure functions testable (extract decision logic from side-effecting dispatch)

### T2: Implement `HOOKS.RECONCILE` command + reconciliation logic
- Add `reconcile_command(ctx: &Context, _args: Vec<RedisString>) -> RedisResult`
- Uses `ctx.call("SCAN", ...)` and `ctx.call("TTL", ...)` to find stale keys
- Reuses `on_match_prefix` for dispatch, `ctx.call("DEL", ...)` to clean up
- Register in `redis_module!` macro: `["hooks.reconcile", reconcile_command, "write", 0, 0, 0]`
- Returns `RedisValue::Integer(count)` — number of keys reconciled

### T3: Add init auto-timer
- Add `init` function to `redis_module!` macro
- Schedule `reconcile` to run 2 seconds after module load via `ctx.create_timer()`
- If timer API unavailable: document manual invocation requirement, skip this task

### T4: E2E tests (Docker Compose, full stack)
- `docker-compose.test.yaml`: Redis with module + mock webhook receiver (simple HTTP server)
- Mock webhook server: minimal Rust binary or shell script that logs received POSTs to a file
- Test runner script orchestrates: start services, set up keys via `redis-cli`, call `HOOKS.RECONCILE`, verify mock received correct payloads, verify keys deleted
- Test cases:
  - Key with no TTL → reconciled, webhook dispatched, key deleted
  - Key with TTL > 0 → skipped, still exists
  - Non-matching key → ignored
  - Empty DB → returns 0
  - Multiple expired keys → all reconciled
  - Auto-timer on fresh startup: pre-seed a key with no TTL, start Redis, verify webhook received without manual RECONCILE call

### T5: Documentation
- Update README with reconciliation behavior
- Add ADR entry in backchannel-ui repo documenting the design decision
- Update `docs/WIP/BUGS/redis-dev-instability.md` to reference this fix

## Files
- `redis_hooks/src/lib.rs` — main module code (reconcile command, init timer, refactored dispatch)
- `redis_hooks/Cargo.toml` — dev-dependencies for tests
- `tests/integration.rs` or `tests/reconcile_test.rs` — integration tests
- `docker-compose.test.yaml` — test infrastructure
- `conf/settings.yaml` — no changes needed (reuses existing config)
- `README.md` — documentation updates

## Open Questions
None — downstream idempotency makes this safe to implement without coordination.
