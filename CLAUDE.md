# Brain in the Fish

Universal document evaluation engine. Evaluates any document — essays, policies, contracts, clinical reports, surveys, legislation, tender bids — against any criteria. Given a document and an evaluation intent, it spawns a panel of cognitively-modelled AI agents that independently score, debate, and reach consensus on quality, producing a structured evaluation report with full audit trail.

## Relationship to open-ontologies

This project depends on `open-ontologies` as a library (`../open-ontologies`). It uses the ontology engine to represent documents, evaluation criteria, and scoring rubrics as typed knowledge graphs. It is NOT a fork — it consumes `open-ontologies` as a dependency.

## Module Overview

| Module       | Purpose                                                           |
|--------------|-------------------------------------------------------------------|
| `types`      | Core evaluation domain types                                      |
| `ingest`     | PDF ingestion and document ontology generation                    |
| `criteria`   | Evaluation criteria ontology loading/generation                   |
| `agent`      | Agent cognitive model and spawning                                |
| `scoring`    | Independent scoring engine with ReACT loop                        |
| `debate`     | Multi-round structured debate orchestrator                        |
| `moderation` | Consensus moderation and convergence detection                    |
| `report`     | Evaluation report generation                                      |
| `snn`        | Evidence density scorer — deterministic evidence-grounded scoring |
| `server`     | MCP server exposing eval_* and eds_* tools                        |

## EDS Tools (Evidence Density Scorer)

Subagents use these MCP tools to work through the SNN instead of scoring with vibes:

| Tool | When to use |
|------|-------------|
| `eds_feed` | After extracting evidence — push structured evidence into the SNN for scoring |
| `eds_score` | After feeding evidence — get SNN score, confidence, low-confidence criteria |
| `eds_challenge` | During debate — apply lateral inhibition to target agent's SNN |
| `eds_consensus` | After scoring round — check if agents' SNN scores have converged |

### EDS Workflow

1. Read the scoring task prompt (from `eval_scoring_tasks`)
2. Extract structured evidence from the aligned document sections
3. Call `eds_feed` with typed evidence items (claim, evidence, quantified_data, citation)
4. Call `eds_score` to get the SNN-computed score and confidence
5. If low-confidence criteria are flagged, re-read the document and extract more evidence
6. Call `eds_feed` again with new evidence, then `eds_score` again
7. Record final score via `eval_record_score` (which also auto-feeds EDS)
8. During debate, use `eds_challenge` for lateral inhibition
9. Call `eds_consensus` to check convergence

The subagent thinks through the SNN — it extracts evidence, feeds it into the SNN, reads the SNN's assessment, and makes a judgment informed by both its qualitative reading and the quantitative evidence structure.

## Build

```sh
cargo build
```

## CLI

```sh
brain-in-the-fish evaluate <document> --intent "..." [--criteria <file>] [--output <dir>]
brain-in-the-fish serve [--host 127.0.0.1] [--port 8080]
```
