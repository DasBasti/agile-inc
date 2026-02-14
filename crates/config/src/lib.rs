use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    pub base_topic: String,
    pub client_id_prefix: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RedisConfig {
    pub url: String,
    #[serde(default = "default_dedupe_ttl")]
    pub dedupe_ttl_seconds: u64,
}

fn default_dedupe_ttl() -> u64 {
    86400
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct OpencodeConfig {
    #[serde(default = "default_opencode_command")]
    pub command: String,
    #[serde(default)]
    pub default_args: Vec<String>,
    #[serde(default = "default_workdir")]
    pub workdir: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_output_kb")]
    pub max_output_kb: u64,
}

fn default_opencode_command() -> String {
    "opencode".to_string()
}

fn default_workdir() -> String {
    "./".to_string()
}

fn default_timeout() -> u64 {
    1800
}

fn default_max_output_kb() -> u64 {
    2048
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AgentsConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct LlmConfig {
    #[serde(default = "default_llm_model")]
    pub model: String,
    #[serde(default = "default_llm_temperature")]
    pub temperature: f32,
    #[serde(default = "default_llm_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_llm_base_url")]
    pub base_url: String,
}

fn default_llm_model() -> String {
    "minimax-m2.5:cloud".to_string()
}

fn default_llm_temperature() -> f32 {
    0.7
}

fn default_llm_max_tokens() -> usize {
    2048
}

fn default_llm_base_url() -> String {
    "http://localhost:11434".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PromptsConfig {
    #[serde(default = "default_po_system_prompt")]
    pub po_system_prompt: String,
    #[serde(default = "default_max_llm_rounds")]
    pub max_llm_rounds: u32,
}

fn default_po_system_prompt() -> String {
    "You are a Product Owner agent. Analyze the story and respond with your thoughts.".to_string()
}

fn default_max_llm_rounds() -> u32 {
    10
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Config {
    pub mqtt: MqttConfig,
    pub redis: RedisConfig,
    pub opencode: OpencodeConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub prompts: PromptsConfig,
}

impl Config {
    pub fn load(path: &str) -> Result<Self, ConfigError> {
        let mut config: Config = toml::from_str(&fs::read_to_string(path)?)?;
        config.apply_env_overrides();
        Ok(config)
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(host) = env::var("AGILE_INC_MQTT_HOST") {
            self.mqtt.host = host;
        }
        if let Ok(port) = env::var("AGILE_INC_MQTT_PORT") {
            self.mqtt.port = port.parse().unwrap_or(self.mqtt.port);
        }
        if let Ok(base_topic) = env::var("AGILE_INC_MQTT_BASE_TOPIC") {
            self.mqtt.base_topic = base_topic;
        }
        if let Ok(url) = env::var("AGILE_INC_REDIS_URL") {
            self.redis.url = url;
        }
        if let Ok(ttl) = env::var("AGILE_INC_REDIS_DEDUPE_TTL") {
            self.redis.dedupe_ttl_seconds = ttl.parse().unwrap_or(self.redis.dedupe_ttl_seconds);
        }
        if let Ok(cmd) = env::var("AGILE_INC_OPENCODE_COMMAND") {
            self.opencode.command = cmd;
        }
        if let Ok(timeout) = env::var("AGILE_INC_OPENCODE_TIMEOUT") {
            self.opencode.timeout_seconds = timeout.parse().unwrap_or(self.opencode.timeout_seconds);
        }
        if let Ok(level) = env::var("AGILE_INC_AGENTS_LOG_LEVEL") {
            self.agents.log_level = level;
        }
    }
}

pub struct TopicBuilder {
    base: String,
}

impl TopicBuilder {
    pub fn new(base_topic: &str) -> Self {
        Self {
            base: base_topic.to_string(),
        }
    }

    pub fn role_inbox(&self, role: &str) -> String {
        format!("{}/role/{}/inbox", self.base, role)
    }

    pub fn events(&self) -> String {
        format!("{}/events", self.base)
    }

    pub fn system_alerts(&self) -> String {
        format!("{}/system/alerts", self.base)
    }

    pub fn policy_changed(&self) -> String {
        format!("{}/policy/changed", self.base)
    }

    pub fn role_topic(&self, role: &str) -> String {
        self.role_inbox(role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::env;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_config_from_toml() {
        let toml_content = r#"
[mqtt]
host = "mqtt.example.com"
port = 1884
base_topic = "testtopic"
client_id_prefix = "test"

[redis]
url = "redis://localhost:6379"
dedupe_ttl_seconds = 3600

[opencode]
command = "myopencode"
timeout_seconds = 600

[agents]
log_level = "debug"
"#;
        let temp = NamedTempFile::new().unwrap();
        fs::write(&temp, toml_content).unwrap();

        let config = Config::load(temp.path().to_str().unwrap()).unwrap();

        assert_eq!(config.mqtt.host, "mqtt.example.com");
        assert_eq!(config.mqtt.port, 1884);
        assert_eq!(config.mqtt.base_topic, "testtopic");
        assert_eq!(config.redis.url, "redis://localhost:6379");
        assert_eq!(config.redis.dedupe_ttl_seconds, 3600);
        assert_eq!(config.opencode.command, "myopencode");
        assert_eq!(config.opencode.timeout_seconds, 600);
        assert_eq!(config.agents.log_level, "debug");
    }

    #[test]
    fn test_env_overrides() {
        let toml_content = r#"
[mqtt]
host = "default.example.com"
port = 1883
base_topic = "default"
client_id_prefix = "default"

[redis]
url = "redis://localhost:6379"

[opencode]
command = "opencode"

[agents]
log_level = "info"
"#;
        let temp = NamedTempFile::new().unwrap();
        fs::write(&temp, toml_content).unwrap();

        env::set_var("AGILE_INC_MQTT_HOST", "env.example.com");
        env::set_var("AGILE_INC_MQTT_PORT", "1885");
        env::set_var("AGILE_INC_REDIS_URL", "redis://env:6379");
        env::set_var("AGILE_INC_OPENCODE_TIMEOUT", "900");

        let config = Config::load(temp.path().to_str().unwrap()).unwrap();

        assert_eq!(config.mqtt.host, "env.example.com");
        assert_eq!(config.mqtt.port, 1885);
        assert_eq!(config.redis.url, "redis://env:6379");
        assert_eq!(config.opencode.timeout_seconds, 900);

        env::remove_var("AGILE_INC_MQTT_HOST");
        env::remove_var("AGILE_INC_MQTT_PORT");
        env::remove_var("AGILE_INC_REDIS_URL");
        env::remove_var("AGILE_INC_OPENCODE_TIMEOUT");
    }

    #[test]
    fn test_topic_builder() {
        let builder = TopicBuilder::new("agileinc");

        assert_eq!(builder.role_inbox("po"), "agileinc/role/po/inbox");
        assert_eq!(builder.role_inbox("dev"), "agileinc/role/dev/inbox");
        assert_eq!(builder.events(), "agileinc/events");
        assert_eq!(builder.system_alerts(), "agileinc/system/alerts");
        assert_eq!(builder.policy_changed(), "agileinc/policy/changed");
    }

    #[test]
    fn test_default_config_values() {
        let toml_content = r#"
[mqtt]
host = "example.com"
port = 1883
base_topic = "test"
client_id_prefix = "test"

[redis]
url = "redis://localhost:6379"
"#;
        let temp = NamedTempFile::new().unwrap();
        fs::write(&temp, toml_content).unwrap();

        let config = Config::load(temp.path().to_str().unwrap()).unwrap();

        assert_eq!(config.redis.dedupe_ttl_seconds, 86400);
        assert_eq!(config.opencode.command, "opencode");
        assert_eq!(config.opencode.workdir, "./");
        assert_eq!(config.opencode.timeout_seconds, 1800);
        assert_eq!(config.opencode.max_output_kb, 2048);
        assert_eq!(config.agents.log_level, "info");
    }
}
