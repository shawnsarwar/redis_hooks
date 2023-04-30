#[macro_use]
extern crate redis_module;

use redis_module::{Context, NotifyEvent };

use std::collections::HashMap;

use config::{ Config, FileFormat, File };
use ureq::Error as WebError;

use lazy_static::lazy_static;
use std::sync::RwLock;

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

fn match_predicate<'a>(key: &'a str, pat: &str) -> Option<&'a str> {
    Some(key).filter(|s| s.contains(pat))
}

fn on_matching_predicate(key: &str, url: &str, auth: &str){
    let parts = key.split(":").collect::<Vec<&str>>();
    let event_type = parts.get(1);
    let event_id = parts.get(2);
    match (event_type, event_id) {
        (Some(event_type), Some(event_id)) => {
            println!("type: {} id: {}", event_type, event_id);
            call_endpoint(url, auth, event_type, event_id)
        }
        _ => {
            println!("missing value in matching predicate: {}", key);
        }
    }
}

fn on_event(ctx: &Context, event_type: NotifyEvent, event: &str, key: &str) {
    let msg = format!(
        "Received event: {:?} on key: {} via event: {}",
        event_type, key, event
    );
    ctx.log_debug(msg.as_str());
    match match_predicate(key, "iExpire") {
        Some(key) => {
            let url = settings_value_or_panic("url");
            let auth = settings_value_or_panic("auth");
            on_matching_predicate(key, &url, &auth);
        }
        _ => { }
    }
}

//////////////////////////////////////////////////////

redis_module! {
    name: "hooks",
    version: 1,
    data_types: [],
    commands: [],
    event_handlers: [
        [@EXPIRED @EVICTED: on_event],
    ]
}

//////////////////////////////////////////////////////

#[cfg(test)]
mod tests {}
