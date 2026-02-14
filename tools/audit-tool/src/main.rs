use std::sync::{Arc, Mutex};

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
use redis_client::{RedisClient, RedisSubscriber};

#[derive(Clone)]
struct AppState {
    redis: Arc<Mutex<RedisClient>>,
    mqtt: Arc<MqttSubscriber>,
    redis_sub: Arc<RedisSubscriber>,
}

#[derive(Debug, Deserialize)]
struct DocQuery {
    doc_type: Option<String>,
    status: Option<String>,
    view: Option<String>,
}

async fn docs_handler(
    State(state): State<AppState>,
    Query(query): Query<DocQuery>,
) -> Html<String> {
    let mut redis = state.redis.lock().unwrap();
    let doc_type = query.doc_type.as_ref().filter(|s| !s.is_empty()).map(|s| s.as_str());
    let status = query.status.as_ref().filter(|s| !s.is_empty()).map(|s| s.as_str());
    let docs = redis
        .list_docs(doc_type, status)
        .unwrap_or_default();

    let mut html = String::from("<table class=\"striped\"><thead><tr><th>Type</th><th>Title</th><th>Status</th><th>Owner</th><th>Updated</th></tr></thead><tbody>");

    for doc in docs {
        let updated_short = doc.updated_at.chars().take(19).collect::<String>();
        html.push_str(&format!(
            "<tr hx-get=\"/api/docs/{}?view=pretty\" hx-target=\"#doc-detail\" hx-swap=\"innerHTML\" style=\"cursor:pointer\"><td><span class=\"tag\">{}</span></td><td>{}</td><td><span class=\"{}\">{}</span></td><td>{}</td><td>{}</td></tr>",
            doc.id,
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
    Query(query): Query<DocQuery>,
    State(state): State<AppState>,
) -> Html<String> {
    let mut redis = state.redis.lock().unwrap();
    match redis.get_doc(&id) {
        Ok(doc) => {
            if query.view.as_deref() == Some("raw") {
                let json = serde_json::to_string(&doc).unwrap_or_default();
                Html(format!("<pre><code>{}</code></pre>", json))
            } else {
                let pretty = render_doc_pretty(&doc);
                Html(format!("<div data-doc-id=\"{}\">{}</div>", id, pretty))
            }
        }
        Err(e) => Html(format!("<p>Error: {}</p>", e)),
    }
}

fn render_doc_pretty(doc: &serde_json::Value) -> String {
    let mut html = String::new();
    
    if let Some(obj) = doc.as_object() {
        for (key, value) in obj {
            html.push_str(&render_field(key, value));
        }
    } else {
        html.push_str(&format!("<pre><code>{}</code></pre>", doc));
    }
    
    html
}

fn render_field(name: &str, value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut html = format!("<div class=\"field-group\"><h3>{}</h3>", name);
            for (k, v) in map {
                html.push_str(&render_field(k, v));
            }
            html.push_str("</div>");
            html
        }
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                return format!("<div class=\"field-group\"><span class=\"field-label\">{}</span>: []</div>", name);
            }
            let mut html = format!("<div class=\"field-group\"><span class=\"field-label\">{}</span>: [", name);
            for (i, item) in arr.iter().enumerate() {
                if i > 0 { html.push_str(", "); }
                html.push_str(&format!("{:?}", item));
            }
            html.push_str("]</div>");
            html
        }
        serde_json::Value::String(s) => {
            if s.len() > 500 {
                format!("<div class=\"field-group\"><span class=\"field-label\">{}</span><pre>{}</pre></div>", name, html_escape(s))
            } else {
                format!("<div class=\"field-group\"><span class=\"field-label\">{}</span><p>{}</p></div>", name, html_escape(s))
            }
        }
        _ => {
            format!("<div class=\"field-group\"><span class=\"field-label\">{}</span><p>{:?}</p></div>", name, value)
        }
    }
}

async fn stream_handler(
    State(state): State<AppState>,
) -> Html<String> {
    let mqtt = state.mqtt.clone();
    let (messages, count) = mqtt.get_messages(50);
    
    let mut html = format!("<div data-msg-count=\"{}\">", count);
    for msg in messages.iter().rev() {
        html.push_str(&format!(
            "<div class=\"mqtt-msg\"><span class=\"topic\">{}</span> <span class=\"meta\">{} → {} [{}]</span></div>",
            msg.topic,
            msg.from,
            msg.to,
            msg.event_type
        ));
    }
    
    if messages.is_empty() {
        html.push_str("<p>Waiting for messages...</p>");
    }
    
    html.push_str("</div>");
    Html(html)
}

async fn sse_handler(
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    let mut rx = state.redis_sub.subscribe();
    
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    let event = axum::response::sse::Event::default()
                        .data(msg);
                    yield Ok::<_, std::convert::Infallible>(event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    
    axum::response::sse::Sse::new(stream)
}

async fn status_handler(
    State(state): State<AppState>,
) -> Html<String> {
    let redis_connected = {
        let mut redis = state.redis.lock().unwrap();
        redis.is_connected()
    };
    let mqtt_connected = state.mqtt.is_connected();
    
    let redis_class = if redis_connected { "status-success" } else { "status-error" };
    let redis_text = if redis_connected { "Connected" } else { "Disconnected" };
    
    let mqtt_class = if mqtt_connected { "status-success" } else { "status-error" };
    let mqtt_text = if mqtt_connected { "Connected" } else { "Disconnected" };
    
    Html(format!(
        r#"<div class="grid" style="grid-template-columns: 1fr 1fr; gap: 8px;">
            <div><span class="{}">●</span> Redis: {}</div>
            <div><span class="{}">●</span> MQTT: {}</div>
        </div>"#,
        redis_class, redis_text, mqtt_class, mqtt_text
    ))
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
        ":root { --pico-font-size: 14px; --pico-background: #ffffff; --pico-secondary-background: #ffffff; }",
        "html, body { height: 100%; margin: 0; padding: 0; overflow: hidden; }",
        "main { display: flex; flex-direction: column; height: 100vh; padding: 10px; box-sizing: border-box; }",
        ".header { display: flex; justify-content: space-between; align-items: center; padding: 10px 20px; border-bottom: 1px solid #ddd; flex-shrink: 0; }",
        ".header h1 { margin: 0; font-size: 1.5rem; }",
        ".header .status { display: flex; gap: 20px; font-size: 0.9rem; }",
        ".middle { flex: 3; display: flex; gap: 20px; padding: 10px; min-height: 0; overflow: hidden; }",
        ".bottom { flex: 1; min-height: 150px; border-top: 1px solid #ddd; padding: 10px 20px; overflow: auto; }",
        ".left-panel, .right-panel { flex: 1; display: flex; flex-direction: column; min-width: 0; overflow: hidden; }",
        ".left-panel h2, .right-panel h2, .bottom h2 { margin: 0 0 10px 0; font-size: 1.1rem; flex-shrink: 0; }",
        ".doc-list { flex: 1; overflow: auto; border: 1px solid #ddd; }",
        ".doc-list table { margin: 0; font-size: 0.85rem; }",
        ".doc-list th, .doc-list td { padding: 6px 8px; }",
        ".doc-detail { flex: 1; overflow: auto; border: 1px solid #ddd; padding: 10px; }",
        ".doc-detail pre { margin: 0; white-space: pre-wrap; word-break: break-all; font-size: 0.85rem; }",
        ".mqtt-msg { padding: 6px; margin: 3px 0; background: #f5f5f5; border-radius: 4px; font-family: monospace; font-size: 0.8rem; }",
        ".mqtt-msg .topic { color: #0066cc; font-weight: bold; }",
        ".status-success { color: #28a745; }",
        ".status-error { color: #dc3545; }",
        ".tag { background: #e0e0e0; color: #333; padding: 2px 6px; border-radius: 4px; font-size: 0.75em; }",
        ".field-group { margin-bottom: 15px; }",
        ".field-group h3 { font-size: 1rem; margin-bottom: 8px; color: #0066cc; }",
        ".field-group p { margin: 4px 0; font-size: 0.9rem; }",
        ".field-label { font-weight: bold; color: #666; font-size: 0.8rem; }",
        "</style>",
        "</head>",
        "<body>",
        "<main>",
        "<div class=\"header\">",
        "<h1>Agile Inc. Audit Tool</h1>",
        "<div class=\"status\" id=\"connection-status\" hx-get=\"/api/status\" hx-trigger=\"load, every 5s\" hx-swap=\"innerHTML\"></div>",
        "</div>",
        "<div class=\"middle\">",
        "<div class=\"left-panel\">",
        "<h2>Documents</h2>",
        "<div class=\"grid\" style=\"margin-bottom: 10px;\">",
        "<input type=\"text\" name=\"doc_type\" placeholder=\"Filter by type\" hx-get=\"/api/docs\" hx-trigger=\"change\" hx-target=\"#docs-table\" hx-debounce=\"300ms\">",
        "<input type=\"text\" name=\"status\" placeholder=\"Filter by status\" hx-get=\"/api/docs\" hx-trigger=\"change\" hx-target=\"#docs-table\" hx-debounce=\"300ms\">",
        "</div>",
        "<div id=\"docs-table\" class=\"doc-list\" hx-get=\"/api/docs\" hx-trigger=\"load, redis-msg\"><p>Loading documents...</p></div>",
        "</div>",
        "<div class=\"right-panel\">",
        "<h2>Document Detail</h2>",
        "<div id=\"doc-detail\" class=\"doc-detail\"><p>Select a document to view details</p></div>",
        "</div>",
        "</div>",
        "<div class=\"bottom\">",
        "<h2>MQTT Stream</h2>",
        "<button onclick=\"clearMessages()\" style=\"padding: 4px 8px; font-size: 0.8rem;\">Clear</button>",
        "<div id=\"mqtt-stream\" hx-get=\"/api/stream\" hx-trigger=\"load, every 2s, mqtt-msg\" hx-swap=\"innerHTML\"><p>Waiting for messages...</p></div>",
        "</div>",
        "</main>",
        "<script>",
        "const evtSource = new EventSource('/api/events');",
        "evtSource.onmessage = function(e) {",
        "  console.log('Redis event:', e.data);",
        "  htmx.trigger('#docs-table', 'redis-msg');",
        "};",
        "evtSource.onerror = function() {",
        "  console.log('SSE connection lost, reconnecting...');",
        "};",
        "function clearMessages(){document.getElementById('mqtt-stream').innerHTML='<p>Messages cleared</p>';}",
        "let lastMsgCount = 0;",
        "document.body.addEventListener('htmx:afterSwap', function(e) {",
        "  if (e.detail.target.id === 'mqtt-stream') {",
        "    const container = e.detail.target.closest('[data-msg-count]');",
        "    if (container) {",
        "      const count = parseInt(container.dataset.msgCount, 10);",
        "      if (count > lastMsgCount && lastMsgCount > 0) {",
        "        htmx.trigger('#docs-table', 'mqtt-msg');",
        "      }",
        "      lastMsgCount = count;",
        "    }",
        "  }",
        "});",
        "</script>",
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

    let redis_sub = Arc::new(RedisSubscriber::new(&config.redis.url));
    redis_sub.start();

    let state = AppState {
        redis: Arc::new(Mutex::new(redis)),
        mqtt: Arc::new(mqtt),
        redis_sub: redis_sub.clone(),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/api/docs", get(docs_handler))
        .route("/api/docs/:id", get(doc_detail_handler))
        .route("/api/stream", get(stream_handler))
        .route("/api/status", get(status_handler))
        .route("/api/events", get(sse_handler))
        .with_state(state);

    let port = 3000;
    println!("Starting audit tool on http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
