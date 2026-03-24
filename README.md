<p align="center">
  <img src="assets/logo.png" alt="Brain in the Fish" width="200" />
</p>

<h1 align="center">Brain in the Fish</h1>

<p align="center">
  <strong>Evaluate anything. Predict everything. Hallucinate nothing.</strong>
  <br>
  <em>Evidence-verified document evaluation & prediction credibility — the brain that MiroFish was missing.</em>
</p>

<p align="center">
  <img src="https://github.com/fabio-rovai/brain-in-the-fish/actions/workflows/ci.yml/badge.svg" alt="CI" />
  <img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT" />
  <img src="https://img.shields.io/badge/tests-260%20passing-brightgreen" alt="Tests" />
  <img src="https://img.shields.io/badge/rust-edition%202024-orange" alt="Rust" />
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README-CN.md">中文</a> | <a href="README-JP.md">日本語</a>
</p>

---

## Screenshots

<p align="center">
  <img src="assets/screenshot-tree-overview.png" alt="Hierarchical evaluation graph" width="100%" />
  <br><em>Hierarchical knowledge graph — document structure, evaluation criteria, agent panel, and scoring connected in one tree</em>
</p>

<p align="center">
  <img src="assets/screenshot-detail-document.png" alt="Document detail panel" width="100%" />
  <br><em>Detail panel showing ontology reasoning — what the node is, its structure, and why it exists in the knowledge graph</em>
</p>

<p align="center">
  <img src="assets/screenshot-detail-evidence.png" alt="Evidence detail with properties" width="100%" />
  <br><em>Evidence node inspection — properties, ontology role, connections, and provenance</em>
</p>

---

## What It Does

A Rust MCP server that evaluates any document against any criteria using Claude subagents, with an Evidence Density Scorer (EDS) that makes hallucination mathematically detectable. Feed it a PDF and an intent — it returns structured scores, weakness analysis, and a full audit trail. Document evaluation is the core differentiator: BITF scores 2.8pp from expert scores where raw Claude drifts ~15pp. An optional prediction credibility module provides structured extraction with evidence-based verification.

```bash
# As MCP server (recommended — Claude orchestrates subagent evaluation)
brain-in-the-fish serve

# As CLI (deterministic evidence scoring, no API key needed)
brain-in-the-fish evaluate policy.pdf --intent "evaluate against Green Book standards" --open
```

---

## Performance

Benchmarked against real expert-scored documents across education, policy, heritage, public health, technology, and research domains.

### Document Evaluation (12 real expert-evaluated documents)

| Metric | Value |
| ------ | ----- |
| **Average scoring delta** | **2.8 percentage points** from expert scores |
| **Direction accuracy** | **12/12** — never scored a weak document high or strong document low |
| **Weakness identification** | **92%** match with real evaluator comments |
| **Perfect criterion-level matches** | 2 documents where every criterion matched exactly |

### BITF vs Raw Claude

| Method | Avg delta from expert | Weakness detection | Overclaiming |
| ------ | --------------------- | ------------------ | ------------ |
| **BITF subagent** | **2.8pp** | **92%** | Rare (pessimistic bias) |
| Raw Claude (no framework) | ~15pp | ~70% | Systematic (generous) |

Raw Claude scores writing quality. BITF scores substance against criteria — catches domain mismatches, missing evidence, factual errors, and calibrates to real scoring bands.

### Essay Scoring (ELLIPSE Corpus, 45 essays, 1.0–5.0 scale)

| Method | Pearson r | QWK | MAE |
| ------ | --------- | --- | --- |
| EDS-only (deterministic) | 0.442 | 0.258 | 1.08 |
| Raw Claude | 0.937 | — | 0.39 |
| **BITF subagent** | **0.955** | **0.902** | **0.32** |

QWK of 0.902 exceeds the 0.80 threshold for "reliable" inter-rater agreement. State-of-the-art fine-tuned AES systems score QWK 0.75–0.85.

### Prediction Credibility (5 UK policy targets with known outcomes)

| Method | Correct directional calls |
| ------ | ------------------------ |
| BITF subagent | 5/5 |
| Raw Claude | 5/5 |
| BITF rule-based | 1/5 |

Subagent prediction matches raw Claude performance (both 5/5). The value of this module is structured extraction (prediction types, timeframes, evidence mapping) + evidence-based verification + audit trail, not improved accuracy over the base model. The rule-based fallback was replaced after proving actively harmful.

---

## Architecture

```mermaid
graph TB
    subgraph Input
        DOC[Document PDF/TXT]
        INT[Evaluation Intent]
    end

    subgraph "Core Pipeline (10 stages)"
        ING[1. Ingest]
        EXT[2. Extract — hybrid rule + LLM]
        VAL[3. Validate — 8 core checks]
        CRI[4. Load Criteria]
        ALI[5. Align — 7 structural signals]
        SPA[6. Spawn Agent Panel]
        SNN[7. Evidence Scoring]
        DEB[8. Debate]
        MOD[9. Moderation]
        REP[10. Report]
    end

    subgraph "Optional Extensions"
        PHI[--philosophy]
        EPI[--epistemology]
        PRE[--predict]
        ORC[--orchestrate]
        DEP[--deep-validate]
    end

    subgraph "Knowledge Graph (Oxigraph)"
        DO[Document Ontology]
        CO[Criteria Ontology]
        AO[Agent Ontology]
    end

    subgraph Output
        MD[evaluation-report.md]
        TTL[evaluation.ttl]
        HTML[evaluation-graph.html]
        JSON[orchestration.json]
    end

    DOC --> ING
    INT --> ING
    ING --> EXT --> VAL --> CRI --> ALI --> SPA --> SNN --> DEB --> MOD --> REP
    ING --> DO
    CRI --> CO
    SPA --> AO
    DO & CO & AO --> ALI
    DO & CO & AO --> SNN
    REP --> MD & TTL & HTML
    ORC --> JSON
```

### Three Ontologies, One Graph

```mermaid
graph LR
    subgraph "Document Ontology"
        D[Document] --> S1[Section 1]
        D --> S2[Section 2]
        S1 --> C1[Claim]
        S1 --> E1[Evidence]
        S2 --> C2[Claim]
    end

    subgraph "Criteria Ontology"
        F[Framework] --> CR1[Criterion 1]
        F --> CR2[Criterion 2]
        CR1 --> R1[Rubric Level 4: 7-8]
        CR1 --> R2[Rubric Level 3: 5-6]
    end

    subgraph "Agent Ontology"
        A1[Subject Expert] --> N1[Need: Esteem]
        A1 --> T1[Trust → Writing Specialist: 0.7]
        A2[Writing Specialist] --> N2[Need: Safety]
    end

    S1 -.->|onto_align| CR1
    S2 -.->|onto_align| CR2
    A1 -.->|scores| CR1
    A2 -.->|scores| CR2
```

### Evidence Verification Layer

```mermaid
graph LR
    subgraph "Evidence from Graph"
        QD[Quantified Data — 0.8-1.0]
        VC[Verifiable Claim — 0.6-0.8]
        CI[Citation — 0.5-0.7]
        GC[General Claim — 0.3-0.5]
    end

    subgraph "Evidence Scorer Neuron"
        MP[Membrane Potential]
        TH[Threshold]
        FR[Firing Rate → Score]
    end

    subgraph "Blended Output"
        SS[Evidence Score]
        LS[LLM Score]
        FS[Final Score]
        HF[Hallucination Flag]
    end

    QD & VC & CI & GC -->|spikes| MP
    MP -->|exceeds| TH -->|fires| FR
    FR --> SS
    SS & LS --> FS
    SS -->|"Evidence low + LLM high"| HF
```

---

## What We Tried and What Didn't Work

Systematic ablation studies — toggle each component on/off, measure accuracy — identified which parts earn their complexity.

| Component | Result | Action |
| --------- | ------ | ------ |
| **Evidence scoring** | Essential — without it, Pearson drops to 0.000 | **Core** |
| **Ontology alignment** | Essential — without it, Pearson drops from 0.684 to 0.592 | **Core** |
| **Validation signals** | Hurts accuracy — removing them improves Pearson 0.684→0.786 | Capped at -0.05, inhibition reduced |
| **Hedging check** | Harmful — penalises correct academic hedging | Removed from core |
| **Specificity check** | Noisy — flags normal academic vocabulary | Removed from core |
| **Transition check** | High-school heuristic, no accuracy improvement | Removed from core |
| **Maslow dynamics** | Zero measurable impact on scores | Opt-in (`--epistemology`) |
| **Multi-round debate** | No impact in deterministic mode | Only active with LLM subagents |
| **Philosophy module** | Interesting, not useful for accuracy (316 lines, ~0 ROI) | Opt-in (`--philosophy`) |
| **Epistemology module** | Academic exercise, no accuracy improvement | Opt-in (`--epistemology`) |
| **Rule-based predictions** | Actively harmful — 3/11 found, duplicates, misparses | Replaced with subagent + evidence scorer |
| **Number checker (old)** | 111 false positives per document (years as "inconsistencies") | Fixed — filtered years/dates, down to 14 FPs |

**Key insight:** Evidence scoring and ontology alignment are the only two components that provably improve accuracy. Everything else either has zero impact or hurts. The 10-stage core pipeline reflects this.

---

## How It Works

### Core Pipeline (always runs)

1. **Ingest** — PDF/text → sections → Document Ontology (RDF triples in Oxigraph)
2. **Extract** — Hybrid rule + LLM claim/evidence extraction with confidence scores
3. **Validate** — 8 core deterministic checks (citations, consistency, structure, reading level, duplicates, evidence quality, referencing)
4. **Load Criteria** — 7 built-in frameworks + YAML/JSON custom rubrics
5. **Align** — Map sections ↔ criteria via 7 structural signals (AlignmentEngine)
6. **Spawn Agents** — Domain-specialist panel + moderator with cognitive model
7. **Evidence Score** — Evidence-grounded deterministic scoring (no evidence = score zero)
8. **Debate** — Disagreement detection, challenge/response, convergence
9. **Moderate** — Trust-weighted consensus with outlier detection
10. **Report** — Markdown + Turtle RDF + interactive graph HTML

### Optional Extensions (CLI flags)

| Flag | What it adds |
| ---- | ------------ |
| `--predict` | Extract predictions/targets from document, assess credibility against evidence |
| `--philosophy` | Kantian, utilitarian, virtue ethics analysis |
| `--epistemology` | Justified beliefs with empirical/normative/testimonial bases |
| `--deep-validate` | All 15 validation checks (adds hedging, transitions, specificity, fallacies, etc.) |
| `--orchestrate` | Generate Claude subagent task files for LLM-enhanced scoring |

---

## Evidence Scorer: How It Works

MiroFish agents can "justify" a 9/10 score for a criterion with no supporting evidence. This is hallucination with a confidence score attached. The evidence scorer makes this detectable.

### Biological inspiration

The scorer borrows four properties from [Spiking Neural Networks](https://en.wikipedia.org/wiki/Spiking_neural_network) (third-generation neural networks that model how real neurons communicate via discrete electrical pulses). We don't claim this is neuromorphic computing — it's an evidence density scorer that uses biologically-inspired dynamics because they provide useful properties for document evaluation.

### Property 1: Membrane potential + threshold = minimum evidence bar

Each agent has one neuron per evaluation criterion. Evidence from the knowledge graph generates input spikes:

| Evidence type | Spike strength | Example |
| ------------- | -------------- | ------- |
| Quantified data | 0.8–1.0 | "FTSE 100 rose 45%" |
| Verifiable claim | 0.6–0.8 | "Bank of England purchased £895bn in assets" |
| Citation | 0.5–0.7 | "(Bernanke, 2009)" |
| General claim | 0.3–0.5 | "QE was effective as a stabilisation tool" |
| Section alignment | 0.2–0.4 | Section title matches criterion |

Spikes accumulate in the membrane potential. When it exceeds the threshold, the neuron fires. **No evidence = no spikes = no firing = score of zero.** This is the anti-hallucination property.

### Property 2: Leaky integration = diminishing returns

```text
membrane_potential *= (1.0 - decay_rate)   // after each timestep
```

Real neurons leak charge over time. We use this to model **diminishing returns** — the 10th citation about the same topic adds less value than the 1st. Without decay, a document could game the score by repeating weak evidence 50 times.

### Property 3: Lateral inhibition = debate challenges

```text
When Agent A challenges Agent B's score:
  Agent B's neuron.apply_inhibition(challenge_strength)
  → reduces membrane potential
  → requires MORE evidence to maintain the same score
```

In real neural networks, nearby neurons inhibit each other to sharpen responses. We use this for debate: a challenged score needs stronger evidence to survive.

### Property 4: Refractory period = no double-counting

After firing, the neuron enters a refractory period where new spikes are ignored. This prevents the same piece of evidence from being counted multiple times in quick succession.

### The actual scoring formula

Strip away the biological framing and here's the math:

```text
evidence_saturation = ln(1 + total_spikes) / ln(16)     // log scale, saturates at ~15 items
spike_quality       = mean(spike_strengths)               // 0.0–1.0
firing_rate         = fire_count / timesteps              // traditional SNN signal

raw_score = evidence_saturation × 0.50                    // how much evidence exists
          + spike_quality       × 0.35                    // how strong is the evidence
          + firing_rate         × 0.15                    // how consistently did it accumulate

final = raw_score × (1.0 - inhibition) × max_score       // penalise if challenged in debate
```

**In plain English:** Score = 50% "how much evidence" + 35% "how strong" + 15% "how consistent." Then penalise if other agents challenged the score.

### Why not just count evidence?

A weighted sum gets you 80% of the way. The SNN-inspired properties add four things a simple counter can't:

1. **Temporal dynamics** — evidence arriving in bursts (all in one section) vs spread across the document produces different firing patterns
2. **Inhibition from debate** — a simple counter can't model "this score was challenged and needs more evidence to survive"
3. **Refractory period** — prevents the same evidence type from flooding the score (five citations from the same author don't each get full credit)
4. **Threshold-based firing** — creates a natural minimum evidence bar, cleaner than an arbitrary minimum score

### Hallucination detection

When the LLM and evidence scorer disagree, the system flags it:

```text
LLM says 9/10. Evidence scorer says 2/10 (only 2 weak spikes received).
→ hallucination_risk = true
→ "WARNING: LLM scored significantly higher than evidence supports."
```

The final score blends both: `final = eds × eds_weight + llm × llm_weight`. The scorer is deterministic: given the same evidence, it always produces the same score. When evidence is abundant, the scorer dominates. When sparse, the LLM fills in — but the hallucination flag is raised.

### ARIA Alignment

This implements the gatekeeper architecture from [ARIA's Safeguarded AI programme](https://www.aria.org.uk/programme-safeguarded-ai/) (Bengio, Russell, Tegmark): **don't make the LLM deterministic — make the verification deterministic.**

| ARIA framework | Brain in the Fish |
| -------------- | ----------------- |
| World model | OWL ontology (knowledge graph) |
| Safety specification | Rubric levels + evidence scorer thresholds |
| Deterministic verifier | Evidence scorer (same evidence → same score, always) |
| Proof certificate | Spike log + onto_lineage |

---

## Getting Started

### Prerequisites

- Rust 1.85+ (edition 2024)
- [open-ontologies](https://github.com/fabio-rovai/open-ontologies) cloned alongside this repo

```bash
git clone https://github.com/fabio-rovai/open-ontologies.git
git clone https://github.com/fabio-rovai/brain-in-the-fish.git
cd brain-in-the-fish
cargo build --release
```

### As MCP Server (recommended)

Add to Claude Code (`~/.claude.json`) or Claude Desktop:

```json
{
  "mcpServers": {
    "brain-in-the-fish": {
      "command": "/path/to/brain-in-the-fish-mcp",
      "args": []
    }
  }
}
```

Then ask Claude: *"Evaluate this policy document against Green Book standards"*

### As CLI

```bash
# Deterministic evaluation (no API key needed)
brain-in-the-fish evaluate document.pdf --intent "mark this essay" --open

# With custom criteria
brain-in-the-fish evaluate policy.pdf --intent "evaluate" --criteria rubric.yaml

# With all extensions
brain-in-the-fish evaluate report.pdf --intent "audit" --predict --deep-validate --orchestrate

# Benchmark against labeled dataset
brain-in-the-fish benchmark --dataset data/ellipse-sample.json --ablation
```

### Output

| File | Description |
| ---- | ----------- |
| `evaluation-report.md` | Scorecard, gap analysis, debate trail, recommendations |
| `evaluation.ttl` | Turtle RDF export for cross-evaluation analysis |
| `evaluation-graph.html` | Interactive hierarchical knowledge graph |
| `orchestration.json` | Subagent tasks for Claude-enhanced scoring |

---

## Workspace Structure

```mermaid
graph TB
    subgraph "Cargo Workspace"
        ROOT[Cargo.toml — workspace root]

        subgraph "crates/core"
            CORE[brain-in-the-fish-core — library]
            CORE --> TYPES[types]
            CORE --> INGEST[ingest]
            CORE --> EXTRACT[extract]
            CORE --> VALIDATE[validate]
            CORE --> CRITERIA[criteria]
            CORE --> ALIGNMENT[alignment]
            CORE --> AGENT[agent]
            CORE --> SNN_MOD[snn]
            CORE --> SCORING[scoring]
            CORE --> DEBATE[debate]
            CORE --> MODERATION[moderation]
            CORE --> REPORT[report]
            CORE --> PREDICT[predict]
            CORE --> VIS[visualize]
            CORE --> BENCH[benchmark]
        end

        subgraph "crates/cli"
            CLI[brain-in-the-fish — binary]
        end

        subgraph "crates/mcp"
            MCP[brain-in-the-fish-mcp — binary]
        end
    end

    subgraph "External"
        OO[open-ontologies — Oxigraph + OWL reasoning]
    end

    CLI --> CORE
    MCP --> CORE
    CORE --> OO
```

**~20K lines of Rust across 25 modules, compiled to 2 binaries (CLI + MCP server).**

---

## MCP Tools

| Tool | Description |
| ---- | ----------- |
| `eval_status` | Server status, session state, triple count |
| `eval_ingest` | Ingest document and build Document Ontology |
| `eval_criteria` | Load evaluation framework |
| `eval_align` | Run ontology alignment (sections ↔ criteria) |
| `eval_spawn` | Generate evaluator agent panel |
| `eval_score_prompt` | Get scoring prompt for one agent-criterion pair |
| `eval_record_score` | Record a score from a subagent |
| `eval_scoring_tasks` | Get all scoring tasks for orchestration |
| `eval_debate_status` | Disagreements, convergence, drift velocity |
| `eval_challenge_prompt` | Generate challenge prompt for debate |
| `eval_whatif` | Simulate text change, estimate score impact |
| `eval_predict` | Extract predictions with credibility assessment |
| `eval_report` | Generate final evaluation report |

---

## Built on open-ontologies

Brain in the Fish consumes [open-ontologies](https://github.com/fabio-rovai/open-ontologies) as a library crate. It uses:

| Component | Purpose |
| --------- | ------- |
| `GraphStore` | Triple storage + SPARQL queries |
| `Reasoner` | OWL-RL inference |
| `AlignmentEngine` | 7-signal ontology alignment |
| `StateDb` | Persistent state |
| `LineageLog` | Full audit trail |
| `DriftDetector` | Convergence monitoring |
| `Enforcer` | Quality gates |
| `TextEmbedder` | Semantic similarity (optional) |

All run as in-process Rust function calls. Zero network overhead.

---

## Testing

```bash
cargo test --workspace        # 260 tests across all crates
cargo clippy --workspace      # Zero warnings
cargo run --bin brain-in-the-fish -- benchmark  # Run synthetic benchmark
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Acknowledgments

- [MiroFish](https://github.com/666ghj/MiroFish) — multi-agent swarm prediction that inspired the agent debate architecture
- [AgentSociety](https://github.com/tsinghua-fib-lab/AgentSociety) — cognitive agent simulation that inspired the Maslow + TPB model
- [open-ontologies](https://github.com/fabio-rovai/open-ontologies) — OWL ontology engine providing the knowledge graph backbone
- [epistemic-deconstructor](https://github.com/NikolasMarkou/epistemic-deconstructor) — Bayesian tracking and falsification-first epistemology
- [ARIA Safeguarded AI](https://www.aria.org.uk/programme-safeguarded-ai/) — gatekeeper architecture validation

## License

MIT
