use agile_bus_mqtt::{EventBus, EventEnvelope};
use agile_common::types::{AuditEntry, Doc, DocType, Role};
use agile_config::{Config, TopicBuilder};
use agile_store_redis::RedisStore;
use serde_json::json;
use uuid::Uuid;

mod prompts;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("PO Agent starting...");

    let config = Config::load("config/agile-inc.toml")?;
    println!(
        "Config loaded: MQTT={}:{}, Redis={}",
        config.mqtt.host, config.mqtt.port, config.redis.url
    );

    let mut store = RedisStore::new(&config.redis.url)?;
    store.ping()?;
    println!("Redis connected");

    let bus = EventBus::new(
        &config.mqtt.host,
        config.mqtt.port,
        &format!("{}-po", config.mqtt.client_id_prefix),
        &config.mqtt.username,
        &config.mqtt.password,
        &config.mqtt.base_topic,
    )?;
    println!("MQTT bus connected");

    bus.subscribe_to_role("po")?;
    println!("Subscribed to PO inbox");

    println!("\n=== PO Agent Ready ===");
    println!("Listening for events...");

    loop {
        if let Some(event) = bus.try_recv_event() {
            println!(
                "\nReceived event: {} (trace: {})",
                event.r#type, event.trace_id
            );
            handle_event(&bus, &mut store, &event);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn handle_event(bus: &EventBus, store: &mut RedisStore, event: &EventEnvelope) {
    match event.r#type.as_str() {
        "story.new" | "story.create" => {
            handle_story_creation(bus, store, event);
        }
        "policy.changed" => {
            println!("Policy changed, acknowledging...");
        }
        _ => {
            println!("Unhandled event type: {}", event.r#type);
        }
    }
}

fn handle_story_creation(bus: &EventBus, store: &mut RedisStore, event: &EventEnvelope) {
    let story_id = Uuid::new_v4().to_string();
    let trace_id = event.trace_id.clone();
    let event_id = event.event_id.clone();

    let payload = &event.payload;
    let title = payload
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("New Story")
        .to_string();
    let body = payload
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("No description provided")
        .to_string();
    let acceptance_criteria: Vec<String> = payload
        .get("acceptance_criteria")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let definition_of_done: Vec<String> = payload
        .get("definition_of_done")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| {
            vec![
                "cargo test".to_string(),
                "cargo fmt --check".to_string(),
            ]
        });

    let mut doc = Doc::new(&story_id, DocType::Story, &title, &body, "ready", Role::Po);
    doc.trace_id = Some(trace_id.clone());
    doc.fields = json!({
        "acceptance_criteria": acceptance_criteria,
        "definition_of_done": definition_of_done,
    });

    if let Some(ref docs) = event.refs.get("docIds").and_then(|v| v.as_array()) {
        doc.links = docs.iter().filter_map(|v| v.as_str().map(String::from)).collect();
    }

    match store.doc_put(&doc, Some(&event_id)) {
        Ok(saved_doc) => {
            println!("Created story: {} - {}", saved_doc.id, saved_doc.title);

            let audit = AuditEntry::new(
                &trace_id,
                &event_id,
                Role::Po,
                "story.created",
                &format!("Created story: {}", title),
            );
            store.stream_append("events", &audit).ok();

            let prompt = prompts::generate_po_prompt(&saved_doc);
            println!("\n{}", prompt);

            let runlog_id = Uuid::new_v4().to_string();
            let mut runlog = Doc::new(
                &runlog_id,
                DocType::Runlog,
                &format!("PO Agent - Story {}", saved_doc.id),
                &prompt,
                "completed",
                Role::Po,
            );
            runlog.trace_id = Some(trace_id.clone());
            runlog.fields = json!({
                "event_id": event_id,
                "action": "story.created"
            });
            store.doc_put(&runlog, None).ok();

            let ready_event = EventEnvelope::new(
                &Uuid::new_v4().to_string(),
                &trace_id,
                "story.ready",
                "po",
                "dev",
                json!({
                    "storyId": saved_doc.id,
                    "title": saved_doc.title,
                }),
                json!({
                    "summary": format!("Story '{}' is ready for development", title)
                }),
            );
            bus.publish_to_role("dev", ready_event).ok();
            println!("Published story.ready event to Dev");
        }
        Err(e) => {
            println!("Failed to create story: {}", e);
        }
    }
}
