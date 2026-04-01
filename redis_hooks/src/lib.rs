#[cfg(not(test))]
#[macro_use]
extern crate redis_module;

use std::time::Duration;

#[cfg(not(test))]
use std::collections::HashMap;
#[cfg(not(test))]
use std::sync::RwLock;

#[cfg(not(test))]
use redis_module::{Context, NotifyEvent, RedisResult, RedisString, RedisValue, Status};

#[cfg(not(test))]
use config::{Config, File, FileFormat};
use ureq::Error as WebError;
#[cfg(not(test))]
use lazy_static::lazy_static;

// ── Configuration (Redis-only) ──────────────────────────────

#[cfg(not(test))]
const CONFIG_PATH: &str = "/conf/settings.yaml";

#[cfg(not(test))]
lazy_static! {
    static ref SETTINGS: RwLock<HashMap<String, String>> = RwLock::new(get_config());
}

#[cfg(not(test))]
fn get_config() -> HashMap<String, String> {
    let builder = Config::builder().add_source(File::new(CONFIG_PATH, FileFormat::Yaml));
    match builder.build() {
        Ok(config) => config.try_deserialize::<HashMap<String, String>>().unwrap(),
        Err(e) => {
            panic!("no config found at {}: {}", CONFIG_PATH, e);
        }
    }
}

#[cfg(not(test))]
fn settings_value_or_panic(key: &str) -> String {
    SETTINGS
        .read()
        .unwrap()
        .get::<str>(key)
        .unwrap()
        .to_string()
        .clone()
}

// ── Pure functions (always compiled, testable) ──────────────

fn call_endpoint(url: &str, auth: &str, event_type: &str, event_id: &str) {
    match ureq::post(url)
        .timeout(Duration::new(3, 0))
        .set("Authorization", auth)
        .send_json(ureq::json!({
            "type": event_type,
            "id": event_id
        })) {
        Ok(response) => {
            println!("ok: {}", response.status());
        }
        Err(WebError::Status(code, _)) => {
            println!("err: {}", code);
        }
        Err(err) => {
            println!("err: {}", err);
        }
    }
}

fn match_prefix<'a>(key: &'a str, pat: &str) -> Option<&'a str> {
    Some(key).filter(|s| s.starts_with(pat))
}

/// Parse a matched key into (event_type, event_id).
/// Key format: `{prefix}:{type}:{id}` where id may contain colons.
/// Returns None if fewer than 3 colon-separated parts exist.
fn parse_matched_key(key: &str) -> Option<(&str, &str)> {
    let mut parts = key.splitn(3, ':');
    let _prefix = parts.next()?;
    let event_type = parts.next()?;
    let event_id = parts.next()?;
    if event_type.is_empty() || event_id.is_empty() {
        return None;
    }
    Some((event_type, event_id))
}

/// Decide whether a key needs reconciliation based on its TTL.
///   -2 → key doesn't exist (already gone) → false
///   -1 → key exists but no expiry set (should have expired) → true
///    0 → about to expire (may have been missed) → true
///   >0 → still alive → false
fn should_reconcile(ttl: i64) -> bool {
    ttl == -1 || ttl == 0
}

fn on_match_prefix(key: &str, url: &str, auth: &str) {
    match parse_matched_key(key) {
        Some((event_type, event_id)) => {
            call_endpoint(url, auth, event_type, event_id);
        }
        None => {
            println!("missing value in matched key: {}", key);
        }
    }
}

// ── Redis module integration (not compiled during tests) ────

#[cfg(not(test))]
fn make_prefix_handler<'a>(prefix: String, url: String, auth: String) -> impl Fn(&str) + 'a {
    move |key| {
        if let Some(key) = match_prefix(key, &prefix) {
            on_match_prefix(key, &url, &auth);
        }
    }
}

#[cfg(not(test))]
fn make_event_handler() -> impl Fn(&Context, NotifyEvent, &str, &str) {
    let prefix = settings_value_or_panic("prefixMatch");
    let url = settings_value_or_panic("url");
    let auth = settings_value_or_panic("auth");
    let inner = make_prefix_handler(prefix, url, auth);
    move |ctx: &Context, event_type: NotifyEvent, event: &str, key: &str| {
        let msg = format!(
            "Received event: {:?} on key: {} via event: {}",
            event_type, key, event
        );
        ctx.log_debug(msg.as_str());
        inner(key);
    }
}

/// Scan for iExpire:* keys with expired or missing TTL, dispatch their
/// webhooks, and delete them. Returns the number of reconciled keys.
#[cfg(not(test))]
fn reconcile(ctx: &Context, prefix: &str, url: &str, auth: &str) -> i64 {
    let pattern = format!("{}:*", prefix);
    let mut cursor = "0".to_string();
    let mut reconciled: i64 = 0;

    loop {
        let scan_result = ctx.call("SCAN", &[&cursor, "MATCH", &pattern, "COUNT", "100"]);
        match scan_result {
            Ok(RedisValue::Array(ref arr)) if arr.len() == 2 => {
                // Parse cursor
                cursor = match &arr[0] {
                    RedisValue::BulkString(s) => s.clone(),
                    RedisValue::SimpleString(s) => s.clone(),
                    _ => break,
                };
                // Parse keys
                let keys = match &arr[1] {
                    RedisValue::Array(keys) => keys,
                    _ => break,
                };
                for key_val in keys.iter() {
                    let key = match key_val {
                        RedisValue::BulkString(s) => s.as_str(),
                        RedisValue::SimpleString(s) => s.as_str(),
                        _ => continue,
                    };
                    // Check TTL
                    if let Ok(RedisValue::Integer(ttl)) = ctx.call("TTL", &[key]) {
                        if should_reconcile(ttl) {
                            ctx.log_notice(&format!(
                                "reconcile: dispatching stale key {} (ttl={})",
                                key, ttl
                            ));
                            on_match_prefix(key, url, auth);
                            let _ = ctx.call("DEL", &[key]);
                            reconciled += 1;
                        }
                    }
                }
                // cursor "0" means scan complete
                if cursor == "0" {
                    break;
                }
            }
            _ => break,
        }
    }
    ctx.log_notice(&format!("reconcile: {} keys dispatched", reconciled));
    reconciled
}

/// HOOKS.RECONCILE command handler — manually trigger reconciliation.
#[cfg(not(test))]
fn reconcile_command(ctx: &Context, _args: Vec<RedisString>) -> RedisResult {
    let prefix = settings_value_or_panic("prefixMatch");
    let url = settings_value_or_panic("url");
    let auth = settings_value_or_panic("auth");
    let count = reconcile(ctx, &prefix, &url, &auth);
    Ok(RedisValue::Integer(count))
}

/// Timer callback — auto-reconcile after module load.
#[cfg(not(test))]
fn reconcile_on_load(ctx: &Context, _data: ()) {
    let prefix = settings_value_or_panic("prefixMatch");
    let url = settings_value_or_panic("url");
    let auth = settings_value_or_panic("auth");
    reconcile(ctx, &prefix, &url, &auth);
}

/// Module init — schedule reconciliation 2 seconds after load.
#[cfg(not(test))]
fn on_init(ctx: &Context, _args: &[RedisString]) -> Status {
    ctx.log_notice("hooks: scheduling startup reconciliation in 2s");
    ctx.create_timer(Duration::from_secs(2), reconcile_on_load, ());
    Status::Ok
}

//////////////////////////////////////////////////////

#[cfg(not(test))]
redis_module! {
    name: "hooks",
    version: 1,
    data_types: [],
    init: on_init,
    commands: [
        ["hooks.reconcile", reconcile_command, "write", 0, 0, 0],
    ],
    event_handlers: [
        [@EXPIRED @EVICTED: make_event_handler()],
    ]
}

//////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    // ── match_prefix ────────────────────────────────────────────

    #[test]
    fn match_prefix_returns_key_when_prefix_matches() {
        assert_eq!(
            match_prefix("iExpire:turn:game1/t1", "iExpire"),
            Some("iExpire:turn:game1/t1")
        );
    }

    #[test]
    fn match_prefix_returns_none_for_non_matching_key() {
        assert_eq!(match_prefix("otherKey:turn:game1", "iExpire"), None);
    }

    #[test]
    fn match_prefix_returns_none_for_empty_key() {
        assert_eq!(match_prefix("", "iExpire"), None);
    }

    #[test]
    fn match_prefix_matches_prefix_only_key() {
        assert_eq!(match_prefix("iExpire", "iExpire"), Some("iExpire"));
    }

    // ── parse_matched_key ───────────────────────────────────────

    #[test]
    fn parse_matched_key_extracts_type_and_id() {
        assert_eq!(
            parse_matched_key("iExpire:turn:game1/t1"),
            Some(("turn", "game1/t1"))
        );
    }

    #[test]
    fn parse_matched_key_returns_none_when_missing_id() {
        assert_eq!(parse_matched_key("iExpire:turn"), None);
    }

    #[test]
    fn parse_matched_key_returns_none_for_prefix_only() {
        assert_eq!(parse_matched_key("iExpire"), None);
    }

    #[test]
    fn parse_matched_key_handles_extra_colons_in_id() {
        assert_eq!(
            parse_matched_key("iExpire:phase:game1/t1:reinforce"),
            Some(("phase", "game1/t1:reinforce"))
        );
    }

    #[test]
    fn parse_matched_key_returns_none_for_empty_type() {
        assert_eq!(parse_matched_key("iExpire::game1"), None);
    }

    #[test]
    fn parse_matched_key_returns_none_for_empty_id() {
        assert_eq!(parse_matched_key("iExpire:turn:"), None);
    }

    // ── should_reconcile ────────────────────────────────────────

    #[test]
    fn should_reconcile_true_for_no_expiry() {
        assert!(should_reconcile(-1));
    }

    #[test]
    fn should_reconcile_true_for_zero_ttl() {
        assert!(should_reconcile(0));
    }

    #[test]
    fn should_reconcile_false_for_positive_ttl() {
        assert!(!should_reconcile(300));
        assert!(!should_reconcile(1));
    }

    #[test]
    fn should_reconcile_false_for_missing_key() {
        assert!(!should_reconcile(-2));
    }
}
