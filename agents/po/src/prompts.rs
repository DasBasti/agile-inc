use agile_common::types::Doc;

pub fn generate_po_prompt(story: &Doc) -> String {
    let mut prompt = String::new();
    prompt.push_str("===========================================\n");
    prompt.push_str("        PO AGENT - STORY ANALYSIS\n");
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

        if let Some(dod) = fields.get("definition_of_done") {
            if let Some(gates) = dod.as_array() {
                prompt.push_str("Definition of Done (DoD):\n");
                for (i, gate) in gates.iter().enumerate() {
                    if let Some(text) = gate.as_str() {
                        prompt.push_str(&format!("  {}. {}\n", i + 1, text));
                    }
                }
                prompt.push_str("\n");
            }
        }
    }

    if !story.links.is_empty() {
        prompt.push_str(&format!("Linked Documents: {:?}\n\n", story.links));
    }

    prompt.push_str("===========================================\n");
    prompt.push_str("        NEXT ACTIONS FOR PO\n");
    prompt.push_str("===========================================\n");
    prompt.push_str("1. Review the story for completeness\n");
    prompt.push_str("2. Ensure acceptance criteria are testable\n");
    prompt.push_str("3. Verify DoD gates are machine-executable\n");
    prompt.push_str("4. Publish 'story.ready' event when ready\n");
    prompt.push_str("===========================================\n");

    prompt
}

pub fn generate_story_creation_prompt(
    title: &str,
    description: &str,
    requested_by: &str,
) -> String {
    let mut prompt = String::new();
    prompt.push_str("===========================================\n");
    prompt.push_str("        CREATE NEW STORY\n");
    prompt.push_str("===========================================\n\n");
    prompt.push_str(&format!("Title: {}\n", title));
    prompt.push_str(&format!("Description: {}\n", description));
    prompt.push_str(&format!("Requested by: {}\n\n", requested_by));
    prompt.push_str("Please create a well-structured story with:\n");
    prompt.push_str("  1. Clear title and description\n");
    prompt.push_str("  2. Specific acceptance criteria\n");
    prompt.push_str("  3. Definition of Done (DoD) gates\n\n");
    prompt
}
