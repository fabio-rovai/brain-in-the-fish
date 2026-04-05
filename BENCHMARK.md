# Tardygrada vs Naive Rust Benchmark

Same pipeline, two coordination models, real subagent scores.

## What This Branch Does

Benchmarks the full brain-in-the-fish evaluation pipeline through two coordination layers. Scores come from real Claude subagents — 4 evaluators scoring independently, debate rounds with challenges and responses. Nothing changes in core brain-in-the-fish. This is an optional showcase branch.

## Two Options

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

### Code Comparison

| | Naive | Tardygrada |
|---|---|---|
| Application logic | 1,559 lines | 431 lines (3.6x more concise) |
| Type definitions | 338 lines | 0 lines (VM handles it) |
| FFI boilerplate | 0 lines | 940 lines (one-time cost) |

## Results

Real subagent scores from 4 evaluators + debate:

| Metric | Naive | Tardygrada |
|---|---|---|
| Coordination time | 7.8ms | 468ms |
| Verdict | FLAGGED | FLAGGED |
| Overall score | ~8.0/10 | ~7.9/10 |
| Application code | 1,559 lines | 431 lines |

## How To Run

### Coordination benchmarks (no API needed)

```sh
cargo bench
```

### Full pipeline with real scores

```sh
cargo run -p benchmarks --release
```

### Bring your own scores

Edit `bench_scores.json` with your subagent outputs and re-run.

## Choose What Fits

- **Need speed and simplicity?** Use `bench-naive` patterns.
- **Need formal verification and integrity?** Use `bench-tardygrada` patterns.
- **Want both?** The coordination layer is swappable — core brain-in-the-fish doesn't change either way.
