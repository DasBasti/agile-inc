use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::broadcast;

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
    conn: Mutex<redis::Connection>,
}

impl RedisClient {
    pub fn new(url: &str) -> AuditResult<Self> {
        let client = redis::Client::open(url).map_err(|e| AuditError::Redis(e.to_string()))?;
        let conn = client.get_connection().map_err(|e| AuditError::Redis(e.to_string()))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn is_connected(&self) -> bool {
        let mut conn = self.conn.lock().unwrap();
        let result: Result<String, _> = redis::cmd("PING").query(&mut *conn);
        result.map(|r| r == "PONG").unwrap_or(false)
    }

    pub fn list_docs(&self, doc_type: Option<&str>, status: Option<&str>) -> AuditResult<Vec<DocSummary>> {
        let mut conn = self.conn.lock().unwrap();
        let mut docs = Vec::new();

        if let Some(dt) = doc_type {
            let type_key = format!("docs:index:type:{}", dt);
            let ids: Vec<String> = redis::cmd("SMEMBERS")
                .arg(&type_key)
                .query(&mut *conn)
                .unwrap_or_default();

            for id in ids {
                if let Ok(summary) = self.get_doc_summary_internal(&mut *conn, &id) {
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
                    .query(&mut *conn)
                    .unwrap_or_default();
                
                for id in ids {
                    if let Ok(summary) = self.get_doc_summary_internal(&mut *conn, &id) {
                        docs.push(summary);
                    }
                }
            }
        } else {
            let keys: Vec<String> = redis::cmd("KEYS")
                .arg("doc:*")
                .query(&mut *conn)
                .unwrap_or_default();

            for key in keys {
                if key.contains(":version") {
                    continue;
                }
                if let Some(id) = key.strip_prefix("doc:") {
                    if let Ok(summary) = self.get_doc_summary_internal(&mut *conn, id) {
                        docs.push(summary);
                    }
                }
            }
        }

        docs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        
        Ok(docs)
    }

    fn get_doc_summary_internal(&self, conn: &mut redis::Connection, id: &str) -> AuditResult<DocSummary> {
        let key = format!("doc:{}", id);
        let result: Option<Vec<(String, String)>> = redis::cmd("HGETALL")
            .arg(&key)
            .query(conn)
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

    pub fn get_doc(&self, id: &str) -> AuditResult<serde_json::Value> {
        let mut conn = self.conn.lock().unwrap();
        let key = format!("doc:{}", id);
        let result: Option<Vec<(String, String)>> = redis::cmd("HGETALL")
            .arg(&key)
            .query(&mut *conn)
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

pub struct RedisSubscriber {
    url: String,
    tx: broadcast::Sender<String>,
    known_docs: Arc<tokio::sync::Mutex<HashSet<String>>>,
}

impl RedisSubscriber {
    pub fn new(url: &str) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            url: url.to_string(),
            tx,
            known_docs: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    pub fn start(&self) {
        let url = self.url.clone();
        let tx = self.tx.clone();
        let known_docs = self.known_docs.clone();
        
        std::thread::spawn(move || {
            let client = redis::Client::open(url.as_str()).expect("Failed to create Redis client");
            let mut conn = client.get_connection().expect("Failed to connect to Redis");
            
            println!("Redis change detector started, polling for changes...");
            
            loop {
                let keys: Vec<String> = redis::cmd("KEYS")
                    .arg("doc:*")
                    .query(&mut conn)
                    .unwrap_or_default();

                let mut known = known_docs.blocking_lock();
                
                for key in keys {
                    if key.contains(":version") {
                        continue;
                    }
                    if let Some(id) = key.strip_prefix("doc:") {
                        if !known.contains(id) {
                            known.insert(id.to_string());
                            println!("Redis: New doc detected: {}", id);
                            let _ = tx.send(id.to_string());
                        }
                    }
                }
                
                drop(known);
                std::thread::sleep(Duration::from_secs(2));
            }
        });
    }
}
