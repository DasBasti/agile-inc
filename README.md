# Agile Inc. — Monorepo (Rust)

This repository contains the Agile Inc. workspace skeleton.

Run dev stack:

1. Copy `config/agile-inc.toml.example` -> `config/agile-inc.toml` and edit values.
2. Start infra: `docker compose -f infra/docker-compose.yml up -d`
3. Build workspace: `cargo build --workspace`
4. Start agents (examples):
   - `cargo run -p agents-po --bin po --release` (placeholder)

Note: agent binaries are placeholders in this skeleton. Replace with actual implementations.
