# Brain in the Fish

Universal document evaluation engine. Given any document and an evaluation intent, it spawns a panel of AI agents that independently score, debate, and reach consensus on quality — producing a structured evaluation report.

## Relationship to open-ontologies

This project depends on `open-ontologies` as a library (`../open-ontologies`). It uses the ontology engine to represent documents, evaluation criteria, and scoring rubrics as typed knowledge graphs. It is NOT a fork — it consumes `open-ontologies` as a dependency.

## Module Overview

| Module       | Purpose                                           |
|--------------|---------------------------------------------------|
| `types`      | Core evaluation domain types                      |
| `ingest`     | PDF ingestion and document ontology generation     |
| `criteria`   | Evaluation criteria ontology loading/generation    |
| `agent`      | Agent cognitive model and spawning                 |
| `scoring`    | Independent scoring engine with ReACT loop         |
| `debate`     | Multi-round structured debate orchestrator         |
| `moderation` | Consensus moderation and convergence detection     |
| `report`     | Evaluation report generation                       |
| `server`     | MCP server exposing eval_* tools                   |

## Build

```sh
cargo build
```

## CLI

```sh
brain-in-the-fish evaluate <document> --intent "..." [--criteria <file>] [--output <dir>]
brain-in-the-fish serve [--host 127.0.0.1] [--port 8080]
```
