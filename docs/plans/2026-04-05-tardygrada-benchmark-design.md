# Tardygrada vs Naive Rust Benchmark — Design

**Date:** 2026-04-05
**Branch:** `bench/tardygrada-vs-naive`
**Repo:** brain-in-the-fish

## Goal

Showcase Tardygrada's C VM agent model against idiomatic Rust by benchmarking the full brain-in-the-fish evaluation pipeline three ways. No changes to core bitf. Users see the numbers and choose for themselves.

## Three Contestants

### 1. core (control)

The real brain-in-the-fish as it exists today. Full pipeline, all deps, all modules. This is the baseline.

### 2. bench-naive

Same full pipeline rewritten in flat idiomatic Rust. Same dependencies (open-ontologies, tokio, serde, reqwest, etc). No trait hierarchies or GraphStore indirection — just structs and function calls doing the same work. Proves what plain Rust gives you.

### 3. bench-tardygrada

Same full pipeline, same dependencies, but every operation goes through Tardygrada's C VM:

| bitf operation | Tardygrada equivalent |
|---|---|
| Spawn EvaluatorAgent | `tardy_vm_spawn` with trust level + constitution |
| Store score | `tardy_vm_spawn` a Fact agent with provenance |
| Read score | `tardy_vm_read` (hash-verified for @verified, BFT for @sovereign) |
| Update trust weight | `tardy_vm_mutate` on mutable agent |
| Debate challenge | `tardy_vm_send` message between agents |
| Receive response | `tardy_vm_recv` |
| Moderation consensus | VM message passing + trust-weighted read |
| OWL grounding | `tardy_ontology_ground` through ontology bridge |
| SPARQL rules | Same rule mining, results stored as Fact agents |
| Argument graph | Each node/edge is an agent with typed relationships |
| Gate verdict | Pipeline result stored as @verified Fact |
| Agent lifecycle/GC | Live→Static demotion, tombstones |
| Verification pipeline | Full 8-layer `tardy_pipeline_verify` on every LLM-produced Fact |

Tardygrada does ALL operations bitf does, plus its own integrity layer on top (hash checks, mprotect, provenance, verification pipeline).

## Crate Structure

```
brain-in-the-fish/
├── crates/
│   ├── core/                    # UNTOUCHED
│   ├── cli/                     # UNTOUCHED
│   ├── mcp/                     # UNTOUCHED
│   ├── bench-naive/             # NEW — full pipeline, flat Rust
│   │   ├── Cargo.toml           # same deps as core
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs         # same domain types, plain structs
│   │       ├── ingest.rs        # section splitting
│   │       ├── agent.rs         # panel spawning, trust wiring
│   │       ├── alignment.rs     # section→criteria mapping via OWL
│   │       ├── scoring.rs       # scoring prompts + recording
│   │       ├── debate.rs        # disagreement, challenges, drift, convergence
│   │       ├── moderation.rs    # trust-weighted consensus, outlier detection
│   │       ├── rules.rs         # SPARQL rule derivation
│   │       ├── gate.rs          # verdict engine (structural + quality + evidence)
│   │       ├── argument_graph.rs
│   │       └── orchestrator.rs  # full pipeline glue
│   └── bench-tardygrada/        # NEW — full pipeline on Tardygrada C VM
│       ├── Cargo.toml           # same deps as core + cc/bindgen for C VM
│       ├── build.rs             # compiles Tardygrada VM as static lib
│       ├── tardygrada/          # vendored tardygrada src/ (all C files)
│       └── src/
│           ├── lib.rs
│           ├── ffi.rs           # safe Rust wrappers around C VM FFI
│           ├── vm_agents.rs     # agents modelled as tardy_vm_spawn/read/mutate
│           ├── pipeline.rs      # full pipeline using VM primitives
│           └── orchestrator.rs  # coordination via VM message passing
├── benches/
│   ├── full_pipeline.rs         # criterion: all 3, ingest→gate
│   ├── debate_rounds.rs         # criterion: debate loop isolation
│   ├── scaling.rs               # criterion: 5→50→500→5000 agents
│   └── fixtures/
│       └── mock_data.rs         # deterministic docs, scores, LLM responses
└── Cargo.toml                   # workspace adds new crates + criterion
```

## Benchmark Harness

**Framework:** criterion.rs

**Workload:** Full evaluation pipeline (ingest → criteria → agent spawn → alignment → scoring → debate loop → moderation → rule mining → gate verdict).

**LLM responses:** Deterministic mocks — reproducible results, isolates coordination cost.

**Benchmarks:**

1. **full_pipeline** — end-to-end, all 3 contestants, same document + framework
2. **debate_rounds** — debate loop in isolation (spawn → score → N rounds → converge)
3. **scaling** — agent count sweep: 5, 50, 500, 5000 agents

**Metrics:**
- ops/sec
- Latency: p50, p95, p99
- Memory usage (peak RSS)
- Tardygrada-specific: hash checks/sec, pipeline verifications/sec, mprotect calls

## What This Proves

The benchmark answers: "Even with formal verification, cryptographic integrity, OS-enforced immutability, provenance tracking, and an 8-layer verification pipeline on every Fact — what does it cost in wall-clock time versus plain Rust?"

If Tardygrada is competitive or faster (due to mmap/mprotect being cheaper than Rust's ownership bookkeeping at scale), it makes a compelling case. If it's slower, users see exactly how much integrity costs and decide if it's worth it.

Either way: honest numbers, no Tardygrada forced on anyone.
