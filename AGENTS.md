# AGENTS.md — Agent Coding Guidelines

This document provides guidelines for agentic coding agents operating in this repository.

## Project Overview

Agile Inc. is a Rust monorepo containing:
- **Crates**: common, config, bus_mqtt, store_redis, opencode_runner
- **Agents**: po, dev, qa (placeholder implementations)
- **Tools**: publish_event

## Build Commands

```bash
# Build entire workspace
cargo build --workspace

# Build specific crate
cargo build -p agile_common
cargo build -p agents-po

# Release build
cargo build --workspace --release

# Run agent binaries
cargo run -p agents-po --bin po --release
cargo run -p agents-dev --bin dev --release
cargo run -p agents-qa --bin qa --release
```

## Test Commands

```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p agile_common

# Run a single test by name
cargo test -p agile_common <test_name>

# Run tests with output
cargo test --workspace -- --nocapture

# Check (compile without running)
cargo check --workspace

# Clippy lints
cargo clippy --workspace -- -D warnings
```

## Code Style Guidelines

### Formatting
- Run `cargo fmt` before committing (uses default rustfmt)
- Use 4 spaces for indentation
- Max line length: 100 characters (rustfmt default)

### Imports
- Group imports: std → external (crates) → internal
- Use inline imports for single items, grouped for multiple
- Example:
  ```rust
  use std::fs;
  use serde::{Deserialize, Serialize};
  use agile_common::types::Foo;
  ```

### Naming Conventions
- **Variables/functions**: snake_case (`my_function`, `config_value`)
- **Types/Enums**: PascalCase (`struct MyStruct`, `enum MyEnum`)
- **Constants**: SCREAMING_SNAKE_CASE
- **Crates**: kebab-case in Cargo.toml, convert to snake_case in use statements

### Types
- Prefer explicit type annotations for public APIs
- Use `&str` for string slices, `String` for owned strings
- Use `Result<T, E>` for fallible operations; avoid `unwrap()` in production code
- Clone only when necessary; prefer references where possible

### Error Handling
- Use `?` operator for propagating errors
- Use `anyhow` for application error handling (if needed)
- Use `thiserror` for library error types (if needed)
- Avoid `.expect()` in production code; prefer proper error handling
- Example:
  ```rust
  pub fn load_config(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
      let s = fs::read_to_string(path)?;
      let config: Config = toml::from_str(&s)?;
      Ok(config)
  }
  ```

### Structs and Enums
- Derive `Debug`, `Clone`, `Serialize`, `Deserialize` for data types
- Use tuple structs for wrapper types: `pub struct Id(pub u64)`
- Use field structs for multiple fields

### Functions
- Keep functions focused and small (< 50 lines)
- Use descriptive names; avoid abbreviations except well-known ones
- Document public APIs with doc comments (`///`)
- Prefer early returns over deeply nested conditionals

### Modules
- One module per file, match file name to module name
- Use `pub mod` for public modules, keep private by default
- Re-export types from crate root for convenient access

### Dependencies
- Minimize external dependencies
- Pin versions in Cargo.toml for reproducibility
- Use workspace dependencies for shared crates

### Testing
- Unit tests in same file under `#[cfg(test)]` module
- Integration tests in `tests/` directory
- Use descriptive test names: `test_function_name_when_condition`

### Git Conventions
- Commit messages: imperative mood, 50 chars max for title
- Use conventional commits format if adopted: `feat:`, `fix:`, `chore:`
- Run `cargo fmt` and `cargo clippy` before committing
- Make a commit every time you made changes to a file
- Commit as often as possible to keep the changes per commit small

## Configuration

- Copy `config/agile-inc.toml.example` to `config/agile-inc.toml` and configure
- Start infrastructure: `docker compose -f infra/docker-compose.yml up -d`
