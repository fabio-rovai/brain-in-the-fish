//! Plain domain types for the bench-naive evaluation pipeline.
//!
//! No trait hierarchies, no GraphStore indirection — just flat structs
//! that mirror the core pipeline's semantics.

use serde::{Deserialize, Serialize};
// ============================================================================
// Document types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub doc_type: String,
    pub total_pages: Option<u32>,
    pub total_word_count: Option<u32>,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub text: String,
    pub word_count: u32,
    pub claims: Vec<Claim>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub text: String,
    pub specificity: f64,
    pub verifiable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub source: String,
    pub evidence_type: String,
    pub text: String,
    pub has_quantified_outcome: bool,
}

// ============================================================================
// Framework types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Framework {
    pub id: String,
    pub name: String,
    pub total_weight: f64,
    pub pass_mark: Option<f64>,
    pub criteria: Vec<Criterion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criterion {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub max_score: f64,
    pub weight: f64,
    pub rubric_levels: Vec<RubricLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricLevel {
    pub level: String,
    pub score_range: String,
    pub descriptor: String,
}

// ============================================================================
// Agent types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaslowNeed {
    pub need_type: MaslowLevel,
    pub expression: String,
    pub salience: f64,
    pub satisfied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MaslowLevel {
    Physiological,
    Safety,
    Belonging,
    Esteem,
    SelfActualisation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustRelation {
    pub target_agent_id: String,
    pub domain: String,
    pub trust_level: f64,
}

// ============================================================================
// Scoring types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    pub challenger_id: String,
    pub target_agent_id: String,
    pub criterion_id: String,
    pub round: u32,
    pub argument: String,
    pub response: Option<String>,
    pub score_change: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeratedScore {
    pub criterion_id: String,
    pub consensus_score: f64,
    pub panel_mean: f64,
    pub panel_std_dev: f64,
    pub dissents: Vec<Dissent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dissent {
    pub agent_id: String,
    pub score: f64,
    pub reason: String,
}

// ============================================================================
// Alignment types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentMapping {
    pub section_id: String,
    pub criterion_id: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    pub criterion_id: String,
    pub criterion_title: String,
    pub best_partial_match: Option<AlignmentMapping>,
}

// ============================================================================
// Debate types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateRound {
    pub round_number: u32,
    pub scores: Vec<Score>,
    pub challenges: Vec<Challenge>,
    pub drift_velocity: Option<f64>,
    pub converged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub document: Document,
    pub framework: Framework,
    pub agents: Vec<Agent>,
    pub alignments: Vec<AlignmentMapping>,
    pub gaps: Vec<Gap>,
    pub rounds: Vec<DebateRound>,
    pub final_scores: Vec<ModeratedScore>,
    pub created_at: String,
}

// ============================================================================
// Argument graph types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeType {
    Thesis,
    SubClaim,
    Evidence,
    QuantifiedEvidence,
    Citation,
    Counter,
    Rebuttal,
    Structural,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdgeType {
    Supports,
    Warrants,
    Counters,
    Rebuts,
    Contains,
    References,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgNode {
    pub iri: String,
    pub node_type: NodeType,
    pub text: String,
    pub llm_score: Option<f64>,
    pub llm_justification: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgEdge {
    pub from: String,
    pub edge_type: EdgeType,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgGraph {
    pub doc_id: String,
    pub nodes: Vec<ArgNode>,
    pub edges: Vec<ArgEdge>,
}

// ============================================================================
// Gate types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Verdict {
    Confirmed { reason: String },
    Flagged { reason: String, recommended_score: f64 },
    Rejected { reason: String },
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verdict::Confirmed { reason } => write!(f, "CONFIRMED: {reason}"),
            Verdict::Flagged { reason, recommended_score } => {
                write!(f, "FLAGGED: {reason} (recommended: {recommended_score:.1})")
            }
            Verdict::Rejected { reason } => write!(f, "REJECTED: {reason}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateWeights {
    pub gate_a: f64,
    pub gate_b: f64,
}

impl Default for GateWeights {
    fn default() -> Self {
        Self {
            gate_a: 0.06,
            gate_b: 0.02,
        }
    }
}

// ============================================================================
// Rule types
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMetrics {
    pub node_count: usize,
    pub edge_count: usize,
    pub claim_count: usize,
    pub evidence_count: usize,
    pub max_depth: usize,
    pub connectivity: f64,
    pub evidence_coverage: f64,
    pub has_counter: bool,
    pub has_rebuttal: bool,
}

impl Default for GraphMetrics {
    fn default() -> Self {
        Self {
            node_count: 0,
            edge_count: 0,
            claim_count: 0,
            evidence_count: 0,
            max_depth: 0,
            connectivity: 0.0,
            evidence_coverage: 0.0,
            has_counter: false,
            has_rebuttal: false,
        }
    }
}
