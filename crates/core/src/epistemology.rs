//! Epistemological framework for agent knowledge and belief.
//!
//! Models HOW agents know things, not just WHAT they score. Each claim
//! an agent makes about a document is grounded in an epistemic justification:
//!
//! - **Empirical**: "I scored this because I observed evidence X in section Y"
//! - **Rational**: "I scored this because criterion A logically implies requirement B"
//! - **Normative**: "I scored this because rubric level 3 defines this standard"
//! - **Testimonial**: "I adjusted my score because Agent B provided a compelling argument"
//! - **Coherentist**: "This score is consistent with my scores on related criteria"

use crate::types::*;
use serde::Serialize;

/// How an agent justifies a knowledge claim.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum EpistemicBasis {
    /// Based on observed evidence in the document
    Empirical { evidence_ids: Vec<String>, observation: String },
    /// Based on logical inference from criteria/rules
    Rational { premise: String, inference: String, conclusion: String },
    /// Based on comparison against a rubric or standard
    Normative { rubric_level: String, standard: String, comparison: String },
    /// Based on another agent's testimony (during debate)
    Testimonial { source_agent: String, argument: String, trust_level: f64 },
    /// Based on coherence with other beliefs
    Coherentist { related_scores: Vec<(String, f64)>, reasoning: String },
}

/// A justified belief held by an agent.
#[derive(Debug, Clone, Serialize)]
pub struct JustifiedBelief {
    pub agent_id: String,
    pub criterion_id: String,
    pub belief: String,           // "This section demonstrates strong knowledge"
    pub confidence: f64,          // 0.0-1.0
    pub basis: EpistemicBasis,
    pub defeaters: Vec<String>,   // reasons this belief might be wrong
    pub timestamp: u32,           // when this belief was formed (debate round)
}

/// Track and update an agent's epistemic state across rounds.
#[derive(Debug, Clone, Serialize)]
pub struct EpistemicState {
    pub agent_id: String,
    pub beliefs: Vec<JustifiedBelief>,
    pub revision_history: Vec<BeliefRevision>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BeliefRevision {
    pub round: u32,
    pub criterion_id: String,
    pub old_belief: String,
    pub new_belief: String,
    pub reason: String,
    pub basis: EpistemicBasis,
}

impl EpistemicState {
    pub fn new(agent_id: &str) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            beliefs: Vec::new(),
            revision_history: Vec::new(),
        }
    }

    /// Form a belief based on evidence found in the document.
    pub fn form_empirical_belief(
        &mut self,
        criterion_id: &str,
        evidence_ids: Vec<String>,
        observation: &str,
        confidence: f64,
    ) {
        self.beliefs.push(JustifiedBelief {
            agent_id: self.agent_id.clone(),
            criterion_id: criterion_id.to_string(),
            belief: observation.to_string(),
            confidence,
            basis: EpistemicBasis::Empirical {
                evidence_ids,
                observation: observation.to_string(),
            },
            defeaters: Vec::new(),
            timestamp: 0,
        });
    }

    /// Form a belief based on rubric comparison.
    pub fn form_normative_belief(
        &mut self,
        criterion_id: &str,
        rubric_level: &str,
        standard: &str,
        comparison: &str,
        confidence: f64,
    ) {
        self.beliefs.push(JustifiedBelief {
            agent_id: self.agent_id.clone(),
            criterion_id: criterion_id.to_string(),
            belief: comparison.to_string(),
            confidence,
            basis: EpistemicBasis::Normative {
                rubric_level: rubric_level.to_string(),
                standard: standard.to_string(),
                comparison: comparison.to_string(),
            },
            defeaters: Vec::new(),
            timestamp: 0,
        });
    }

    /// Revise a belief based on new evidence (from debate).
    pub fn revise_belief(
        &mut self,
        criterion_id: &str,
        new_belief: &str,
        reason: &str,
        basis: EpistemicBasis,
        round: u32,
    ) {
        let old = self.beliefs.iter()
            .find(|b| b.criterion_id == criterion_id)
            .map(|b| b.belief.clone())
            .unwrap_or_else(|| "no prior belief".to_string());

        self.revision_history.push(BeliefRevision {
            round,
            criterion_id: criterion_id.to_string(),
            old_belief: old,
            new_belief: new_belief.to_string(),
            reason: reason.to_string(),
            basis: basis.clone(),
        });

        // Update or add the belief
        if let Some(existing) = self.beliefs.iter_mut().find(|b| b.criterion_id == criterion_id) {
            existing.belief = new_belief.to_string();
            existing.basis = basis;
            existing.timestamp = round;
        } else {
            self.beliefs.push(JustifiedBelief {
                agent_id: self.agent_id.clone(),
                criterion_id: criterion_id.to_string(),
                belief: new_belief.to_string(),
                confidence: 0.5,
                basis,
                defeaters: Vec::new(),
                timestamp: round,
            });
        }
    }

    /// Add a defeater to an existing belief.
    pub fn add_defeater(&mut self, criterion_id: &str, defeater: &str) {
        if let Some(belief) = self.beliefs.iter_mut().find(|b| b.criterion_id == criterion_id) {
            belief.defeaters.push(defeater.to_string());
            // Reduce confidence proportionally to defeaters
            belief.confidence *= 0.85; // Each defeater reduces confidence by 15%
        }
    }

    /// Get belief confidence for a criterion.
    pub fn confidence_for(&self, criterion_id: &str) -> f64 {
        self.beliefs.iter()
            .find(|b| b.criterion_id == criterion_id)
            .map(|b| b.confidence)
            .unwrap_or(0.0)
    }
}

/// Generate epistemic beliefs from SNN scoring data.
/// This creates the epistemic grounding for why each score exists.
pub fn beliefs_from_snn(
    agent: &EvaluatorAgent,
    snn_scores: &[(String, crate::snn::SNNScore)],
    criteria: &[EvaluationCriterion],
    validation_signals: &[crate::validate::ValidationSignal],
) -> EpistemicState {
    let mut state = EpistemicState::new(&agent.id);

    for (criterion_id, snn_score) in snn_scores {
        let criterion = criteria.iter().find(|c| c.id == *criterion_id);
        let crit_title = criterion.map(|c| c.title.as_str()).unwrap_or("unknown");

        // Empirical belief from evidence spikes
        if snn_score.evidence_count > 0 {
            state.form_empirical_belief(
                criterion_id,
                Vec::new(), // SNN doesn't track individual IDs at this level
                &format!(
                    "Observed {} evidence items for '{}'. Spike quality: {:.0}%.",
                    snn_score.evidence_count, crit_title,
                    snn_score.spike_quality * 100.0
                ),
                snn_score.confidence,
            );
        }

        // Normative belief from rubric comparison
        if let Some(crit) = criterion
            && !crit.rubric_levels.is_empty() {
                let score_pct = snn_score.snn_score / crit.max_score;
                let matching_level = crit.rubric_levels.iter()
                    .find(|r| {
                        let range = parse_score_range(&r.score_range, crit.max_score);
                        snn_score.snn_score >= range.0 && snn_score.snn_score <= range.1
                    })
                    .or_else(|| crit.rubric_levels.last());

                if let Some(level) = matching_level {
                    state.form_normative_belief(
                        criterion_id,
                        &level.level,
                        &level.descriptor,
                        &format!(
                            "Score {:.1}/{:.0} ({:.0}%) maps to {} level: '{}'",
                            snn_score.snn_score, crit.max_score, score_pct * 100.0,
                            level.level, level.descriptor
                        ),
                        snn_score.confidence,
                    );
                }
            }

        // Add defeaters from validation signals
        for signal in validation_signals {
            if signal.spike_effect < -0.05
                && (signal.criterion_id.as_deref() == Some(criterion_id.as_str()) || signal.criterion_id.is_none()) {
                state.add_defeater(criterion_id, &signal.title);
            }
        }
    }

    state
}

fn parse_score_range(range: &str, max: f64) -> (f64, f64) {
    let parts: Vec<&str> = range.split('-').collect();
    if parts.len() == 2 {
        let low = parts[0].trim().parse::<f64>().unwrap_or(0.0);
        let high = parts[1].trim().parse::<f64>().unwrap_or(max);
        (low, high)
    } else {
        (0.0, max)
    }
}

/// Convert epistemic state to Turtle for the knowledge graph.
pub fn epistemic_state_to_turtle(state: &EpistemicState) -> String {
    use crate::ingest::{iri_safe, turtle_escape};

    let mut t = String::from(
        "@prefix epist: <http://brain-in-the-fish.dev/epistemology/> .\n\
         @prefix agent: <http://brain-in-the-fish.dev/agent/> .\n\
         @prefix eval: <http://brain-in-the-fish.dev/eval/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n"
    );

    for belief in &state.beliefs {
        let bid = iri_safe(&format!("{}_{}", belief.agent_id, belief.criterion_id));
        let basis_type = match &belief.basis {
            EpistemicBasis::Empirical { .. } => "Empirical",
            EpistemicBasis::Rational { .. } => "Rational",
            EpistemicBasis::Normative { .. } => "Normative",
            EpistemicBasis::Testimonial { .. } => "Testimonial",
            EpistemicBasis::Coherentist { .. } => "Coherentist",
        };
        t.push_str(&format!(
            "epist:{} a epist:JustifiedBelief ;\n\
             \tepist:agent agent:{} ;\n\
             \tepist:belief \"{}\" ;\n\
             \tepist:confidence \"{}\"^^xsd:decimal ;\n\
             \tepist:basis \"{}\" ;\n\
             \tepist:defeaterCount \"{}\"^^xsd:integer .\n\n",
            bid,
            iri_safe(&belief.agent_id),
            turtle_escape(&belief.belief),
            belief.confidence,
            basis_type,
            belief.defeaters.len(),
        ));
    }

    for rev in &state.revision_history {
        let rid = iri_safe(&format!("rev_{}_{}_R{}", state.agent_id, rev.criterion_id, rev.round));
        t.push_str(&format!(
            "epist:{} a epist:BeliefRevision ;\n\
             \tepist:round \"{}\"^^xsd:integer ;\n\
             \tepist:oldBelief \"{}\" ;\n\
             \tepist:newBelief \"{}\" ;\n\
             \tepist:reason \"{}\" .\n\n",
            rid, rev.round,
            turtle_escape(&rev.old_belief),
            turtle_escape(&rev.new_belief),
            turtle_escape(&rev.reason),
        ));
    }

    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_form_empirical_belief() {
        let mut state = EpistemicState::new("agent-1");
        state.form_empirical_belief("c1", vec!["ev1".into()], "Strong evidence found", 0.8);
        assert_eq!(state.beliefs.len(), 1);
        assert!(matches!(state.beliefs[0].basis, EpistemicBasis::Empirical { .. }));
    }

    #[test]
    fn test_revise_belief() {
        let mut state = EpistemicState::new("agent-1");
        state.form_empirical_belief("c1", vec![], "Initial belief", 0.8);
        state.revise_belief("c1", "Revised belief", "New evidence from debate",
            EpistemicBasis::Testimonial { source_agent: "agent-2".into(), argument: "Compelling".into(), trust_level: 0.7 },
            2,
        );
        assert_eq!(state.beliefs[0].belief, "Revised belief");
        assert_eq!(state.revision_history.len(), 1);
    }

    #[test]
    fn test_defeater_reduces_confidence() {
        let mut state = EpistemicState::new("agent-1");
        state.form_empirical_belief("c1", vec![], "Belief", 1.0);
        state.add_defeater("c1", "Counter-evidence found");
        assert!(state.beliefs[0].confidence < 1.0);
        state.add_defeater("c1", "Another counter-evidence");
        assert!(state.beliefs[0].confidence < 0.85);
    }

    #[test]
    fn test_epistemic_turtle() {
        let mut state = EpistemicState::new("agent-1");
        state.form_empirical_belief("c1", vec!["ev1".into()], "Found evidence", 0.8);
        let turtle = epistemic_state_to_turtle(&state);
        assert!(turtle.contains("epist:JustifiedBelief"));
        assert!(turtle.contains("Empirical"));
    }
}
