<p align="center">
  <img src="assets/logo.png" alt="Brain in the Fish" width="200" />
</p>

<h1 align="center">Brain in the Fish</h1>

<p align="center">
  <strong>A Rust-native universal document evaluation engine where cognitively-modelled AI agents evaluate any document for any purpose — with their entire mental state living inside an OWL ontology.</strong>
</p>

<p align="center">
  <img src="https://github.com/fabio-rovai/brain-in-the-fish/actions/workflows/ci.yml/badge.svg" alt="CI" />
  <img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT" />
  <img src="https://img.shields.io/badge/tests-104%20passing-brightgreen" alt="Tests" />
  <img src="https://img.shields.io/badge/rust-edition%202024-orange" alt="Rust" />
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README-CN.md">中文</a>
</p>

---

## The Problem

Two prior systems attempted multi-agent document evaluation. Both fell short.

**MiroFish** gave fish a swarm -- agents debate a document and converge on a prediction. But MiroFish agents are stateless LLM prompts. They have no memory between rounds, no structured cognition, and no formal link between what they read and what they score. Prediction from an LLM swarm is fundamentally hallucination-prone: agents invent plausible-sounding justifications without grounding them in the document's actual content.

**AgentSociety** gave agents a mind -- Maslow needs, Theory of Planned Behaviour, trust dynamics. But the cognitive model lives in Python dictionaries. It is opaque to reasoning, not queryable by SPARQL, not diffable between debate rounds, and not interoperable with any external knowledge system. The mind exists, but nobody can examine it.

Both systems share a deeper flaw: there is no structured, auditable mapping between a question asked and the evidence found. Scores appear, but the chain from document content to criterion to agent judgement is implicit and unreproducible.

## The Fix

Brain in the Fish gives the mind a skeleton -- a structured, queryable, diffable OWL ontology substrate that agents don't just use but exist within.

**Three ontologies, one graph.** Document, Criteria, and Agent ontologies all live as OWL triples in a shared Oxigraph store (via [open-ontologies](https://github.com/fabio-rovai/open-ontologies)). Every section, claim, criterion, rubric level, agent belief, Maslow need, and trust weight is a first-class RDF node.

**Evaluation over prediction.** MiroFish predicts what a score should be. Brain in the Fish evaluates what the document actually contains against explicit criteria. Evaluation is fundamentally more reliable than prediction because the evidence IS the document -- it is concrete, present, and verifiable. The system does not guess; it maps, scores, and justifies.

**Agent cognition IS the ontology.** Each evaluator agent's Maslow needs, trust relationships, and domain expertise are OWL individuals. When an agent's trust in a colleague changes after a debate challenge, that change is a triple update -- queryable, diffable, and auditable.

**Ontology alignment maps document to criteria.** `onto_align` produces a mathematical mapping between document sections and evaluation criteria. Gaps are identified before scoring begins. No criterion goes unanswered silently.

**Versioned debate.** Each debate round produces new score triples. `onto_diff` between rounds reveals exactly which agents moved, by how much, and why. Drift velocity measures convergence. The entire deliberation is reproducible from the graph state.

## Comparison

| Feature | MiroFish | AgentSociety | Brain in the Fish |
|---------|----------|--------------|-------------------|
| Agent cognition | Stateless LLM prompts | Maslow + TPB in Python dicts | Maslow + TPB as OWL individuals in Oxigraph |
| Evidence basis | LLM generates justifications | LLM generates justifications | Document content mapped to criteria via ontology alignment |
| Debate tracking | Round counter, text logs | Round counter, JSON state | Versioned RDF triples with `onto_diff` and drift velocity |
| Reproducibility | Non-deterministic | Non-deterministic | Deterministic graph state per round, SPARQL-queryable |
| Cross-evaluation learning | None | None | Turtle export enables cross-session analysis |
| Runtime dependencies | Python + multiple LLM APIs | Python + LLM APIs | Single Rust binary, Oxigraph embedded |
| Deploy complexity | Multi-service Python stack | Multi-service Python stack | `cargo build` produces one binary |

## How It Works

The evaluation pipeline runs in 9 stages:

1. **Ingest** -- Extract text from PDF (or plain text), split into sections by heading detection, build the Document Ontology as RDF triples in Oxigraph.

2. **Load Criteria** -- Select or generate an evaluation framework (academic marking rubric, tender ITT criteria, generic quality framework). Each criterion, rubric level, and weight becomes an OWL individual in the Criteria Ontology.

3. **Spawn Agent Panel** -- Detect the evaluation domain from the intent string and spawn 3-5 specialist agents plus a moderator. Each agent's cognitive model (Maslow needs, trust weights, domain expertise) is loaded as the Agent Ontology.

4. **Align** -- Map document sections to criteria using keyword overlap (and eventually semantic embedding via open-ontologies). Identify gaps where no document content addresses a criterion.

5. **Score (Round 1)** -- Each agent independently scores each criterion. Scoring prompts include the agent's persona, the criterion rubric, and the relevant document sections. Scores are recorded as RDF triples.

6. **Detect Disagreements** -- Find criterion-agent pairs where score deltas exceed a threshold. These become debate targets.

7. **Debate** -- Challenger agents construct evidence-based arguments against target scores. Targets defend or revise. Trust weights update based on persuasion outcomes. Each round produces new score triples.

8. **Moderate** -- When drift velocity drops below threshold (convergence), the moderator calculates trust-weighted consensus scores, identifies outlier dissents, and produces moderated results.

9. **Report** -- Generate a structured Markdown report with executive summary, scorecard table, gap analysis, full debate trail, improvement recommendations, and panel summary. Export the complete evaluation as Turtle RDF for cross-session analysis.

## Quick Start

```bash
cargo build

# Evaluate a document
brain-in-the-fish evaluate document.pdf --intent "mark this essay"

# With custom criteria and output directory
brain-in-the-fish evaluate proposal.pdf --intent "score this tender bid" --criteria rubric.yaml --output ./results

# Start the MCP server (stdio transport)
brain-in-the-fish serve
```

## Universal Evaluation

The system does not know what it is evaluating until you tell it. The same engine handles any document type by adapting its three ontologies to the domain.

| Use Case | Document Ontology | Criteria Ontology | Agent Panel |
|----------|-------------------|-------------------|-------------|
| Mark a student essay | Paragraphs, arguments, citations, thesis | Marking rubric, grade boundaries, learning outcomes | Subject expert, writing specialist, critical thinking assessor |
| Score a tender bid | Sections, claims, evidence, case studies | ITT criteria, weights, pass/fail thresholds | Procurement lead, domain expert, social value champion, finance assessor |
| Assess a policy | Objectives, measures, impact projections | Policy framework, impact criteria, stakeholder needs | Policy analyst, stakeholder representative, implementation expert |
| Analyse survey results | Response themes, methodology, demographics | Research questions, validity criteria | Statistician, research designer, ethics reviewer |
| Review a contract | Clauses, obligations, terms, definitions | Legal checklist, risk criteria, regulatory requirements | Legal reviewer, compliance officer, commercial analyst |

## Architecture

All modules compile into a single binary. No microservices, no Python, no network calls to the ontology engine.

| Module | Purpose | Lines |
|--------|---------|-------|
| `types` | Core evaluation domain types (Document, Criteria, Agent, Score, Session) | 289 |
| `ingest` | PDF text extraction, section splitting, Document Ontology RDF generation | 559 |
| `criteria` | Evaluation framework loading, Criteria Ontology RDF generation | 451 |
| `agent` | Agent cognitive model (Maslow + trust), Agent Ontology RDF, panel spawning | 779 |
| `scoring` | SPARQL queries, score recording, scoring prompt generation for subagents | 811 |
| `debate` | Disagreement detection, challenge prompts, drift velocity, convergence | 879 |
| `moderation` | Trust-weighted consensus, outlier detection, overall result calculation | 678 |
| `report` | Markdown report generation, Turtle RDF session export | 714 |
| `server` | MCP server with 10 eval_* tools (rmcp, stdio + HTTP transport) | 737 |
| `main` | CLI entry point (clap), evaluate and serve subcommands | 200 |
| `lib` | Module declarations | 9 |

**Total: ~6,100 lines of Rust.**

## MCP Tools

The MCP server exposes 10 tools for orchestrating evaluations programmatically:

| Tool | Description |
|------|-------------|
| `eval_status` | Server status, version, session state, triple count |
| `eval_ingest` | Ingest a PDF and build the Document Ontology |
| `eval_criteria` | Load an evaluation framework (generic, academic, tender) |
| `eval_align` | Run ontology alignment between document sections and criteria |
| `eval_spawn` | Generate an evaluator agent panel from the intent |
| `eval_score_prompt` | Generate a scoring prompt for a specific agent-criterion pair |
| `eval_record_score` | Record a score from an agent into the graph store |
| `eval_debate_status` | Disagreements, drift velocity, and convergence for the active round |
| `eval_challenge_prompt` | Generate a challenge prompt for one agent to challenge another |
| `eval_report` | Generate the full evaluation report with moderation and consensus |

## Built on open-ontologies

Brain in the Fish is not a fork of [open-ontologies](https://github.com/fabio-rovai/open-ontologies). It is a dependent crate that consumes open-ontologies as a library.

```toml
open-ontologies = { path = "../open-ontologies", features = ["embeddings"] }
```

It uses `GraphStore` for triple storage and SPARQL queries, `Reasoner` for inference, `Aligner` for ontology alignment, and `Embedder` for semantic similarity -- all as in-process Rust function calls. Zero network overhead. No serialisation boundaries. The ontology engine runs in the same address space as the evaluation logic.

## Testing

95 tests covering all modules: ingestion, criteria loading, agent spawning, scoring, debate mechanics, moderation, report generation, and MCP server tools.

```bash
cargo test
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

MIT
