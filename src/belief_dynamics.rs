//! Belief dynamics — how agent mental states evolve during evaluation.
//!
//! Implements Bayesian belief updating: agents update their confidence
//! based on new evidence, debate challenges, and validation signals.
//! Maslow needs become satisfied/unsatisfied based on what the agent finds.

use crate::types::*;
use crate::snn::SNNScore;
use crate::validate::ValidationSignal;

/// Update an agent's Maslow needs based on evaluation findings.
/// This makes the cognitive model DYNAMIC, not static.
pub fn update_needs(
    agent: &mut EvaluatorAgent,
    snn_scores: &[(String, SNNScore)],
    validation_signals: &[ValidationSignal],
    criteria: &[EvaluationCriterion],
) {
    for need in &mut agent.needs {
        match need.need_type {
            MaslowLevel::Physiological => {
                // "Does this meet minimum requirements?"
                // Satisfied if all scores are above 30% of max
                let all_above_min = snn_scores.iter().all(|(cid, score)| {
                    let max = criteria.iter().find(|c| c.id == *cid)
                        .map(|c| c.max_score).unwrap_or(10.0);
                    score.snn_score >= max * 0.3
                });
                need.satisfied = all_above_min;
            }
            MaslowLevel::Safety => {
                // "Is this deliverable? Are risks mitigated?"
                // Satisfied if no error-level validation signals
                let no_errors = !validation_signals.iter()
                    .any(|s| s.severity == crate::validate::Severity::Error);
                let has_structure = validation_signals.iter()
                    .any(|s| s.signal_type == crate::validate::SignalType::StructureCompliance
                        && s.spike_effect > 0.0);
                need.satisfied = no_errors && has_structure;
            }
            MaslowLevel::Belonging => {
                // "Does this fit our organisational culture?"
                // Satisfied if reading level is appropriate
                let reading_ok = validation_signals.iter()
                    .any(|s| s.signal_type == crate::validate::SignalType::ReadingLevel
                        && s.spike_effect >= 0.0);
                need.satisfied = reading_ok;
            }
            MaslowLevel::Esteem => {
                // "Does this demonstrate excellence?"
                // Satisfied if average SNN confidence > 0.7 and >50% scores above 60%
                let avg_confidence = if snn_scores.is_empty() { 0.0 } else {
                    snn_scores.iter().map(|(_, s)| s.confidence).sum::<f64>() / snn_scores.len() as f64
                };
                let high_scores = snn_scores.iter().filter(|(cid, score)| {
                    let max = criteria.iter().find(|c| c.id == *cid)
                        .map(|c| c.max_score).unwrap_or(10.0);
                    score.snn_score >= max * 0.6
                }).count();
                let high_pct = if snn_scores.is_empty() { 0.0 } else {
                    high_scores as f64 / snn_scores.len() as f64
                };
                need.satisfied = avg_confidence > 0.7 && high_pct > 0.5;
            }
            MaslowLevel::SelfActualisation => {
                // "Does this innovate beyond the spec?"
                // Satisfied if evidence quality is strong AND all criteria are covered
                let evidence_strong = validation_signals.iter()
                    .any(|s| s.signal_type == crate::validate::SignalType::EvidenceQuality
                        && s.spike_effect > 0.05);
                let all_grounded = snn_scores.iter().all(|(_, s)| s.grounded);
                need.satisfied = evidence_strong && all_grounded;
            }
        }
    }
}

/// Bayesian confidence update: combine prior belief with new evidence.
/// P(H|E) = P(E|H) * P(H) / P(E)
/// Simplified: new_confidence = prior * likelihood / (prior * likelihood + (1-prior) * (1-likelihood))
pub fn bayesian_update(prior: f64, likelihood: f64) -> f64 {
    let numerator = likelihood * prior;
    let denominator = numerator + (1.0 - likelihood) * (1.0 - prior);
    if denominator < 1e-10 { prior } else { (numerator / denominator).clamp(0.01, 0.99) }
}

/// Compute belief convergence across the panel.
/// Returns a measure of how much agents agree (0=total disagreement, 1=consensus).
pub fn panel_convergence(agents_beliefs: &[Vec<(String, f64)>]) -> f64 {
    if agents_beliefs.is_empty() || agents_beliefs[0].is_empty() { return 1.0; }

    let mut total_variance = 0.0;
    let mut criterion_count = 0;

    // Group by criterion
    let criteria: Vec<&String> = agents_beliefs[0].iter().map(|(c, _)| c).collect();
    for crit in &criteria {
        let scores: Vec<f64> = agents_beliefs.iter()
            .filter_map(|beliefs| beliefs.iter().find(|(c, _)| c == *crit).map(|(_, s)| *s))
            .collect();
        if scores.len() < 2 { continue; }
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / scores.len() as f64;
        total_variance += variance;
        criterion_count += 1;
    }

    if criterion_count == 0 { return 1.0; }
    let avg_variance = total_variance / criterion_count as f64;
    // Convert variance to convergence score (0-1)
    // Low variance = high convergence
    (1.0 / (1.0 + avg_variance * 10.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bayesian_update_strong_evidence() {
        let posterior = bayesian_update(0.5, 0.9);
        assert!(posterior > 0.8, "Strong evidence should increase belief: {}", posterior);
    }

    #[test]
    fn test_bayesian_update_weak_evidence() {
        let posterior = bayesian_update(0.5, 0.3);
        assert!(posterior < 0.4, "Weak evidence should decrease belief: {}", posterior);
    }

    #[test]
    fn test_panel_convergence_agreement() {
        let beliefs = vec![
            vec![("c1".to_string(), 0.8), ("c2".to_string(), 0.7)],
            vec![("c1".to_string(), 0.8), ("c2".to_string(), 0.7)],
        ];
        let conv = panel_convergence(&beliefs);
        assert!(conv > 0.9, "Identical beliefs should show high convergence: {}", conv);
    }

    #[test]
    fn test_panel_convergence_disagreement() {
        let beliefs = vec![
            vec![("c1".to_string(), 0.9), ("c2".to_string(), 0.2)],
            vec![("c1".to_string(), 0.2), ("c2".to_string(), 0.9)],
        ];
        let conv = panel_convergence(&beliefs);
        assert!(conv < 0.5, "Opposite beliefs should show low convergence: {}", conv);
    }

    #[test]
    fn test_update_needs_physiological() {
        let mut agent = EvaluatorAgent {
            id: "a1".into(), name: "Test".into(), role: "Expert".into(), domain: "Test".into(),
            years_experience: None, persona_description: "Test".into(),
            needs: vec![MaslowNeed { need_type: MaslowLevel::Physiological, expression: "Minimum requirements".into(), salience: 0.9, satisfied: false }],
            trust_weights: vec![],
        };
        let criteria = vec![EvaluationCriterion { id: "c1".into(), title: "Quality".into(), description: None, max_score: 10.0, weight: 1.0, rubric_levels: vec![], sub_criteria: vec![] }];
        let snn_scores = vec![("c1".to_string(), crate::snn::SNNScore {
            snn_score: 5.0, confidence: 0.8, firing_rate: 0.4, evidence_count: 5, spike_quality: 0.7, grounded: true, explanation: String::new(),
        })];
        update_needs(&mut agent, &snn_scores, &[], &criteria);
        assert!(agent.needs[0].satisfied, "5/10 > 30% minimum, should be satisfied");
    }
}
