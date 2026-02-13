use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    extract::{Query, State, Path},
    response::Html,
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::json;

mod mqtt_client;
mod redis_client;

use mqtt_client::MqttSubscriber;
use redis_client::RedisClient;

#[derive(Clone)]
struct AppState {
    redis: Arc<Mutex<RedisClient>>,
    mqtt: Arc<MqttSubscriber>,
}

#[derive(Debug, Deserialize)]
struct DocQuery {
    doc_type: Option<String>,
    status: Option<String>,
}

async fn docs_handler(
    State(state): State<AppState>,
    Query(query): Query<DocQuery>,
) -> Html<String> {
    let mut redis = state.redis.lock().unwrap();
    let docs = redis
        .list_docs(query.doc_type.as_deref(), query.status.as_deref())
        .unwrap_or_default();

    let mut html = String::from("<table class=\"striped\"><thead><tr><th>ID</th><th>Type</th><th>Title</th><th>Status</th><th>Owner</th><th>Updated</th></tr></thead><tbody>");

    for doc in docs {
        let id_short = doc.id.chars().take(8).collect::<String>();
        let updated_short = doc.updated_at.chars().take(19).collect::<String>();
        html.push_str(&format!(
            "<tr hx-get=\"/api/docs/{}\" hx-target=\"#doc-detail\" hx-swap=\"innerHTML\" style=\"cursor:pointer\"><td>{}</td><td><span class=\"tag\">{}</span></td><td>{}</td><td><span class=\"{}\">{}</span></td><td>{}</td><td>{}</td></tr>",
            doc.id,
            id_short,
            doc.doc_type,
            doc.title,
            status_class(&doc.status),
            doc.status,
            doc.owner_role,
            updated_short
        ));
    }

    html.push_str("</tbody></table>");
    Html(html)
}

fn status_class(status: &str) -> &'static str {
    match status {
        "ready" | "completed" | "passed" => "status-success",
        "failed" | "error" | "open" => "status-error",
        "in_progress" => "status-warning",
        _ => "",
    }
}

async fn doc_detail_handler(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Html<String> {
    let mut redis = state.redis.lock().unwrap();
    match redis.get_doc(&id) {
        Ok(doc) => {
            let json = serde_json::to_string_pretty(&doc).unwrap_or_default();
            Html(format!("<pre><code>{}</code></pre>", json))
        }
        Err(e) => Html(format!("<p>Error: {}</p>", e)),
    }
}

async fn stream_handler(
    State(state): State<AppState>,
) -> Html<String> {
    let mqtt = state.mqtt.clone();
    let messages = mqtt.get_messages(50);
    
    let mut html = String::new();
    for msg in messages.iter().rev() {
        let payload_short = if msg.payload.len() > 100 {
            format!("{}...", &msg.payload[..100])
        } else {
            msg.payload.clone()
        };
        html.push_str(&format!(
            "<div class=\"mqtt-msg\"><span class=\"topic\">{}</span> <span class=\"payload\">{}</span></div>",
            msg.topic,
            html_escape(&payload_short)
        ));
    }
    
    if html.is_empty() {
        html = "<p>Waiting for messages...</p>".to_string();
    }
    
    Html(html)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

async fn index() -> Html<String> {
    let html = concat!(
        "<!DOCTYPE html>",
        "<html lang=\"en\">",
        "<head>",
        "<meta charset=\"UTF-8\">",
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">",
        "<title>Agile Inc. Audit Tool</title>",
        "<link rel=\"stylesheet\" href=\"https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css\">",
        "<script src=\"https://unpkg.com/htmx.org@1.9.10\"></script>",
        "<style>",
        ":root { --pico-font-size: 14px; }",
        ".container { max-width: 1400px; }",
        ".grid { grid-template-columns: 1fr 1fr; }",
        "@media (max-width: 900px) { .grid { grid-template-columns: 1fr; } }",
        ".status-success { color: #28a745; font-weight: bold; }",
        ".status-error { color: #dc3545; font-weight: bold; }",
        ".status-warning { color: #ffc107; font-weight: bold; }",
        ".tag { background: var(--pico-primary-background); color: var(--pico-primary-color); padding: 2px 8px; border-radius: 4px; font-size: 0.8em; }",
        ".mqtt-msg { padding: 8px; margin: 4px 0; background: var(--pico-secondary-background); border-radius: 4px; font-family: monospace; font-size: 0.85em; }",
        ".mqtt-msg .topic { color: var(--pico-primary-color); font-weight: bold; }",
        "#doc-detail { padding: 16px; background: var(--pico-secondary-background); border-radius: 8px; margin-top: 16px; }",
        "#doc-detail pre { margin: 0; white-space: pre-wrap; word-break: break-all; }",
        "</style>",
        "</head>",
        "<body>",
        "<main class=\"container\">",
        "<h1>Agile Inc. Audit Tool</h1>",
        "<div class=\"grid\">",
        "<section>",
        "<h2>Documents</h2>",
        "<form>",
        "<div class=\"grid\">",
        "<input type=\"text\" name=\"doc_type\" placeholder=\"Filter by type\" hx-get=\"/api/docs\" hx-trigger=\"change\" hx-target=\"#docs-table\" hx-debounce=\"300ms\">",
        "<input type=\"text\" name=\"status\" placeholder=\"Filter by status\" hx-get=\"/api/docs\" hx-trigger=\"change\" hx-target=\"#docs-table\" hx-debounce=\"300ms\">",
        "</div>",
        "</form>",
        "<div id=\"docs-table\" hx-get=\"/api/docs\" hx-trigger=\"load\"><p>Loading documents...</p></div>",
        "<details><summary>Document Detail</summary><div id=\"doc-detail\"><p>Click a document to view details</p></div></details>",
        "</section>",
        "<section>",
        "<h2>MQTT Stream</h2>",
        "<button onclick=\"clearMessages()\">Clear</button>",
        "<div id=\"mqtt-stream\" hx-get=\"/api/stream\" hx-trigger=\"load, every 2s\" hx-swap=\"innerHTML\"><p>Waiting for messages...</p></div>",
        "</section>",
        "</div>",
        "</main>",
        "<script>function clearMessages(){document.getElementById('mqtt-stream').innerHTML='<p>Messages cleared</p>';}</script>",
        "</body>",
        "</html>"
    );
    Html(html.to_string())
}

#[tokio::main]
async fn main() {
    let config = agile_config::Config::load("config/agile-inc.toml").unwrap_or_else(|_| {
        eprintln!("Warning: Could not load config, using defaults");
        agile_config::Config::default()
    });

    let redis = RedisClient::new(&config.redis.url).expect("Failed to connect to Redis");
    let mqtt = MqttSubscriber::new(
        &config.mqtt.host,
        config.mqtt.port,
        "audit-tool",
        &config.mqtt.username,
        &config.mqtt.password,
        &config.mqtt.base_topic,
    )
    .expect("Failed to connect to MQTT");

    let state = AppState {
        redis: Arc::new(Mutex::new(redis)),
        mqtt: Arc::new(mqtt),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/api/docs", get(docs_handler))
        .route("/api/docs/:id", get(doc_detail_handler))
        .route("/api/stream", get(stream_handler))
        .with_state(state);

    let port = 3000;
    println!("Starting audit tool on http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
