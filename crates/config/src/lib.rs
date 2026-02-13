use serde::{Deserialize,Serialize};
use std::fs;

#[derive(Serialize,Deserialize,Debug,Clone)]
pub struct MqttConfig { pub host:String, pub port:u16, pub username:String, pub password:String, pub base_topic:String }
#[derive(Serialize,Deserialize,Debug,Clone)]
pub struct RedisConfig { pub url:String, pub dedupe_ttl_seconds:u64 }
#[derive(Serialize,Deserialize,Debug,Clone)]
pub struct OpencodeConfig { pub command:String, pub default_args:Vec<String>, pub workdir:String, pub timeout_seconds:u64 }

#[derive(Serialize,Deserialize,Debug,Clone)]
pub struct Config {
    pub mqtt: MqttConfig,
    pub redis: RedisConfig,
    pub opencode: OpencodeConfig,
    pub agents: serde_json::Value,
}

pub fn load_config(path:&str)->Config{
    let s = fs::read_to_string(path).expect("config read");
    toml::from_str(&s).expect("parse config")
}
