#[macro_use]
extern crate redis_module;

use std::collections::HashMap;
use std::sync::RwLock;

use redis_module::{Context, NotifyEvent };

use config::{ Config, FileFormat, File };
use ureq::Error as WebError;
use lazy_static::lazy_static;

const CONFIG_PATH: &str = "/conf/settings.yaml";
lazy_static! {
    static ref SETTINGS: RwLock<HashMap<String, String>> = RwLock::new(get_config());
}

fn get_config() -> HashMap<String, String>{
    let builder = Config::builder()
        .add_source(File::new(CONFIG_PATH, FileFormat::Yaml));
    match builder.build() {
        Ok(config) => {
            return config.try_deserialize::<HashMap<String, String>>().unwrap();
        },
        Err(e) => {
            panic!("no config found at {}: {}", CONFIG_PATH, e);
        }
    }
}

fn settings_value_or_panic(key: &str) -> String {
    return SETTINGS.read().unwrap().get::<str>(key).unwrap().to_string().clone();
}

fn call_endpoint(url: &str, auth: &str, event_type: &str, event_id: &str){
    match ureq::post(&url)
        .set("Authorization", &auth)
        .send_json(ureq::json!(
            {
                "type" : event_type,
                "id": event_id
            }
    )) {
        Ok(response) => {
            println!("ok {}", response.status());
        }
        Err(WebError::Status(code, _)) => {
            println!("err {}", code);
        }
        Err(err) => {
            println!("err {}", err);
        }
    }
}

fn match_prefix<'a>(key: &'a str, pat: &str) -> Option<&'a str> {
    Some(key).filter(|s| s.starts_with(pat))
}

fn on_match_prefix(key: &str, url: &str, auth: &str){
    let parts = key.split(":").collect::<Vec<&str>>();
    let event_type = parts.get(1);
    let event_id = parts.get(2);
    match (event_type, event_id) {
        (Some(event_type), Some(event_id)) => {
            call_endpoint(url, auth, event_type, event_id)
        }
        _ => {
            println!("missing value in matched key: {}", key);
        }
    }
}

fn make_prefix_handler<'a>(prefix: String, url: String, auth: String) -> impl Fn(&str) + 'a{
    move |key| {
        match match_prefix(key, &prefix) {
            Some(key) => {
                on_match_prefix(key, &url, &auth);
            }
            _ => { }
    }
}}

// TODO Macro to handle different event types
fn make_event_handler() -> impl Fn(&Context, NotifyEvent, &str, &str){
    // TODO pass in conf
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

//////////////////////////////////////////////////////

redis_module! {
    name: "hooks",
    version: 1,
    data_types: [],
    commands: [],
    event_handlers: [
        [@EXPIRED @EVICTED: make_event_handler()],
    ]
}

//////////////////////////////////////////////////////

#[cfg(test)]
mod tests {}
