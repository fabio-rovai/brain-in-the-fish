# Tardygrada vs Naive Rust Benchmark

Same pipeline, two coordination models, real subagent scores.

## What This Branch Does

Benchmarks the full brain-in-the-fish evaluation pipeline through two coordination layers. Scores come from real Claude subagents — 4 evaluators scoring independently, debate rounds with challenges and responses. Nothing changes in core brain-in-the-fish. This is an optional showcase branch.

## Three Options

### Option A: Naive Rust (`bench-naive`)

- **1,908 lines**, 11 modules
- Plain structs + functions, same deps as core
- **7.8ms** coordination time
- No integrity guarantees beyond Rust's type system

### Option B: Tardygrada C VM (`bench-tardygrada`)

- **431 lines** application logic (+ 940 lines one-time FFI)
- Every value is a VM agent with provenance
- **468ms** coordination time (60x slower)
- Gets: `mprotect` immutability, SHA-256 hash verification on every read, provenance tracking, agent lifecycle/GC, message-based coordination

### Option C: Pure Tardygrada C (zero deps)

- **240 KB** standalone binary
- **~180 lines** of C application code
- Self-hosted ontology (Datalog + frames)
- Zero external dependencies (no Rust, no crates, no malloc)
- Same VM operations: spawn, verified read, message passing, GC

### Code Comparison

| | Naive | Tardygrada | Pure C |
|---|---|---|---|
| Application logic | 1,559 lines | 431 lines (3.6x more concise) | ~180 lines (8.7x more concise) |
| Type definitions | 338 lines | 0 lines (VM handles it) | 0 lines (VM handles it) |
| FFI boilerplate | 0 lines | 940 lines (one-time cost) | 0 lines (direct C) |

## Results

Real subagent scores from 4 evaluators + debate:

| Metric | Naive | Tardygrada | Pure C |
|---|---|---|---|
| Coordination time | 7.8ms | 468ms | ~689ms |
| Verdict | FLAGGED | FLAGGED | CONFIRMED |
| Overall score | ~8.0/10 | ~7.9/10 | ~7.9/10 |
| Application code | 1,559 lines | 431 lines | ~180 lines |
| Binary size | N/A (Rust) | N/A (Rust+FFI) | 240 KB |

## How To Run

### Coordination benchmarks (no API needed)

```sh
cargo bench
```

### Full pipeline with real scores

```sh
cargo run -p benchmarks --release
```

### Pure C benchmark (no Rust needed)

```sh
cd crates/bench-tardygrada-pure && make bench_pure_simple
./bench_pure
```

### Bring your own scores

Edit `bench_scores.json` with your subagent outputs and re-run.

## Choose What Fits

- **Need speed and simplicity?** Use `bench-naive` patterns.
- **Need formal verification and integrity?** Use `bench-tardygrada` patterns.
- **Need a standalone C binary with zero deps?** Use `bench-tardygrada-pure` — 240 KB, same VM guarantees, no Rust toolchain required.
- **Want both?** The coordination layer is swappable — core brain-in-the-fish doesn't change either way.
