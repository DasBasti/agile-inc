use std::sync::mpsc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc as tokio_mpsc;
use tokio::runtime::Runtime;

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

enum BusCommand {
    Publish(String, String),
    Subscribe(String),
    Shutdown,
}

pub struct EventBus {
    sender: tokio_mpsc::Sender<BusCommand>,
    receiver: mpsc::Receiver<EventEnvelope>,
    base_topic: String,
    _runtime: Runtime,
}

impl EventBus {
    pub fn new(
        host: &str,
        port: u16,
        client_id: &str,
        username: &str,
        password: &str,
        base_topic: &str,
    ) -> BusResult<Self> {
        let (cmd_tx, mut cmd_rx) = tokio_mpsc::channel::<BusCommand>(100);
        let (event_tx, event_rx) = mpsc::channel::<EventEnvelope>();

        let client_id_clone = client_id.to_string();
        let host_clone = host.to_string();
        let username_clone = username.to_string();
        let password_clone = password.to_string();

        let runtime = Runtime::new().map_err(|e| BusError::Mqtt(e.to_string()))?;

        let runtime_handle = runtime.handle().clone();
        runtime_handle.spawn(async move {
            let mut mqttoptions = rumqttc::MqttOptions::new(&client_id_clone, &host_clone, port);
            mqttoptions.set_keep_alive(Duration::from_secs(10));

            if !username_clone.is_empty() {
                mqttoptions.set_credentials(&username_clone, &password_clone);
            }

            let (client, mut eventloop) = rumqttc::AsyncClient::new(mqttoptions, 100);

            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(BusCommand::Publish(topic, payload)) => {
                                if let Err(e) = client.try_publish(topic, rumqttc::QoS::AtMostOnce, false, payload) {
                                    eprintln!("MQTT publish error: {}", e);
                                }
                            }
                            Some(BusCommand::Subscribe(topic)) => {
                                if let Err(e) = client.subscribe(&topic, rumqttc::QoS::AtMostOnce).await {
                                    eprintln!("MQTT subscribe error: {}", e);
                                } else {
                                    println!("Subscribed to {}", topic);
                                }
                            }
                            Some(BusCommand::Shutdown) => {
                                println!("MQTT task shutting down");
                                break;
                            }
                            None => {
                                println!("Command channel closed, MQTT task exiting");
                                break;
                            }
                        }
                    }
                    event = eventloop.poll() => {
                        match event {
                            Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish))) => {
                                let payload = String::from_utf8_lossy(&publish.payload);
                                if let Ok(envelope) = serde_json::from_str::<EventEnvelope>(&payload) {
                                    if let Err(e) = event_tx.send(envelope) {
                                        println!("Event receiver dropped: {}", e);
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("MQTT error: {}", e);
                                tokio::time::sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            sender: cmd_tx,
            receiver: event_rx,
            base_topic: base_topic.to_string(),
            _runtime: runtime,
        })
    }

    pub fn publish(&self, topic: &str, event: EventEnvelope) -> BusResult<()> {
        let payload = serde_json::to_string(&event).map_err(|e| BusError::Serde(e.to_string()))?;
        
        self.sender
            .try_send(BusCommand::Publish(topic.to_string(), payload))
            .map_err(|_| BusError::ChannelClosed)?;
        
        Ok(())
    }

    pub fn publish_to_role(&self, role: &str, event: EventEnvelope) -> BusResult<()> {
        let topic = format!("{}/role/{}/inbox", self.base_topic, role);
        self.publish(&topic, event)
    }

    pub fn subscribe_to_role(&self, role: &str) -> BusResult<()> {
        let topic = format!("{}/role/{}/inbox", self.base_topic, role);
        
        self.sender
            .try_send(BusCommand::Subscribe(topic))
            .map_err(|_| BusError::ChannelClosed)?;
        
        Ok(())
    }

    pub fn subscribe_to_events(&self) -> BusResult<()> {
        let topic = format!("{}/events", self.base_topic);
        
        self.sender
            .try_send(BusCommand::Subscribe(topic))
            .map_err(|_| BusError::ChannelClosed)?;
        
        Ok(())
    }

    pub fn try_recv_event(&self) -> Option<EventEnvelope> {
        self.receiver.try_recv().ok()
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
