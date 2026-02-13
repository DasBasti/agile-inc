# Agile Inc. — Runtime Adapter Layer v1 (implementation binding)

This document binds the tech-agnostic **Core Charter** to a concrete runtime.

## Purpose
- Keep system/role prompts stable even if we swap technologies.
- Centralize implementation details (transport, storage, execution tools).

## Abstractions

### Shared Document Store
**Interface (conceptual tools):**
- `doc.get(id)`
- `doc.put(doc)`
- `doc.update(id, patch)`
- `doc.index(type/status/tags)`

### Team Event Bus
**Interface (conceptual tools):**
- `bus.publish(topic, eventEnvelope)`
- `bus.subscribe(topic)`

### Audit Log
**Interface (conceptual tools):**
- `audit.append(entry)`

### Idempotency / Dedupe
**Interface (conceptual tools):**
- `dedupe.check_and_mark(eventId, ttl)`

### Executor (for Developer)
**Interface (conceptual tools):**
- `exec.run(command, args, workdir, timeout)`

## Current Binding (today)
- Document Store: **Redis** (persisted)
- Event Bus: **MQTT**
- Audit Log: **Redis Streams**
- Dedupe: **Redis SETNX + TTL**
- Executor: **opencode CLI**

## Configuration (central)
All concrete endpoints/credentials/commands must be configurable in one place (e.g. `config/agile-inc.toml`).

## Notes on MCP (Model Context Protocol)
MCP is a good fit to separate *capabilities* from *implementation*:
- Implement an MCP server that exposes tools like `doc.get/put`, `bus.publish`, `audit.append`, `exec.run`.
- Agents then use only those tools, and do not care whether behind the scenes it is Redis/MQTT today or something else tomorrow.

In other words:
- **Core Charter**: describes *what* the agent must do.
- **MCP tools (adapter layer)**: defines *how* those actions map to infrastructure.
