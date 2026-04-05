# Tardygrada vs Naive Rust Benchmark

This branch (`bench/tardygrada-vs-naive`) benchmarks the brain-in-the-fish
evaluation pipeline using two competing implementations:

| Contestant | Description |
|---|---|
| **bench-naive** | Pure Rust implementation with plain structs, HashMap-based trust, and in-memory scoring. No verification overhead. |
| **bench-tardygrada** | Tardygrada VM implementation where every operation (spawn, score, read, mutate, message) goes through the C VM with provenance tracking, trust tiers, hash-verified reads, and garbage collection. |

## Two benchmark modes

### 1. Coordination benchmarks (`cargo bench`)

Measures pure coordination overhead — no LLM API calls, deterministic mock
scores. Runs under Criterion with statistical analysis and HTML reports.

```sh
# All coordination benchmarks
cargo bench -p benchmarks

# Individual benchmarks
cargo bench --bench full_pipeline -p benchmarks
cargo bench --bench debate_rounds -p benchmarks
cargo bench --bench scaling -p benchmarks
```

These benchmarks use pre-computed mock scores (60 scores: 4 agents x 5 criteria
x 3 rounds). Both contestants receive identical workloads. The tardygrada
pipeline does strictly more work (trust-tier enforcement, provenance tracking,
hash verification, GC) so the overhead is the cost of integrity.

HTML reports are generated in `target/criterion/`.

### 2. Full pipeline with real LLM subagents (`cargo run -p benchmarks`)

Runs the complete evaluation pipeline with **real Claude API calls**. Each
evaluator agent is an independent async task making real LLM calls. The
orchestrator spawns these agent tasks, coordinates debate between them, and
collects results.

```sh
# Requires ANTHROPIC_API_KEY
ANTHROPIC_API_KEY=sk-... cargo run -p benchmarks

# Optional: choose model and round count
BRAIN_MODEL=claude-sonnet-4-6 BENCH_MAX_ROUNDS=2 cargo run -p benchmarks
```

**What happens:**
1. 4 evaluator agents are spawned (Budget Expert, Technical Evaluator, etc.)
2. Each agent scores 3 criteria in parallel (12 concurrent LLM calls per round)
3. Disagreements trigger LLM-powered challenge/response debate
4. Scores converge through debate rounds
5. Trust-weighted moderation produces final scores
6. Gate check verifies scores against structural evidence

Both bench-naive and bench-tardygrada run the same LLM pipeline. The
tardygrada version additionally routes all operations through the Tardygrada
VM for provenance, hash-verified reads, and sovereign verdict storage.

**Environment variables:**
| Variable | Default | Description |
|---|---|---|
| `ANTHROPIC_API_KEY` | *(required)* | Claude API key |
| `BRAIN_MODEL` | `claude-sonnet-4-6` | Model for scoring and debate |
| `BENCH_MAX_ROUNDS` | `3` | Maximum debate rounds |

## What is measured

### Coordination benchmarks

#### `full_pipeline`

End-to-end evaluation: spawn agents, align sections to criteria, 3-round
debate loop (score, find disagreements, challenge, drift, convergence),
trust-weighted moderation, argument graph construction, and gate verdict.

#### `debate_rounds`

The debate phase in isolation: 3 rounds of find_disagreements + drift
velocity + convergence checking (naive), or spawn + record scores + read
verified + send challenges + drain responses + GC (tardygrada).

#### `scaling`

Agent count sweep at 5, 50, 500, and 5000 agents. Measures spawn + wire
trust + score + moderate (naive) or spawn + record + read verified + GC
(tardygrada).

### Real LLM benchmarks

Wall-clock time for the full pipeline including all LLM API latency. This
measures end-to-end user-facing performance, not just coordination overhead.
The LLM calls dominate the timing, so the comparison shows how much the
tardygrada VM overhead matters (or doesn't) when real API latency is in play.

## Methodology

- **Deterministic mocks** (coordination): Both contestants use identical
  pre-computed scores with no LLM calls.
- **Real LLM** (full pipeline): Both contestants make identical Claude API calls
  with parallel tokio tasks. One retry on transient failure.
- **Same workload**: Same document, same framework, same debate parameters.
- **Tardygrada does strictly more work**: Every operation goes through the C VM
  with trust-tier enforcement, provenance tracking, hash verification, and GC.
- **Fair comparison**: The extra cost of tardygrada is the cost of integrity.

## Results

Benchmark run on Apple Silicon (Darwin 25.2.0), `--release` profile.

### Coordination benchmarks (full_pipeline)

| Contestant | Time |
|---|---|
| naive/full_pipeline | 4.52 ms |
| tardygrada/full_pipeline | *skipped (FFI layout issue)* |

### Coordination benchmarks (debate_rounds)

| Contestant | Time |
|---|---|
| naive/debate_3rounds | 4.36 us |
| tardygrada/debate_3rounds | *skipped (FFI layout issue)* |

### Coordination benchmarks (scaling, naive only)

| Agent count | Time |
|---|---|
| 5 | 10.7 us |
| 50 | 207 us |
| 500 | 15.1 ms |
| 5000 | 1.45 s |

### Real LLM benchmarks

*Run `cargo run -p benchmarks` with your API key to get results.*

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
