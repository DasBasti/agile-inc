use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuditError {
    #[error("Redis error: {0}")]
    Redis(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Serialization error: {0}")]
    Serde(String),
}

pub type AuditResult<T> = Result<T, AuditError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocSummary {
    pub id: String,
    pub doc_type: String,
    pub title: String,
    pub status: String,
    pub owner_role: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct RedisClient {
    conn: redis::Connection,
}

impl RedisClient {
    pub fn new(url: &str) -> AuditResult<Self> {
        let client = redis::Client::open(url).map_err(|e| AuditError::Redis(e.to_string()))?;
        let conn = client.get_connection().map_err(|e| AuditError::Redis(e.to_string()))?;
        Ok(Self { conn })
    }

    pub fn is_connected(&mut self) -> bool {
        redis::cmd("PING")
            .query::<String>(&mut self.conn)
            .map(|r| r == "PONG")
            .unwrap_or(false)
    }

    pub fn list_docs(&mut self, doc_type: Option<&str>, status: Option<&str>) -> AuditResult<Vec<DocSummary>> {
        let mut docs = Vec::new();

        if let Some(dt) = doc_type {
            let type_key = format!("docs:index:type:{}", dt);
            let ids: Vec<String> = redis::cmd("SMEMBERS")
                .arg(&type_key)
                .query(&mut self.conn)
                .unwrap_or_default();

            for id in ids {
                if let Ok(summary) = self.get_doc_summary(&id) {
                    if let Some(s) = status {
                        if summary.status == s {
                            docs.push(summary);
                        }
                    } else {
                        docs.push(summary);
                    }
                }
            }
        } else if let Some(s) = status {
            for doctype in &["story", "bug", "task", "testplan", "policy", "artifact", "runlog"] {
                let key = format!("docs:index:status:{}:{}", doctype, s);
                let ids: Vec<String> = redis::cmd("SMEMBERS")
                    .arg(&key)
                    .query(&mut self.conn)
                    .unwrap_or_default();
                
                for id in ids {
                    if let Ok(summary) = self.get_doc_summary(&id) {
                        docs.push(summary);
                    }
                }
            }
        } else {
            let keys: Vec<String> = redis::cmd("KEYS")
                .arg("doc:*")
                .query(&mut self.conn)
                .unwrap_or_default();

            for key in keys {
                if key.contains(":version") {
                    continue;
                }
                if let Some(id) = key.strip_prefix("doc:") {
                    if let Ok(summary) = self.get_doc_summary(id) {
                        docs.push(summary);
                    }
                }
            }
        }

        docs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        
        Ok(docs)
    }

    fn get_doc_summary(&mut self, id: &str) -> AuditResult<DocSummary> {
        let key = format!("doc:{}", id);
        let result: Option<Vec<(String, String)>> = redis::cmd("HGETALL")
            .arg(&key)
            .query(&mut self.conn)
            .ok();

        let fields = result.ok_or(AuditError::NotFound(id.to_string()))?;

        let mut doc = DocSummary {
            id: id.to_string(),
            doc_type: "unknown".to_string(),
            title: "(no title)".to_string(),
            status: "unknown".to_string(),
            owner_role: "unknown".to_string(),
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };

        for (field, value) in fields {
            match field.as_str() {
                "type" => doc.doc_type = value,
                "title" => doc.title = value,
                "status" => doc.status = value,
                "owner_role" => doc.owner_role = value,
                "created_at" => doc.created_at = value,
                "updated_at" => doc.updated_at = value,
                _ => {}
            }
        }

        Ok(doc)
    }

    pub fn get_doc(&mut self, id: &str) -> AuditResult<serde_json::Value> {
        let key = format!("doc:{}", id);
        let result: Option<Vec<(String, String)>> = redis::cmd("HGETALL")
            .arg(&key)
            .query(&mut self.conn)
            .ok();

        let fields = result.ok_or(AuditError::NotFound(id.to_string()))?;

        let mut map = serde_json::Map::new();
        for (field, value) in fields {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&value) {
                map.insert(field, parsed);
            } else {
                map.insert(field, serde_json::Value::String(value));
            }
        }

        Ok(serde_json::Value::Object(map))
    }
}
