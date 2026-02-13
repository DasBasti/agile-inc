use agile_common::types::Doc;

pub fn generate_dev_prompt(story: &Doc, dod_gates: &[String]) -> String {
    let mut prompt = String::new();
    prompt.push_str("===========================================\n");
    prompt.push_str("        DEV AGENT - IMPLEMENTATION\n");
    prompt.push_str("===========================================\n\n");
    prompt.push_str(&format!("Story ID: {}\n", story.id));
    prompt.push_str(&format!("Title: {}\n\n", story.title));
    prompt.push_str(&format!("Description:\n{}\n\n", story.body));

    if let Some(fields) = story.fields.as_object() {
        if let Some(ac) = fields.get("acceptance_criteria") {
            if let Some(criteria) = ac.as_array() {
                prompt.push_str("Acceptance Criteria:\n");
                for (i, crit) in criteria.iter().enumerate() {
                    if let Some(text) = crit.as_str() {
                        prompt.push_str(&format!("  {}. {}\n", i + 1, text));
                    }
                }
                prompt.push_str("\n");
            }
        }
    }

    prompt.push_str("Definition of Done (must pass):\n");
    for (i, gate) in dod_gates.iter().enumerate() {
        prompt.push_str(&format!("  {}. {}\n", i + 1, gate));
    }
    prompt.push_str("\n");

    prompt.push_str("===========================================\n");
    prompt.push_str("        NEXT ACTIONS FOR DEV\n");
    prompt.push_str("===========================================\n");
    prompt.push_str("1. Analyze the story and acceptance criteria\n");
    prompt.push_str("2. Implement the required changes\n");
    prompt.push_str("3. Run all DoD gates (tests, lints, etc.)\n");
    prompt.push_str("4. Check change budget is within limits\n");
    prompt.push_str("5. If all gates pass → emit 'build.done'\n");
    prompt.push_str("6. If budget exceeded → emit 'dev.needs_approval'\n");
    prompt.push_str("===========================================\n");

    prompt
}

pub fn generate_fix_prompt(bug: &Doc) -> String {
    let mut prompt = String::new();
    prompt.push_str("===========================================\n");
    prompt.push_str("        DEV AGENT - BUG FIX\n");
    prompt.push_str("===========================================\n\n");
    prompt.push_str(&format!("Bug ID: {}\n", bug.id));
    prompt.push_str(&format!("Title: {}\n\n", bug.title));
    prompt.push_str(&format!("Description:\n{}\n\n", bug.body));

    if let Some(fields) = bug.fields.as_object() {
        if let Some(repro) = fields.get("repro_steps") {
            prompt.push_str(&format!("Reproduction Steps:\n{}\n\n", repro));
        }
    }

    prompt.push_str("===========================================\n");
    prompt.push_str("        NEXT ACTIONS FOR DEV\n");
    prompt.push_str("===========================================\n");
    prompt.push_str("1. Understand and reproduce the bug\n");
    prompt.push_str("2. Implement the fix\n");
    prompt.push_str("3. Run tests to verify fix works\n");
    prompt.push_str("4. Ensure no regressions\n");
    prompt.push_str("5. Emit 'bug.fixed' event when done\n");
    prompt.push_str("===========================================\n");

    prompt
}
