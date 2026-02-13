// Minimal MQTT wrapper skeleton
use rumqttc::{MqttOptions, Client, QoS};
use std::time::Duration;

pub fn connect(host:&str, port:u16, client_id:&str)->Client{
    let mut mqttoptions = MqttOptions::new(client_id, host, port);
    mqttoptions.set_keep_alive(Duration::from_secs(10));
    let (client, mut connection) = Client::new(mqttoptions, 10);
    client
}
