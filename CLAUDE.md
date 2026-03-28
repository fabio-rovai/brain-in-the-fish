# Brain in the Fish

Score any document. Prove every claim. The LLM decomposes documents into OWL knowledge graphs, SPARQL rules mine facts from the graph, and a gate verifies the score is consistent with the evidence.

## Relationship to open-ontologies

This project depends on `open-ontologies` as a library (`../open-ontologies`). It uses the ontology engine (GraphStore, Reasoner, SPARQL) to load and query document argument graphs. It is NOT a fork — it consumes `open-ontologies` as a dependency.

## Module Overview

| Module           | Purpose                                                      |
|------------------|--------------------------------------------------------------|
| `types`          | Core evaluation domain types                                 |
| `ingest`         | PDF ingestion and document ontology generation                |
| `criteria`       | Evaluation criteria ontology loading/generation               |
| `agent`          | Agent cognitive model and spawning                            |
| `scoring`        | Independent scoring engine with ReACT loop                    |
| `debate`         | Multi-round structured debate orchestrator                    |
| `moderation`     | Consensus moderation and convergence detection                |
| `report`         | Evaluation report generation                                  |
| `gate`           | Verdict system — CONFIRMED/FLAGGED/REJECTED based on evidence |
| `rules`          | SPARQL rule mining over argument graphs (8 rules)             |
| `argument_graph` | OWL Turtle → typed argument graph with structural metrics     |
| `server`         | MCP server exposing eval_* and eds_* tools                    |
| `snn`            | **Deprecated** — legacy scoring, use `gate` module instead    |

## Pipeline

1. LLM reads document → produces OWL Turtle (typed nodes with source quotes)
2. Turtle loads into GraphStore → structural metrics extracted via SPARQL
3. SPARQL INSERT rules derive facts (StrongClaim, UnsupportedClaim, etc.)
4. Gate compares LLM holistic score against structural + quality evidence
5. Verdict: CONFIRMED / FLAGGED / REJECTED

## MCP Tools

| Tool | What it does |
|------|-------------|
| `eds_feed` | Push structured evidence into the scorer |
| `eds_score` | Get score, confidence, low-confidence criteria |
| `eds_challenge` | Apply lateral inhibition during debate |
| `eds_consensus` | Check if agents have converged |

## Build

```sh
cargo build
```

## CLI

```sh
brain-in-the-fish demo                                    # 3 examples with verdicts
brain-in-the-fish evaluate <document> --intent "..."      # evaluate a document
brain-in-the-fish serve                                   # MCP server for Claude
```
