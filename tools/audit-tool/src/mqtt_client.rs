use std::sync::Arc;
use std::collections::VecDeque;
use std::thread;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};

#[derive(Error, Debug)]
pub enum MqttError {
    #[error("MQTT error: {0}")]
    Mqtt(String),
}

pub type MqttResult<T> = Result<T, MqttError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
    pub ts: String,
}

pub struct MqttSubscriber {
    messages: Arc<std::sync::Mutex<VecDeque<MqttMessage>>>,
}

impl MqttSubscriber {
    pub fn new(
        host: &str,
        port: u16,
        client_id: &str,
        username: &str,
        password: &str,
        base_topic: &str,
    ) -> MqttResult<Self> {
        let messages = Arc::new(std::sync::Mutex::new(VecDeque::new()));
        let messages_clone = messages.clone();

        let client_id_clone = client_id.to_string();
        let host_clone = host.to_string();
        let username_clone = username.to_string();
        let password_clone = password.to_string();
        let base_topic_clone = base_topic.to_string();

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async move {
                let mut mqttoptions = MqttOptions::new(&client_id_clone, &host_clone, port);
                mqttoptions.set_keep_alive(Duration::from_secs(10));

                if !username_clone.is_empty() {
                    mqttoptions.set_credentials(&username_clone, &password_clone);
                }

                let (client, mut eventloop) = AsyncClient::new(mqttoptions, 100);
                let topic = format!("{}/#", base_topic_clone);

                if let Err(e) = client.subscribe(&topic, QoS::AtMostOnce).await {
                    eprintln!("Failed to subscribe to {}: {}", topic, e);
                }

                loop {
                    match eventloop.poll().await {
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            let topic = publish.topic.clone();
                            let payload = String::from_utf8_lossy(&publish.payload).to_string();
                            let ts = chrono::Utc::now().to_rfc3339();
                            
                            let msg = MqttMessage { topic, payload, ts };
                            messages_clone.lock().unwrap().push_back(msg);
                            
                            if messages_clone.lock().unwrap().len() > 1000 {
                                messages_clone.lock().unwrap().pop_front();
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("MQTT error: {}", e);
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            });
        });

        Ok(Self { messages })
    }

    pub fn get_messages(&self, count: usize) -> Vec<MqttMessage> {
        let msgs = self.messages.lock().unwrap();
        msgs.iter()
            .rev()
            .take(count)
            .cloned()
            .collect()
    }

    pub fn clear_messages(&self) {
        self.messages.lock().unwrap().clear();
    }
}
