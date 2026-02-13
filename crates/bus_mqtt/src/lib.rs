use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use thiserror::Error;

pub use agile_common::types::EventEnvelope;

#[derive(Error, Debug)]
pub enum BusError {
    #[error("mqtt error: {0}")]
    Mqtt(String),
    #[error("serialization error: {0}")]
    Serde(String),
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("timeout")]
    Timeout,
    #[error("channel closed")]
    ChannelClosed,
}

pub type BusResult<T> = Result<T, BusError>;

pub struct EventBus {
    topics: Arc<Mutex<VecDeque<(String, EventEnvelope)>>>,
    base_topic: String,
}

impl EventBus {
    pub fn new(
        _host: &str,
        _port: u16,
        _client_id: &str,
        _username: &str,
        _password: &str,
        base_topic: &str,
    ) -> BusResult<Self> {
        Ok(Self {
            topics: Arc::new(Mutex::new(VecDeque::new())),
            base_topic: base_topic.to_string(),
        })
    }

    pub fn publish(&self, topic: &str, event: EventEnvelope) -> BusResult<()> {
        let full_topic = format!("{}/{}", self.base_topic, topic);
        self.topics.lock().unwrap().push_back((full_topic, event));
        Ok(())
    }

    pub fn publish_to_role(&self, role: &str, event: EventEnvelope) -> BusResult<()> {
        let topic = format!("{}/role/{}/inbox", self.base_topic, role);
        self.publish(&topic, event)
    }

    pub fn subscribe_to_role(&self, _role: &str) -> BusResult<()> {
        Ok(())
    }

    pub fn subscribe_to_events(&self) -> BusResult<()> {
        Ok(())
    }

    pub fn try_recv_event(&self) -> Option<EventEnvelope> {
        self.topics.lock().unwrap().pop_front().map(|(_, e)| e)
    }

    pub fn poll_event(&self) -> BusResult<EventEnvelope> {
        self.try_recv_event().ok_or(BusError::ChannelClosed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = EventEnvelope::new(
            "evt-123",
            "trace-456",
            "story.ready",
            "po",
            "dev",
            serde_json::json!({}),
            serde_json::json!({"summary": "test"}),
        );
        let json = serde_json::to_string(&event).unwrap();
        let parsed: EventEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_id, "evt-123");
    }
}
