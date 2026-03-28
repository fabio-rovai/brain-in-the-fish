<p align="center">
  <img src="assets/logo.png" alt="Brain in the Fish" width="200" />
</p>

<h1 align="center">Brain in the Fish</h1>

<p align="center">
  <strong>LLM evaluation where every score is grounded in evidence you can verify.</strong>
</p>

<p align="center">
  <img src="https://github.com/fabio-rovai/brain-in-the-fish/actions/workflows/ci.yml/badge.svg" alt="CI" />
  <img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT" />
  <img src="https://img.shields.io/badge/rust-edition%202024-orange" alt="Rust" />
</p>

---

## The Problem

Ask an LLM to score a document and it says "8/10 — strong methodology section." But:

- Did the LLM actually read the methodology, or did it skim and guess?
- Is "strong" based on evidence in the text, or on the LLM's training data?
- If you challenge the score, can the LLM show its working?

LLMs don't hallucinate scores — they hallucinate **reasoning**. They claim evidence exists when it doesn't, invent citations, and confuse fluent writing with substantive argument.

## The Solution

Brain in the Fish forces the LLM to decompose its reasoning into an OWL knowledge graph. Every claim becomes a typed node. Every piece of evidence becomes a triple. Every relationship (supports, counters, rebuts) becomes an edge. Every node carries an exact quote from the source document.

The ontology is the LLM's working memory — visible, queryable, and verifiable.

```
Document → LLM decomposes into OWL graph → Every node grounded in source text
                                                    ↓
                                           Score derived from graph
                                                    ↓
                                           Gate verifies consistency
```

---

## What "No Hallucination" Means

Hallucination doesn't mean "disagrees with a human expert." Two experts routinely disagree on essay scores. That's judgment, not error.

Hallucination means the LLM **fabricates evidence** — claims a citation exists when it doesn't, invents statistics, or attributes arguments the document never made.

The ontology makes this impossible:

1. Every node has `source_text` — an **exact quote** from the document
2. The quote is verified against the original text — it either exists or it doesn't
3. The score derives from nodes that are verified — no node, no score

**Tested on 200 blind-scored essays (ASAP Set 1), 1,271 argument nodes:**

| Metric | Result |
| ------ | ------ |
| Nodes with verified source quotes | 1,271 / 1,271 (100%) |
| Fabricated evidence | 0 |
| Invented citations | 0 |
| Misattributed claims | 0 |

Every single node traces to text that actually exists in the document. The LLM can disagree with an expert on what score the evidence deserves — but it cannot claim evidence that isn't there.

---

## See It Work

Run `brain-in-the-fish demo` to see three examples:

**Essay A** — Eloquent, says nothing:

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

Verdict: REJECTED — 4 claims, 0 evidence. Score has no evidentiary support.
```

**Essay B** — Three sentences, every word counts:

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

Verdict: CONFIRMED — structural 7.8, quality 10.0, combined 8.9/12 (±10%).
```

**Essay C** — Well-written, fabricated citations:

```
LLM subagent score: 7.0/12

Ontology mapping:
  arg:thesis [Thesis] score: 0.50  "Common claim, plausible"
    └─ source: "According to Smith & Johnson (2023), 78% of students who use computers..."
  arg:cite_1 [Citation] score: 0.25  "Generic author names, no DOI, unverifiable"
    └─ source: "According to Smith & Johnson (2023), 78% of students..."
  arg:cite_2 [Citation] score: 0.20  "Organisation may not exist, no URL or reference number"
    └─ source: "The National Technology Council confirmed these findings..."

  Nodes: 3 | Evidence: 2 | Claims: 1 | Connectivity: 100%

Verdict: FLAGGED — LLM scored 7.0 but evidence supports 5.8. Gap 10% exceeds ±7%.
```

The structure of essays B and C is identical (3 nodes, 2 evidence, 100% connected). The difference is evidence **quality** — the ontology captures both structure and quality, and the gate uses both signals.

---

## How It Works

### Layer 1: LLM Decomposes

The LLM reads the document and produces an OWL Turtle ontology:

```turtle
arg:thesis_1 a arg:Thesis ;
    arg:hasText "Voting should be compulsory." .

arg:ev_1 a arg:QuantifiedEvidence ;
    arg:hasText "Australia's mandatory voting, enacted in 1924,
                 consistently yields 90%+ turnout" .

arg:ev_1 arg:supports arg:thesis_1 .
```

Every node is typed (Thesis, Claim, Evidence, Citation, Counter, Rebuttal). Every node carries the exact source text. Every relationship is explicit.

The LLM can't say "good essay" without showing **what** is good and **where** in the document it found it.

### Layer 2: Ontology Verifies

The Turtle loads into [open-ontologies](https://github.com/fabio-rovai/open-ontologies) GraphStore. SPARQL queries extract structural metrics:

- How many argument nodes exist (density)
- What fraction are evidence vs bare claims (evidence ratio)
- How connected the argument structure is (connectivity)
- How deep the reasoning chains go (depth)
- Whether counter-arguments are addressed (sophistication)

These are **facts about the graph**, not LLM opinions. Same graph → same metrics, always.

### Layer 3: Gate Checks Consistency

The gate compares the LLM's holistic score against the structural evidence using a learned tolerance curve:

```
tolerance = gate_a × ln(nodes + 1) + gate_b
```

Two parameters, calibrated from data. Fewer nodes = tighter tolerance (less room for the LLM to overclaim). Low-quality evidence tightens it further.

- **CONFIRMED**: LLM score consistent with evidence → score is emitted with full proof
- **FLAGGED**: LLM score exceeds evidence → score emitted with warning + recommended adjustment
- **REJECTED**: No evidence at all → score withheld

---

## Benchmarks

### Grounding: 0% hallucination on 200 essays

200 essays from ASAP Set 1, scored blind (subagents never saw expert scores). 1,271 argument nodes extracted with source quotes. Every quote verified against the original essay text.

**Result: 0 fabricated nodes out of 1,271.** The ontology forces grounding.

### Accuracy: LLM holistic scoring

The LLM's holistic scores on the same 200 essays, compared to expert scores:

| Metric | Value |
| ------ | ----- |
| Pearson r with experts | 0.746 |
| MAE | 2.71 |
| Hallucination rate (>30% off) | 24.5% |

### Gate effectiveness

The gate splits essays into CONFIRMED (evidence supports the score) and FLAGGED/REJECTED (evidence doesn't match):

| Subset | N | LLM Pearson | Halluc rate |
| ------ | -- | ----------- | ----------- |
| All essays | 200 | 0.746 | 24.5% |
| CONFIRMED only | 107 | 0.659 | 16.8% |
| FLAGGED only | 78 | 0.783 | 35.9% |

The gate reduces hallucinations from 24.5% to 16.8% on confirmed essays — a 31% reduction. The FLAGGED essays have a 36% hallucination rate, confirming the gate identifies unreliable scores.

**Important caveat:** "Hallucination" here means the LLM score diverges >30% from expert — this is a scoring disagreement, not evidence fabrication. Evidence fabrication is 0% (see grounding test above).

---

## What Didn't Work

| Approach | Result | Lesson |
| -------- | ------ | ------ |
| Using the ontology to replace the LLM score | Pearson 0.56 max | Structure captures ~25% of essay quality; writing quality lives in the text |
| Regex evidence extraction | Found ~20% of what LLM finds | Can't parse natural language with rules |
| Hardcoded gate thresholds | Brittle | Replaced with learned curve (2 params, Nelder-Mead) |
| Adding more structural features | Made things worse | 14 features is the ceiling; more = overfitting on N=100 |
| Stacking models | Collapsed | N=100 is too small for ensemble methods |

**The insight:** The ontology's job isn't to score — it's to **decompose and verify**. The LLM scores. The ontology proves what's actually in the document. The gate checks consistency between the two.

---

## Quick Start

```bash
git clone https://github.com/fabio-rovai/open-ontologies.git
git clone https://github.com/fabio-rovai/brain-in-the-fish.git
cd brain-in-the-fish
cargo build --release

# See it work — 3 examples with verdicts
brain-in-the-fish demo

# Evaluate a document
brain-in-the-fish evaluate document.pdf --intent "mark this essay" --open

# As MCP server (Claude orchestrates)
brain-in-the-fish serve
```

### MCP Server Config

```json
{
  "mcpServers": {
    "brain-in-the-fish": {
      "command": "/path/to/brain-in-the-fish-mcp"
    }
  }
}
```

---

## Built With

- **[open-ontologies](https://github.com/fabio-rovai/open-ontologies)** — OWL knowledge graph engine (GraphStore, Reasoner, SPARQL, AlignmentEngine)
- **Rust** — deterministic scoring, structural analysis, gate logic
- **[ARIA Safeguarded AI](https://aria.org.uk/opportunity-spaces/mathematics-for-safe-ai/safeguarded-ai/)** — gatekeeper architecture: don't make the LLM deterministic, make the verification deterministic

## License

MIT
