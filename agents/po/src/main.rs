use agile_bus_mqtt::{EventBus, EventEnvelope};
use agile_common::types::{AuditEntry, Doc, DocType, Role};
use agile_config::Config;
use agile_llm::LlmClient;
use agile_store_redis::RedisStore;
use serde_json::json;
use uuid::Uuid;

mod prompts;

struct Prompts {
    system: String,
    refinement: String,
    criteria: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("PO Agent starting...");

    let config = Config::load("config/agile-inc.toml")?;
    println!(
        "Config loaded: MQTT={}:{}, Redis={}, LLM={}, MaxRounds={}",
        config.mqtt.host, config.mqtt.port, config.redis.url, config.llm.model, config.prompts.max_llm_rounds
    );

    let system_prompt = std::fs::read_to_string(&config.prompts.po_system_prompt_file)
        .unwrap_or_else(|e| {
            eprintln!("Warning: Could not read prompt file {}: {}", config.prompts.po_system_prompt_file, e);
            "You are a helpful assistant.".to_string()
        });
    println!("Loaded system prompt from: {}", config.prompts.po_system_prompt_file);

    let refinement_prompt = std::fs::read_to_string(&config.prompts.po_refinement_prompt_file)
        .unwrap_or_else(|e| {
            eprintln!("Warning: Could not read refinement prompt file: {}", e);
            "Please refine this story.".to_string()
        });
    println!("Loaded refinement prompt from: {}", config.prompts.po_refinement_prompt_file);

    let refinement_criteria = std::fs::read_to_string(&config.prompts.po_refinement_criteria_file)
        .unwrap_or_else(|e| {
            eprintln!("Warning: Could not read refinement criteria file: {}", e);
            "".to_string()
        });
    println!("Loaded refinement criteria from: {}", config.prompts.po_refinement_criteria_file);

    let prompts = Prompts {
        system: system_prompt,
        refinement: refinement_prompt,
        criteria: refinement_criteria,
    };

    let store = RedisStore::new(&config.redis.url)?;
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

    let llm = LlmClient::new(
        &config.llm.model,
        config.llm.temperature,
        config.llm.max_tokens,
        &config.llm.base_url,
    )?;
    println!("LLM client connected: {}", config.llm.model);

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
            handle_event(&config, &bus, &store, &llm, &event, &prompts);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn handle_event(config: &Config, bus: &EventBus, store: &RedisStore, llm: &LlmClient, event: &EventEnvelope, prompts: &Prompts) {
    match event.r#type.as_str() {
        "po.run" => {
            handle_po_run(config, bus, store, llm, event, &prompts.system);
        }
        "po.refine" => {
            handle_story_refinement(config, bus, store, llm, event, prompts);
        }
        "story.new" | "story.create" => {
            handle_story_creation(config, bus, store, llm, event);
        }
        "policy.changed" => {
            println!("Policy changed, acknowledging...");
        }
        _ => {
            println!("Unhandled event type: {}", event.r#type);
        }
    }
}

fn handle_po_run(config: &Config, bus: &EventBus, store: &RedisStore, _llm: &LlmClient, event: &EventEnvelope, system_prompt: &str) {
    let trace_id = event.trace_id.clone();
    let event_id = event.event_id.clone();

    let payload = &event.payload;
    let prompt = payload
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let context = payload
        .get("context")
        .cloned();

    if prompt.is_empty() {
        println!("Error: po.run requires a 'prompt' field");
        return;
    }

    let run_id = Uuid::new_v4().to_string();
    println!("Starting PO run: {}", run_id);

    let mut runlog = Doc::new(
        &run_id,
        DocType::Runlog,
        &format!("PO Run {}", &run_id[..8]),
        &prompt,
        "in_progress",
        Role::Po,
    );
    runlog.trace_id = Some(trace_id.clone());
    runlog.fields = json!({
        "event_id": event_id,
        "prompt": prompt,
        "system_prompt": system_prompt,
        "context": context,
        "llm_rounds": 0,
    });

    match store.doc_put(&runlog, None) {
        Ok(_) => {}
        Err(e) => {
            println!("Failed to create runlog: {}", e);
            return;
        }
    }

    let result = run_llm_loop(config, store, &run_id, &prompt, system_prompt, context.as_ref());

    let final_status = if result.is_ok() { "completed" } else { "failed" };
    
    if let Ok(updated) = store.doc_get(&run_id) {
        let mut updated_doc = updated;
        updated_doc.status = final_status.to_string();
        updated_doc.fields["llm_response"] = json!(result.clone().unwrap_or_default());
        updated_doc.fields["final_status"] = json!(final_status);
        store.doc_put(&updated_doc, None).ok();
    }

    let response_event = EventEnvelope::new(
        &Uuid::new_v4().to_string(),
        &trace_id,
        "po.completed",
        "po",
        &event.from,
        json!({ "runId": run_id }),
        json!({ 
            "status": final_status,
            "response": result.unwrap_or_default()
        }),
    );
    bus.publish(&format!("agileinc/role/{}/inbox", event.from), response_event).ok();
}

fn run_llm_loop(config: &Config, store: &RedisStore, run_id: &str, initial_prompt: &str, system_prompt: &str, _context: Option<&serde_json::Value>) -> Result<String, String> {
    let llm_config = config.llm.clone();
    let max_rounds = config.prompts.max_llm_rounds;
    
    let mut current_prompt = format!("{}\n\n---\n\n{}", system_prompt, initial_prompt);
    let mut full_response = String::new();
    let mut round = 0;

    loop {
        round += 1;
        println!("LLM Round {}/{}", round, max_rounds);

        if round > max_rounds {
            println!("Max rounds ({}) reached, stopping", max_rounds);
            return Err(format!("Max rounds ({}) reached", max_rounds));
        }

        let prompt_clone = current_prompt.clone();
        let llm_cfg = llm_config.clone();
        let llm_result = std::thread::spawn(move || {
            let llm = LlmClient::new(&llm_cfg.model, llm_cfg.temperature, llm_cfg.max_tokens, &llm_cfg.base_url)?;
            llm.generate(&prompt_clone)
        }).join().unwrap();

        let response = match llm_result {
            Ok(r) => r,
            Err(e) => {
                println!("LLM error: {}", e);
                return Err(e.to_string());
            }
        };

        full_response.push_str(&format!("\n--- Round {} ---\n{}\n", round, response));

        if let Ok(doc) = store.doc_get(run_id) {
            let mut updated = doc;
            updated.fields["llm_rounds"] = json!(round);
            updated.fields["llm_response"] = json!(response);
            store.doc_put(&updated, None).ok();
        }

        let should_continue = analyze_for_continuation(&response);
        if !should_continue {
            println!("LLM indicated completion");
            return Ok(response);
        }

        current_prompt = format!(
            "{}\n\n---\n\nPrevious response:\n{}\n\nDo you need to continue? If yes, provide the next step. If done, say DONE.",
            initial_prompt, response
        );
    }
}

fn analyze_for_continuation(response: &str) -> bool {
    let lower = response.to_lowercase();
    if lower.contains("done") || lower.contains("completed") || lower.contains("finished") {
        return false;
    }
    true
}

fn handle_story_refinement(config: &Config, bus: &EventBus, store: &RedisStore, _llm: &LlmClient, event: &EventEnvelope, prompts: &Prompts) {
    let trace_id = event.trace_id.clone();
    let event_id = event.event_id.clone();

    let payload = &event.payload;
    let story_id = payload
        .get("storyId")
        .and_then(|v| v.as_str())
        .map(String::from);

    if story_id.is_none() {
        println!("Error: po.refine requires a 'storyId' field");
        return;
    }

    let story_id = story_id.unwrap();
    println!("Starting story refinement for: {}", story_id);

    let original_doc = match store.doc_get(&story_id) {
        Ok(doc) => doc,
        Err(e) => {
            println!("Error: Could not find story {}: {}", story_id, e);
            return;
        }
    };

    let title = &original_doc.title;
    let body = &original_doc.body;
    let acceptance_criteria: Vec<String> = original_doc.fields
        .get("acceptance_criteria")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let definition_of_done: Vec<String> = original_doc.fields
        .get("definition_of_done")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let ac_text = if acceptance_criteria.is_empty() {
        "None provided".to_string()
    } else {
        acceptance_criteria.iter().enumerate().map(|(i, c)| format!("{}. {}", i + 1, c)).collect::<Vec<_>>().join("\n")
    };

    let dod_text = if definition_of_done.is_empty() {
        "None provided".to_string()
    } else {
        definition_of_done.iter().enumerate().map(|(i, d)| format!("{}. {}", i + 1, d)).collect::<Vec<_>>().join("\n")
    };

    let refinement_prompt = prompts.refinement
        .replace("{title}", title)
        .replace("{description}", &format!("{}\n\n{}", body, &prompts.criteria))
        .replace("{acceptance_criteria}", &ac_text)
        .replace("{definition_of_done}", &dod_text);

    let run_id = Uuid::new_v4().to_string();
    let mut runlog = Doc::new(
        &run_id,
        DocType::Runlog,
        &format!("PO Refinement {}", &story_id[..8]),
        &refinement_prompt,
        "in_progress",
        Role::Po,
    );
    runlog.trace_id = Some(trace_id.clone());
    runlog.fields = json!({
        "event_id": event_id,
        "story_id": story_id,
        "prompt": refinement_prompt,
        "system_prompt": prompts.system.clone(),
        "llm_rounds": 0,
    });

    if store.doc_put(&runlog, None).is_err() {
        println!("Failed to create runlog");
        return;
    }

    let result = run_llm_loop(config, store, &run_id, &refinement_prompt, &prompts.system, None);

    let final_status = if result.is_ok() { "completed" } else { "failed" };

    if let Ok(updated) = store.doc_get(&run_id) {
        let mut updated_doc = updated;
        updated_doc.status = final_status.to_string();
        updated_doc.fields["llm_response"] = json!(result.clone().unwrap_or_default());
        updated_doc.fields["final_status"] = json!(final_status);
        store.doc_put(&updated_doc, None).ok();
    }

    if let Ok(response) = &result {
        if let Ok(mut story) = store.doc_get(&story_id) {
            let resp_lower = response.to_lowercase();
            
            let is_not_ready = resp_lower.contains("not_ready") 
                || resp_lower.contains("not ready")
                || resp_lower.contains("return to backlog")
                || resp_lower.contains("do not publish")
                || (resp_lower.contains("readiness status") && resp_lower.contains("not_ready"));
            
            if is_not_ready {
                story.status = "refining".to_string();
            } else if resp_lower.contains("readiness status") && 
                      (resp_lower.contains("\nready") || resp_lower.contains("\n ready") || resp_lower.ends_with("ready")) {
                story.status = "ready".to_string();
            } else {
                story.status = "refining".to_string();
            }
            
            story.fields["refinement_response"] = json!(response);
            store.doc_put(&story, None).ok();
        }
    }

    let response_event = EventEnvelope::new(
        &Uuid::new_v4().to_string(),
        &trace_id,
        "po.refinement.completed",
        "po",
        &event.from,
        json!({ "storyId": story_id }),
        json!({
            "status": final_status,
            "response": result.unwrap_or_default()
        }),
    );
    bus.publish(&format!("agileinc/role/{}/inbox", event.from), response_event).ok();
}

fn handle_story_creation(config: &Config, bus: &EventBus, store: &RedisStore, _llm: &LlmClient, event: &EventEnvelope) {
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
            vec!["cargo test".to_string(), "cargo fmt --check".to_string()]
        });

    let mut doc = Doc::new(&story_id, DocType::Story, &title, &body, "draft", Role::Po);
    doc.trace_id = Some(trace_id.clone());
    doc.fields = json!({
        "acceptance_criteria": acceptance_criteria,
        "definition_of_done": definition_of_done,
    });

    if let Some(ref docs) = event.refs.get("docIds").and_then(|v| v.as_array()) {
        doc.links = docs.iter().filter_map(|v| v.as_str().map(String::from)).collect();
    }

    match store.doc_put(&doc, Some(&event_id)) {
        Ok(mut saved_doc) => {
            println!("Created story: {} - {}", saved_doc.id, saved_doc.title);

            let prompt = prompts::generate_po_prompt(&saved_doc);
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
                    println!("Rolling back - deleting story doc");
                    
                    let _ = store.doc_put(&Doc::new(&story_id, DocType::Story, &title, &body, "failed", Role::Po), None);
                    
                    let retry_event = EventEnvelope::new(
                        &Uuid::new_v4().to_string(),
                        &trace_id,
                        "story.failed",
                        "po",
                        "system",
                        json!({ "storyId": story_id }),
                        json!({ "summary": format!("Story creation failed: {}", e) }),
                    );
                    bus.publish_to_role("system", retry_event).ok();
                    return;
                }
            };

            let audit = AuditEntry::new(
                &trace_id,
                &event_id,
                Role::Po,
                "story.created",
                &format!("Created story: {}", title),
            );
            store.stream_append("events", &audit).ok();

            let runlog_id = Uuid::new_v4().to_string();
            let mut runlog = Doc::new(
                &runlog_id,
                DocType::Runlog,
                &format!("PO Agent - Story {}", saved_doc.id),
                &format!("PROMPT:\n{}\n\nLLM RESPONSE:\n{}", prompt, llm_response),
                "completed",
                Role::Po,
            );
            runlog.trace_id = Some(trace_id.clone());
            runlog.fields = json!({
                "event_id": event_id,
                "action": "story.created",
                "llm_response": llm_response
            });
            store.doc_put(&runlog, None).ok();

            let llm_lower = llm_response.to_lowercase();
            let story_is_ready = !llm_lower.contains("not_ready") 
                && !llm_lower.contains("not ready")
                && !llm_lower.contains("return to backlog")
                && !llm_lower.contains("do not publish");

            if story_is_ready {
                saved_doc.status = "ready".to_string();
                store.doc_put(&saved_doc, None).ok();
                
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
            } else {
                saved_doc.status = "refining".to_string();
                store.doc_put(&saved_doc, None).ok();
                println!("Story requires refinement, not publishing story.ready");
            }
        }
        Err(e) => {
            println!("Failed to create story: {}", e);
        }
    }
}
