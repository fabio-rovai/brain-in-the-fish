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
| --- | --- | --- |
| `types` | `src/types.rs` | Core data structures shared across all modules |
| `ingest` | `src/ingest.rs` | PDF and plain text document ingestion with section splitting |
| `criteria` | `src/criteria.rs` | Evaluation criteria ontology and built-in frameworks (7 built-in + YAML/JSON) |
| `agent` | `src/agent.rs` | Agent cognitive model (Maslow hierarchy + Theory of Planned Behaviour) and domain-specific panel spawning |
| `scoring` | `src/scoring.rs` | Independent scoring engine with ReACT loop prompts and SPARQL queries |
| `debate` | `src/debate.rs` | Multi-round structured debate with disagreement detection and convergence |
| `moderation` | `src/moderation.rs` | Trust-weighted consensus moderation with outlier/dissent handling |
| `report` | `src/report.rs` | Evaluation report generation (Markdown scorecard + Turtle export) |
| `snn` | `src/snn.rs` | Spiking neural network verification layer for anti-hallucination scoring |
| `llm` | `src/llm.rs` | LLM client abstraction (Anthropic API, demo mode fallback) |
| `alignment` | `src/alignment.rs` | Ontology alignment between document sections and evaluation criteria (7 structural signals) |
| `research` | `src/research.rs` | Research pipeline for evidence gathering and synthesis |
| `memory` | `src/memory.rs` | Agent memory persistence across evaluation rounds |
| `visualize` | `src/visualize.rs` | Evaluation visualization, interactive graph HTML, chart generation |
| `validate` | `src/validate.rs` | 15 deterministic document validation checks feeding SNN spikes/inhibition |
| `batch` | `src/batch.rs` | Batch evaluation of multiple documents |
| `belief_dynamics` | `src/belief_dynamics.rs` | Maslow needs update from evaluation findings |
| `epistemology` | `src/epistemology.rs` | Justified beliefs with empirical, normative, and testimonial bases |
| `philosophy` | `src/philosophy.rs` | Kantian, utilitarian, and virtue ethics analysis |
| `orchestrator` | `src/orchestrator.rs` | Subagent task generation for Claude-enhanced scoring |
| `semantic` | `src/semantic.rs` | Semantic similarity via embeddings (TextEmbedder + VecStore) |
| `server` | `src/server.rs` | MCP server with 12 `eval_*` tools via rmcp |
| `main` | `src/main.rs` | CLI interface: `evaluate` and `serve` commands, full pipeline orchestration |
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
