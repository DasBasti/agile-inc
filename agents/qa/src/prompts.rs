use agile_common::types::Doc;

pub fn generate_qa_prompt(
    story: &Doc,
    gate_results: &[(String, bool, String)],
) -> String {
    let mut prompt = String::new();
    prompt.push_str("===========================================\n");
    prompt.push_str("        QA AGENT - TEST RESULTS\n");
    prompt.push_str("===========================================\n\n");
    prompt.push_str(&format!("Story ID: {}\n", story.id));
    prompt.push_str(&format!("Title: {}\n\n", story.title));

    prompt.push_str("DoD Gate Results:\n");
    for (gate, passed, output) in gate_results {
        let status = if *passed { "✓ PASS" } else { "✗ FAIL" };
        prompt.push_str(&format!("  {} {}\n    {}\n", status, gate, output));
    }
    prompt.push_str("\n");

    let all_passed = gate_results.iter().all(|(_, passed, _)| *passed);
    if all_passed {
        prompt.push_str("Result: ALL GATES PASSED\n");
        prompt.push_str("Action: Emit 'test.pass' event to PO\n");
    } else {
        prompt.push_str("Result: SOME GATES FAILED\n");
        prompt.push_str("Action: Create bug document, emit 'bug.new' to Dev\n");
    }

    prompt.push_str("===========================================\n");
    prompt.push_str("        NEXT ACTIONS FOR QA\n");
    prompt.push_str("===========================================\n");
    prompt.push_str("1. Review gate results\n");
    prompt.push_str("2. If failed → create bug doc + emit 'bug.new'\n");
    prompt.push_str("3. If passed → emit 'test.pass'\n");
    prompt.push_str("4. Update runlog with results\n");
    prompt.push_str("===========================================\n");

    prompt
}

pub fn generate_verification_prompt(bug: &Doc) -> String {
    let mut prompt = String::new();
    prompt.push_str("===========================================\n");
    prompt.push_str("        QA AGENT - BUG VERIFICATION\n");
    prompt.push_str("===========================================\n\n");
    prompt.push_str(&format!("Bug ID: {}\n", bug.id));
    prompt.push_str(&format!("Title: {}\n\n", bug.title));
    prompt.push_str(&format!("Description:\n{}\n\n", bug.body));

    if let Some(fields) = bug.fields.as_object() {
        if let Some(story_id) = fields.get("story_id") {
            prompt.push_str(&format!("Related Story: {}\n", story_id));
        }
    }

    prompt.push_str("===========================================\n");
    prompt.push_str("        NEXT ACTIONS FOR QA\n");
    prompt.push_str("===========================================\n");
    prompt.push_str("1. Verify the fix works as expected\n");
    prompt.push_str("2. Run regression tests\n");
    prompt.push_str("3. If fixed → emit 'bug.closed'\n");
    prompt.push_str("4. If still failing → emit 'bug.still_open'\n");
    prompt.push_str("===========================================\n");

    prompt
}
