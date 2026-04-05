//! Gate — verifies LLM scores against ontology evidence.
//!
//! Structural score + quality score -> tolerance curve -> verdict.

use crate::argument_graph;
use crate::types::{ArgGraph, DerivedFacts, GateWeights, Verdict};

/// Structural score from graph topology: density + evidence ratio.
pub fn structural_score(graph: &ArgGraph) -> f64 {
    let metrics = argument_graph::compute_metrics(graph);
    if metrics.node_count == 0 {
        return 0.0;
    }

    let density = if metrics.node_count > 1 {
        metrics.edge_count as f64 / (metrics.node_count as f64 * (metrics.node_count as f64 - 1.0))
    } else {
        0.0
    };

    let evidence_ratio = if metrics.claim_count > 0 {
        metrics.evidence_count as f64 / metrics.claim_count as f64
    } else {
        0.0
    };

    // Combine: weighted average of density, evidence_ratio, connectivity, coverage
    let raw = 0.25 * density.min(1.0)
        + 0.30 * evidence_ratio.min(1.0)
        + 0.25 * metrics.connectivity
        + 0.20 * metrics.evidence_coverage;

    raw.clamp(0.0, 1.0)
}

/// Quality score from derived facts.
pub fn quality_score(facts: &DerivedFacts, total_claims: usize) -> f64 {
    if total_claims == 0 {
        return 0.0;
    }
    let supported_frac = facts.supported_claims as f64 / total_claims as f64;
    let strong_frac = facts.strong_claims as f64 / total_claims as f64;
    let thesis_bonus = if facts.evidenced_thesis { 0.15 } else { 0.0 };
    let sophistication_bonus = if facts.sophisticated_arguments > 0 {
        0.10
    } else {
        0.0
    };

    (0.40 * supported_frac + 0.30 * strong_frac + thesis_bonus + sophistication_bonus)
        .clamp(0.0, 1.0)
}

/// Run the gate check: compare LLM score against structural + quality evidence.
pub fn check(
    llm_score: f64,
    max_score: f64,
    graph: &ArgGraph,
    facts: &DerivedFacts,
    total_claims: usize,
    weights: &GateWeights,
) -> Verdict {
    let metrics = argument_graph::compute_metrics(graph);
    let total_nodes = metrics.node_count;
    let evidence_count = metrics.evidence_count;
    let claim_count = metrics.claim_count;
    let normalized_llm = if max_score > 0.0 {
        llm_score / max_score
    } else {
        0.0
    };

    // REJECTED: no nodes
    if total_nodes == 0 || (evidence_count == 0 && claim_count == 0) {
        return Verdict::Rejected {
            reason: "No argument nodes found in knowledge graph.".into(),
        };
    }

    // REJECTED: zero evidence, high score
    if evidence_count == 0 && normalized_llm > 0.3 {
        return Verdict::Rejected {
            reason: format!(
                "{claim_count} claims but 0 evidence. Score {llm_score:.1} has no support."
            ),
        };
    }

    let struct_s = structural_score(graph);
    let qual_s = quality_score(facts, total_claims);
    let evidence_score = 0.5 * struct_s + 0.5 * qual_s;

    // Tolerance curve: tolerance = gate_a * ln(nodes+1) + gate_b
    let base_tolerance =
        (weights.gate_a * (total_nodes as f64 + 1.0).ln() + weights.gate_b).clamp(0.05, 0.30);
    let quality_factor = (qual_s / 0.5).clamp(0.5, 1.0);
    let tolerance = base_tolerance * quality_factor;

    let gap = (normalized_llm - evidence_score).abs();

    if gap <= tolerance {
        Verdict::Confirmed {
            reason: format!(
                "Evidence supports score {llm_score:.1}/{max_score:.0}. \
                 Structural: {ss:.1}, quality: {qs:.1}, combined: {es:.1}/{max_score:.0} (+-{tol:.0}%). \
                 Graph: {total_nodes} nodes ({evidence_count} evidence, {claim_count} claims).",
                ss = struct_s * max_score,
                qs = qual_s * max_score,
                es = evidence_score * max_score,
                tol = tolerance * 100.0,
            ),
        }
    } else {
        let recommended = evidence_score * max_score;
        Verdict::Flagged {
            reason: format!(
                "LLM scored {llm_score:.1}/{max_score:.0} but evidence supports {es:.1}/{max_score:.0}. \
                 Gap {gap_pct:.0}% exceeds +-{tol:.0}%. \
                 Graph: {total_nodes} nodes ({evidence_count} evidence, {claim_count} claims).",
                es = evidence_score * max_score,
                gap_pct = gap * 100.0,
                tol = tolerance * 100.0,
            ),
            recommended_score: recommended,
        }
    }
}
