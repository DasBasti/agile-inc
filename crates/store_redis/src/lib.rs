use std::sync::Mutex;
use thiserror::Error;

pub use agile_common::types::{
    AuditEntry, DedupRequest, DedupResult, DedupScope, Doc, DocType, Role,
};

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("redis error: {0}")]
    Redis(String),
    #[error("document not found: {0}")]
    NotFound(String),
    #[error("serialization error: {0}")]
    Serde(String),
}

pub type StoreResult<T> = Result<T, StoreError>;

pub struct RedisStore {
    conn: Mutex<redis::Connection>,
}

impl RedisStore {
    pub fn new(url: &str) -> StoreResult<Self> {
        let client = redis::Client::open(url).map_err(|e| StoreError::Redis(e.to_string()))?;
        let conn = client.get_connection().map_err(|e| StoreError::Redis(e.to_string()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn ping(&self) -> StoreResult<bool> {
        let r: String = redis::cmd("PING")
            .query(&mut *self.conn.lock().unwrap())
            .map_err(|e| StoreError::Redis(e.to_string()))?;
        Ok(r == "PONG")
    }

    fn doc_key(&self, id: &str) -> String {
        format!("doc:{}", id)
    }

    fn type_index_key(&self, doc_type: &DocType) -> String {
        format!("docs:index:type:{}", doc_type)
    }

    fn status_index_key(&self, doc_type: &DocType, status: &str) -> String {
        format!("docs:index:status:{}:{}", doc_type, status)
    }

    fn tags_index_key(&self, tag: &str) -> String {
        format!("docs:index:tag:{}", tag)
    }

    fn version_key(&self, id: &str) -> String {
        format!("doc:{}:version", id)
    }

    fn idempotency_key(&self, key: &str) -> String {
        format!("idempotency:{}", key)
    }

    pub fn doc_get(&self, id: &str) -> StoreResult<Doc> {
        let key = self.doc_key(id);
        let mut conn = self.conn.lock().unwrap();
        let doc: Option<Vec<(String, String)>> = redis::cmd("HGETALL")
            .arg(&key)
            .query(&mut *conn)
            .map_err(|e| StoreError::Redis(e.to_string()))?;

        match doc {
            Some(fields) => {
                let mut doc = Doc::new(
                    id,
                    DocType::Runlog,
                    "",
                    "",
                    "unknown",
                    Role::System,
                );
                for (field, value) in fields {
                    match field.as_str() {
                        "type" => doc.r#type = value.parse().unwrap_or(DocType::Runlog),
                        "title" => doc.title = value,
                        "body" => doc.body = value,
                        "status" => doc.status = value,
                        "owner_role" => doc.owner_role = value.parse().unwrap_or(Role::System),
                        "tags" => doc.tags = serde_json::from_str(&value).unwrap_or_default(),
                        "links" => doc.links = serde_json::from_str(&value).unwrap_or_default(),
                        "created_at" => doc.created_at = value,
                        "updated_at" => doc.updated_at = value,
                        "trace_id" => doc.trace_id = Some(value),
                        "version" => doc.version = value.parse().unwrap_or(1),
                        "fields" => doc.fields = serde_json::from_str(&value).unwrap_or_default(),
                        _ => {}
                    }
                }
                Ok(doc)
            }
            None => Err(StoreError::NotFound(id.to_string())),
        }
    }

    pub fn doc_put(&self, doc: &Doc, idempotency_key: Option<&str>) -> StoreResult<Doc> {
        if let Some(ik) = idempotency_key {
            let ik_key = self.idempotency_key(ik);
            let mut conn = self.conn.lock().unwrap();
            let existing: Option<String> = redis::cmd("GET")
                .arg(&ik_key)
                .query(&mut *conn)
                .map_err(|e| StoreError::Redis(e.to_string()))?;
            if let Some(existing_id) = existing {
                drop(conn);
                return self.doc_get(&existing_id);
            }
        }

        let key = self.doc_key(&doc.id);
        let mut conn = self.conn.lock().unwrap();

        redis::cmd("HSET")
            .arg(&key)
            .arg("id")
            .arg(&doc.id)
            .arg("type")
            .arg(doc.r#type.to_string())
            .arg("title")
            .arg(&doc.title)
            .arg("body")
            .arg(&doc.body)
            .arg("status")
            .arg(&doc.status)
            .arg("owner_role")
            .arg(doc.owner_role.to_string())
            .arg("tags")
            .arg(serde_json::to_string(&doc.tags).map_err(|e| StoreError::Serde(e.to_string()))?)
            .arg("links")
            .arg(serde_json::to_string(&doc.links).map_err(|e| StoreError::Serde(e.to_string()))?)
            .arg("created_at")
            .arg(&doc.created_at)
            .arg("updated_at")
            .arg(&doc.updated_at)
            .arg("trace_id")
            .arg(doc.trace_id.as_deref().unwrap_or(""))
            .arg("version")
            .arg(doc.version.to_string())
            .arg("fields")
            .arg(serde_json::to_string(&doc.fields).map_err(|e| StoreError::Serde(e.to_string()))?)
            .query(&mut *conn)
            .map_err(|e| StoreError::Redis(e.to_string()))?;

        redis::cmd("SADD")
            .arg(self.type_index_key(&doc.r#type))
            .arg(&doc.id)
            .query(&mut *conn)
            .map_err(|e| StoreError::Redis(e.to_string()))?;

        redis::cmd("SADD")
            .arg(self.status_index_key(&doc.r#type, &doc.status))
            .arg(&doc.id)
            .query(&mut *conn)
            .map_err(|e| StoreError::Redis(e.to_string()))?;

        for tag in &doc.tags {
            redis::cmd("SADD")
                .arg(self.tags_index_key(tag))
                .arg(&doc.id)
                .query(&mut *conn)
                .map_err(|e| StoreError::Redis(e.to_string()))?;
        }

        redis::cmd("SET")
            .arg(self.version_key(&doc.id))
            .arg(doc.version.to_string())
            .query(&mut *conn)
            .map_err(|e| StoreError::Redis(e.to_string()))?;

        if let Some(ik) = idempotency_key {
            let ik_key = self.idempotency_key(ik);
            redis::cmd("SET")
                .arg(&ik_key)
                .arg(&doc.id)
                .query(&mut *conn)
                .map_err(|e| StoreError::Redis(e.to_string()))?;
        }

        Ok(doc.clone())
    }

    pub fn dedupe_check_and_mark(&self, req: &DedupRequest) -> StoreResult<DedupResult> {
        let key = match req.scope {
            DedupScope::Global => self.idempotency_key(&req.key),
            DedupScope::Role => {
                if let Some(role) = &req.role {
                    format!("idempotency:{}:{}", role, req.key)
                } else {
                    self.idempotency_key(&req.key)
                }
            }
        };

        let mut conn = self.conn.lock().unwrap();
        let result: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(req.ttl_seconds)
            .query(&mut *conn)
            .map_err(|e| StoreError::Redis(e.to_string()))?;

        Ok(DedupResult {
            first_time: result.is_some(),
        })
    }

    pub fn stream_append(&self, stream: &str, entry: &AuditEntry) -> StoreResult<String> {
        let mut conn = self.conn.lock().unwrap();
        let id: String = redis::cmd("XADD")
            .arg(format!("stream:{}", stream))
            .arg("*")
            .arg("ts")
            .arg(&entry.ts)
            .arg("trace_id")
            .arg(&entry.trace_id)
            .arg("event_id")
            .arg(&entry.event_id)
            .arg("actor_role")
            .arg(entry.actor_role.to_string())
            .arg("action")
            .arg(&entry.action)
            .arg("refs")
            .arg(serde_json::to_string(&entry.refs).map_err(|e| StoreError::Serde(e.to_string()))?)
            .arg("summary")
            .arg(&entry.summary)
            .arg("details")
            .arg(serde_json::to_string(&entry.details).map_err(|e| StoreError::Serde(e.to_string()))?)
            .query(&mut *conn)
            .map_err(|e| StoreError::Redis(e.to_string()))?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> RedisStore {
        RedisStore::new("redis://localhost:49155").unwrap()
    }

    #[test]
    fn test_ping() {
        let store = test_store();
        assert!(store.ping().unwrap());
    }
}
