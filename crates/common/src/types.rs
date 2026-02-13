use serde::{Serialize,Deserialize};

#[derive(Serialize,Deserialize,Debug,Clone)]
pub struct EventEnvelope {
    pub event_id: String,
    pub trace_id: String,
    pub r#type: String,
    pub from: String,
    pub to: String,
    pub refs: serde_json::Value,
    pub payload: serde_json::Value,
    pub ts: String,
}

#[derive(Serialize,Deserialize,Debug,Clone)]
pub struct Doc {
    pub id: String,
    pub r#type: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub owner_role: String,
    pub tags: Option<Vec<String>>,
    pub links: Option<Vec<String>>,
    pub created_at: String,
    pub updated_at: String,
    pub trace_id: Option<String>,
    pub fields: Option<serde_json::Value>,
}
