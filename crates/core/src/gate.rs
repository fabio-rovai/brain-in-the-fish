//! Gate — verifies LLM scores against ontology evidence.
//!
//! The gate compares the LLM's holistic score against structural evidence
//! derived from the OWL knowledge graph. It produces a verdict:
//! CONFIRMED, FLAGGED, or REJECTED.
//!
//! No spike dynamics, no membrane potentials, no refractory periods.
//! Just: structural score + quality score → tolerance curve → verdict.

use serde::{Deserialize, Serialize};
use crate::argument_graph::{self, ArgumentGraph};

/// Gate verdict.
#[derive(Debug, Clone, Serialize)]
pub enum Verdict {
    Confirmed { reason: String },
    Flagged { reason: String, recommended_score: f64 },
    Rejected { reason: String },
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verdict::Confirmed { reason } => write!(f, "CONFIRMED: {}", reason),
            Verdict::Flagged { reason, recommended_score } => {
                write!(f, "FLAGGED: {} (recommended: {:.1})", reason, recommended_score)
            }
            Verdict::Rejected { reason } => write!(f, "REJECTED: {}", reason),
        }
    }
}

/// Gate parameters — learned from data via Nelder-Mead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateWeights {
    /// Tolerance curve slope: tolerance = gate_a * ln(nodes+1) + gate_b
    pub gate_a: f64,
    /// Tolerance curve intercept
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

/// Run the gate: compare LLM score against graph evidence.
pub fn check(
    llm_score: f64,
    max_score: f64,
    graph: &ArgumentGraph,
    weights: &GateWeights,
) -> Verdict {
    let metrics = argument_graph::compute_metrics(graph);
    let normalized_llm = if max_score > 0.0 { llm_score / max_score } else { 0.0 };

    let evidence_count = metrics.evidence_count;
    let claim_count = metrics.claim_count;
    let total_nodes = metrics.node_count;

    // REJECTED: no nodes at all
    if total_nodes == 0 || (evidence_count == 0 && claim_count == 0) {
        return Verdict::Rejected {
            reason: "No argument nodes found in knowledge graph. Cannot verify any claims.".into(),
        };
    }

    // REJECTED: only bare claims, zero evidence, and LLM scored above minimum
    if evidence_count == 0 && normalized_llm > 0.3 {
        return Verdict::Rejected {
            reason: format!(
                "{} claims found but 0 evidence nodes. Score {:.1} has no evidentiary support.",
                claim_count, llm_score
            ),
        };
    }

    // Structural score (from graph topology, LLM-free)
    let struct_score = argument_graph::structural_score(&metrics);
    let struct_scaled = struct_score * max_score;

    // Quality score (from node-level LLM assessments, independent from holistic)
    let node_scores: Vec<f64> = graph.nodes.iter()
        .filter_map(|n| n.llm_score)
        .collect();
    let quality_score = if node_scores.is_empty() {
        None
    } else {
        Some(node_scores.iter().sum::<f64>() / node_scores.len() as f64)
    };

    // Combined evidence score
    let evidence_score = match quality_score {
        Some(q) => 0.5 * struct_score + 0.5 * q,
        None => struct_score,
    };
    let evidence_scaled = evidence_score * max_score;

    // Learned tolerance curve
    let base_tolerance = (weights.gate_a * (total_nodes as f64 + 1.0).ln() + weights.gate_b)
        .clamp(0.05, 0.30);

    // Quality factor: low quality = tighter tolerance
    let quality_factor = match quality_score {
        Some(q) => (q / 0.5).clamp(0.5, 1.0),
        None => 1.0,
    };
    let tolerance = base_tolerance * quality_factor;

    let gap = (normalized_llm - evidence_score).abs();

    if gap <= tolerance {
        Verdict::Confirmed {
            reason: format!(
                "Evidence supports score {:.1}/{:.0}. \
                 Structural: {:.1}, quality: {:.1}, combined: {:.1}/{:.0} (±{:.0}%). \
                 Graph: {} nodes ({} evidence, {} claims), {:.0}% connected, depth {}.",
                llm_score, max_score,
                struct_scaled,
                quality_score.map(|q| q * max_score).unwrap_or(0.0),
                evidence_scaled, max_score, tolerance * 100.0,
                total_nodes, evidence_count, claim_count,
                metrics.connectivity * 100.0, metrics.max_depth
            ),
        }
    } else {
        Verdict::Flagged {
            reason: format!(
                "LLM scored {:.1}/{:.0} but evidence supports {:.1}/{:.0}. \
                 Structural: {:.1}, quality: {:.1}. Gap {:.0}% exceeds ±{:.0}%. \
                 Graph: {} nodes ({} evidence, {} claims), {:.0}% connected, depth {}.",
                llm_score, max_score, evidence_scaled, max_score,
                struct_scaled,
                quality_score.map(|q| q * max_score).unwrap_or(0.0),
                gap * 100.0, tolerance * 100.0,
                total_nodes, evidence_count, claim_count,
                metrics.connectivity * 100.0, metrics.max_depth
            ),
            recommended_score: evidence_scaled,
        }
    }
}
