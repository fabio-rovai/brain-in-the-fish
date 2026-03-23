# Contributing to brain-in-the-fish

Thank you for your interest in contributing! This document explains how to get started.

## Prerequisites

- **Rust 1.85+** (stable toolchain)
- **open-ontologies** cloned alongside this repository (i.e. `../open-ontologies`)

Your directory layout should look like:

```
parent/
  brain-in-the-fish/
  open-ontologies/
```

## Getting Started

```bash
# Clone both repositories
git clone https://github.com/fabio-rovai/open-ontologies.git
git clone https://github.com/fabio-rovai/brain-in-the-fish.git

# Build
cd brain-in-the-fish
cargo build

# Run tests
cargo test

# Run linter
cargo clippy -- -D warnings
```

## Workflow

1. **Fork** the repository
2. **Create a branch** from `main` (`git checkout -b feature/my-change`)
3. **Make your changes** and add tests where appropriate
4. **Run all checks** before submitting:
   ```bash
   cargo build
   cargo test
   cargo clippy -- -D warnings
   ```
5. **Open a Pull Request** against `main`

All tests must pass before a PR will be merged.

## Module Architecture

| Module | File | Purpose |
|---|---|---|
| `types` | `src/types.rs` | Core data structures shared across all modules |
| `ingest` | `src/ingest.rs` | PDF and plain text document ingestion with section splitting |
| `criteria` | `src/criteria.rs` | Evaluation criteria ontology and built-in frameworks (generic, academic, tender) |
| `agent` | `src/agent.rs` | Agent cognitive model (Maslow hierarchy + Theory of Planned Behaviour) and domain-specific panel spawning |
| `scoring` | `src/scoring.rs` | Independent scoring engine with ReACT loop prompts and SPARQL queries |
| `debate` | `src/debate.rs` | Multi-round structured debate with disagreement detection and convergence |
| `moderation` | `src/moderation.rs` | Trust-weighted consensus moderation with outlier/dissent handling |
| `report` | `src/report.rs` | Evaluation report generation (Markdown scorecard + Turtle export) |
| `server` | `src/server.rs` | MCP server with 10 `eval_*` tools via rmcp |
| `main` | `src/main.rs` | CLI interface: `evaluate` and `serve` commands |
| `lib` | `src/lib.rs` | Library root, re-exports all modules |

## Code Style

- Follow standard Rust conventions (`rustfmt` defaults)
- Use `anyhow::Result` for error handling
- MCP tool functions use the `eval_*` prefix (e.g. `eval_ingest`, `eval_score`)
- Keep modules focused: one responsibility per file
- Write unit tests in the same file, integration tests in `tests/`

## Reporting Issues

Open an issue on GitHub with a clear description of the problem, steps to reproduce, and expected vs actual behaviour.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
