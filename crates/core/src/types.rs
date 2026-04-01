//! Core evaluation domain types.
//!
//! These types represent the data structures that flow through the evaluation
//! pipeline. They are Rust structs that serialize to/from JSON (for MCP tool
//! responses) and can be converted to Turtle by other modules.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Document Ontology types
// ============================================================================

/// A document being evaluated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalDocument {
    pub id: String,
    pub title: String,
    pub doc_type: String,
    pub total_pages: Option<u32>,
    pub total_word_count: Option<u32>,
    pub sections: Vec<Section>,
}

impl EvalDocument {
    pub fn new(title: String, doc_type: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            doc_type,
            total_pages: None,
            total_word_count: None,
            sections: Vec::new(),
        }
    }
}

/// A section of the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// A claim made in the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub text: String,
    /// 0.0–1.0
    pub specificity: f64,
    pub verifiable: bool,
}

/// Evidence cited in the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub source: String,
    /// "case_study", "statistic", "citation", "primary_data"
    pub evidence_type: String,
    pub text: String,
    pub has_quantified_outcome: bool,
}

// ============================================================================
// Tender Structure types (multi-lot ingestion)
// ============================================================================

/// Structure of a multi-lot tender folder after ingestion.
///
/// Documents are classified as either shared (applying to all lots) or
/// belonging to a specific lot. Detection is based on folder names and
/// filename patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenderStructure {
    /// Documents that apply to all lots (ITT, T&Cs, specifications, etc.).
    pub shared_docs: Vec<EvalDocument>,
    /// Per-lot documents keyed by normalised lot identifier (e.g. "lot_1").
    pub lots: std::collections::HashMap<String, Vec<EvalDocument>>,
    /// Ordered list of lot names as detected from the folder/file names.
    pub lot_names: Vec<String>,
}

// ============================================================================
// Criteria Ontology types
// ============================================================================

/// An evaluation framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationFramework {
    pub id: String,
    pub name: String,
    pub total_weight: f64,
    pub pass_mark: Option<f64>,
    pub criteria: Vec<EvaluationCriterion>,
}

impl EvaluationFramework {
    pub fn new(name: String, total_weight: f64) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            total_weight,
            pass_mark: None,
            criteria: Vec::new(),
        }
    }
}

/// A single criterion to evaluate against.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationCriterion {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub max_score: f64,
    pub weight: f64,
    pub rubric_levels: Vec<RubricLevel>,
    pub sub_criteria: Vec<EvaluationCriterion>,
}

/// A rubric level descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricLevel {
    /// e.g. "Excellent", "Good", "Poor", "Level 4"
    pub level: String,
    /// e.g. "9-10", "7-8"
    pub score_range: String,
    pub descriptor: String,
}

// ============================================================================
// Agent Ontology types
// ============================================================================

/// An evaluator agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatorAgent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub domain: String,
    pub years_experience: Option<u32>,
    pub persona_description: String,
    pub needs: Vec<MaslowNeed>,
    pub trust_weights: Vec<TrustRelation>,
}

/// A Maslow need and how it manifests in evaluation context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaslowNeed {
    pub need_type: MaslowLevel,
    /// How this need manifests in evaluation context.
    pub expression: String,
    /// 0.0–1.0 how important.
    pub salience: f64,
    pub satisfied: bool,
}

/// Maslow need hierarchy levels applied to document evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MaslowLevel {
    /// "Does this meet minimum requirements?"
    Physiological,
    /// "Is this deliverable? Risks mitigated?"
    Safety,
    /// "Does this fit our culture?"
    Belonging,
    /// "Does this demonstrate excellence?"
    Esteem,
    /// "Does this innovate beyond the spec?"
    SelfActualisation,
}

/// Trust relationship between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustRelation {
    pub target_agent_id: String,
    /// e.g. "technical", "compliance", "social value"
    pub domain: String,
    /// 0.0–1.0
    pub trust_level: f64,
}

// ============================================================================
// Scoring types
// ============================================================================

/// A score given by an agent for a criterion.
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

/// A challenge from one agent to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    pub challenger_id: String,
    pub target_agent_id: String,
    pub criterion_id: String,
    pub round: u32,
    pub argument: String,
    pub response: Option<String>,
    /// (from, to) score change.
    pub score_change: Option<(f64, f64)>,
}

/// Final moderated score for a criterion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeratedScore {
    pub criterion_id: String,
    pub consensus_score: f64,
    pub panel_mean: f64,
    pub panel_std_dev: f64,
    pub dissents: Vec<Dissent>,
}

/// A dissenting opinion from an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dissent {
    pub agent_id: String,
    pub score: f64,
    pub reason: String,
}

// ============================================================================
// Alignment types
// ============================================================================

/// A mapping between a document section and a criterion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentMapping {
    pub section_id: String,
    pub criterion_id: String,
    /// 0.0–1.0
    pub confidence: f64,
}

/// A gap where no document content maps to a criterion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    pub criterion_id: String,
    pub criterion_title: String,
    pub best_partial_match: Option<AlignmentMapping>,
}

// ============================================================================
// Evaluation session
// ============================================================================

/// The complete evaluation state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationSession {
    pub id: String,
    pub document: EvalDocument,
    pub framework: EvaluationFramework,
    pub agents: Vec<EvaluatorAgent>,
    pub alignments: Vec<AlignmentMapping>,
    pub gaps: Vec<Gap>,
    pub rounds: Vec<DebateRound>,
    pub final_scores: Vec<ModeratedScore>,
    /// ISO 8601
    pub created_at: String,
}

impl EvaluationSession {
    pub fn new(
        document: EvalDocument,
        framework: EvaluationFramework,
        agents: Vec<EvaluatorAgent>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            document,
            framework,
            agents,
            alignments: Vec::new(),
            gaps: Vec::new(),
            rounds: Vec::new(),
            final_scores: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// A single round of debate between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateRound {
    pub round_number: u32,
    pub scores: Vec<Score>,
    pub challenges: Vec<Challenge>,
    pub drift_velocity: Option<f64>,
    pub converged: bool,
}
