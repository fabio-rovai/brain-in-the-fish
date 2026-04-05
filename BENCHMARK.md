# Tardygrada vs Naive Rust Benchmark

This branch (`bench/tardygrada-vs-naive`) benchmarks the brain-in-the-fish
evaluation pipeline using two competing implementations:

| Contestant | Description |
|---|---|
| **bench-naive** | Pure Rust implementation with plain structs, HashMap-based trust, and in-memory scoring. No verification overhead. |
| **bench-tardygrada** | Tardygrada VM implementation where every operation (spawn, score, read, mutate, message) goes through the C VM with provenance tracking, trust tiers, hash-verified reads, and garbage collection. |

## What is measured

### `full_pipeline`

End-to-end evaluation: spawn agents, align sections to criteria, 3-round
debate loop (score, find disagreements, challenge, drift, convergence),
trust-weighted moderation, argument graph construction, and gate verdict.

### `debate_rounds`

The debate phase in isolation: 3 rounds of find_disagreements + drift
velocity + convergence checking (naive), or spawn + record scores + read
verified + send challenges + drain responses + GC (tardygrada).

### `scaling`

Agent count sweep at 5, 50, 500, and 5000 agents. Measures spawn + wire
trust + score + moderate (naive) or spawn + record + read verified + GC
(tardygrada).

## How to run

```sh
# All benchmarks
cargo bench -p benchmarks

# Individual benchmarks
cargo bench --bench full_pipeline -p benchmarks
cargo bench --bench debate_rounds -p benchmarks
cargo bench --bench scaling -p benchmarks
```

HTML reports are generated in `target/criterion/`.

## Methodology

- **Deterministic mocks**: Both contestants use identical pre-computed scores
  (60 scores: 4 agents x 5 criteria x 3 rounds) with no LLM calls.
- **Same workload**: Same document (10 sections, 30 claims, 20 evidence items),
  same framework (5 criteria), same debate parameters (3 rounds, 2.0
  disagreement threshold, 0.5 convergence threshold).
- **Tardygrada does strictly more work**: Every operation in the tardygrada
  pipeline goes through the C VM with trust-tier enforcement, provenance
  tracking, hash verification on reads, and garbage collection. The naive
  pipeline does none of this.
- **Fair comparison**: The extra cost of tardygrada is the cost of integrity.
  The benchmark quantifies that cost.

## Results

Benchmark run on Apple Silicon (Darwin 25.2.0), `--release` profile.

### full_pipeline

| Contestant | Time |
|---|---|
| naive/full_pipeline | 4.52 ms |
| tardygrada/full_pipeline | *skipped (FFI layout issue)* |

### debate_rounds

| Contestant | Time |
|---|---|
| naive/debate_3rounds | 4.36 us |
| tardygrada/debate_3rounds | *skipped (FFI layout issue)* |

### scaling (naive only)

| Agent count | Time |
|---|---|
| 5 | 10.7 us |
| 50 | 207 us |
| 500 | 15.1 ms |
| 5000 | 1.45 s |

**Note on tardygrada results**: The Tardygrada C VM's `tardy_vm_spawn`
currently returns a zero UUID when called from the Rust FFI layer. This is a
known struct-layout mismatch between the Rust `repr(C)` bindings and the
actual C `tardy_vm_t` memory layout (the VM is ~5 GB of lazily-allocated
memory). The benchmark harness detects this at startup and gracefully skips
the tardygrada benchmarks rather than panicking. Once the FFI layout is
fixed, the tardygrada benchmarks will run automatically.

### Naive scaling observations

The naive pipeline scales roughly as O(n^2) due to the trust-weight wiring
(every agent gets a trust relation to every other agent):

- 5 agents: 10.7 us (baseline)
- 50 agents (10x): 207 us (19x) -- slightly better than quadratic
- 500 agents (100x): 15.1 ms (1,411x) -- approaching quadratic
- 5000 agents (1000x): 1.45 s (135,514x) -- super-quadratic from allocation pressure
