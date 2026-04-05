# Tardygrada vs Naive Rust Benchmark

Same evaluation. Same subagents. Same verdict. Different coordination.

## What This Measures

4 real Claude subagents independently score a tender document, debate disagreements, and reach consensus. The scoring takes ~9 seconds (LLM calls). The coordination layer processes their scores through debate, moderation, rules, and a gate verdict.

This benchmark measures the coordination layer — **not** the LLM calls.

## The Numbers

| | Naive Rust | Pure Tardygrada C |
|---|---:|---:|
| **Lines of code you write** | 1,559 | **180** |
| **Dependencies** | 1,162 crates | **0** |
| **Binary size** | Rust toolchain required | **240 KB, copy and run** |
| **Coordination time** | 6 ms | 372 ms |
| **Integrity** | none | hash-verified, mprotect, provenance |
| **Verdict** | FLAGGED | CONFIRMED |
| **Overall score** | 7.98/10 | 7.89/10 |

## Where the time actually goes

```
Subagent scoring (4 agents, parallel)     ~9,000 ms
Subagent debate (challenge + response)     ~3,000 ms
─────────────────────────────────────────────────────
Total LLM time                            ~12,000 ms

Coordination overhead (naive Rust)               6 ms  (0.05%)
Coordination overhead (Tardygrada C)           372 ms  (3.0%)
```

The 366ms difference disappears inside a single LLM call.

## What Tardygrada does with those 372ms

Every operation in the pipeline is a VM operation with integrity guarantees:

- **Spawn agent** — mmap-allocated, page-aligned, mprotect-enforced
- **Store score** — SHA-256 hashed at birth, verified on every read
- **Read score** — hash check (197ns) or BFT vote (1,538ns) depending on trust level
- **Send challenge** — message queued with provenance chain
- **Update trust** — mutable agent, mutation tracked
- **Store verdict** — @sovereign: ed25519 signed, 5 replicas, BFT consensus
- **GC** — dead agents tombstoned with provenance preserved

Naive Rust stores scores in a `Vec<Score>`. No hash. No verification. No provenance. No way to prove a score wasn't tampered with after the fact.

## What you write

**Naive Rust** — 11 modules, 1,559 lines:
```
types.rs        338 lines    (define every struct)
debate.rs       182 lines    (disagreement detection, drift, convergence)
rules.rs        179 lines    (fact mining)
alignment.rs    176 lines    (section-to-criteria mapping)
ingest.rs       172 lines    (document parsing)
moderation.rs   142 lines    (trust-weighted consensus)
orchestrator.rs 130 lines    (pipeline glue)
gate.rs         127 lines    (verdict engine)
agent.rs        126 lines    (panel spawning)
scoring.rs       82 lines    (prompt generation)
+ argument_graph, lib.rs
```

**Pure Tardygrada C** — 1 file, 180 lines:
```c
tardy_vm_spawn(vm, root, "Budget_Expert", TARDY_TYPE_AGENT, TARDY_TRUST_VERIFIED, "", 0);
tardy_vm_spawn(vm, agent, "score_crit-0", TARDY_TYPE_FLOAT, TARDY_TRUST_VERIFIED, &score, 8);
tardy_vm_read(vm, agent, "score_crit-0", &val, 8);  // hash-verified
tardy_vm_send(vm, challenger, target, challenge, len, TARDY_TYPE_STR);
tardy_vm_spawn(vm, root, "verdict", TARDY_TYPE_STR, TARDY_TRUST_SOVEREIGN, "CONFIRMED", 9);
```

No types to define. No debate module. No moderation module. No rules module. The VM handles agent lifecycle, trust, verification, and garbage collection. You just spawn, read, write, send.

## The three contestants

### Option A: Naive Rust (`bench-naive`)
1,559 lines. 1,162 crate dependencies. 6ms coordination. No integrity. You write everything.

### Option B: Tardygrada via Rust FFI (`bench-tardygrada`)
431 lines of application logic + 940 lines one-time FFI bindings. Same Rust deps plus C VM. 390ms coordination. Full integrity. FFI adds translation overhead.

### Option C: Pure Tardygrada C (`bench-tardygrada-pure`)
180 lines. Zero dependencies. 240 KB binary. 372ms coordination. Full integrity. No Rust toolchain needed.

## How the subagents worked

Real Claude subagents scored independently (no mocks, no API wrappers):

1. **Budget Expert**, **Technical Evaluator**, **Delivery Specialist**, **Social Value Assessor** — spawned in parallel, each scored 5 criteria
2. **Biggest disagreement**: Social Value — Delivery Specialist scored 7.5, Social Value Assessor scored 8.5 (delta: 1.0)
3. **Debate**: Social Value Assessor challenged Delivery Specialist with evidence about specificity scores (0.8-0.9) and proven track record
4. **Result**: Delivery Specialist adjusted from 7.5 to 8.2
5. Both coordination pipelines processed the same 40 real scores (20 per round)

## How to run

### Full benchmark (all three contestants)
```sh
cargo run -p benchmarks --release
```

### Coordination microbenchmarks (criterion, no LLM)
```sh
cargo bench
```

### Pure C only (no Rust toolchain needed)
```sh
cd crates/bench-tardygrada-pure
make bench_pure_simple
./bench_pure
```

### Bring your own scores
Edit `bench_scores.json` with outputs from your own subagents and re-run.

## Ontology configuration

Tardygrada supports two ontology engines:

| Engine | What it is | When to use |
|---|---|---|
| **open-ontologies** (bridge) | Full OWL reasoning via Unix socket | Production — needs open-ontologies running |
| **Self-hosted** (built-in) | Datalog inference + Minsky frames | Standalone — no external deps |

Control via `TARDY_ONTOLOGY` env var:
- `both` (default) — open-ontologies preferred, self-hosted fallback
- `bridge` — open-ontologies only, fail if unavailable
- `self` — self-hosted only
- `none` — skip ontology

The pure C benchmark uses self-hosted. The Rust FFI benchmark links open-ontologies as a crate.
