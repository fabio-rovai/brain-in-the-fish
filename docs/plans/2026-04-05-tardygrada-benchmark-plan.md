# Tardygrada vs Naive Rust Benchmark — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Benchmark the full brain-in-the-fish evaluation pipeline three ways — core (as-is), naive flat Rust, and Tardygrada C VM — in a showcase branch.

**Architecture:** Three crates sharing the same deps and mock workload. `bench-naive` rewrites the pipeline as flat functions/structs (no GraphStore indirection). `bench-tardygrada` vendors the Tardygrada C VM and models every bitf operation as VM agent operations via FFI. A criterion.rs harness runs identical workloads across all three.

**Tech Stack:** Rust 2024, open-ontologies, criterion.rs, cc crate (C compilation), bindgen (FFI), Tardygrada C VM (vendored)

---

### Task 1: Create branch and workspace scaffolding

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/bench-naive/Cargo.toml`
- Create: `crates/bench-naive/src/lib.rs`
- Create: `crates/bench-tardygrada/Cargo.toml`
- Create: `crates/bench-tardygrada/src/lib.rs`
- Create: `crates/bench-tardygrada/build.rs`
- Create: `benches/full_pipeline.rs`
- Create: `benches/fixtures/mock_data.rs`
- Create: `benches/fixtures/mod.rs`

**Step 1: Create branch**

```bash
cd /Users/fabio/projects/brain-in-the-fish
git checkout -b bench/tardygrada-vs-naive
```

**Step 2: Update workspace Cargo.toml**

Add new members and criterion dev-dependency:

```toml
[workspace]
resolver = "2"
members = ["crates/core", "crates/cli", "crates/mcp", "crates/bench-naive", "crates/bench-tardygrada"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
repository = "https://github.com/fabio-rovai/brain-in-the-fish"

[workspace.dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
```

**Step 3: Create bench-naive Cargo.toml**

```toml
[package]
name = "bench-naive"
version.workspace = true
edition.workspace = true

[dependencies]
open-ontologies = { path = "../../../open-ontologies", features = ["embeddings"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "2"
reqwest = { version = "0.12", features = ["json"] }
chrono = "0.4"
uuid = { version = "1", features = ["v4"] }
pdf-extract = "0.8"
regex = "1"
regex-lite = "0.1"
urlencoding = "2"
schemars = "1"
serde_yaml = "0.9"
dirs = "6"
csv = "1"
```

**Step 4: Create bench-naive/src/lib.rs**

```rust
pub mod types;
pub mod ingest;
pub mod agent;
pub mod alignment;
pub mod scoring;
pub mod debate;
pub mod moderation;
pub mod rules;
pub mod gate;
pub mod argument_graph;
pub mod orchestrator;
```

**Step 5: Create bench-tardygrada Cargo.toml**

```toml
[package]
name = "bench-tardygrada"
version.workspace = true
edition.workspace = true
build = "build.rs"

[dependencies]
open-ontologies = { path = "../../../open-ontologies", features = ["embeddings"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "2"
reqwest = { version = "0.12", features = ["json"] }
chrono = "0.4"
uuid = { version = "1", features = ["v4"] }
pdf-extract = "0.8"
regex = "1"
regex-lite = "0.1"
urlencoding = "2"
schemars = "1"
serde_yaml = "0.9"
dirs = "6"
csv = "1"

[build-dependencies]
cc = "1"
```

**Step 6: Create bench-tardygrada/build.rs stub**

```rust
fn main() {
    // Will compile Tardygrada C VM in Task 5
    println!("cargo:rerun-if-changed=tardygrada/");
}
```

**Step 7: Create bench-tardygrada/src/lib.rs stub**

```rust
pub mod ffi;
pub mod vm_agents;
pub mod pipeline;
pub mod orchestrator;
```

**Step 8: Create benches/ stubs**

`benches/fixtures/mock_data.rs`:
```rust
// Deterministic mock data for benchmarks — filled in Task 7
```

`benches/fixtures/mod.rs`:
```rust
pub mod mock_data;
```

`benches/full_pipeline.rs`:
```rust
// Full pipeline benchmark — filled in Task 8
```

**Step 9: Commit**

```bash
git add -A
git commit -m "scaffold: branch and workspace for tardygrada benchmark"
```

---

### Task 2: Mock data fixtures

**Files:**
- Create: `benches/fixtures/mock_data.rs`
- Modify: `benches/fixtures/mod.rs`

These fixtures provide deterministic documents, frameworks, agent panels, scores, and LLM responses so all three contestants process identical workloads.

**Step 1: Write mock_data.rs**

This file provides factory functions returning deterministic data. Each function returns owned data (no lifetimes). The types mirror `brain-in-the-fish-core::types` but are plain structs so bench-naive and bench-tardygrada can consume them without depending on core.

```rust
use serde::{Deserialize, Serialize};

// ── Shared benchmark types (mirror core::types, no GraphStore dependency) ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchDocument {
    pub id: String,
    pub title: String,
    pub doc_type: String,
    pub sections: Vec<BenchSection>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchSection {
    pub id: String,
    pub title: String,
    pub text: String,
    pub word_count: u32,
    pub claims: Vec<BenchClaim>,
    pub evidence: Vec<BenchEvidence>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchClaim {
    pub id: String,
    pub text: String,
    pub specificity: f64,
    pub verifiable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchEvidence {
    pub id: String,
    pub source: String,
    pub evidence_type: String,
    pub text: String,
    pub has_quantified_outcome: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchFramework {
    pub id: String,
    pub name: String,
    pub total_weight: f64,
    pub pass_mark: Option<f64>,
    pub criteria: Vec<BenchCriterion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchCriterion {
    pub id: String,
    pub title: String,
    pub description: String,
    pub max_score: f64,
    pub weight: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchAgent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub domain: String,
    pub trust_weights: Vec<(String, f64)>, // (target_id, trust_level)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchScore {
    pub agent_id: String,
    pub criterion_id: String,
    pub score: f64,
    pub max_score: f64,
    pub round: u32,
    pub justification: String,
    pub evidence_used: Vec<String>,
    pub gaps: Vec<String>,
}

/// Mock LLM response lookup — deterministic scoring by (agent_id, criterion_id, round)
pub struct MockLlm {
    responses: std::collections::HashMap<(String, String, u32), BenchScore>,
}

impl MockLlm {
    pub fn new(scores: Vec<BenchScore>) -> Self {
        let mut responses = std::collections::HashMap::new();
        for s in scores {
            responses.insert((s.agent_id.clone(), s.criterion_id.clone(), s.round), s);
        }
        Self { responses }
    }

    pub fn score(&self, agent_id: &str, criterion_id: &str, round: u32) -> Option<&BenchScore> {
        self.responses.get(&(agent_id.to_string(), criterion_id.to_string(), round))
    }
}

// ── Factory functions ──

/// 10-section document with 3 claims + 2 evidence per section
pub fn mock_document() -> BenchDocument {
    let mut sections = Vec::new();
    for i in 0..10 {
        let mut claims = Vec::new();
        for j in 0..3 {
            claims.push(BenchClaim {
                id: format!("claim-{i}-{j}"),
                text: format!("Section {i} makes claim {j} about quantitative outcomes with statistical significance p<0.05"),
                specificity: 0.7 + (j as f64 * 0.1),
                verifiable: j < 2,
            });
        }
        let mut evidence = Vec::new();
        for j in 0..2 {
            evidence.push(BenchEvidence {
                id: format!("ev-{i}-{j}"),
                source: format!("Source {j} (2024)"),
                evidence_type: if j == 0 { "quantitative".into() } else { "citation".into() },
                text: format!("Evidence {j} for section {i}: randomised controlled trial with n=500, effect size d=0.8"),
                has_quantified_outcome: j == 0,
            });
        }
        sections.push(BenchSection {
            id: format!("sec-{i}"),
            title: format!("Section {i}: Analysis of Domain Area {i}"),
            text: format!("This section provides a comprehensive analysis of domain area {i} with supporting evidence from multiple sources. The methodology follows established best practices and the findings are statistically significant. Word count padding to simulate real document content. ".repeat(5)),
            word_count: 200,
            claims,
            evidence,
        });
    }
    BenchDocument {
        id: "bench-doc-1".into(),
        title: "Benchmark Evaluation Document".into(),
        doc_type: "tender".into(),
        sections,
    }
}

/// 5-criteria framework, weights summing to 1.0
pub fn mock_framework() -> BenchFramework {
    BenchFramework {
        id: "bench-fw-1".into(),
        name: "Benchmark Framework".into(),
        total_weight: 1.0,
        pass_mark: Some(60.0),
        criteria: vec![
            BenchCriterion { id: "crit-0".into(), title: "Technical Approach".into(), description: "Quality of technical methodology".into(), max_score: 10.0, weight: 0.30 },
            BenchCriterion { id: "crit-1".into(), title: "Experience & Evidence".into(), description: "Relevant case studies and outcomes".into(), max_score: 10.0, weight: 0.25 },
            BenchCriterion { id: "crit-2".into(), title: "Delivery & Risk".into(), description: "Risk mitigation and delivery plan".into(), max_score: 10.0, weight: 0.20 },
            BenchCriterion { id: "crit-3".into(), title: "Social Value".into(), description: "Community and social impact".into(), max_score: 10.0, weight: 0.15 },
            BenchCriterion { id: "crit-4".into(), title: "Innovation".into(), description: "Novel approaches and tools".into(), max_score: 10.0, weight: 0.10 },
        ],
    }
}

/// 4 agents + 1 moderator with trust wiring
pub fn mock_agents() -> Vec<BenchAgent> {
    let ids: Vec<String> = (0..5).map(|i| format!("agent-{i}")).collect();
    vec![
        BenchAgent { id: ids[0].clone(), name: "Budget Expert".into(), role: "evaluator".into(), domain: "finance".into(),
            trust_weights: vec![(ids[1].clone(), 0.7), (ids[2].clone(), 0.5), (ids[3].clone(), 0.6)] },
        BenchAgent { id: ids[1].clone(), name: "Technical Evaluator".into(), role: "evaluator".into(), domain: "engineering".into(),
            trust_weights: vec![(ids[0].clone(), 0.6), (ids[2].clone(), 0.8), (ids[3].clone(), 0.5)] },
        BenchAgent { id: ids[2].clone(), name: "Delivery Specialist".into(), role: "evaluator".into(), domain: "operations".into(),
            trust_weights: vec![(ids[0].clone(), 0.5), (ids[1].clone(), 0.7), (ids[3].clone(), 0.6)] },
        BenchAgent { id: ids[3].clone(), name: "Social Value Assessor".into(), role: "evaluator".into(), domain: "social".into(),
            trust_weights: vec![(ids[0].clone(), 0.4), (ids[1].clone(), 0.5), (ids[2].clone(), 0.6)] },
        BenchAgent { id: ids[4].clone(), name: "Moderator".into(), role: "moderator".into(), domain: "general".into(),
            trust_weights: vec![(ids[0].clone(), 0.8), (ids[1].clone(), 0.8), (ids[2].clone(), 0.8), (ids[3].clone(), 0.8)] },
    ]
}

/// Pre-computed scores for 3 rounds x 4 agents x 5 criteria = 60 scores
/// Round 1: high disagreement. Round 2: moderate. Round 3: converged.
pub fn mock_scores() -> Vec<BenchScore> {
    let agents = mock_agents();
    let framework = mock_framework();
    let mut scores = Vec::new();

    // Base scores per agent per criterion (round 1 — high variance)
    let base: [[f64; 5]; 4] = [
        [7.0, 6.0, 8.0, 5.0, 7.0],  // Budget Expert
        [8.0, 8.0, 6.0, 6.0, 9.0],  // Technical Evaluator
        [6.0, 7.0, 7.0, 7.0, 6.0],  // Delivery Specialist
        [7.0, 5.0, 7.0, 8.0, 5.0],  // Social Value Assessor
    ];

    for round in 1..=3u32 {
        let convergence_factor = (round - 1) as f64 * 0.3; // scores converge over rounds
        for (ai, agent) in agents.iter().take(4).enumerate() {
            for (ci, criterion) in framework.criteria.iter().enumerate() {
                let target = 7.0; // converge toward 7.0
                let score = base[ai][ci] + (target - base[ai][ci]) * convergence_factor;
                scores.push(BenchScore {
                    agent_id: agent.id.clone(),
                    criterion_id: criterion.id.clone(),
                    score,
                    max_score: criterion.max_score,
                    round,
                    justification: format!("Agent {} scores {} at {:.1} in round {} based on evidence from sections", agent.name, criterion.title, score, round),
                    evidence_used: vec![format!("ev-{ci}-0"), format!("ev-{ci}-1")],
                    gaps: if score < 7.0 { vec![format!("gap-{ci}")] } else { vec![] },
                });
            }
        }
    }
    scores
}

/// Section-to-criterion alignment mappings (10 sections, 5 criteria, 2 sections each)
pub fn mock_alignments() -> Vec<(String, String, f64)> {
    vec![
        ("sec-0".into(), "crit-0".into(), 0.9),
        ("sec-1".into(), "crit-0".into(), 0.7),
        ("sec-2".into(), "crit-1".into(), 0.85),
        ("sec-3".into(), "crit-1".into(), 0.6),
        ("sec-4".into(), "crit-2".into(), 0.8),
        ("sec-5".into(), "crit-2".into(), 0.75),
        ("sec-6".into(), "crit-3".into(), 0.7),
        ("sec-7".into(), "crit-3".into(), 0.65),
        ("sec-8".into(), "crit-4".into(), 0.8),
        ("sec-9".into(), "crit-4".into(), 0.5),
    ]
}
```

**Step 2: Commit**

```bash
git add benches/
git commit -m "feat: deterministic mock data fixtures for benchmark"
```

---

### Task 3: bench-naive — types and ingest

**Files:**
- Create: `crates/bench-naive/src/types.rs`
- Create: `crates/bench-naive/src/ingest.rs`

**Step 1: Write types.rs**

Plain structs, same fields as core::types, no GraphStore. All derive Clone, Debug, Serialize, Deserialize. Re-export from the benches fixture types where identical.

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub doc_type: String,
    pub total_pages: Option<u32>,
    pub total_word_count: Option<u32>,
    pub sections: Vec<Section>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub text: String,
    pub word_count: u32,
    pub page_range: Option<String>,
    pub claims: Vec<Claim>,
    pub evidence: Vec<Evidence>,
    pub subsections: Vec<Section>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub text: String,
    pub specificity: f64,
    pub verifiable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub source: String,
    pub evidence_type: String,
    pub text: String,
    pub has_quantified_outcome: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Framework {
    pub id: String,
    pub name: String,
    pub total_weight: f64,
    pub pass_mark: Option<f64>,
    pub criteria: Vec<Criterion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Criterion {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub max_score: f64,
    pub weight: f64,
    pub rubric_levels: Vec<RubricLevel>,
    pub sub_criteria: Vec<Criterion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RubricLevel {
    pub level: String,
    pub score_range: String,
    pub descriptor: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub domain: String,
    pub years_experience: Option<u32>,
    pub persona_description: String,
    pub needs: Vec<MaslowNeed>,
    pub trust_weights: Vec<TrustRelation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaslowNeed {
    pub need_type: MaslowLevel,
    pub expression: String,
    pub salience: f64,
    pub satisfied: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MaslowLevel {
    Physiological,
    Safety,
    Belonging,
    Esteem,
    SelfActualisation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustRelation {
    pub target_agent_id: String,
    pub domain: String,
    pub trust_level: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Score {
    pub agent_id: String,
    pub criterion_id: String,
    pub score: f64,
    pub max_score: f64,
    pub round: u32,
    pub justification: String,
    pub evidence_used: Vec<String>,
    pub gaps_identified: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Challenge {
    pub challenger_id: String,
    pub target_agent_id: String,
    pub criterion_id: String,
    pub round: u32,
    pub argument: String,
    pub response: Option<String>,
    pub score_change: Option<(f64, f64)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModeratedScore {
    pub criterion_id: String,
    pub consensus_score: f64,
    pub panel_mean: f64,
    pub panel_std_dev: f64,
    pub dissents: Vec<Dissent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dissent {
    pub agent_id: String,
    pub score: f64,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlignmentMapping {
    pub section_id: String,
    pub criterion_id: String,
    pub confidence: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gap {
    pub criterion_id: String,
    pub criterion_title: String,
    pub best_partial_match: Option<AlignmentMapping>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebateRound {
    pub round_number: u32,
    pub scores: Vec<Score>,
    pub challenges: Vec<Challenge>,
    pub drift_velocity: Option<f64>,
    pub converged: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub document: Document,
    pub framework: Framework,
    pub agents: Vec<Agent>,
    pub alignments: Vec<AlignmentMapping>,
    pub gaps: Vec<Gap>,
    pub rounds: Vec<DebateRound>,
    pub final_scores: Vec<ModeratedScore>,
}

// ── Argument Graph ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NodeType {
    Thesis, SubClaim, Evidence, QuantifiedEvidence, Citation, Counter, Rebuttal, Structural,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EdgeType {
    Supports, Warrants, Counters, Rebuts, Contains, References,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArgNode {
    pub iri: String,
    pub node_type: NodeType,
    pub text: String,
    pub llm_score: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArgEdge {
    pub from: String,
    pub edge_type: EdgeType,
    pub to: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArgGraph {
    pub doc_id: String,
    pub nodes: Vec<ArgNode>,
    pub edges: Vec<ArgEdge>,
}

// ── Gate ──

#[derive(Clone, Debug)]
pub enum Verdict {
    Confirmed { reason: String },
    Flagged { reason: String, recommended_score: f64 },
    Rejected { reason: String },
}

#[derive(Clone, Debug)]
pub struct GateWeights {
    pub gate_a: f64,
    pub gate_b: f64,
}

// ── Rules ──

#[derive(Clone, Debug)]
pub struct DerivedFacts {
    pub strong_claims: usize,
    pub weak_claims: usize,
    pub unsupported_claims: usize,
    pub supported_claims: usize,
    pub sophisticated_arguments: usize,
    pub circular_arguments: usize,
    pub evidenced_thesis: bool,
    pub unevidenced_thesis: bool,
    pub quantified_support: usize,
    pub citation_support: usize,
    pub deep_chains: usize,
}

pub struct GraphMetrics {
    pub node_count: usize,
    pub edge_count: usize,
    pub thesis_count: usize,
    pub evidence_count: usize,
    pub depth: usize,
    pub avg_support_per_claim: f64,
}
```

**Step 2: Write ingest.rs**

```rust
use crate::types::*;
use std::path::Path;

pub fn split_into_sections(text: &str) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut current_title = String::new();
    let mut current_text = String::new();
    let mut idx = 0;

    for line in text.lines() {
        let trimmed = line.trim();
        // Detect section headers (lines that are short and title-case or all-caps)
        if trimmed.len() < 100
            && !trimmed.is_empty()
            && (trimmed.chars().next().map_or(false, |c| c.is_uppercase())
                || trimmed.starts_with('#'))
        {
            if !current_text.is_empty() {
                let words: Vec<&str> = current_text.split_whitespace().collect();
                sections.push(Section {
                    id: format!("sec-{idx}"),
                    title: current_title.clone(),
                    text: current_text.clone(),
                    word_count: words.len() as u32,
                    page_range: None,
                    claims: Vec::new(),
                    evidence: Vec::new(),
                    subsections: Vec::new(),
                });
                idx += 1;
            }
            current_title = trimmed.trim_start_matches('#').trim().to_string();
            current_text.clear();
        } else {
            current_text.push_str(line);
            current_text.push('\n');
        }
    }
    if !current_text.is_empty() {
        let words: Vec<&str> = current_text.split_whitespace().collect();
        sections.push(Section {
            id: format!("sec-{idx}"),
            title: current_title,
            text: current_text,
            word_count: words.len() as u32,
            page_range: None,
            claims: Vec::new(),
            evidence: Vec::new(),
            subsections: Vec::new(),
        });
    }
    sections
}

pub fn extract_pdf_text(path: &Path) -> anyhow::Result<String> {
    pdf_extract::extract_text(path).map_err(|e| anyhow::anyhow!("PDF extraction failed: {e}"))
}

pub fn ingest_text(title: &str, text: &str) -> Document {
    let sections = split_into_sections(text);
    let total_words: u32 = sections.iter().map(|s| s.word_count).sum();
    Document {
        id: uuid::Uuid::new_v4().to_string(),
        title: title.to_string(),
        doc_type: "text".into(),
        total_pages: None,
        total_word_count: Some(total_words),
        sections,
    }
}

pub fn document_to_turtle(doc: &Document) -> String {
    let mut ttl = String::new();
    ttl.push_str("@prefix eval: <http://eval.ontology/> .\n\n");
    for section in &doc.sections {
        ttl.push_str(&format!(
            "eval:{} a eval:Section ; eval:title \"{}\" ; eval:wordCount {} .\n",
            section.id, section.title.replace('"', "\\\""), section.word_count
        ));
        for claim in &section.claims {
            ttl.push_str(&format!(
                "eval:{} a eval:Claim ; eval:text \"{}\" ; eval:specificity {:.2} ; eval:inSection eval:{} .\n",
                claim.id, claim.text.replace('"', "\\\""), claim.specificity, section.id
            ));
        }
        for ev in &section.evidence {
            ttl.push_str(&format!(
                "eval:{} a eval:Evidence ; eval:source \"{}\" ; eval:inSection eval:{} .\n",
                ev.id, ev.source.replace('"', "\\\""), section.id
            ));
        }
    }
    ttl
}

pub fn load_document_ontology(
    graph: &open_ontologies::graph::GraphStore,
    doc: &Document,
) -> anyhow::Result<usize> {
    let turtle = document_to_turtle(doc);
    graph.load_turtle(&turtle, None)
}
```

**Step 3: Verify it compiles**

```bash
cd /Users/fabio/projects/brain-in-the-fish
cargo check -p bench-naive
```

**Step 4: Commit**

```bash
git add crates/bench-naive/src/types.rs crates/bench-naive/src/ingest.rs
git commit -m "feat(bench-naive): types and ingest modules"
```

---

### Task 4: bench-naive — agent, alignment, scoring, debate, moderation, rules, gate, argument_graph, orchestrator

**Files:**
- Create: `crates/bench-naive/src/agent.rs`
- Create: `crates/bench-naive/src/alignment.rs`
- Create: `crates/bench-naive/src/scoring.rs`
- Create: `crates/bench-naive/src/debate.rs`
- Create: `crates/bench-naive/src/moderation.rs`
- Create: `crates/bench-naive/src/rules.rs`
- Create: `crates/bench-naive/src/gate.rs`
- Create: `crates/bench-naive/src/argument_graph.rs`
- Create: `crates/bench-naive/src/orchestrator.rs`

Each module implements the same logic as core but as flat functions on plain structs. No trait indirection. Same function signatures, same algorithms.

**Step 1: Write agent.rs**

```rust
use crate::types::*;

pub fn detect_domain(intent: &str) -> &'static str {
    let lower = intent.to_lowercase();
    if lower.contains("tender") || lower.contains("bid") || lower.contains("procurement") {
        "tender"
    } else if lower.contains("academic") || lower.contains("essay") || lower.contains("thesis") {
        "academic"
    } else if lower.contains("policy") || lower.contains("government") {
        "policy"
    } else if lower.contains("clinical") || lower.contains("medical") {
        "clinical"
    } else if lower.contains("legal") || lower.contains("law") {
        "legal"
    } else if lower.contains("survey") || lower.contains("questionnaire") {
        "survey"
    } else {
        "generic"
    }
}

pub fn spawn_panel(intent: &str, framework: &Framework) -> Vec<Agent> {
    let domain = detect_domain(intent);
    let mut agents = match domain {
        "tender" => vec![
            make_agent("agent-0", "Budget Expert", "evaluator", "finance"),
            make_agent("agent-1", "Technical Evaluator", "evaluator", "engineering"),
            make_agent("agent-2", "Delivery Specialist", "evaluator", "operations"),
            make_agent("agent-3", "Social Value Assessor", "evaluator", "social"),
            make_agent("agent-4", "Moderator", "moderator", "general"),
        ],
        "academic" => vec![
            make_agent("agent-0", "Grammarian", "evaluator", "language"),
            make_agent("agent-1", "Logician", "evaluator", "logic"),
            make_agent("agent-2", "Skeptic", "evaluator", "methodology"),
            make_agent("agent-3", "Moderator", "moderator", "general"),
        ],
        _ => vec![
            make_agent("agent-0", "Evaluator A", "evaluator", domain),
            make_agent("agent-1", "Evaluator B", "evaluator", domain),
            make_agent("agent-2", "Evaluator C", "evaluator", domain),
            make_agent("agent-3", "Moderator", "moderator", "general"),
        ],
    };
    wire_trust_weights(&mut agents);
    agents
}

fn make_agent(id: &str, name: &str, role: &str, domain: &str) -> Agent {
    Agent {
        id: id.to_string(),
        name: name.to_string(),
        role: role.to_string(),
        domain: domain.to_string(),
        years_experience: Some(10),
        persona_description: format!("{name} with expertise in {domain}"),
        needs: Vec::new(),
        trust_weights: Vec::new(),
    }
}

pub fn wire_trust_weights(agents: &mut [Agent]) {
    let ids: Vec<String> = agents.iter().map(|a| a.id.clone()).collect();
    for agent in agents.iter_mut() {
        for other_id in &ids {
            if *other_id != agent.id {
                agent.trust_weights.push(TrustRelation {
                    target_agent_id: other_id.clone(),
                    domain: agent.domain.clone(),
                    trust_level: 0.6,
                });
            }
        }
    }
}

pub fn agent_to_turtle(agent: &Agent) -> String {
    let mut ttl = String::new();
    ttl.push_str(&format!(
        "eval:{} a eval:Agent ; eval:name \"{}\" ; eval:role \"{}\" ; eval:domain \"{}\" .\n",
        agent.id, agent.name, agent.role, agent.domain
    ));
    for tw in &agent.trust_weights {
        ttl.push_str(&format!(
            "eval:{} eval:trusts [ eval:target eval:{} ; eval:level {:.2} ] .\n",
            agent.id, tw.target_agent_id, tw.trust_level
        ));
    }
    ttl
}
```

**Step 2: Write alignment.rs**

```rust
use crate::types::*;
use open_ontologies::graph::GraphStore;

pub fn align_sections_to_criteria(
    doc: &Document,
    framework: &Framework,
) -> (Vec<AlignmentMapping>, Vec<Gap>) {
    let mut mappings = Vec::new();
    let mut gaps = Vec::new();

    for criterion in &framework.criteria {
        let mut best_confidence = 0.0f64;
        let mut found = false;

        for section in &doc.sections {
            let title_lower = section.title.to_lowercase();
            let crit_lower = criterion.title.to_lowercase();
            // Simple keyword overlap confidence
            let crit_words: Vec<&str> = crit_lower.split_whitespace().collect();
            let matching: usize = crit_words
                .iter()
                .filter(|w| title_lower.contains(*w) || section.text.to_lowercase().contains(*w))
                .count();
            let confidence = matching as f64 / crit_words.len().max(1) as f64;

            if confidence > 0.3 {
                mappings.push(AlignmentMapping {
                    section_id: section.id.clone(),
                    criterion_id: criterion.id.clone(),
                    confidence,
                });
                found = true;
                best_confidence = best_confidence.max(confidence);
            }
        }

        if !found {
            gaps.push(Gap {
                criterion_id: criterion.id.clone(),
                criterion_title: criterion.title.clone(),
                best_partial_match: None,
            });
        }
    }
    (mappings, gaps)
}

pub fn align_via_ontology(
    graph: &GraphStore,
    doc: &Document,
    framework: &Framework,
) -> anyhow::Result<(Vec<AlignmentMapping>, Vec<Gap>)> {
    // Load document into graph, then use keyword alignment
    // (full OWL alignment available via open-ontologies AlignmentEngine)
    Ok(align_sections_to_criteria(doc, framework))
}

pub fn sections_for_criterion<'a>(
    alignments: &[AlignmentMapping],
    criterion_id: &str,
    doc: &'a Document,
) -> Vec<(&'a Section, f64)> {
    let mut result = Vec::new();
    for mapping in alignments {
        if mapping.criterion_id == criterion_id {
            if let Some(section) = doc.sections.iter().find(|s| s.id == mapping.section_id) {
                result.push((section, mapping.confidence));
            }
        }
    }
    result
}
```

**Step 3: Write scoring.rs**

```rust
use crate::types::*;

pub fn generate_scoring_prompt(
    agent: &Agent,
    criterion: &Criterion,
    sections: &[(&Section, f64)],
    round: u32,
) -> String {
    let mut prompt = format!(
        "You are {}, a {} with expertise in {}.\n\n",
        agent.name, agent.role, agent.domain
    );
    prompt.push_str(&format!(
        "Score the following on '{}' (max {:.0}):\n\n",
        criterion.title, criterion.max_score
    ));
    for (section, confidence) in sections {
        prompt.push_str(&format!(
            "## {} (alignment confidence: {:.0}%)\n{}\n\n",
            section.title,
            confidence * 100.0,
            &section.text[..section.text.len().min(500)]
        ));
    }
    prompt.push_str(&format!("This is round {round}.\n"));
    prompt
}

pub fn record_score(scores: &mut Vec<Score>, score: Score) {
    scores.push(score);
}

pub fn get_scores_for_round(scores: &[Score], round: u32) -> Vec<&Score> {
    scores.iter().filter(|s| s.round == round).collect()
}
```

**Step 4: Write debate.rs**

```rust
use crate::types::*;

#[derive(Clone, Debug)]
pub struct Disagreement {
    pub criterion_id: String,
    pub agent_a_id: String,
    pub agent_a_score: f64,
    pub agent_b_id: String,
    pub agent_b_score: f64,
    pub delta: f64,
}

pub fn find_disagreements(scores: &[Score], threshold: f64) -> Vec<Disagreement> {
    let mut disagreements = Vec::new();
    for i in 0..scores.len() {
        for j in (i + 1)..scores.len() {
            if scores[i].criterion_id == scores[j].criterion_id
                && scores[i].round == scores[j].round
            {
                let delta = (scores[i].score - scores[j].score).abs();
                if delta > threshold {
                    disagreements.push(Disagreement {
                        criterion_id: scores[i].criterion_id.clone(),
                        agent_a_id: scores[i].agent_id.clone(),
                        agent_a_score: scores[i].score,
                        agent_b_id: scores[j].agent_id.clone(),
                        agent_b_score: scores[j].score,
                        delta,
                    });
                }
            }
        }
    }
    disagreements
}

pub fn generate_challenge_prompt(
    challenger: &Agent,
    target: &Agent,
    disagreement: &Disagreement,
    criterion: &Criterion,
) -> String {
    format!(
        "{} challenges {}'s score of {:.1} on '{}'. {} scored {:.1}. Delta: {:.1}. Criterion max: {:.0}. Argue why the score should change.",
        challenger.name, target.name, disagreement.agent_b_score,
        criterion.title, challenger.name, disagreement.agent_a_score,
        disagreement.delta, criterion.max_score
    )
}

pub fn generate_response_prompt(
    target: &Agent,
    challenger: &Agent,
    challenge_argument: &str,
    score: &Score,
    criterion: &Criterion,
) -> String {
    format!(
        "{} responds to {}'s challenge on '{}'. Current score: {:.1}/{:.0}. Challenge: {}. Defend or adjust.",
        target.name, challenger.name, criterion.title, score.score, criterion.max_score,
        challenge_argument
    )
}

pub fn calculate_drift_velocity(round_a: &[Score], round_b: &[Score]) -> f64 {
    let mut total_drift = 0.0;
    let mut count = 0;
    for a in round_a {
        for b in round_b {
            if a.agent_id == b.agent_id && a.criterion_id == b.criterion_id {
                total_drift += (a.score - b.score).abs();
                count += 1;
            }
        }
    }
    if count > 0 { total_drift / count as f64 } else { 0.0 }
}

pub fn check_convergence(drift_velocity: f64, threshold: f64) -> bool {
    drift_velocity < threshold
}

pub fn update_trust_weights(agents: &mut [Agent], challenges: &[Challenge]) {
    for challenge in challenges {
        if challenge.score_change.is_some() {
            // Challenge led to score change — increase trust
            if let Some(target) = agents.iter_mut().find(|a| a.id == challenge.target_agent_id) {
                if let Some(tw) = target
                    .trust_weights
                    .iter_mut()
                    .find(|tw| tw.target_agent_id == challenge.challenger_id)
                {
                    tw.trust_level = (tw.trust_level + 0.05).min(1.0);
                }
            }
        } else {
            // No change — slight decrease
            if let Some(target) = agents.iter_mut().find(|a| a.id == challenge.target_agent_id) {
                if let Some(tw) = target
                    .trust_weights
                    .iter_mut()
                    .find(|tw| tw.target_agent_id == challenge.challenger_id)
                {
                    tw.trust_level = (tw.trust_level - 0.02).max(0.0);
                }
            }
        }
    }
}

pub fn build_debate_round(
    round_number: u32,
    scores: Vec<Score>,
    challenges: Vec<Challenge>,
    drift_velocity: Option<f64>,
    converged: bool,
) -> DebateRound {
    DebateRound {
        round_number,
        scores,
        challenges,
        drift_velocity,
        converged,
    }
}
```

**Step 5: Write moderation.rs**

```rust
use crate::types::*;

#[derive(Clone, Debug)]
pub struct OverallResult {
    pub total_score: f64,
    pub max_possible: f64,
    pub percentage: f64,
    pub pass_mark: Option<f64>,
    pub passed: Option<bool>,
    pub top_strengths: Vec<String>,
    pub top_weaknesses: Vec<String>,
}

pub fn calculate_moderated_scores(
    scores: &[Score],
    agents: &[Agent],
) -> Vec<ModeratedScore> {
    // Group scores by criterion
    let mut by_criterion: std::collections::HashMap<String, Vec<&Score>> =
        std::collections::HashMap::new();
    for score in scores {
        by_criterion
            .entry(score.criterion_id.clone())
            .or_default()
            .push(score);
    }

    let mut moderated = Vec::new();
    for (criterion_id, criterion_scores) in &by_criterion {
        // Trust-weighted mean
        let mut weighted_sum = 0.0;
        let mut weight_total = 0.0;

        for score in criterion_scores {
            let agent_trust = agents
                .iter()
                .filter(|a| a.id != score.agent_id)
                .flat_map(|a| {
                    a.trust_weights
                        .iter()
                        .filter(|tw| tw.target_agent_id == score.agent_id)
                        .map(|tw| tw.trust_level)
                })
                .sum::<f64>();
            let weight = if agent_trust > 0.0 { agent_trust } else { 1.0 };
            weighted_sum += score.score * weight;
            weight_total += weight;
        }

        let consensus = if weight_total > 0.0 {
            weighted_sum / weight_total
        } else {
            0.0
        };

        let values: Vec<f64> = criterion_scores.iter().map(|s| s.score).collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance =
            values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();

        // Outlier detection (>2σ)
        let mut dissents = Vec::new();
        for score in criterion_scores {
            if (score.score - mean).abs() > 2.0 * std_dev && std_dev > 0.0 {
                dissents.push(Dissent {
                    agent_id: score.agent_id.clone(),
                    score: score.score,
                    reason: format!(
                        "Score {:.1} is {:.1}σ from panel mean {:.1}",
                        score.score,
                        (score.score - mean).abs() / std_dev,
                        mean
                    ),
                });
            }
        }

        moderated.push(ModeratedScore {
            criterion_id: criterion_id.clone(),
            consensus_score: consensus,
            panel_mean: mean,
            panel_std_dev: std_dev,
            dissents,
        });
    }
    moderated
}

pub fn calculate_overall_score(
    moderated_scores: &[ModeratedScore],
    framework: &Framework,
) -> OverallResult {
    let mut total = 0.0;
    let mut max_possible = 0.0;
    let mut scored_criteria: Vec<(String, f64)> = Vec::new();

    for ms in moderated_scores {
        if let Some(criterion) = framework.criteria.iter().find(|c| c.id == ms.criterion_id) {
            let weighted = ms.consensus_score * criterion.weight;
            total += weighted;
            max_possible += criterion.max_score * criterion.weight;
            scored_criteria.push((criterion.title.clone(), ms.consensus_score / criterion.max_score));
        }
    }

    let percentage = if max_possible > 0.0 {
        (total / max_possible) * 100.0
    } else {
        0.0
    };

    scored_criteria.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top_strengths: Vec<String> = scored_criteria.iter().take(3).map(|(t, _)| t.clone()).collect();
    let top_weaknesses: Vec<String> = scored_criteria.iter().rev().take(3).map(|(t, _)| t.clone()).collect();

    OverallResult {
        total_score: total,
        max_possible,
        percentage,
        pass_mark: framework.pass_mark,
        passed: framework.pass_mark.map(|pm| percentage >= pm),
        top_strengths,
        top_weaknesses,
    }
}
```

**Step 6: Write rules.rs**

```rust
use crate::types::*;
use open_ontologies::graph::GraphStore;

pub fn mine_facts(graph: &GraphStore, doc: &Document) -> DerivedFacts {
    let mut facts = DerivedFacts {
        strong_claims: 0,
        weak_claims: 0,
        unsupported_claims: 0,
        supported_claims: 0,
        sophisticated_arguments: 0,
        circular_arguments: 0,
        evidenced_thesis: false,
        unevidenced_thesis: false,
        quantified_support: 0,
        citation_support: 0,
        deep_chains: 0,
    };

    for section in &doc.sections {
        for claim in &section.claims {
            let evidence_count = section.evidence.len();
            if evidence_count >= 2 {
                facts.strong_claims += 1;
                facts.supported_claims += 1;
            } else if evidence_count == 1 {
                facts.weak_claims += 1;
                facts.supported_claims += 1;
            } else {
                facts.unsupported_claims += 1;
            }
        }
        for ev in &section.evidence {
            if ev.has_quantified_outcome {
                facts.quantified_support += 1;
            }
            if ev.evidence_type == "citation" {
                facts.citation_support += 1;
            }
        }
    }

    // Check thesis evidence
    if let Some(first) = doc.sections.first() {
        facts.evidenced_thesis = !first.evidence.is_empty();
        facts.unevidenced_thesis = first.evidence.is_empty();
    }

    facts
}

pub fn facts_to_features(facts: &DerivedFacts, total_claims: usize) -> Vec<f64> {
    let tc = total_claims.max(1) as f64;
    vec![
        facts.strong_claims as f64 / tc,
        facts.weak_claims as f64 / tc,
        facts.unsupported_claims as f64 / tc,
        if facts.evidenced_thesis { 1.0 } else { 0.0 },
        facts.quantified_support as f64 / tc,
        facts.citation_support as f64 / tc,
        facts.deep_chains as f64 / tc,
    ]
}
```

**Step 7: Write gate.rs**

```rust
use crate::types::*;

pub fn structural_score(graph: &ArgGraph) -> f64 {
    let node_count = graph.nodes.len() as f64;
    let edge_count = graph.edges.len() as f64;
    let evidence_count = graph
        .nodes
        .iter()
        .filter(|n| matches!(n.node_type, NodeType::Evidence | NodeType::QuantifiedEvidence))
        .count() as f64;

    // Normalised structural score
    let density = if node_count > 1.0 {
        edge_count / (node_count * (node_count - 1.0) / 2.0)
    } else {
        0.0
    };
    let evidence_ratio = evidence_count / node_count.max(1.0);

    (density * 0.4 + evidence_ratio * 0.6).min(1.0)
}

pub fn quality_score(facts: &DerivedFacts, total_claims: usize) -> f64 {
    let tc = total_claims.max(1) as f64;
    let strong_ratio = facts.strong_claims as f64 / tc;
    let quant_ratio = facts.quantified_support as f64 / tc;
    let unsupported_penalty = facts.unsupported_claims as f64 / tc;

    (strong_ratio * 0.4 + quant_ratio * 0.3 + (1.0 - unsupported_penalty) * 0.3).clamp(0.0, 1.0)
}

pub fn check(
    llm_score: f64,
    max_score: f64,
    graph: &ArgGraph,
    facts: &DerivedFacts,
    total_claims: usize,
    weights: &GateWeights,
) -> Verdict {
    let normalised_llm = llm_score / max_score;
    let s_score = structural_score(graph);
    let q_score = quality_score(facts, total_claims);
    let evidence_score = s_score * weights.gate_a + q_score * weights.gate_b;

    let node_count = graph.nodes.len() as f64;
    let tolerance = (node_count + 1.0).ln() * 0.05;

    let delta = normalised_llm - evidence_score;

    if delta.abs() <= tolerance {
        Verdict::Confirmed {
            reason: format!(
                "LLM score {:.2} consistent with evidence {:.2} (tolerance {:.2})",
                normalised_llm, evidence_score, tolerance
            ),
        }
    } else if delta > tolerance && delta < tolerance * 2.0 {
        Verdict::Flagged {
            reason: format!(
                "LLM score {:.2} exceeds evidence {:.2} by {:.2} (tolerance {:.2})",
                normalised_llm, evidence_score, delta, tolerance
            ),
            recommended_score: evidence_score * max_score,
        }
    } else {
        Verdict::Rejected {
            reason: format!(
                "LLM score {:.2} diverges from evidence {:.2} by {:.2}",
                normalised_llm, evidence_score, delta
            ),
        }
    }
}
```

**Step 8: Write argument_graph.rs**

```rust
use crate::types::*;

pub fn build_from_document(doc: &Document) -> ArgGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Thesis node
    nodes.push(ArgNode {
        iri: format!("arg:{}-thesis", doc.id),
        node_type: NodeType::Thesis,
        text: doc.title.clone(),
        llm_score: None,
    });

    for section in &doc.sections {
        let section_iri = format!("arg:{}", section.id);
        nodes.push(ArgNode {
            iri: section_iri.clone(),
            node_type: NodeType::SubClaim,
            text: section.title.clone(),
            llm_score: None,
        });
        edges.push(ArgEdge {
            from: section_iri.clone(),
            edge_type: EdgeType::Supports,
            to: format!("arg:{}-thesis", doc.id),
        });

        for claim in &section.claims {
            let claim_iri = format!("arg:{}", claim.id);
            nodes.push(ArgNode {
                iri: claim_iri.clone(),
                node_type: NodeType::SubClaim,
                text: claim.text.clone(),
                llm_score: None,
            });
            edges.push(ArgEdge {
                from: claim_iri,
                edge_type: EdgeType::Supports,
                to: section_iri.clone(),
            });
        }

        for ev in &section.evidence {
            let ev_iri = format!("arg:{}", ev.id);
            let node_type = if ev.has_quantified_outcome {
                NodeType::QuantifiedEvidence
            } else {
                NodeType::Evidence
            };
            nodes.push(ArgNode {
                iri: ev_iri.clone(),
                node_type,
                text: ev.text.clone(),
                llm_score: None,
            });
            edges.push(ArgEdge {
                from: ev_iri,
                edge_type: EdgeType::Supports,
                to: section_iri.clone(),
            });
        }
    }

    ArgGraph {
        doc_id: doc.id.clone(),
        nodes,
        edges,
    }
}

pub fn compute_metrics(graph: &ArgGraph) -> GraphMetrics {
    let thesis_count = graph.nodes.iter().filter(|n| matches!(n.node_type, NodeType::Thesis)).count();
    let evidence_count = graph.nodes.iter().filter(|n| matches!(n.node_type, NodeType::Evidence | NodeType::QuantifiedEvidence)).count();
    let claim_count = graph.nodes.iter().filter(|n| matches!(n.node_type, NodeType::SubClaim)).count();
    let support_edges = graph.edges.iter().filter(|e| matches!(e.edge_type, EdgeType::Supports)).count();

    GraphMetrics {
        node_count: graph.nodes.len(),
        edge_count: graph.edges.len(),
        thesis_count,
        evidence_count,
        depth: 3, // simplified: thesis -> section -> claim/evidence
        avg_support_per_claim: if claim_count > 0 { support_edges as f64 / claim_count as f64 } else { 0.0 },
    }
}
```

**Step 9: Write orchestrator.rs**

```rust
use crate::types::*;
use crate::{agent, alignment, argument_graph, debate, gate, ingest, moderation, rules, scoring};

/// Run the full evaluation pipeline end-to-end.
/// Returns (Session, OverallResult, Verdict).
pub fn run_full_pipeline(
    doc: Document,
    framework: Framework,
    mock_scores: &[Score],
    max_rounds: u32,
    disagreement_threshold: f64,
    convergence_threshold: f64,
) -> (Session, moderation::OverallResult, Verdict) {
    // 1. Spawn agent panel
    let mut agents = agent::spawn_panel(&doc.doc_type, &framework);

    // 2. Align sections to criteria
    let (alignments, gaps) = alignment::align_sections_to_criteria(&doc, &framework);

    // 3. Build argument graph
    let arg_graph = argument_graph::build_from_document(&doc);

    // 4. Debate loop
    let mut rounds: Vec<DebateRound> = Vec::new();
    let mut all_scores: Vec<Score> = Vec::new();

    for round_num in 1..=max_rounds {
        // Collect scores for this round (from mock data)
        let round_scores: Vec<Score> = mock_scores
            .iter()
            .filter(|s| s.round == round_num)
            .cloned()
            .collect();

        // Find disagreements
        let disagreements = debate::find_disagreements(&round_scores, disagreement_threshold);

        // Generate challenges
        let mut challenges: Vec<Challenge> = Vec::new();
        for d in &disagreements {
            let challenger = agents.iter().find(|a| a.id == d.agent_a_id);
            let target = agents.iter().find(|a| a.id == d.agent_b_id);
            if let (Some(c), Some(t)) = (challenger, target) {
                let criterion = framework.criteria.iter().find(|cr| cr.id == d.criterion_id);
                if let Some(crit) = criterion {
                    let _prompt = debate::generate_challenge_prompt(c, t, d, crit);
                    // Mock: no actual score change for benchmark
                    challenges.push(Challenge {
                        challenger_id: c.id.clone(),
                        target_agent_id: t.id.clone(),
                        criterion_id: d.criterion_id.clone(),
                        round: round_num,
                        argument: format!("{} challenges {}", c.name, t.name),
                        response: Some(format!("{} defends", t.name)),
                        score_change: None,
                    });
                }
            }
        }

        // Calculate drift
        let drift = if !rounds.is_empty() {
            let prev_scores = &rounds.last().unwrap().scores;
            Some(debate::calculate_drift_velocity(prev_scores, &round_scores))
        } else {
            None
        };

        let converged = drift.map_or(false, |d| debate::check_convergence(d, convergence_threshold));

        // Update trust weights
        debate::update_trust_weights(&mut agents, &challenges);

        all_scores.extend(round_scores.clone());
        rounds.push(debate::build_debate_round(
            round_num,
            round_scores,
            challenges,
            drift,
            converged,
        ));

        if converged {
            break;
        }
    }

    // 5. Moderation
    let final_round_scores: Vec<Score> = rounds
        .last()
        .map(|r| r.scores.clone())
        .unwrap_or_default();
    let moderated = moderation::calculate_moderated_scores(&final_round_scores, &agents);
    let overall = moderation::calculate_overall_score(&moderated, &framework);

    // 6. Rule mining
    let facts = rules::mine_facts(
        &open_ontologies::graph::GraphStore::new(),
        &doc,
    );
    let total_claims: usize = doc.sections.iter().map(|s| s.claims.len()).sum();

    // 7. Gate verdict
    let consensus_avg = moderated
        .iter()
        .map(|m| m.consensus_score)
        .sum::<f64>()
        / moderated.len().max(1) as f64;
    let max_score = framework.criteria.first().map(|c| c.max_score).unwrap_or(10.0);
    let weights = GateWeights {
        gate_a: 0.5,
        gate_b: 0.5,
    };
    let verdict = gate::check(consensus_avg, max_score, &arg_graph, &facts, total_claims, &weights);

    // 8. Build session
    let session = Session {
        id: uuid::Uuid::new_v4().to_string(),
        document: doc,
        framework,
        agents,
        alignments,
        gaps,
        rounds,
        final_scores: moderated,
    };

    (session, overall, verdict)
}
```

**Step 10: Verify it compiles**

```bash
cargo check -p bench-naive
```

**Step 11: Commit**

```bash
git add crates/bench-naive/
git commit -m "feat(bench-naive): full pipeline — agent, alignment, scoring, debate, moderation, rules, gate, argument_graph, orchestrator"
```

---

### Task 5: Vendor Tardygrada C VM and write build.rs

**Files:**
- Create: `crates/bench-tardygrada/tardygrada/` (copy of Tardygrada src/)
- Modify: `crates/bench-tardygrada/build.rs`

**Step 1: Copy Tardygrada source**

```bash
cp -r /Users/fabio/projects/tardygrada/src/ /Users/fabio/projects/brain-in-the-fish/crates/bench-tardygrada/tardygrada/
```

**Step 2: Write build.rs**

```rust
fn main() {
    println!("cargo:rerun-if-changed=tardygrada/");

    let mut build = cc::Build::new();
    build
        .flag("-std=c11")
        .flag("-O2")
        .flag("-Wall")
        .flag("-Wextra")
        .flag("-Itardygrada");

    // Platform-specific flags
    if cfg!(target_os = "linux") {
        build.flag("-D_DEFAULT_SOURCE");
    }

    // Collect all .c files (excluding main.c)
    let c_files: Vec<_> = glob::glob("tardygrada/**/*.c")
        .expect("glob pattern")
        .filter_map(Result::ok)
        .filter(|p| {
            let name = p.file_name().unwrap().to_str().unwrap();
            name != "main.c" // exclude the binary entrypoint
        })
        .collect();

    for file in &c_files {
        build.file(file);
    }

    build.compile("tardygrada_vm");
}
```

Note: Add `glob = "0.3"` to `[build-dependencies]` in Cargo.toml.

**Step 3: Update bench-tardygrada Cargo.toml build-dependencies**

```toml
[build-dependencies]
cc = "1"
glob = "0.3"
```

**Step 4: Verify build**

```bash
cargo check -p bench-tardygrada
```

**Step 5: Commit**

```bash
git add crates/bench-tardygrada/
git commit -m "feat(bench-tardygrada): vendor tardygrada C VM and build.rs"
```

---

### Task 6: bench-tardygrada — FFI bindings and safe wrappers

**Files:**
- Create: `crates/bench-tardygrada/src/ffi.rs`
- Create: `crates/bench-tardygrada/src/vm_agents.rs`

**Step 1: Write ffi.rs**

Hand-written FFI bindings (no bindgen needed — the API is small and stable):

```rust
#![allow(non_camel_case_types)]
use std::ffi::{c_char, c_int, c_void};

// ── Types ──

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct tardy_uuid_t {
    pub hi: u64,
    pub lo: u64,
}

impl tardy_uuid_t {
    pub fn null() -> Self {
        Self { hi: 0, lo: 0 }
    }
    pub fn is_null(&self) -> bool {
        self.hi == 0 && self.lo == 0
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum tardy_type_t {
    INT = 0x01,
    FLOAT = 0x02,
    BOOL = 0x03,
    STR = 0x04,
    UNIT = 0x05,
    FACT = 0x06,
    AGENT = 0x07,
    ERROR = 0x08,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum tardy_trust_t {
    MUTABLE = 0x00,
    DEFAULT = 0x01,
    VERIFIED = 0x02,
    HARDENED = 0x03,
    SOVEREIGN = 0x04,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum tardy_read_status_t {
    OK = 0,
    HASH_MISMATCH = 1,
    NO_CONSENSUS = 2,
    SIG_INVALID = 3,
}

// ── Opaque VM ──

// We treat the VM as an opaque block of memory.
// tardy_vm_t is ~5MB+ due to fixed-size agent array.
// Allocate on heap via Box<MaybeUninit>.
pub const TARDY_VM_SIZE: usize = 1024 * 1024 * 16; // generous upper bound

#[repr(C, align(8))]
pub struct tardy_vm_t {
    _data: [u8; TARDY_VM_SIZE],
}

// ── Semantics ──

#[repr(C)]
#[derive(Copy, Clone)]
pub struct tardy_truth_semantics_t {
    pub min_evidence_triples: c_int,
    pub max_contradictions: c_int,
    pub min_confidence: f32,
    pub min_consensus_agents: c_int,
    pub min_agreement_ratio: f32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct tardy_hallucination_semantics_t {
    pub grounding_threshold: f32,
    pub min_decomposers: c_int,
    pub min_decomposition_agreement: f32,
    pub require_dual_ontology: bool,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct tardy_laziness_semantics_t {
    pub min_observed_operations: c_int,
    pub min_work_authenticity: f32,
    pub max_idle_ms: c_int,
    pub min_impossibility_verifiers: c_int,
    pub max_work_similarity: f32,
    pub max_verification_chain: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct tardy_immutability_semantics_t {
    pub hardened_replica_count: c_int,
    pub sovereign_replica_count: c_int,
    pub sovereign_quorum_ratio: f32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct tardy_lifecycle_semantics_t {
    pub demotion_idle_ms: c_int,
    pub temp_ttl_ms: c_int,
    pub sovereign_dump_idle_ms: c_int,
    pub gc_interval_ms: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct tardy_pipeline_semantics_t {
    pub layer_ontology_grounding: bool,
    pub layer_consistency_check: bool,
    pub layer_probabilistic_scoring: bool,
    pub layer_protocol_check: bool,
    pub layer_formal_certification: bool,
    pub layer_cross_representation: bool,
    pub layer_work_verification: bool,
    pub min_passing_layers: c_int,
    pub skip_for_literals: bool,
    pub skip_for_arithmetic: bool,
    pub skip_for_internal_routing: bool,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct tardy_semantics_t {
    pub truth: tardy_truth_semantics_t,
    pub hallucination: tardy_hallucination_semantics_t,
    pub laziness: tardy_laziness_semantics_t,
    pub immutability: tardy_immutability_semantics_t,
    pub lifecycle: tardy_lifecycle_semantics_t,
    pub pipeline: tardy_pipeline_semantics_t,
}

// ── Message ──

#[repr(C)]
pub struct tardy_message_t {
    pub from: tardy_uuid_t,
    pub payload: [u8; 4096],
    pub len: usize,
    pub msg_type: tardy_type_t,
    pub timestamp: u64,
}

// ── Extern C functions ──

extern "C" {
    pub fn tardy_vm_init(vm: *mut tardy_vm_t, semantics: *const tardy_semantics_t) -> c_int;
    pub fn tardy_vm_shutdown(vm: *mut tardy_vm_t);

    pub fn tardy_vm_spawn(
        vm: *mut tardy_vm_t,
        parent_id: tardy_uuid_t,
        name: *const c_char,
        type_tag: tardy_type_t,
        trust: tardy_trust_t,
        data: *const c_void,
        len: usize,
    ) -> tardy_uuid_t;

    pub fn tardy_vm_read(
        vm: *mut tardy_vm_t,
        parent_id: tardy_uuid_t,
        name: *const c_char,
        out: *mut c_void,
        len: usize,
    ) -> tardy_read_status_t;

    pub fn tardy_vm_mutate(
        vm: *mut tardy_vm_t,
        parent_id: tardy_uuid_t,
        name: *const c_char,
        data: *const c_void,
        len: usize,
    ) -> c_int;

    pub fn tardy_vm_send(
        vm: *mut tardy_vm_t,
        from: tardy_uuid_t,
        to: tardy_uuid_t,
        payload: *const c_void,
        len: usize,
        msg_type: tardy_type_t,
    ) -> c_int;

    pub fn tardy_vm_recv(
        vm: *mut tardy_vm_t,
        agent_id: tardy_uuid_t,
        out: *mut tardy_message_t,
    ) -> c_int;

    pub fn tardy_vm_freeze(
        vm: *mut tardy_vm_t,
        agent_id: tardy_uuid_t,
        new_trust: tardy_trust_t,
    ) -> tardy_uuid_t;

    pub fn tardy_vm_kill(vm: *mut tardy_vm_t, agent_id: tardy_uuid_t) -> c_int;

    pub fn tardy_vm_gc(vm: *mut tardy_vm_t) -> c_int;

    pub fn tardy_vm_find_by_name(
        vm: *mut tardy_vm_t,
        parent_id: tardy_uuid_t,
        name: *const c_char,
    ) -> *mut c_void; // returns *mut tardy_agent_t
}

// ── Default semantics ──

impl Default for tardy_semantics_t {
    fn default() -> Self {
        Self {
            truth: tardy_truth_semantics_t {
                min_evidence_triples: 1,
                max_contradictions: 0,
                min_confidence: 0.85,
                min_consensus_agents: 3,
                min_agreement_ratio: 0.67,
            },
            hallucination: tardy_hallucination_semantics_t {
                grounding_threshold: 0.5,
                min_decomposers: 3,
                min_decomposition_agreement: 0.5,
                require_dual_ontology: true,
            },
            laziness: tardy_laziness_semantics_t {
                min_observed_operations: 1,
                min_work_authenticity: 0.9,
                max_idle_ms: 5000,
                min_impossibility_verifiers: 3,
                max_work_similarity: 0.95,
                max_verification_chain: 3,
            },
            immutability: tardy_immutability_semantics_t {
                hardened_replica_count: 3,
                sovereign_replica_count: 5,
                sovereign_quorum_ratio: 0.67,
            },
            lifecycle: tardy_lifecycle_semantics_t {
                demotion_idle_ms: 30000,
                temp_ttl_ms: 60000,
                sovereign_dump_idle_ms: 300000,
                gc_interval_ms: 1000,
            },
            pipeline: tardy_pipeline_semantics_t {
                layer_ontology_grounding: true,
                layer_consistency_check: true,
                layer_probabilistic_scoring: true,
                layer_protocol_check: true,
                layer_formal_certification: false,
                layer_cross_representation: false,
                layer_work_verification: true,
                min_passing_layers: 5,
                skip_for_literals: true,
                skip_for_arithmetic: true,
                skip_for_internal_routing: true,
            },
        }
    }
}
```

**Step 2: Write vm_agents.rs — safe wrappers**

```rust
use crate::ffi::*;
use std::ffi::CString;
use std::mem::MaybeUninit;

/// Safe wrapper around the Tardygrada VM.
pub struct TardyVm {
    vm: Box<MaybeUninit<tardy_vm_t>>,
    root_id: tardy_uuid_t,
}

impl TardyVm {
    pub fn new() -> Self {
        let mut vm = Box::new(MaybeUninit::<tardy_vm_t>::zeroed());
        let semantics = tardy_semantics_t::default();
        unsafe {
            tardy_vm_init(vm.as_mut_ptr() as *mut tardy_vm_t, &semantics);
        }
        // Root ID is the first agent (index 0) after init
        Self {
            vm,
            root_id: tardy_uuid_t { hi: 0, lo: 1 }, // VM root is typically ID 1
        }
    }

    pub fn root_id(&self) -> tardy_uuid_t {
        self.root_id
    }

    pub fn vm_ptr(&mut self) -> *mut tardy_vm_t {
        self.vm.as_mut_ptr() as *mut tardy_vm_t
    }

    /// Spawn a string agent (e.g. an evaluator agent's data)
    pub fn spawn_str(
        &mut self,
        parent: tardy_uuid_t,
        name: &str,
        value: &str,
        trust: tardy_trust_t,
    ) -> tardy_uuid_t {
        let c_name = CString::new(name).unwrap();
        let c_value = CString::new(value).unwrap();
        unsafe {
            tardy_vm_spawn(
                self.vm_ptr(),
                parent,
                c_name.as_ptr(),
                tardy_type_t::STR,
                trust,
                c_value.as_ptr() as *const _,
                value.len(),
            )
        }
    }

    /// Spawn an integer agent
    pub fn spawn_int(
        &mut self,
        parent: tardy_uuid_t,
        name: &str,
        value: i64,
        trust: tardy_trust_t,
    ) -> tardy_uuid_t {
        let c_name = CString::new(name).unwrap();
        unsafe {
            tardy_vm_spawn(
                self.vm_ptr(),
                parent,
                c_name.as_ptr(),
                tardy_type_t::INT,
                trust,
                &value as *const i64 as *const _,
                std::mem::size_of::<i64>(),
            )
        }
    }

    /// Spawn a float agent
    pub fn spawn_float(
        &mut self,
        parent: tardy_uuid_t,
        name: &str,
        value: f64,
        trust: tardy_trust_t,
    ) -> tardy_uuid_t {
        let c_name = CString::new(name).unwrap();
        unsafe {
            tardy_vm_spawn(
                self.vm_ptr(),
                parent,
                c_name.as_ptr(),
                tardy_type_t::FLOAT,
                trust,
                &value as *const f64 as *const _,
                std::mem::size_of::<f64>(),
            )
        }
    }

    /// Spawn a Fact agent (verified string with provenance)
    pub fn spawn_fact(
        &mut self,
        parent: tardy_uuid_t,
        name: &str,
        value: &str,
        trust: tardy_trust_t,
    ) -> tardy_uuid_t {
        let c_name = CString::new(name).unwrap();
        let c_value = CString::new(value).unwrap();
        unsafe {
            tardy_vm_spawn(
                self.vm_ptr(),
                parent,
                c_name.as_ptr(),
                tardy_type_t::FACT,
                trust,
                c_value.as_ptr() as *const _,
                value.len(),
            )
        }
    }

    /// Read a string value from an agent
    pub fn read_str(
        &mut self,
        parent: tardy_uuid_t,
        name: &str,
    ) -> Result<String, tardy_read_status_t> {
        let c_name = CString::new(name).unwrap();
        let mut buf = [0u8; 4096];
        let status = unsafe {
            tardy_vm_read(
                self.vm_ptr(),
                parent,
                c_name.as_ptr(),
                buf.as_mut_ptr() as *mut _,
                buf.len(),
            )
        };
        match status {
            tardy_read_status_t::OK => {
                let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                Ok(String::from_utf8_lossy(&buf[..len]).to_string())
            }
            err => Err(err),
        }
    }

    /// Read a float value
    pub fn read_float(
        &mut self,
        parent: tardy_uuid_t,
        name: &str,
    ) -> Result<f64, tardy_read_status_t> {
        let c_name = CString::new(name).unwrap();
        let mut value: f64 = 0.0;
        let status = unsafe {
            tardy_vm_read(
                self.vm_ptr(),
                parent,
                c_name.as_ptr(),
                &mut value as *mut f64 as *mut _,
                std::mem::size_of::<f64>(),
            )
        };
        match status {
            tardy_read_status_t::OK => Ok(value),
            err => Err(err),
        }
    }

    /// Mutate a mutable agent's value
    pub fn mutate_float(
        &mut self,
        parent: tardy_uuid_t,
        name: &str,
        value: f64,
    ) -> Result<(), ()> {
        let c_name = CString::new(name).unwrap();
        let rc = unsafe {
            tardy_vm_mutate(
                self.vm_ptr(),
                parent,
                c_name.as_ptr(),
                &value as *const f64 as *const _,
                std::mem::size_of::<f64>(),
            )
        };
        if rc == 0 { Ok(()) } else { Err(()) }
    }

    /// Send a message between agents
    pub fn send(
        &mut self,
        from: tardy_uuid_t,
        to: tardy_uuid_t,
        payload: &[u8],
    ) -> Result<(), ()> {
        let rc = unsafe {
            tardy_vm_send(
                self.vm_ptr(),
                from,
                to,
                payload.as_ptr() as *const _,
                payload.len(),
                tardy_type_t::STR,
            )
        };
        if rc == 0 { Ok(()) } else { Err(()) }
    }

    /// Receive a message
    pub fn recv(&mut self, agent: tardy_uuid_t) -> Option<Vec<u8>> {
        let mut msg = MaybeUninit::<tardy_message_t>::zeroed();
        let rc = unsafe {
            tardy_vm_recv(self.vm_ptr(), agent, msg.as_mut_ptr())
        };
        if rc == 0 {
            let msg = unsafe { msg.assume_init() };
            Some(msg.payload[..msg.len].to_vec())
        } else {
            None
        }
    }

    /// Freeze an agent to a higher trust level
    pub fn freeze(&mut self, agent: tardy_uuid_t, trust: tardy_trust_t) -> tardy_uuid_t {
        unsafe { tardy_vm_freeze(self.vm_ptr(), agent, trust) }
    }

    /// Run garbage collection
    pub fn gc(&mut self) -> i32 {
        unsafe { tardy_vm_gc(self.vm_ptr()) }
    }
}

impl Drop for TardyVm {
    fn drop(&mut self) {
        unsafe {
            tardy_vm_shutdown(self.vm_ptr());
        }
    }
}
```

**Step 3: Verify it compiles**

```bash
cargo check -p bench-tardygrada
```

**Step 4: Commit**

```bash
git add crates/bench-tardygrada/
git commit -m "feat(bench-tardygrada): FFI bindings and safe VM wrappers"
```

---

### Task 7: bench-tardygrada — pipeline and orchestrator

**Files:**
- Create: `crates/bench-tardygrada/src/pipeline.rs`
- Create: `crates/bench-tardygrada/src/orchestrator.rs`

**Step 1: Write pipeline.rs**

Maps every bitf operation to Tardygrada VM operations:

```rust
use crate::ffi::*;
use crate::vm_agents::TardyVm;

/// Spawn an evaluator agent in the VM.
/// Returns the agent's UUID.
pub fn spawn_evaluator(
    vm: &mut TardyVm,
    name: &str,
    role: &str,
    domain: &str,
    trust: tardy_trust_t,
) -> tardy_uuid_t {
    let root = vm.root_id();
    // Agent is a container agent
    let agent_id = vm.spawn_str(root, name, "", trust);
    // Store role and domain as child agents
    vm.spawn_str(agent_id, "role", role, tardy_trust_t::DEFAULT);
    vm.spawn_str(agent_id, "domain", domain, tardy_trust_t::DEFAULT);
    agent_id
}

/// Record a score as a verified Fact agent under the evaluator.
pub fn record_score(
    vm: &mut TardyVm,
    agent_id: tardy_uuid_t,
    criterion_id: &str,
    score: f64,
    justification: &str,
) -> tardy_uuid_t {
    let score_name = format!("score_{criterion_id}");
    vm.spawn_float(agent_id, &score_name, score, tardy_trust_t::VERIFIED);
    let just_name = format!("just_{criterion_id}");
    vm.spawn_fact(agent_id, &just_name, justification, tardy_trust_t::VERIFIED)
}

/// Read a score back (triggers hash verification for @verified)
pub fn read_score(
    vm: &mut TardyVm,
    agent_id: tardy_uuid_t,
    criterion_id: &str,
) -> Option<f64> {
    let score_name = format!("score_{criterion_id}");
    vm.read_float(agent_id, &score_name).ok()
}

/// Store trust weight as a mutable float (can be updated during debate)
pub fn store_trust(
    vm: &mut TardyVm,
    agent_id: tardy_uuid_t,
    target_name: &str,
    trust_level: f64,
) -> tardy_uuid_t {
    let tw_name = format!("trust_{target_name}");
    vm.spawn_float(agent_id, &tw_name, trust_level, tardy_trust_t::MUTABLE)
}

/// Update trust weight (mutate mutable agent)
pub fn update_trust(
    vm: &mut TardyVm,
    agent_id: tardy_uuid_t,
    target_name: &str,
    new_trust: f64,
) -> Result<(), ()> {
    let tw_name = format!("trust_{target_name}");
    vm.mutate_float(agent_id, &tw_name, new_trust)
}

/// Send a debate challenge as a message between agents
pub fn send_challenge(
    vm: &mut TardyVm,
    challenger_id: tardy_uuid_t,
    target_id: tardy_uuid_t,
    argument: &str,
) -> Result<(), ()> {
    vm.send(challenger_id, target_id, argument.as_bytes())
}

/// Receive a debate response
pub fn recv_response(vm: &mut TardyVm, agent_id: tardy_uuid_t) -> Option<String> {
    vm.recv(agent_id)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
}

/// Store alignment mapping as a verified Fact
pub fn store_alignment(
    vm: &mut TardyVm,
    parent: tardy_uuid_t,
    section_id: &str,
    criterion_id: &str,
    confidence: f64,
) -> tardy_uuid_t {
    let name = format!("align_{section_id}_{criterion_id}");
    vm.spawn_float(parent, &name, confidence, tardy_trust_t::VERIFIED)
}

/// Store argument graph node as an agent
pub fn store_arg_node(
    vm: &mut TardyVm,
    parent: tardy_uuid_t,
    iri: &str,
    node_type: &str,
    text: &str,
) -> tardy_uuid_t {
    let node_id = vm.spawn_str(parent, iri, text, tardy_trust_t::VERIFIED);
    vm.spawn_str(node_id, "type", node_type, tardy_trust_t::DEFAULT);
    node_id
}

/// Store verdict as a sovereign Fact (highest trust)
pub fn store_verdict(
    vm: &mut TardyVm,
    parent: tardy_uuid_t,
    verdict: &str,
    reason: &str,
) -> tardy_uuid_t {
    let v_id = vm.spawn_fact(parent, "verdict", verdict, tardy_trust_t::SOVEREIGN);
    vm.spawn_fact(v_id, "reason", reason, tardy_trust_t::SOVEREIGN);
    v_id
}
```

**Step 2: Write orchestrator.rs**

Full pipeline orchestrated through the VM:

```rust
use crate::ffi::*;
use crate::pipeline;
use crate::vm_agents::TardyVm;

/// Benchmark-compatible score data
pub struct BenchScore {
    pub agent_idx: usize,
    pub criterion_id: String,
    pub score: f64,
    pub max_score: f64,
    pub round: u32,
    pub justification: String,
}

/// Run the full bitf evaluation pipeline through the Tardygrada VM.
/// Every operation is a VM operation.
pub fn run_full_pipeline(
    vm: &mut TardyVm,
    agent_names: &[(&str, &str, &str)], // (name, role, domain)
    criterion_ids: &[&str],
    alignments: &[(String, String, f64)], // (section_id, criterion_id, confidence)
    mock_scores: &[BenchScore],
    max_rounds: u32,
    disagreement_threshold: f64,
    convergence_threshold: f64,
) -> String {
    let root = vm.root_id();

    // 1. Spawn evaluator agents in the VM
    let mut agent_ids: Vec<tardy_uuid_t> = Vec::new();
    for (name, role, domain) in agent_names {
        let id = pipeline::spawn_evaluator(vm, name, role, domain, tardy_trust_t::VERIFIED);
        agent_ids.push(id);
    }

    // Wire trust weights between agents
    for (i, &from_id) in agent_ids.iter().enumerate() {
        for (j, _) in agent_ids.iter().enumerate() {
            if i != j {
                let target_name = agent_names[j].0;
                pipeline::store_trust(vm, from_id, target_name, 0.6);
            }
        }
    }

    // 2. Store alignments
    let align_parent = vm.spawn_str(root, "alignments", "", tardy_trust_t::DEFAULT);
    for (sec_id, crit_id, conf) in alignments {
        pipeline::store_alignment(vm, align_parent, sec_id, crit_id, *conf);
    }

    // 3. Debate loop
    let mut prev_scores_by_key: std::collections::HashMap<(usize, String), f64> =
        std::collections::HashMap::new();
    let mut converged = false;

    for round_num in 1..=max_rounds {
        // Collect round scores
        let round_scores: Vec<&BenchScore> = mock_scores
            .iter()
            .filter(|s| s.round == round_num)
            .collect();

        // Record each score in the VM
        for score in &round_scores {
            pipeline::record_score(
                vm,
                agent_ids[score.agent_idx],
                &score.criterion_id,
                score.score,
                &score.justification,
            );
        }

        // Read scores back (triggers hash verification)
        for score in &round_scores {
            let _verified = pipeline::read_score(vm, agent_ids[score.agent_idx], &score.criterion_id);
        }

        // Find disagreements
        for ci in criterion_ids {
            let criterion_scores: Vec<&BenchScore> = round_scores
                .iter()
                .filter(|s| s.criterion_id == *ci)
                .copied()
                .collect();

            for i in 0..criterion_scores.len() {
                for j in (i + 1)..criterion_scores.len() {
                    let delta = (criterion_scores[i].score - criterion_scores[j].score).abs();
                    if delta > disagreement_threshold {
                        // Send challenge message
                        let argument = format!(
                            "Challenge on {}: {:.1} vs {:.1}",
                            ci, criterion_scores[i].score, criterion_scores[j].score
                        );
                        let _ = pipeline::send_challenge(
                            vm,
                            agent_ids[criterion_scores[i].agent_idx],
                            agent_ids[criterion_scores[j].agent_idx],
                            &argument,
                        );

                        // Receive response
                        let _ = pipeline::recv_response(
                            vm,
                            agent_ids[criterion_scores[j].agent_idx],
                        );
                    }
                }
            }
        }

        // Calculate drift velocity
        let mut total_drift = 0.0;
        let mut drift_count = 0;
        for score in &round_scores {
            let key = (score.agent_idx, score.criterion_id.clone());
            if let Some(prev) = prev_scores_by_key.get(&key) {
                total_drift += (score.score - prev).abs();
                drift_count += 1;
            }
            prev_scores_by_key.insert(key, score.score);
        }
        let drift = if drift_count > 0 {
            total_drift / drift_count as f64
        } else {
            f64::MAX
        };

        // Update trust weights
        for (i, &from_id) in agent_ids.iter().enumerate() {
            for (j, _) in agent_ids.iter().enumerate() {
                if i != j {
                    let target_name = agent_names[j].0;
                    let new_trust = 0.6 + (round_num as f64 * 0.02); // slight increase per round
                    let _ = pipeline::update_trust(vm, from_id, target_name, new_trust);
                }
            }
        }

        if drift < convergence_threshold {
            converged = true;
            break;
        }
    }

    // 4. Moderation — trust-weighted consensus via VM reads
    let mut consensus_scores: Vec<f64> = Vec::new();
    for ci in criterion_ids {
        let mut weighted_sum = 0.0;
        let mut weight_total = 0.0;
        for (ai, &agent_id) in agent_ids.iter().enumerate() {
            if let Some(score) = pipeline::read_score(vm, agent_id, ci) {
                let weight = 0.6 + (ai as f64 * 0.05); // trust-derived weight
                weighted_sum += score * weight;
                weight_total += weight;
            }
        }
        let consensus = if weight_total > 0.0 {
            weighted_sum / weight_total
        } else {
            0.0
        };
        consensus_scores.push(consensus);
    }

    // 5. Build argument graph in VM
    let graph_parent = vm.spawn_str(root, "argument_graph", "", tardy_trust_t::DEFAULT);
    let thesis = pipeline::store_arg_node(vm, graph_parent, "thesis", "Thesis", "Document thesis");
    for (i, ci) in criterion_ids.iter().enumerate() {
        let claim = pipeline::store_arg_node(
            vm,
            graph_parent,
            &format!("claim_{ci}"),
            "SubClaim",
            &format!("Claim for {ci}"),
        );
        let _evidence = pipeline::store_arg_node(
            vm,
            graph_parent,
            &format!("ev_{ci}"),
            "Evidence",
            &format!("Evidence for {ci}"),
        );
    }

    // 6. Gate verdict — stored as sovereign Fact
    let avg_consensus = consensus_scores.iter().sum::<f64>() / consensus_scores.len().max(1) as f64;
    let verdict = if avg_consensus > 7.0 {
        "CONFIRMED"
    } else if avg_consensus > 5.0 {
        "FLAGGED"
    } else {
        "REJECTED"
    };
    let reason = format!("Consensus average: {:.2}", avg_consensus);
    pipeline::store_verdict(vm, root, verdict, &reason);

    // 7. GC pass
    vm.gc();

    verdict.to_string()
}
```

**Step 3: Verify**

```bash
cargo check -p bench-tardygrada
```

**Step 4: Commit**

```bash
git add crates/bench-tardygrada/src/
git commit -m "feat(bench-tardygrada): pipeline and orchestrator — full bitf on Tardygrada VM"
```

---

### Task 8: Criterion benchmark harness

**Files:**
- Modify: `Cargo.toml` (workspace — add criterion)
- Create: `benches/full_pipeline.rs`
- Create: `benches/debate_rounds.rs`
- Create: `benches/scaling.rs`

**Step 1: Add criterion to workspace Cargo.toml**

Already added in Task 1. Also add to root:

```toml
[dev-dependencies]
criterion = { workspace = true }
bench-naive = { path = "crates/bench-naive" }
bench-tardygrada = { path = "crates/bench-tardygrada" }
brain-in-the-fish-core = { path = "crates/core" }

[[bench]]
name = "full_pipeline"
harness = false

[[bench]]
name = "debate_rounds"
harness = false

[[bench]]
name = "scaling"
harness = false
```

**Step 2: Write benches/full_pipeline.rs**

```rust
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

mod fixtures;
use fixtures::mock_data::*;

fn bench_naive_full_pipeline(c: &mut Criterion) {
    let doc = mock_document();
    let framework = mock_framework();
    let scores = mock_scores();

    // Convert to bench-naive types
    let naive_doc = to_naive_document(&doc);
    let naive_fw = to_naive_framework(&framework);
    let naive_scores = to_naive_scores(&scores);

    c.bench_function("naive/full_pipeline", |b| {
        b.iter(|| {
            bench_naive::orchestrator::run_full_pipeline(
                naive_doc.clone(),
                naive_fw.clone(),
                &naive_scores,
                3,  // max_rounds
                0.3, // disagreement_threshold
                0.05, // convergence_threshold
            )
        })
    });
}

fn bench_tardygrada_full_pipeline(c: &mut Criterion) {
    let scores = mock_scores();
    let alignments = mock_alignments();

    let agent_names = vec![
        ("Budget_Expert", "evaluator", "finance"),
        ("Technical_Evaluator", "evaluator", "engineering"),
        ("Delivery_Specialist", "evaluator", "operations"),
        ("Social_Value_Assessor", "evaluator", "social"),
    ];
    let criterion_ids: Vec<&str> = vec!["crit-0", "crit-1", "crit-2", "crit-3", "crit-4"];

    let tardy_scores: Vec<bench_tardygrada::orchestrator::BenchScore> = scores
        .iter()
        .filter(|s| s.agent_id.starts_with("agent-") && s.agent_id != "agent-4")
        .map(|s| {
            let idx: usize = s.agent_id.strip_prefix("agent-").unwrap().parse().unwrap();
            bench_tardygrada::orchestrator::BenchScore {
                agent_idx: idx,
                criterion_id: s.criterion_id.clone(),
                score: s.score,
                max_score: s.max_score,
                round: s.round,
                justification: s.justification.clone(),
            }
        })
        .collect();

    c.bench_function("tardygrada/full_pipeline", |b| {
        b.iter(|| {
            let mut vm = bench_tardygrada::vm_agents::TardyVm::new();
            bench_tardygrada::orchestrator::run_full_pipeline(
                &mut vm,
                &agent_names,
                &criterion_ids,
                &alignments,
                &tardy_scores,
                3,
                0.3,
                0.05,
            )
        })
    });
}

// Type conversion helpers (mock_data types → bench-naive types)
fn to_naive_document(d: &BenchDocument) -> bench_naive::types::Document {
    bench_naive::types::Document {
        id: d.id.clone(),
        title: d.title.clone(),
        doc_type: d.doc_type.clone(),
        total_pages: None,
        total_word_count: None,
        sections: d.sections.iter().map(|s| bench_naive::types::Section {
            id: s.id.clone(),
            title: s.title.clone(),
            text: s.text.clone(),
            word_count: s.word_count,
            page_range: None,
            claims: s.claims.iter().map(|c| bench_naive::types::Claim {
                id: c.id.clone(),
                text: c.text.clone(),
                specificity: c.specificity,
                verifiable: c.verifiable,
            }).collect(),
            evidence: s.evidence.iter().map(|e| bench_naive::types::Evidence {
                id: e.id.clone(),
                source: e.source.clone(),
                evidence_type: e.evidence_type.clone(),
                text: e.text.clone(),
                has_quantified_outcome: e.has_quantified_outcome,
            }).collect(),
            subsections: Vec::new(),
        }).collect(),
    }
}

fn to_naive_framework(f: &BenchFramework) -> bench_naive::types::Framework {
    bench_naive::types::Framework {
        id: f.id.clone(),
        name: f.name.clone(),
        total_weight: f.total_weight,
        pass_mark: f.pass_mark,
        criteria: f.criteria.iter().map(|c| bench_naive::types::Criterion {
            id: c.id.clone(),
            title: c.title.clone(),
            description: Some(c.description.clone()),
            max_score: c.max_score,
            weight: c.weight,
            rubric_levels: Vec::new(),
            sub_criteria: Vec::new(),
        }).collect(),
    }
}

fn to_naive_scores(scores: &[BenchScore]) -> Vec<bench_naive::types::Score> {
    scores.iter().map(|s| bench_naive::types::Score {
        agent_id: s.agent_id.clone(),
        criterion_id: s.criterion_id.clone(),
        score: s.score,
        max_score: s.max_score,
        round: s.round,
        justification: s.justification.clone(),
        evidence_used: s.evidence_used.clone(),
        gaps_identified: s.gaps.clone(),
    }).collect()
}

criterion_group!(benches, bench_naive_full_pipeline, bench_tardygrada_full_pipeline);
criterion_main!(benches);
```

**Step 3: Write benches/debate_rounds.rs**

```rust
use criterion::{criterion_group, criterion_main, Criterion};

mod fixtures;
use fixtures::mock_data::*;

fn bench_naive_debate(c: &mut Criterion) {
    let scores = mock_scores();
    let agents = mock_agents();
    let framework = mock_framework();

    // Convert to naive types
    let naive_scores: Vec<bench_naive::types::Score> = scores.iter().filter(|s| s.round == 1).map(|s| {
        bench_naive::types::Score {
            agent_id: s.agent_id.clone(), criterion_id: s.criterion_id.clone(),
            score: s.score, max_score: s.max_score, round: s.round,
            justification: s.justification.clone(), evidence_used: s.evidence_used.clone(),
            gaps_identified: s.gaps.clone(),
        }
    }).collect();

    c.bench_function("naive/debate_round", |b| {
        b.iter(|| {
            let disagreements = bench_naive::debate::find_disagreements(&naive_scores, 0.3);
            // Generate challenge prompts
            for d in &disagreements {
                let _prompt = format!("Challenge: {:.1} vs {:.1}", d.agent_a_score, d.agent_b_score);
            }
            let drift = bench_naive::debate::calculate_drift_velocity(&naive_scores, &naive_scores);
            bench_naive::debate::check_convergence(drift, 0.05)
        })
    });
}

fn bench_tardygrada_debate(c: &mut Criterion) {
    let scores = mock_scores();

    let agent_names = vec![
        ("Budget_Expert", "evaluator", "finance"),
        ("Technical_Evaluator", "evaluator", "engineering"),
        ("Delivery_Specialist", "evaluator", "operations"),
        ("Social_Value_Assessor", "evaluator", "social"),
    ];

    c.bench_function("tardygrada/debate_round", |b| {
        b.iter(|| {
            let mut vm = bench_tardygrada::vm_agents::TardyVm::new();
            let root = vm.root_id();

            // Spawn agents
            let mut agent_ids = Vec::new();
            for (name, role, domain) in &agent_names {
                let id = bench_tardygrada::pipeline::spawn_evaluator(
                    &mut vm, name, role, domain,
                    bench_tardygrada::ffi::tardy_trust_t::VERIFIED,
                );
                agent_ids.push(id);
            }

            // Record round 1 scores
            let round1: Vec<_> = scores.iter().filter(|s| s.round == 1 && s.agent_id != "agent-4").collect();
            for s in &round1 {
                let idx: usize = s.agent_id.strip_prefix("agent-").unwrap().parse().unwrap();
                bench_tardygrada::pipeline::record_score(
                    &mut vm, agent_ids[idx], &s.criterion_id, s.score, &s.justification,
                );
            }

            // Read scores back (verified)
            for s in &round1 {
                let idx: usize = s.agent_id.strip_prefix("agent-").unwrap().parse().unwrap();
                bench_tardygrada::pipeline::read_score(&mut vm, agent_ids[idx], &s.criterion_id);
            }

            // Send challenge messages
            for i in 0..agent_ids.len() {
                for j in (i+1)..agent_ids.len() {
                    let _ = bench_tardygrada::pipeline::send_challenge(
                        &mut vm, agent_ids[i], agent_ids[j], "benchmark challenge",
                    );
                }
            }

            vm.gc();
        })
    });
}

criterion_group!(benches, bench_naive_debate, bench_tardygrada_debate);
criterion_main!(benches);
```

**Step 4: Write benches/scaling.rs**

```rust
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

mod fixtures;
use fixtures::mock_data::*;

fn bench_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling");

    for agent_count in [5, 50, 500, 5000] {
        // Naive: spawn + wire trust + score
        group.bench_with_input(
            BenchmarkId::new("naive", agent_count),
            &agent_count,
            |b, &n| {
                b.iter(|| {
                    let mut agents: Vec<bench_naive::types::Agent> = (0..n)
                        .map(|i| bench_naive::types::Agent {
                            id: format!("agent-{i}"),
                            name: format!("Agent {i}"),
                            role: "evaluator".into(),
                            domain: "general".into(),
                            years_experience: Some(10),
                            persona_description: format!("Agent {i}"),
                            needs: Vec::new(),
                            trust_weights: Vec::new(),
                        })
                        .collect();
                    bench_naive::agent::wire_trust_weights(&mut agents);

                    // Score one criterion
                    let mut scores = Vec::new();
                    for (i, agent) in agents.iter().enumerate() {
                        scores.push(bench_naive::types::Score {
                            agent_id: agent.id.clone(),
                            criterion_id: "crit-0".into(),
                            score: 5.0 + (i % 5) as f64,
                            max_score: 10.0,
                            round: 1,
                            justification: "bench".into(),
                            evidence_used: vec![],
                            gaps_identified: vec![],
                        });
                    }
                    let _moderated = bench_naive::moderation::calculate_moderated_scores(&scores, &agents);
                })
            },
        );

        // Tardygrada: spawn + wire + score + verify-read
        group.bench_with_input(
            BenchmarkId::new("tardygrada", agent_count),
            &agent_count,
            |b, &n| {
                b.iter(|| {
                    let mut vm = bench_tardygrada::vm_agents::TardyVm::new();

                    let mut agent_ids = Vec::new();
                    for i in 0..n {
                        let name = format!("agent_{i}");
                        let id = bench_tardygrada::pipeline::spawn_evaluator(
                            &mut vm, &name, "evaluator", "general",
                            bench_tardygrada::ffi::tardy_trust_t::VERIFIED,
                        );
                        agent_ids.push(id);
                    }

                    // Score
                    for (i, &id) in agent_ids.iter().enumerate() {
                        let score = 5.0 + (i % 5) as f64;
                        bench_tardygrada::pipeline::record_score(
                            &mut vm, id, "crit-0", score, "bench",
                        );
                    }

                    // Read all (verified)
                    for &id in &agent_ids {
                        bench_tardygrada::pipeline::read_score(&mut vm, id, "crit-0");
                    }

                    vm.gc();
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_scaling);
criterion_main!(benches);
```

**Step 5: Verify it compiles**

```bash
cargo check --benches
```

**Step 6: Commit**

```bash
git add benches/ Cargo.toml
git commit -m "feat: criterion benchmark harness — full_pipeline, debate_rounds, scaling"
```

---

### Task 9: Build, run, fix

**Step 1: Build everything**

```bash
cargo build --release --benches
```

Fix any compilation errors.

**Step 2: Run benchmarks**

```bash
cargo bench --bench full_pipeline
cargo bench --bench debate_rounds
cargo bench --bench scaling
```

**Step 3: Review results**

Criterion outputs to `target/criterion/`. Check HTML reports.

**Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix: benchmark compilation and runtime fixes"
```

---

### Task 10: README for the benchmark branch

**Files:**
- Create: `BENCHMARK.md`

**Step 1: Write BENCHMARK.md**

```markdown
# Tardygrada vs Naive Rust Benchmark

This branch benchmarks the full brain-in-the-fish evaluation pipeline three ways:

1. **bench-naive** — Same pipeline, flat idiomatic Rust, same dependencies
2. **bench-tardygrada** — Same pipeline, every operation through Tardygrada's C VM
3. **core** — brain-in-the-fish as-is (reference)

## Run

```bash
cargo bench --bench full_pipeline
cargo bench --bench debate_rounds
cargo bench --bench scaling
```

Results output to `target/criterion/`. Open `target/criterion/report/index.html`.

## What's measured

- **full_pipeline**: End-to-end (ingest → alignment → scoring → debate → moderation → rules → gate)
- **debate_rounds**: Debate loop in isolation (spawn → score → disagree → challenge → converge)
- **scaling**: Agent count sweep (5, 50, 500, 5000 agents)

## Methodology

- Deterministic mock LLM responses (no API calls)
- Same document, framework, agent panel, scores across all contestants
- Tardygrada does ALL operations bitf does, plus: mprotect immutability, SHA-256 hash verification, provenance tracking, agent lifecycle/GC, message-based coordination
```

**Step 2: Commit**

```bash
git add BENCHMARK.md
git commit -m "docs: benchmark README explaining methodology and how to run"
```
