use agile_bus_mqtt::{EventBus, EventEnvelope};
use agile_common::types::{
    AuditEntry, DedupRequest, DedupResult, DedupScope, Doc, DocType, ExecJob, OutputLimits, Role,
};
use agile_config::Config;
use agile_llm::LlmClient;
use agile_opencode_runner::run as run_exec;
use agile_store_redis::RedisStore;
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

mod prompts;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("QA Agent starting...");

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
        &format!("{}-qa", config.mqtt.client_id_prefix),
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

    bus.subscribe_to_role("qa")?;
    println!("Subscribed to QA inbox");

    println!("\n=== QA Agent Ready ===");
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
        "build.done" => {
            handle_build_done(config, bus, store, llm, event).await;
        }
        "bug.new" => {
            handle_bug_new(config, bus, store, llm, event).await;
        }
        _ => {
            println!("Unhandled event type: {}", event.r#type);
        }
    }
}

async fn handle_build_done(
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

    println!("Testing story: {} - {}", story.id, story.title);

    let fields = story.fields.as_object();
    let dod_gates: Vec<String> = fields
        .and_then(|f| f.get("definition_of_done"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["cargo test".to_string()]);

    let mut all_passed = true;
    let mut gate_results: Vec<(String, bool, String)> = Vec::new();

    for gate in &dod_gates {
        println!("Running DoD gate: {}", gate);

        let job = ExecJob {
            trace_id: trace_id.clone(),
            job_id: Uuid::new_v4().to_string(),
            command: gate.split_whitespace().next().unwrap_or("cargo").to_string(),
            args: gate.split_whitespace().skip(1).map(String::from).collect(),
            workdir: config.opencode.workdir.clone(),
            timeout_seconds: 300,
            env: HashMap::new(),
            output_limits: OutputLimits::default(),
        };

        let result = run_exec(&job).await;
        let passed = result.as_ref().map(|r| r.exit_code == 0).unwrap_or(false);
        let output = result
            .map(|r| format!("exit={}, {}ms", r.exit_code, r.duration_ms))
            .unwrap_or_else(|e| format!("error: {}", e));

        gate_results.push((gate.clone(), passed, output.clone()));
        println!("  → {}: {}", if passed { "PASS" } else { "FAIL" }, output);

        if !passed {
            all_passed = false;
        }
    }

    let prompt = prompts::generate_qa_prompt(&story, &gate_results);
    println!("\nGenerating LLM response...");

    let llm_config = config.llm.clone();
    let prompt_clone = prompt.clone();
    let llm_response = match std::thread::spawn(move || {
        let llm = LlmClient::new(&llm_config.model, llm_config.temperature, llm_config.max_tokens, &llm_config.base_url)?;
        llm.generate(&prompt_clone)
    }).join().unwrap() {
        Ok(response) => {
            println!("LLM response received ({} chars)", response.len());
            response
        }
        Err(e) => {
            println!("LLM call failed: {}", e);
            println!("Rolling back - re-emitting build.done.retry");
            
            let retry_event = EventEnvelope::new(
                &Uuid::new_v4().to_string(),
                &trace_id,
                "build.done.retry",
                "qa",
                "qa",
                json!({ "storyId": story.id }),
                json!({ "summary": format!("Build {} needs retry: {}", story.id, e) }),
            );
            bus.publish_to_role("qa", retry_event).ok();
            return;
        }
    };

    let runlog_id = Uuid::new_v4().to_string();
    let mut runlog = Doc::new(
        &runlog_id,
        DocType::Runlog,
        &format!("QA Agent - Story {}", story.id),
        &format!("PROMPT:\n{}\n\nLLM RESPONSE:\n{}", prompt, llm_response),
        if all_passed { "passed" } else { "failed" },
        Role::Qa,
    );
    runlog.trace_id = Some(trace_id.clone());
    runlog.fields = json!({
        "event_id": event_id,
        "action": "test.execution",
        "story_id": story.id,
        "gate_results": gate_results,
        "llm_response": llm_response
    });
    store.doc_put(&runlog, None).ok();

    let audit = AuditEntry::new(
        &trace_id,
        &event_id,
        Role::Qa,
        "test.completed",
        &format!(
            "Story {} - gates: {}",
            story.id,
            if all_passed { "all passed" } else { "failed" }
        ),
    );
    store.stream_append("events", &audit).ok();

    let test_result_event = EventEnvelope::new(
        &Uuid::new_v4().to_string(),
        &trace_id,
        if all_passed { "test.pass" } else { "test.fail" },
        "qa",
        "po",
        json!({
            "storyId": story.id,
            "title": story.title,
        }),
        json!({
            "summary": if all_passed {
                format!("All DoD gates passed for story '{}'", story.title)
            } else {
                let failed_gates: Vec<String> = gate_results
                    .iter()
                    .filter(|(_, passed, _)| !*passed)
                    .map(|(gate, _, _)| gate.clone())
                    .collect();
                format!("DoD gates failed for story '{}': {:?}", story.title, failed_gates)
            },
            "gate_results": gate_results
        }),
    );
    bus.publish_to_role("po", test_result_event).ok();
    println!("Published test result to PO");

    if !all_passed {
        let bug_id = Uuid::new_v4().to_string();
        let failed_desc = gate_results
            .iter()
            .filter(|(_, passed, _)| !*passed)
            .map(|(gate, _, output)| format!("{}: {}", gate, output))
            .collect::<Vec<_>>()
            .join("; ");

        let mut bug = Doc::new(
            &bug_id,
            DocType::Bug,
            &format!("Test failure: {}", story.title),
            &format!("Story: {}\n\nFailed gates:\n{}", story.title, failed_desc),
            "open",
            Role::Qa,
        );
        bug.trace_id = Some(trace_id.clone());
        bug.fields = json!({
            "story_id": story.id,
            "gate_results": gate_results
        });
        bug.links = vec![story.id.clone()];

        store.doc_put(&bug, None).ok();
        println!("Created bug: {}", bug.id);

        let bug_event = EventEnvelope::new(
            &Uuid::new_v4().to_string(),
            &trace_id,
            "bug.new",
            "qa",
            "dev",
            json!({
                "bugId": bug.id,
                "storyId": story.id,
            }),
            json!({
                "summary": format!("Bug created for story '{}'", story.title)
            }),
        );
        bus.publish_to_role("dev", bug_event).ok();
        println!("Published bug.new to Dev");
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

    println!("Verifying bug fix: {} - {}", bug.id, bug.title);

    let prompt = prompts::generate_verification_prompt(&bug);
    println!("\nGenerating LLM response...");

    let llm_config = config.llm.clone();
    let prompt_clone = prompt.clone();
    let llm_response = match std::thread::spawn(move || {
        let llm = LlmClient::new(&llm_config.model, llm_config.temperature, llm_config.max_tokens, &llm_config.base_url)?;
        llm.generate(&prompt_clone)
    }).join().unwrap() {
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
                "qa",
                "qa",
                json!({ "bugId": bug.id }),
                json!({ "summary": format!("Bug {} needs retry: {}", bug.id, e) }),
            );
            bus.publish_to_role("qa", retry_event).ok();
            return;
        }
    };

    let runlog_id = Uuid::new_v4().to_string();
    let mut runlog = Doc::new(
        &runlog_id,
        DocType::Runlog,
        &format!("QA Agent - Bug Verification {}", bug.id),
        &format!("PROMPT:\n{}\n\nLLM RESPONSE:\n{}", prompt, llm_response),
        "in_progress",
        Role::Qa,
    );
    runlog.trace_id = Some(trace_id.clone());
    runlog.fields = json!({
        "event_id": event_id,
        "action": "bug.verification",
        "bug_id": bug.id,
        "llm_response": llm_response
    });
    store.doc_put(&runlog, None).ok();

    let event = EventEnvelope::new(
        &Uuid::new_v4().to_string(),
        &trace_id,
        "bug.verifying",
        "qa",
        "po",
        json!({ "bugId": bug.id }),
        json!({ "summary": format!("Verifying bug fix for '{}'", bug.title) }),
    );
    bus.publish_to_role("po", event).ok();
}
