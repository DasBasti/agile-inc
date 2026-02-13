use agile_bus_mqtt::{EventBus, EventEnvelope};
use agile_common::types::{
    AuditEntry, ChangeBudget, DedupRequest, DedupResult, DedupScope, Doc, DocType,
    ExecJob, OutputLimits, Role,
};
use agile_config::Config;
use agile_llm::LlmClient;
use agile_opencode_runner::{diffstat, run as run_exec, RunnerError};
use agile_store_redis::RedisStore;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

mod prompts;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Dev Agent starting...");

    let config = Config::load("config/agile-inc.toml")?;
    println!(
        "Config loaded: MQTT={}:{}, Redis={}, LLM={}",
        config.mqtt.host, config.mqtt.port, config.redis.url, config.llm.model
    );

    let store = RedisStore::new(&config.redis.url)?;
    store.ping()?;
    println!("Redis connected");

    let bus = EventBus::new(
        &config.mqtt.host,
        config.mqtt.port,
        &format!("{}-dev", config.mqtt.client_id_prefix),
        &config.mqtt.username,
        &config.mqtt.password,
        &config.mqtt.base_topic,
    )?;
    println!("MQTT bus connected");

    let llm = LlmClient::new(
        &config.llm.model,
        config.llm.temperature,
        config.llm.max_tokens,
        &config.llm.base_url,
    )?;
    println!("LLM client connected: {}", config.llm.model);

    bus.subscribe_to_role("dev")?;
    println!("Subscribed to Dev inbox");

    println!("\n=== Dev Agent Ready ===");
    println!("Listening for events...");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    loop {
        if let Some(event) = bus.try_recv_event() {
            println!(
                "\nReceived event: {} (trace: {}, event: {})",
                event.r#type, event.trace_id, event.event_id
            );
            rt.block_on(handle_event(&config, &bus, &store, &llm, &event));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn handle_event(
    config: &Config,
    bus: &EventBus,
    store: &RedisStore,
    llm: &LlmClient,
    event: &EventEnvelope,
) {
    match event.r#type.as_str() {
        "story.ready" => {
            handle_story_ready(config, bus, store, llm, event).await;
        }
        "bug.new" => {
            handle_bug_new(config, bus, store, llm, event).await;
        }
        _ => {
            println!("Unhandled event type: {}", event.r#type);
        }
    }
}

async fn handle_story_ready(
    config: &Config,
    bus: &EventBus,
    store: &RedisStore,
    llm: &LlmClient,
    event: &EventEnvelope,
) {
    let trace_id = event.trace_id.clone();
    let event_id = event.event_id.clone();

    let dedupe_result = store
        .dedupe_check_and_mark(&DedupRequest {
            key: event_id.clone(),
            ttl_seconds: config.redis.dedupe_ttl_seconds,
            scope: DedupScope::Global,
            role: None,
        })
        .unwrap_or(DedupResult { first_time: false });

    if !dedupe_result.first_time {
        println!("Event {} already processed, skipping", event_id);
        return;
    }

    let story_id = event
        .refs
        .get("storyId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if story_id.is_empty() {
        println!("No storyId in event refs");
        return;
    }

    let story = match store.doc_get(story_id) {
        Ok(s) => s,
        Err(e) => {
            println!("Failed to get story {}: {}", story_id, e);
            return;
        }
    };

    println!("Processing story: {} - {}", story.id, story.title);

    let workdir = config.opencode.workdir.clone();
    let opencode_command = config.opencode.command.clone();
    let default_args = config.opencode.default_args.clone();
    let timeout = config.opencode.timeout_seconds;

    let job_id = Uuid::new_v4().to_string();

    let fields = story.fields.as_object();
    let dod_gates: Vec<String> = fields
        .and_then(|f| f.get("definition_of_done"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["cargo test".to_string()]);

    let prompt = prompts::generate_dev_prompt(&story, &dod_gates);
    println!("\nGenerating LLM response...");

    let llm_response = match llm.generate(&prompt) {
        Ok(response) => {
            println!("LLM response received ({} chars)", response.len());
            response
        }
        Err(e) => {
            println!("LLM call failed: {}", e);
            println!("Rolling back - emitting story.retry");
            
            let retry_event = EventEnvelope::new(
                &Uuid::new_v4().to_string(),
                &trace_id,
                "story.retry",
                "dev",
                "dev",
                json!({ "storyId": story.id }),
                json!({ "summary": format!("Story {} needs retry: {}", story.id, e) }),
            );
            bus.publish_to_role("dev", retry_event).ok();
            return;
        }
    };

    let runlog_id = Uuid::new_v4().to_string();
    let mut runlog = Doc::new(
        &runlog_id,
        DocType::Runlog,
        &format!("Dev Agent - Story {}", story.id),
        &format!("PROMPT:\n{}\n\nLLM RESPONSE:\n{}", prompt, llm_response),
        "in_progress",
        Role::Dev,
    );
    runlog.trace_id = Some(trace_id.clone());
    runlog.fields = json!({
        "event_id": event_id,
        "action": "story.processing",
        "story_id": story.id,
        "llm_response": llm_response
    });
    store.doc_put(&runlog, None).ok();

    let mut all_args = default_args.clone();
    all_args.push("--run".to_string());
    all_args.push(prompt.clone());

    let job = ExecJob {
        trace_id: trace_id.clone(),
        job_id: job_id.clone(),
        command: opencode_command,
        args: all_args,
        workdir: workdir.clone(),
        timeout_seconds: timeout,
        env: HashMap::new(),
        output_limits: OutputLimits::new(config.opencode.max_output_kb),
    };

    println!("Executing opencode...");
    let result = run_exec(&job).await;

    let output_summary = match &result {
        Ok(res) => format!("exit_code={}, duration={}ms", res.exit_code, res.duration_ms),
        Err(e) => format!("error: {}", e),
    };
    println!("Opencode result: {}", output_summary);

    let mut runlog_output = runlog;
    runlog_output.body = format!("{}\n\nOutput: {}", runlog_output.body, output_summary);
    runlog_output.status = match &result {
        Ok(r) if r.exit_code == 0 => "completed".to_string(),
        Ok(_) => "failed".to_string(),
        Err(_) => "error".to_string(),
    };
    store.doc_put(&runlog_output, None).ok();

    let diff = diffstat(&workdir).await;

    let change_budget = ChangeBudget::default();
    let within_budget = check_change_budget(&diff, &change_budget);

    let audit = AuditEntry::new(
        &trace_id,
        &event_id,
        Role::Dev,
        "build.attempt",
        &format!(
            "Story {} - opencode executed, within_budget={}",
            story.id, within_budget
        ),
    );
    store.stream_append("events", &audit).ok();

    let event_type = if within_budget { "build.done" } else { "dev.needs_approval" };

    let response_event = EventEnvelope::new(
        &Uuid::new_v4().to_string(),
        &trace_id,
        event_type,
        "dev",
        if within_budget { "qa" } else { "po" },
        json!({
            "storyId": story.id,
            "title": story.title,
            "jobId": job_id,
        }),
        json!({
            "summary": if within_budget {
                format!("Build completed for story '{}'", story.title)
            } else {
                format!(
                    "Change budget exceeded for story '{}': files={}, ins={}, del={}",
                    story.title,
                    diff.as_ref().map(|d| d.files_changed).unwrap_or(0),
                    diff.as_ref().map(|d| d.insertions).unwrap_or(0),
                    diff.as_ref().map(|d| d.deletions).unwrap_or(0)
                )
            },
            "exit_code": result.as_ref().map(|r| r.exit_code).unwrap_or(-1),
            "within_budget": within_budget,
        }),
    );

    bus.publish_to_role(if within_budget { "qa" } else { "po" }, response_event).ok();

    println!(
        "Published {} event to {}",
        event_type,
        if within_budget { "QA" } else { "PO" }
    );

    if let Ok(d) = &diff {
        println!(
            "Change stats: {} files, +{} lines, -{} lines",
            d.files_changed, d.insertions, d.deletions
        );
    }
}

async fn handle_bug_new(
    config: &Config,
    bus: &EventBus,
    store: &RedisStore,
    llm: &LlmClient,
    event: &EventEnvelope,
) {
    let trace_id = event.trace_id.clone();
    let event_id = event.event_id.clone();

    let bug_id = event.refs.get("bugId").and_then(|v| v.as_str()).unwrap_or("");

    if bug_id.is_empty() {
        println!("No bugId in event refs");
        return;
    }

    let bug = match store.doc_get(bug_id) {
        Ok(b) => b,
        Err(e) => {
            println!("Failed to get bug {}: {}", bug_id, e);
            return;
        }
    };

    println!("Processing bug: {} - {}", bug.id, bug.title);

    let prompt = prompts::generate_fix_prompt(&bug);
    println!("\nGenerating LLM response...");

    let llm_response = match llm.generate(&prompt) {
        Ok(response) => {
            println!("LLM response received ({} chars)", response.len());
            response
        }
        Err(e) => {
            println!("LLM call failed: {}", e);
            println!("Rolling back - emitting bug.retry");
            
            let retry_event = EventEnvelope::new(
                &Uuid::new_v4().to_string(),
                &trace_id,
                "bug.retry",
                "dev",
                "dev",
                json!({ "bugId": bug.id }),
                json!({ "summary": format!("Bug {} needs retry: {}", bug.id, e) }),
            );
            bus.publish_to_role("dev", retry_event).ok();
            return;
        }
    };

    let runlog_id = Uuid::new_v4().to_string();
    let mut runlog = Doc::new(
        &runlog_id,
        DocType::Runlog,
        &format!("Dev Agent - Bug Fix {}", bug.id),
        &format!("PROMPT:\n{}\n\nLLM RESPONSE:\n{}", prompt, llm_response),
        "in_progress",
        Role::Dev,
    );
    runlog.trace_id = Some(trace_id.clone());
    runlog.fields = json!({
        "event_id": event_id,
        "action": "bug.fix",
        "bug_id": bug.id,
        "llm_response": llm_response
    });
    store.doc_put(&runlog, None).ok();

    let event = EventEnvelope::new(
        &Uuid::new_v4().to_string(),
        &trace_id,
        "bug.acknowledged",
        "dev",
        "qa",
        json!({ "bugId": bug.id }),
        json!({ "summary": format!("Bug '{}' acknowledged, fix in progress", bug.title) }),
    );
    bus.publish_to_role("qa", event).ok();
}

fn check_change_budget(
    diff: &Result<agile_common::types::DiffStat, RunnerError>,
    budget: &ChangeBudget,
) -> bool {
    let d = match diff {
        Ok(d) => d,
        Err(_) => return true,
    };

    d.files_changed <= budget.max_files as u32
        && d.insertions <= budget.max_insertions as u32
        && d.deletions <= budget.max_deletions as u32
}
