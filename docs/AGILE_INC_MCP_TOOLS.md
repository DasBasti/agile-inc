# Agile Inc. — MCP Tool Specification (v0.1)

This document defines a **technology-agnostic tool interface** for Agile Inc.

Agents must use only these tools (capabilities). The runtime can bind them to Redis/MQTT/opencode today, and swap implementations later without changing the role prompts.

---

## Global conventions

### Identifiers
- `docId`: UUID string
- `eventId`: UUID string
- `traceId`: UUID string (stable across a workflow)

### Result envelope (recommended)
All tools return either:

Success:
```json
{ "ok": true, "data": {} }
```

Error:
```json
{
  "ok": false,
  "error": {
    "code": "STRING_ENUM",
    "message": "human readable",
    "retryable": true,
    "details": {}
  }
}
```

### Idempotency
Where applicable, requests include `idempotencyKey` (typically the `eventId`). Tools should be safe for retries.

---

## 1) Document Store tools (`doc.*`)

### 1.1 `doc.get`
**Purpose:** Fetch a document by ID.

**Request**
```json
{
  "docId": "uuid",
  "include": { "links": true, "history": false }
}
```

**Response**
```json
{
  "ok": true,
  "data": {
    "doc": {
      "id": "uuid",
      "type": "story|task|bug|testplan|policy|artifact|runlog",
      "title": "string",
      "body": "string",
      "status": "string",
      "ownerRole": "po|dev|qa|system",
      "tags": ["string"],
      "links": ["uuid"],
      "createdAt": "ISO-8601",
      "updatedAt": "ISO-8601",
      "traceId": "uuid",
      "version": 12,
      "fields": {}
    }
  }
}
```

---

### 1.2 `doc.put`
**Purpose:** Create a new document.

**Request**
```json
{
  "doc": {
    "id": "uuid (optional)",
    "type": "story|task|bug|testplan|policy|artifact|runlog",
    "title": "string",
    "body": "string",
    "status": "string",
    "ownerRole": "po|dev|qa|system",
    "tags": ["string"],
    "links": ["uuid"],
    "traceId": "uuid",
    "fields": {}
  },
  "idempotencyKey": "uuid"
}
```

**Response**
```json
{
  "ok": true,
  "data": { "docId": "uuid", "doc": { "...": "..." } }
}
```

**Semantics**
- If `idempotencyKey` already used with same payload → return same result.
- If different payload → error `IDEMPOTENCY_CONFLICT`.

---

### 1.3 `doc.patch`
**Purpose:** Partial update with optimistic concurrency.

**Request**
```json
{
  "docId": "uuid",
  "expectedVersion": 12,
  "patch": [
    { "op": "set", "path": "/status", "value": "in_progress" },
    { "op": "addToSet", "path": "/tags", "value": "backend" },
    { "op": "appendLink", "path": "/links", "value": "uuid" },
    { "op": "setField", "path": "/fields/dod_commands", "value": ["cargo test"] }
  ],
  "traceId": "uuid",
  "idempotencyKey": "uuid"
}
```

**Response**
```json
{
  "ok": true,
  "data": {
    "doc": { "...": "..." },
    "newVersion": 13
  }
}
```

**Notes**
- If `expectedVersion` mismatches → `VERSION_CONFLICT`.

---

### 1.4 `doc.query`
**Purpose:** Find documents by type/status/tags/links/text.

**Request**
```json
{
  "filter": {
    "type": ["story", "bug"],
    "status": ["ready", "open"],
    "tagsAny": ["p0"],
    "linkedTo": ["uuid"],
    "ownerRole": ["po"]
  },
  "search": { "text": "string (optional)" },
  "sort": { "by": "updatedAt|createdAt|priority", "dir": "asc|desc" },
  "limit": 50,
  "cursor": "string (optional)"
}
```

**Response**
```json
{
  "ok": true,
  "data": {
    "docs": [
      { "id": "uuid", "type": "story", "title": "…", "status": "…", "updatedAt": "ISO-8601" }
    ],
    "nextCursor": "string (optional)"
  }
}
```

---

## 2) Event Bus tools (`bus.*`)

### 2.1 `bus.publish`
**Purpose:** Publish a small event to a topic / role inbox.

**Request**
```json
{
  "topic": "string",
  "event": {
    "eventId": "uuid",
    "traceId": "uuid",
    "type": "story.ready|build.done|test.result|bug.created|policy.changed|...",
    "from": "po|dev|qa|system",
    "to": "po|dev|qa|broadcast",
    "refs": {
      "docIds": ["uuid"],
      "storyId": "uuid",
      "taskId": "uuid",
      "bugId": "uuid",
      "testplanId": "uuid"
    },
    "payload": {
      "summary": "string (optional)",
      "result": "pass|fail (optional)",
      "failed_gates": ["string"]
    },
    "ts": "ISO-8601"
  }
}
```

**Response**
```json
{ "ok": true, "data": { "published": true } }
```

---

### 2.2 `bus.subscribe` (optional; pull-mode)
**Purpose:** Pull a batch of events from a topic (useful for polling runtimes).

**Request**
```json
{
  "topic": "string",
  "mode": "stream",
  "fromCursor": "string (optional)"
}
```

**Response**
```json
{
  "ok": true,
  "data": {
    "events": [ { "eventId": "uuid", "type": "...", "refs": {"docIds": ["uuid"]} } ],
    "nextCursor": "string"
  }
}
```

---

## 3) Audit tools (`audit.*`)

### 3.1 `audit.append`
**Purpose:** Append a structured audit entry (append-only).

**Request**
```json
{
  "entry": {
    "ts": "ISO-8601",
    "traceId": "uuid",
    "eventId": "uuid",
    "actorRole": "po|dev|qa|system",
    "action": "string",
    "refs": { "docIds": ["uuid"] },
    "summary": "string",
    "details": {}
  },
  "idempotencyKey": "uuid"
}
```

**Response**
```json
{ "ok": true, "data": { "auditId": "string" } }
```

---

## 4) Idempotency/Dedupe tools (`dedupe.*`)

### 4.1 `dedupe.check_and_mark`
**Purpose:** Ensure an event/work item is processed once.

**Request**
```json
{
  "key": "string (usually eventId)",
  "ttlSeconds": 86400,
  "scope": "global|role",
  "role": "po|dev|qa (required if scope=role)"
}
```

**Response**
```json
{ "ok": true, "data": { "firstTime": true } }
```

Already seen:
```json
{ "ok": true, "data": { "firstTime": false } }
```

---

## 5) Policy tools (`policy.*`)

### 5.1 `policy.get_current`
**Purpose:** Fetch the active autonomy/gates/change-budget policy.

**Request**
```json
{ "scope": "global|project", "projectId": "string (optional)" }
```

**Response**
```json
{
  "ok": true,
  "data": {
    "policy": {
      "policyId": "uuid",
      "autonomyLevel": "L0|L1|L2|L3|L4",
      "changeBudget": {
        "maxFiles": 20,
        "maxInsertions": 500,
        "maxDeletions": 500,
        "protectedPaths": ["infra/"]
      },
      "requiredGates": ["tests", "fmt", "clippy"]
    }
  }
}
```

---

### 5.2 `policy.request_approval`
**Purpose:** Ask for human/PO approval when blocked by policy.

**Request**
```json
{
  "traceId": "uuid",
  "eventId": "uuid",
  "reason": "string",
  "refs": { "docIds": ["uuid"] },
  "proposedAction": {
    "kind": "merge|deploy|large_change|dependency_bump|protected_path_change",
    "details": {}
  }
}
```

**Response**
```json
{ "ok": true, "data": { "approvalRequestId": "uuid" } }
```

---

## 6) Execution tools (`exec.*`) — Developer-only

### 6.1 `exec.run`
**Purpose:** Execute an external command (e.g., opencode), capture logs, produce artifacts.

**Request**
```json
{
  "traceId": "uuid",
  "jobId": "uuid",
  "command": "string",
  "args": ["string"],
  "workdir": "string",
  "timeoutSeconds": 1800,
  "env": { "KEY": "VALUE" },
  "outputLimits": { "maxStdoutKb": 2048, "maxStderrKb": 2048 }
}
```

**Response**
```json
{
  "ok": true,
  "data": {
    "exitCode": 0,
    "stdout": "string (possibly truncated)",
    "stderr": "string (possibly truncated)",
    "durationMs": 12345,
    "artifacts": [
      { "kind": "opencode_stdout", "docId": "uuid" },
      { "kind": "opencode_stderr", "docId": "uuid" }
    ]
  }
}
```

---

## 7) VCS/Workspace tools (optional but useful)

### 7.1 `vcs.diffstat`
**Purpose:** Obtain change budget metrics without parsing raw diffs.

**Request**
```json
{ "workdir": "string", "traceId": "uuid" }
```

**Response**
```json
{
  "ok": true,
  "data": {
    "filesChanged": 3,
    "insertions": 120,
    "deletions": 15,
    "files": ["src/lib.rs", "Cargo.toml"]
  }
}
```

---

## Minimal MVP set
Start with:
- `doc.get`, `doc.put`, `doc.patch`
- `bus.publish`
- `audit.append`
- `dedupe.check_and_mark`
- `policy.get_current`, `policy.request_approval`
- `exec.run`

Everything else can be layered later.
