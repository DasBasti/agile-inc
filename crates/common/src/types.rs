use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventEnvelope {
    pub event_id: String,
    pub trace_id: String,
    pub r#type: String,
    pub from: String,
    pub to: String,
    pub refs: serde_json::Value,
    pub payload: serde_json::Value,
    pub ts: String,
}

impl EventEnvelope {
    pub fn new(
        event_id: &str,
        trace_id: &str,
        r#type: &str,
        from: &str,
        to: &str,
        refs: serde_json::Value,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_id: event_id.to_string(),
            trace_id: trace_id.to_string(),
            r#type: r#type.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            refs,
            payload,
            ts: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Doc {
    pub id: String,
    pub r#type: DocType,
    pub title: String,
    pub body: String,
    pub status: String,
    pub owner_role: Role,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub trace_id: Option<String>,
    pub version: u64,
    pub fields: serde_json::Value,
}

impl Doc {
    pub fn new(
        id: &str,
        r#type: DocType,
        title: &str,
        body: &str,
        status: &str,
        owner_role: Role,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: id.to_string(),
            r#type,
            title: title.to_string(),
            body: body.to_string(),
            status: status.to_string(),
            owner_role,
            tags: Vec::new(),
            links: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            trace_id: None,
            version: 1,
            fields: serde_json::json!({}),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocType {
    Story,
    Task,
    Bug,
    Testplan,
    Policy,
    Artifact,
    Runlog,
}

impl std::fmt::Display for DocType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocType::Story => write!(f, "story"),
            DocType::Task => write!(f, "task"),
            DocType::Bug => write!(f, "bug"),
            DocType::Testplan => write!(f, "testplan"),
            DocType::Policy => write!(f, "policy"),
            DocType::Artifact => write!(f, "artifact"),
            DocType::Runlog => write!(f, "runlog"),
        }
    }
}

impl std::str::FromStr for DocType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "story" => Ok(DocType::Story),
            "task" => Ok(DocType::Task),
            "bug" => Ok(DocType::Bug),
            "testplan" => Ok(DocType::Testplan),
            "policy" => Ok(DocType::Policy),
            "artifact" => Ok(DocType::Artifact),
            "runlog" => Ok(DocType::Runlog),
            _ => Err(format!("unknown doc type: {}", s)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Po,
    Dev,
    Qa,
    System,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Po => write!(f, "po"),
            Role::Dev => write!(f, "dev"),
            Role::Qa => write!(f, "qa"),
            Role::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "po" => Ok(Role::Po),
            "dev" => Ok(Role::Dev),
            "qa" => Ok(Role::Qa),
            "system" => Ok(Role::System),
            _ => Err(format!("unknown role: {}", s)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuditEntry {
    pub ts: String,
    pub trace_id: String,
    pub event_id: String,
    pub actor_role: Role,
    pub action: String,
    pub refs: AuditRefs,
    pub summary: String,
    pub details: serde_json::Value,
}

impl AuditEntry {
    pub fn new(
        trace_id: &str,
        event_id: &str,
        actor_role: Role,
        action: &str,
        summary: &str,
    ) -> Self {
        Self {
            ts: chrono::Utc::now().to_rfc3339(),
            trace_id: trace_id.to_string(),
            event_id: event_id.to_string(),
            actor_role,
            action: action.to_string(),
            refs: AuditRefs { doc_ids: Vec::new() },
            summary: summary.to_string(),
            details: serde_json::json!({}),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AuditRefs {
    pub doc_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Policy {
    pub policy_id: String,
    pub autonomy_level: AutonomyLevel,
    pub change_budget: ChangeBudget,
    pub required_gates: Vec<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            policy_id: "default".to_string(),
            autonomy_level: AutonomyLevel::L1,
            change_budget: ChangeBudget::default(),
            required_gates: vec!["cargo test".to_string(), "cargo fmt --check".to_string()],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum AutonomyLevel {
    #[serde(rename = "L0")]
    L0,
    #[serde(rename = "L1")]
    L1,
    #[serde(rename = "L2")]
    L2,
    #[serde(rename = "L3")]
    L3,
    #[serde(rename = "L4")]
    L4,
}

impl Default for AutonomyLevel {
    fn default() -> Self {
        AutonomyLevel::L1
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChangeBudget {
    pub max_files: u32,
    pub max_insertions: u32,
    pub max_deletions: u32,
    pub protected_paths: Vec<String>,
}

impl Default for ChangeBudget {
    fn default() -> Self {
        Self {
            max_files: 20,
            max_insertions: 500,
            max_deletions: 500,
            protected_paths: vec!["infra/".to_string()],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DocFilter {
    #[serde(default)]
    pub r#type: Vec<DocType>,
    #[serde(default)]
    pub status: Vec<String>,
    #[serde(default)]
    pub tags_any: Vec<String>,
    #[serde(default)]
    pub linked_to: Vec<String>,
    #[serde(default)]
    pub owner_role: Vec<Role>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DocQuery {
    pub filter: DocFilter,
    pub search: Option<String>,
    pub sort_by: SortField,
    pub sort_dir: SortDir,
    pub limit: usize,
    pub cursor: Option<String>,
}

impl Default for DocQuery {
    fn default() -> Self {
        Self {
            filter: DocFilter::default(),
            search: None,
            sort_by: SortField::UpdatedAt,
            sort_dir: SortDir::Desc,
            limit: 50,
            cursor: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortField {
    #[default]
    UpdatedAt,
    CreatedAt,
    Priority,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DocPatch {
    pub doc_id: String,
    pub expected_version: u64,
    pub ops: Vec<PatchOp>,
    pub trace_id: String,
    pub idempotency_key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum PatchOp {
    Set { path: String, value: serde_json::Value },
    AddToSet { path: String, value: serde_json::Value },
    AppendLink { path: String, value: String },
    SetField { path: String, value: serde_json::Value },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecJob {
    pub trace_id: String,
    pub job_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub workdir: String,
    pub timeout_seconds: u64,
    pub env: std::collections::HashMap<String, String>,
    pub output_limits: OutputLimits,
}

impl Default for ExecJob {
    fn default() -> Self {
        Self {
            trace_id: String::new(),
            job_id: String::new(),
            command: "opencode".to_string(),
            args: Vec::new(),
            workdir: "./".to_string(),
            timeout_seconds: 1800,
            env: std::collections::HashMap::new(),
            output_limits: OutputLimits::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct OutputLimits {
    pub max_stdout_kb: u64,
    pub max_stderr_kb: u64,
}

impl OutputLimits {
    pub fn new(max_kb: u64) -> Self {
        Self {
            max_stdout_kb: max_kb,
            max_stderr_kb: max_kb,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
    pub artifacts: Vec<Artifact>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Artifact {
    pub kind: String,
    pub doc_id: Option<String>,
    pub content: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiffStat {
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
    pub files: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DedupRequest {
    pub key: String,
    pub ttl_seconds: u64,
    pub scope: DedupScope,
    pub role: Option<Role>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum DedupScope {
    #[default]
    Global,
    Role,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DedupResult {
    pub first_time: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApprovalRequest {
    pub trace_id: String,
    pub event_id: String,
    pub reason: String,
    pub refs: AuditRefs,
    pub proposed_action: ProposedAction,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProposedAction {
    pub kind: ApprovalKind,
    pub details: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    Merge,
    Deploy,
    LargeChange,
    DependencyBump,
    ProtectedPathChange,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_type_from_str() {
        assert_eq!("story".parse::<DocType>().unwrap(), DocType::Story);
        assert_eq!("bug".parse::<DocType>().unwrap(), DocType::Bug);
        assert!("invalid".parse::<DocType>().is_err());
    }

    #[test]
    fn test_role_from_str() {
        assert_eq!("po".parse::<Role>().unwrap(), Role::Po);
        assert_eq!("dev".parse::<Role>().unwrap(), Role::Dev);
        assert!("admin".parse::<Role>().is_err());
    }

    #[test]
    fn test_event_envelope_creation() {
        let env = EventEnvelope::new(
            "evt-123",
            "trace-456",
            "story.ready",
            "po",
            "dev",
            serde_json::json!({}),
            serde_json::json!({"summary": "test story"}),
        );
        assert_eq!(env.event_id, "evt-123");
        assert_eq!(env.trace_id, "trace-456");
        assert!(!env.ts.is_empty());
    }

    #[test]
    fn test_doc_creation() {
        let doc = Doc::new(
            "doc-123",
            DocType::Story,
            "Test Story",
            "Story body",
            "draft",
            Role::Po,
        );
        assert_eq!(doc.id, "doc-123");
        assert_eq!(doc.r#type, DocType::Story);
        assert_eq!(doc.status, "draft");
        assert_eq!(doc.owner_role, Role::Po);
        assert_eq!(doc.version, 1);
    }
}
