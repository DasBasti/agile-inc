use clap::{Parser, ValueEnum};
use serde_json::json;

#[derive(Parser, Debug)]
#[command(name = "publish_event")]
#[command(about = "Publish an event to the Agile Inc. event bus", long_about = None)]
struct Args {
    #[arg(long, default_value = "agileinc/role/po/inbox")]
    topic: String,

    #[arg(long, default_value = "story.create")]
    event_type: String,

    #[arg(long)]
    title: Option<String>,

    #[arg(long)]
    body: Option<String>,

    #[arg(long)]
    acceptance_criteria: Vec<String>,

    #[arg(long)]
    definition_of_done: Vec<String>,

    #[arg(long)]
    demo: bool,

    #[arg(long, default_value = "localhost")]
    mqtt_host: String,

    #[arg(long, default_value = "1883")]
    mqtt_port: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let (title, body, acceptance_criteria, definition_of_done) = if args.demo {
        (
            "Todo List Feature".to_string(),
            "Add a todo list to the app with add/remove/complete functionality".to_string(),
            vec![
                "User can add new todo items".to_string(),
                "User can mark items as complete".to_string(),
                "Items persist after app restart".to_string(),
            ],
            vec!["cargo test".to_string(), "cargo fmt --check".to_string()],
        )
    } else {
        (
            args.title.unwrap_or_else(|| "New Story".to_string()),
            args.body.unwrap_or_else(|| "No description provided".to_string()),
            args.acceptance_criteria,
            if args.definition_of_done.is_empty() {
                vec!["cargo test".to_string()]
            } else {
                args.definition_of_done
            },
        )
    };

    let event = agile_common::types::EventEnvelope::new(
        &uuid::Uuid::new_v4().to_string(),
        &uuid::Uuid::new_v4().to_string(),
        &args.event_type,
        "cli",
        "po",
        json!({}),
        json!({
            "title": title,
            "body": body,
            "acceptance_criteria": acceptance_criteria,
            "definition_of_done": definition_of_done
        }),
    );

    println!("Publishing event to {}...", args.topic);
    println!("Event: type={}, title={}", args.event_type, title);

    let bus = agile_bus_mqtt::EventBus::new(
        &args.mqtt_host,
        args.mqtt_port,
        "publish-cli",
        "",
        "",
        "agileinc",
    )?;

    bus.publish(&args.topic, event)?;

    println!("Event published successfully!");
    
    std::thread::sleep(std::time::Duration::from_millis(500));
    
    Ok(())
}
