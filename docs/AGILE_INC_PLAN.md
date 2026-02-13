# Agile Inc. — Plan (v0.4)

**Goal:** Build a multi-agent “agile startup” system where autonomous agent runners communicate via **MQTT**, use an LLM via prompt wrappers, **execute** programming work via a configurable **opencode CLI**, and share a **single source of truth** in **Redis** (persistent).

This plan reflects the latest decisions:
- Roles (MVP): **PO**, **Developer**, **Tester**
- **Redis = Source of Truth** (persisted)
- MQTT broker is available; **root/base topic is configurable**
- Developer executes work via **opencode** (installed as CLI) called by the developer wrapper
- **Redis deduplication** for idempotency
- Autonomy is enabled, but controlled by **Autonomy Levels + machine-checkable Gates**

---

## 1) MVP Roles & Responsibilities

### PO (Product Owner)
- Consolidates incoming requirements
- Creates **Stories** with clear **Acceptance Criteria (AC)**
- Defines **Definition of Done (DoD)** as *machine-checkable gates* (commands + expected signals)
- Prioritizes backlog
- Publishes `story.ready` to Developer
- Performs acceptance / human approval steps when required by policy

### Developer Agent
- Converts story → implementation tasks
- Runs **opencode** to implement changes (if allowed by autonomy policy)
- Enforces **Change Budget** (small, reversible changes)
- Writes artifacts/results back to Redis
- Publishes `build.done` / `dev.needs_approval` to QA/PO

### Tester (QA) Agent
- Derives test cases from AC
- Executes DoD gates (tests/lints/etc.) and verifies fixes
- Creates **Bug** docs when failing
- Publishes `test.result` / `bug.new` to Developer and summary to PO

---

## 2) High-Level Architecture

**MQTT** is the event bus.
**Redis** is the shared persisted document store + audit/event stream + dedupe keys.

**Flow (typical):**
1. Event arrives (MQTT)
2. Agent runner loads context (Redis docs)
3. Agent plans via prompt wrapper (LLM) (MVP can stub/skip)
4. Developer agent executes via `opencode` (CLI) if permitted by policy
5. Agent writes results back to Redis
6. Agent publishes follow-up events via MQTT

---

## 3) MQTT Topics & Message Contract

### Root topic
All topics are derived from `mqtt.base_topic` (config).

### Topic convention
Assuming `base_topic = agileinc`:
- Role inbox: `agileinc/role/<role>/inbox`
- Events: `agileinc/events`
- Alerts: `agileinc/system/alerts`
- Policy changes: `agileinc/policy/changed`

### Event envelope (JSON)
```json
{
  "eventId": "uuid",
  "traceId": "uuid",
  "type": "story.new|story.ready|build.done|test.result|bug.new|dev.needs_approval|status|policy.changed",
  "from": "po|dev|qa|system",
  "to": "po|dev|qa|broadcast",
  "refs": { "docIds": ["..."], "branch": "..." },
  "payload": {},
  "ts": "ISO-8601"
}
```

**Idempotency:** every agent checks `eventId` against Redis before processing.

---

## 4) Redis Document Model (SoT)

Redis will be used as a structured store:
- **Hashes** for documents (`doc:<id>`) and metadata
- **Sets** for indexing by type/status/tags
- **Streams** for append-only event/audit logs (recommended)
- **Dedupe keys** for idempotency (`SETNX dedupe:<eventId> EX <ttl>`)

### Minimal document types
- `story` — created by PO
- `task` — created by Dev
- `testplan` / `testcase` — created by QA
- `bug` — created by QA
- `runlog` — produced by all agents
- `policy` — autonomy + gates + budgets
- `artifact` — diffs, logs, reports, command outputs

### Suggested status sets
- Story: `draft -> ready -> in_progress -> in_review -> done`
- Bug: `open -> fixed -> verified -> closed`

### Redis keys (suggestion)
- `doc:<uuid>` (HASH)
- `docs:index:type:<type>` (SET of ids)
- `docs:index:status:<type>:<status>` (SET of ids)
- `stream:events` (STREAM) — audit log (MQTT events + agent actions)
- `dedupe:<eventId>` (string) — idempotency marker with TTL

### Connectivity
- Redis is reachable at `192.168.0.137:49155` (TCP OK)

---

## 5) Autonomy Levels + Gates (Safety Model)

Autonomy is not “a feeling” — it is a **policy** that controls what the Developer is allowed to do.

### Autonomy Levels (recommended)
- **L0 – Suggest only:** Dev produces plan/patch proposal docs; no opencode execution.
- **L1 – Execute & propose:** Dev may run opencode and produce a branch/patch/PR artifact; no merge/deploy.
- **L2 – Auto-merge behind gates:** Dev may merge only if all required gates are green + QA pass.
- **L3 – Auto-deploy staged:** Dev may deploy to staging; production requires human approval.
- **L4 – Full autonomy:** rare; only for low-risk services with strong guardrails.

### Machine-checkable Gates (Definition of Done)
Each `story` includes DoD gates, e.g.:
- `cargo test`
- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- security/dependency checks (later)

QA (and/or Dev) must produce a `test.result` that is structured and replayable.

---

## 6) Change Budget (Small, Reversible Changes)

To make autonomy safe, Developer work is constrained by a **change budget**.

Examples:
- max changed files
- max added/removed LOC
- protected paths require approval
- dependency bumps require extra QA + PO ack

### Enforcement
Developer agent (post-opencode) collects:
- `git diff --stat` / `--numstat`
- changed file list

If the change budget is exceeded, Dev publishes:
- `dev.needs_approval` instead of `build.done`

---

## 7) Observability & Accountability

### Traceability
- Every MQTT message includes `traceId` and `eventId`.
- Every agent run appends a record to `stream:events` with:
  - input event
  - handler name
  - decision summary
  - output docs/artifacts

### Kill switch / pause
Store a simple control doc in Redis, e.g. `doc:system:agent_status`:
- `enabled.po = true/false`
- `enabled.dev = true/false`
- `enabled.qa = true/false`

Agents must check before executing.

---

## 8) Central Configuration (TOML)

A single editable config file used by all agents and crates.

Path:
- `config/agile-inc.toml` (runtime)
- `config/agile-inc.toml.example` (committed template)

### Example config (`config/agile-inc.toml.example`)
```toml
[mqtt]
host = "192.168.0.11"
port = 1883
username = "agil"
password = "agil"
base_topic = "agileinc"
client_id_prefix = "agileinc"

[redis]
url = "redis://192.168.0.137:49155"
dedupe_ttl_seconds = 86400

[opencode]
command = "opencode"
default_args = ["--non-interactive"]
workdir = "./"
timeout_seconds = 1800
max_output_kb = 2048

[agents]
log_level = "info"
```

**Security note:** prefer committing only the `.example` and override secrets via env vars later.

---

## 9) Monorepo Layout (Rust Cargo Workspace)

We use a single Rust monorepo to share types, config, and infrastructure code.

```txt
agile-inc/
  README.md
  Cargo.toml                  # workspace root
  Cargo.lock
  .gitignore

  config/
    agile-inc.toml            # central configuration (not committed)
    agile-inc.toml.example

  infra/
    docker-compose.yml        # optional local redis/mqtt
    redis/
      redis.conf              # appendonly yes, etc.

  crates/
    common/                   # shared types, IDs, serde models
    config/                   # config loader + ENV override
    bus_mqtt/                 # MQTT client wrapper + topic builder
    store_redis/              # Redis doc store + stream + dedupe
    prompt_wrapper/           # LLM prompt templates + validation (later)
    opencode_runner/          # opencode CLI execution bridge

  agents/
    po/                       # binary crate
    dev/                      # binary crate
    qa/                       # binary crate

  tools/
    publish_event/            # optional: CLI to inject events for testing
```

---

## 10) Execution via opencode (Developer)

Developer agent uses `crates/opencode_runner`:
- Builds a deterministic execution request:
  - input story/task + DoD gates
  - target workdir (repo root)
- Runs configured command:
  - `opencode.command` + `opencode.default_args` + job-specific args
- Captures:
  - exit code
  - bounded stdout/stderr (`max_output_kb`)
  - VCS artifacts (changed files via `git diff --name-only`, diff stats)

---

## 11) E2E Workflow (PO → Dev → QA) with Policy & Gates

1. **PO** creates `story` with AC + DoD + refs policy → publishes `story.ready`.
2. **Dev** processes `story.ready`:
   - dedupe
   - opencode (if allowed)
   - enforce change budget
   - publish `build.done` or `dev.needs_approval`
3. **QA** processes `build.done`:
   - run DoD gates
   - publish `test.result(pass|fail)`
   - on fail: create `bug` + publish `bug.new`
4. **Dev** processes `bug.new` and repeats.
5. **PO** marks story done (and performs any human approvals required).

---

## 12) MVP Delivery Plan (stream-friendly)

### Phase A: Infra & Plumbing
- Verify MQTT publish/subscribe
- Verify Redis connectivity from agent host
- Implement config loader + topic builder

### Phase B: E2E Flow with 3 roles
- PO creates story + AC + DoD → `story.ready`
- Dev runs opencode (within policy/budget) → `build.done`
- QA runs DoD gates → `test.result` + optional `bug.new`

### Phase C: Hardening
- Backoff/reconnect for MQTT
- Idempotency + replay safety via Redis dedupe
- Persisted audit trail via Redis Streams
- Kill switch + policy changes live

---

## 13) Bug ↔ Testplan Linking & Closure Gate (mandatory)

Each bug must be tied to a dedicated testplan/regression case. A bug can only be closed if we have evidence that:
- the regression test **fails before the fix** (tested once is enough), and
- the same regression test **passes after the fix**.

### Bug document requirements
For `type=bug` documents:
- `testplan_id` is **required** and must point to a `type=testplan` doc
- `regression_case_id` is **required** (the single test case that covers the bug)
- `repro_steps` contains the **one-time repro** description (manual repro is OK)
- Optional but recommended evidence fields:
  - `pre_fix_evidence_artifact_id`
  - `post_fix_evidence_artifact_id`

### Testplan requirements
For `type=testplan` documents linked to a bug:
- `bug_id` is **required**
- `cases[]` must include an entry matching `regression_case_id`
- the QA run results should be stored as artifacts and referenced from the bug/testplan

### Status transitions (bug)
Recommended bug status flow:
- `open -> in_fix -> fixed -> verifying -> closed`

**Gate:** transition to `closed` is allowed only when:
- pre-fix run: regression case recorded as **fail** (once)
- post-fix run: regression case recorded as **pass**

---

## 14) Open Questions / Next refinements
- Redis auth/TLS requirements?
- Do we want a minimal dashboard/CLI to inspect docs + streams?
- Exact opencode invocation contract for best determinism (args, prompts, artifacts)?
- Do we need staged deploy events now, or later?

---

## 15) Next implementation step (proposed)
- Create Rust workspace skeleton (Cargo workspace + crate stubs)
- Implement `crates/config` (TOML load) and `crates/bus_mqtt` topic builder
- Implement `crates/store_redis` minimal:
  - `dedupe(eventId)`
  - `put_doc/get_doc`
  - append to `stream:events`
- Implement minimal runners:
  - `agents/po` subscribe + create story
  - `agents/dev` subscribe + run opencode stub + emit build.done
  - `agents/qa` subscribe + run gates stub + emit test.result
