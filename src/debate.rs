//! Multi-round structured debate orchestrator.
//!
//! Manages the structured debate between evaluator agents — identifying
//! disagreements, generating challenge prompts, tracking score changes,
//! and detecting convergence.

use crate::ingest::{iri_safe, turtle_escape};
use crate::types::*;
use open_ontologies::graph::GraphStore;
use std::collections::HashMap;
use std::fmt::Write;

// ============================================================================
// Disagreement detection
// ============================================================================

/// A disagreement between two agents on a criterion score.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Disagreement {
    pub criterion_id: String,
    pub agent_a_id: String,
    pub agent_a_score: f64,
    pub agent_b_id: String,
    pub agent_b_score: f64,
    pub delta: f64,
}

/// Find disagreements between agents' scores for a given round.
///
/// A disagreement exists when the absolute score delta between any pair
/// of agents on the same criterion exceeds `threshold`.
pub fn find_disagreements(scores: &[Score], threshold: f64) -> Vec<Disagreement> {
    // Group scores by criterion_id
    let mut by_criterion: HashMap<&str, Vec<&Score>> = HashMap::new();
    for score in scores {
        by_criterion
            .entry(&score.criterion_id)
            .or_default()
            .push(score);
    }

    let mut disagreements = Vec::new();

    for (criterion_id, criterion_scores) in &by_criterion {
        // Compare every pair of agents
        for i in 0..criterion_scores.len() {
            for j in (i + 1)..criterion_scores.len() {
                let a = criterion_scores[i];
                let b = criterion_scores[j];
                let delta = (a.score - b.score).abs();
                if delta > threshold {
                    disagreements.push(Disagreement {
                        criterion_id: criterion_id.to_string(),
                        agent_a_id: a.agent_id.clone(),
                        agent_a_score: a.score,
                        agent_b_id: b.agent_id.clone(),
                        agent_b_score: b.score,
                        delta,
                    });
                }
            }
        }
    }

    disagreements
}

// ============================================================================
// Challenge prompt generation
// ============================================================================

/// Generate a challenge prompt for one agent to challenge another's score.
///
/// The challenger argues why the target's score is incorrect, given both
/// justifications and the criterion rubric.
pub fn generate_challenge_prompt(
    challenger: &EvaluatorAgent,
    target: &EvaluatorAgent,
    disagreement: &Disagreement,
    challenger_justification: &str,
    target_justification: &str,
    criterion: &EvaluationCriterion,
) -> String {
    format!(
        r#"You are {challenger_name}, a {challenger_role} with expertise in {challenger_domain}.

## Challenge Task

You disagree with {target_name}'s score of {target_score} for the criterion "{criterion_title}".
Your score was {challenger_score} (delta: {delta}).

**Criterion: {criterion_title}**
{criterion_desc}
Maximum Score: {max_score}

## Your Justification
{challenger_justification}

## {target_name}'s Justification
{target_justification}

## Instructions

Construct a clear, evidence-based argument for why {target_name}'s score of {target_score} is incorrect.
Reference specific evidence, rubric levels, and gaps in their reasoning.
Be respectful but rigorous.

Respond in this JSON format:
{{
    "argument": "<your challenge argument>",
    "suggested_score": <what you think the correct score should be>
}}"#,
        challenger_name = challenger.name,
        challenger_role = challenger.role,
        challenger_domain = challenger.domain,
        target_name = target.name,
        target_score = disagreement.agent_b_score,
        challenger_score = disagreement.agent_a_score,
        delta = disagreement.delta,
        criterion_title = criterion.title,
        criterion_desc = criterion.description.as_deref().unwrap_or(""),
        max_score = criterion.max_score,
    )
}

/// Generate a response prompt for the challenged agent.
///
/// The target agent can either defend their score or adjust it based
/// on the challenger's argument.
pub fn generate_response_prompt(
    target: &EvaluatorAgent,
    challenger: &EvaluatorAgent,
    challenge_argument: &str,
    target_score: &Score,
    criterion: &EvaluationCriterion,
) -> String {
    format!(
        r#"You are {target_name}, a {target_role} with expertise in {target_domain}.

## Challenge Response

{challenger_name} has challenged your score of {score} for the criterion "{criterion_title}".

**Criterion: {criterion_title}**
{criterion_desc}
Maximum Score: {max_score}

## Your Original Justification
{justification}

## {challenger_name}'s Challenge
{challenge_argument}

## Instructions

Consider {challenger_name}'s argument carefully. You may either:
1. **Defend** your score with additional reasoning
2. **Adjust** your score if you find their argument convincing

Be honest and evidence-based. Changing your score is not a weakness — it shows rigorous thinking.

Respond in this JSON format:
{{
    "response": "<your response to the challenge>",
    "revised_score": <your score — same or adjusted>,
    "score_changed": <true or false>
}}"#,
        target_name = target.name,
        target_role = target.role,
        target_domain = target.domain,
        challenger_name = challenger.name,
        score = target_score.score,
        criterion_title = criterion.title,
        criterion_desc = criterion.description.as_deref().unwrap_or(""),
        max_score = criterion.max_score,
        justification = target_score.justification,
    )
}

// ============================================================================
// Challenge RDF recording
// ============================================================================

/// Record a challenge in the graph store as Turtle RDF.
pub fn record_challenge(
    graph: &GraphStore,
    challenge: &Challenge,
) -> anyhow::Result<usize> {
    let turtle = challenge_to_turtle(challenge);
    graph.load_turtle(&turtle, None).map_err(Into::into)
}

/// Generate Turtle RDF for a Challenge.
fn challenge_to_turtle(challenge: &Challenge) -> String {
    let mut out = String::new();

    // Prefixes
    let _ = writeln!(out, "@prefix agent: <http://brain-in-the-fish.dev/agent/> .");
    let _ = writeln!(out, "@prefix eval: <http://brain-in-the-fish.dev/eval/> .");
    let _ = writeln!(out, "@prefix crit: <http://brain-in-the-fish.dev/criteria/> .");
    let _ = writeln!(
        out,
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> ."
    );
    let _ = writeln!(out);

    let challenger_id = iri_safe(&challenge.challenger_id);
    let target_id = iri_safe(&challenge.target_agent_id);
    let criterion_id = iri_safe(&challenge.criterion_id);
    let challenge_node = format!(
        "{}_challenges_{}_on_{}_R{}",
        challenger_id, target_id, criterion_id, challenge.round
    );

    let _ = writeln!(out, "agent:{challenge_node} a eval:Challenge ;");
    let _ = writeln!(out, "    eval:challenger agent:{challenger_id} ;");
    let _ = writeln!(out, "    eval:target agent:{target_id} ;");
    let _ = writeln!(out, "    eval:criterion crit:{criterion_id} ;");
    let _ = writeln!(
        out,
        "    eval:round \"{}\"^^xsd:integer ;",
        challenge.round
    );
    let _ = writeln!(
        out,
        "    eval:argument \"{}\" ;",
        turtle_escape(&challenge.argument)
    );

    if let Some(ref response) = challenge.response {
        let _ = writeln!(
            out,
            "    eval:response \"{}\" ;",
            turtle_escape(response)
        );
    }

    if let Some((from, to)) = &challenge.score_change {
        let _ = writeln!(
            out,
            "    eval:scoreFrom \"{}\"^^xsd:decimal ;",
            from
        );
        let _ = writeln!(
            out,
            "    eval:scoreTo \"{}\"^^xsd:decimal ;",
            to
        );
    }

    // Close the last predicate — replace trailing " ;\n" with " .\n"
    // We always have at least the argument line ending with " ;"
    // Simplest: write a dummy closing triple
    let _ = writeln!(
        out,
        "    eval:hasChallenge \"true\"^^xsd:boolean ."
    );
    let _ = writeln!(out);

    out
}

// ============================================================================
// Drift velocity and convergence
// ============================================================================

/// Calculate drift velocity between two rounds.
///
/// drift_velocity = average absolute score change across all matching
/// (agent_id, criterion_id) pairs between the two rounds.
/// Returns 0.0 if no matching pairs are found.
pub fn calculate_drift_velocity(
    round_a_scores: &[Score],
    round_b_scores: &[Score],
) -> f64 {
    // Index round_b scores by (agent_id, criterion_id)
    let b_map: HashMap<(&str, &str), f64> = round_b_scores
        .iter()
        .map(|s| ((s.agent_id.as_str(), s.criterion_id.as_str()), s.score))
        .collect();

    let mut total_delta = 0.0;
    let mut count = 0u32;

    for a in round_a_scores {
        let key = (a.agent_id.as_str(), a.criterion_id.as_str());
        if let Some(&b_score) = b_map.get(&key) {
            total_delta += (a.score - b_score).abs();
            count += 1;
        }
    }

    if count == 0 {
        0.0
    } else {
        total_delta / count as f64
    }
}

/// Check if the debate has converged.
///
/// Converged when drift_velocity < threshold.
pub fn check_convergence(drift_velocity: f64, threshold: f64) -> bool {
    drift_velocity < threshold
}

// ============================================================================
// DebateRound builder
// ============================================================================

/// Build a DebateRound from the current state.
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

// ============================================================================
// Trust weight updates
// ============================================================================

/// Update trust weights based on challenge outcomes.
///
/// If a challenge led to a score change, increase trust in the challenger.
/// If the target maintained their score, slightly decrease trust.
pub fn update_trust_weights(
    agents: &mut [EvaluatorAgent],
    challenges: &[Challenge],
) {
    for challenge in challenges {
        if challenge.score_change.is_some() {
            // Score changed — challenger was persuasive, increase target's trust in challenger
            if let Some(target) = agents
                .iter_mut()
                .find(|a| a.id == challenge.target_agent_id)
            {
                if let Some(tw) = target
                    .trust_weights
                    .iter_mut()
                    .find(|t| t.target_agent_id == challenge.challenger_id)
                {
                    tw.trust_level = (tw.trust_level + 0.1).min(1.0);
                }
            }
        } else {
            // Score maintained — slight trust decrease
            if let Some(target) = agents
                .iter_mut()
                .find(|a| a.id == challenge.target_agent_id)
            {
                if let Some(tw) = target
                    .trust_weights
                    .iter_mut()
                    .find(|t| t.target_agent_id == challenge.challenger_id)
                {
                    tw.trust_level = (tw.trust_level - 0.05).max(0.0);
                }
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_score(agent_id: &str, criterion_id: &str, score: f64, round: u32) -> Score {
        Score {
            agent_id: agent_id.into(),
            criterion_id: criterion_id.into(),
            score,
            max_score: 10.0,
            round,
            justification: format!("Agent {} scores {} on {}", agent_id, score, criterion_id),
            evidence_used: vec![],
            gaps_identified: vec![],
        }
    }

    fn make_agent(id: &str, name: &str) -> EvaluatorAgent {
        EvaluatorAgent {
            id: id.into(),
            name: name.into(),
            role: "Subject Expert".into(),
            domain: "Economics".into(),
            years_experience: Some(10),
            persona_description: format!("{} is an expert evaluator.", name),
            needs: vec![],
            trust_weights: vec![],
        }
    }

    fn make_agent_with_trust(id: &str, name: &str, trust_targets: Vec<(&str, f64)>) -> EvaluatorAgent {
        let mut agent = make_agent(id, name);
        agent.trust_weights = trust_targets
            .into_iter()
            .map(|(target, level)| TrustRelation {
                target_agent_id: target.into(),
                domain: "general".into(),
                trust_level: level,
            })
            .collect();
        agent
    }

    fn make_criterion(id: &str, title: &str) -> EvaluationCriterion {
        EvaluationCriterion {
            id: id.into(),
            title: title.into(),
            description: Some(format!("Evaluates {}", title)),
            max_score: 10.0,
            weight: 0.4,
            rubric_levels: vec![
                RubricLevel {
                    level: "Excellent".into(),
                    score_range: "9-10".into(),
                    descriptor: "Outstanding".into(),
                },
                RubricLevel {
                    level: "Good".into(),
                    score_range: "7-8".into(),
                    descriptor: "Solid work".into(),
                },
            ],
            sub_criteria: vec![],
        }
    }

    // ========================================================================
    // Disagreement detection
    // ========================================================================

    #[test]
    fn test_find_disagreements() {
        let scores = vec![
            make_score("a1", "c1", 8.0, 1),
            make_score("a2", "c1", 5.0, 1),
            make_score("a3", "c1", 7.0, 1),
        ];
        let disagreements = find_disagreements(&scores, 2.0);
        assert_eq!(disagreements.len(), 1); // a1(8) vs a2(5) = delta 3
        assert_eq!(disagreements[0].delta, 3.0);
    }

    #[test]
    fn test_no_disagreements_when_close() {
        let scores = vec![
            make_score("a1", "c1", 7.0, 1),
            make_score("a2", "c1", 6.0, 1),
        ];
        let disagreements = find_disagreements(&scores, 2.0);
        assert!(disagreements.is_empty()); // delta 1.0 < threshold 2.0
    }

    #[test]
    fn test_disagreements_multiple_criteria() {
        let scores = vec![
            make_score("a1", "c1", 9.0, 1),
            make_score("a2", "c1", 3.0, 1), // delta 6 on c1
            make_score("a1", "c2", 5.0, 1),
            make_score("a2", "c2", 5.5, 1), // delta 0.5 on c2 — no disagreement
        ];
        let disagreements = find_disagreements(&scores, 2.0);
        assert_eq!(disagreements.len(), 1);
        assert_eq!(disagreements[0].criterion_id, "c1");
        assert_eq!(disagreements[0].delta, 6.0);
    }

    #[test]
    fn test_disagreements_three_agents_all_disagree() {
        let scores = vec![
            make_score("a1", "c1", 10.0, 1),
            make_score("a2", "c1", 5.0, 1),
            make_score("a3", "c1", 2.0, 1),
        ];
        let disagreements = find_disagreements(&scores, 2.0);
        // a1 vs a2 = 5, a1 vs a3 = 8, a2 vs a3 = 3 => all 3 pairs disagree
        assert_eq!(disagreements.len(), 3);
    }

    #[test]
    fn test_disagreements_empty_scores() {
        let disagreements = find_disagreements(&[], 2.0);
        assert!(disagreements.is_empty());
    }

    #[test]
    fn test_disagreements_single_agent() {
        let scores = vec![make_score("a1", "c1", 8.0, 1)];
        let disagreements = find_disagreements(&scores, 2.0);
        assert!(disagreements.is_empty());
    }

    // ========================================================================
    // Drift velocity and convergence
    // ========================================================================

    #[test]
    fn test_calculate_drift_velocity() {
        let round1 = vec![
            make_score("a1", "c1", 8.0, 1),
            make_score("a2", "c1", 5.0, 1),
        ];
        let round2 = vec![
            make_score("a1", "c1", 7.0, 2),
            make_score("a2", "c1", 6.0, 2),
        ];
        let drift = calculate_drift_velocity(&round1, &round2);
        assert!((drift - 1.0).abs() < 0.01); // avg(|8-7| + |5-6|) = avg(1+1) = 1.0
    }

    #[test]
    fn test_drift_velocity_no_change() {
        let round1 = vec![
            make_score("a1", "c1", 7.0, 1),
            make_score("a2", "c1", 6.0, 1),
        ];
        let round2 = vec![
            make_score("a1", "c1", 7.0, 2),
            make_score("a2", "c1", 6.0, 2),
        ];
        let drift = calculate_drift_velocity(&round1, &round2);
        assert!((drift).abs() < 0.001);
    }

    #[test]
    fn test_drift_velocity_no_matching_pairs() {
        let round1 = vec![make_score("a1", "c1", 7.0, 1)];
        let round2 = vec![make_score("a2", "c2", 6.0, 2)];
        let drift = calculate_drift_velocity(&round1, &round2);
        assert_eq!(drift, 0.0);
    }

    #[test]
    fn test_drift_velocity_empty() {
        assert_eq!(calculate_drift_velocity(&[], &[]), 0.0);
    }

    #[test]
    fn test_convergence() {
        assert!(check_convergence(0.3, 0.5));
        assert!(!check_convergence(0.7, 0.5));
        assert!(!check_convergence(0.5, 0.5)); // equal => not converged
        assert!(check_convergence(0.0, 0.5));
    }

    // ========================================================================
    // Trust weight updates
    // ========================================================================

    #[test]
    fn test_update_trust_weights_score_changed() {
        let mut agents = vec![
            make_agent_with_trust("target1", "Dr. Torres", vec![("challenger1", 0.5)]),
            make_agent("challenger1", "Prof. Chen"),
        ];

        let challenges = vec![Challenge {
            challenger_id: "challenger1".into(),
            target_agent_id: "target1".into(),
            criterion_id: "c1".into(),
            round: 1,
            argument: "Your score is too high.".into(),
            response: Some("I agree, revising.".into()),
            score_change: Some((8.0, 7.0)),
        }];

        update_trust_weights(&mut agents, &challenges);

        let target = &agents[0];
        let tw = target
            .trust_weights
            .iter()
            .find(|t| t.target_agent_id == "challenger1")
            .unwrap();
        assert!(
            (tw.trust_level - 0.6).abs() < 0.001,
            "Trust should increase from 0.5 to 0.6, got {}",
            tw.trust_level
        );
    }

    #[test]
    fn test_update_trust_weights_score_maintained() {
        let mut agents = vec![
            make_agent_with_trust("target1", "Dr. Torres", vec![("challenger1", 0.5)]),
            make_agent("challenger1", "Prof. Chen"),
        ];

        let challenges = vec![Challenge {
            challenger_id: "challenger1".into(),
            target_agent_id: "target1".into(),
            criterion_id: "c1".into(),
            round: 1,
            argument: "Your score is too high.".into(),
            response: Some("I disagree, maintaining.".into()),
            score_change: None,
        }];

        update_trust_weights(&mut agents, &challenges);

        let target = &agents[0];
        let tw = target
            .trust_weights
            .iter()
            .find(|t| t.target_agent_id == "challenger1")
            .unwrap();
        assert!(
            (tw.trust_level - 0.45).abs() < 0.001,
            "Trust should decrease from 0.5 to 0.45, got {}",
            tw.trust_level
        );
    }

    #[test]
    fn test_update_trust_weights_clamp_max() {
        let mut agents = vec![
            make_agent_with_trust("target1", "Dr. Torres", vec![("challenger1", 0.95)]),
            make_agent("challenger1", "Prof. Chen"),
        ];

        let challenges = vec![Challenge {
            challenger_id: "challenger1".into(),
            target_agent_id: "target1".into(),
            criterion_id: "c1".into(),
            round: 1,
            argument: "Challenge".into(),
            response: None,
            score_change: Some((8.0, 7.0)),
        }];

        update_trust_weights(&mut agents, &challenges);

        let tw = agents[0]
            .trust_weights
            .iter()
            .find(|t| t.target_agent_id == "challenger1")
            .unwrap();
        assert!(
            (tw.trust_level - 1.0).abs() < 0.001,
            "Trust should be clamped to 1.0, got {}",
            tw.trust_level
        );
    }

    #[test]
    fn test_update_trust_weights_clamp_min() {
        let mut agents = vec![
            make_agent_with_trust("target1", "Dr. Torres", vec![("challenger1", 0.02)]),
            make_agent("challenger1", "Prof. Chen"),
        ];

        let challenges = vec![Challenge {
            challenger_id: "challenger1".into(),
            target_agent_id: "target1".into(),
            criterion_id: "c1".into(),
            round: 1,
            argument: "Challenge".into(),
            response: None,
            score_change: None,
        }];

        update_trust_weights(&mut agents, &challenges);

        let tw = agents[0]
            .trust_weights
            .iter()
            .find(|t| t.target_agent_id == "challenger1")
            .unwrap();
        assert!(
            tw.trust_level.abs() < 0.001,
            "Trust should be clamped to 0.0, got {}",
            tw.trust_level
        );
    }

    #[test]
    fn test_update_trust_no_matching_trust_relation() {
        // If the target has no trust relation for the challenger, nothing happens (no panic)
        let mut agents = vec![
            make_agent("target1", "Dr. Torres"), // no trust_weights
            make_agent("challenger1", "Prof. Chen"),
        ];

        let challenges = vec![Challenge {
            challenger_id: "challenger1".into(),
            target_agent_id: "target1".into(),
            criterion_id: "c1".into(),
            round: 1,
            argument: "Challenge".into(),
            response: None,
            score_change: Some((8.0, 7.0)),
        }];

        // Should not panic
        update_trust_weights(&mut agents, &challenges);
    }

    // ========================================================================
    // Challenge prompt generation
    // ========================================================================

    #[test]
    fn test_generate_challenge_prompt() {
        let challenger = make_agent("a1", "Dr. Torres");
        let target = make_agent("a2", "Prof. Chen");
        let disagreement = Disagreement {
            criterion_id: "c1".into(),
            agent_a_id: "a1".into(),
            agent_a_score: 8.0,
            agent_b_id: "a2".into(),
            agent_b_score: 5.0,
            delta: 3.0,
        };
        let criterion = make_criterion("c1", "Knowledge and Understanding");

        let prompt = generate_challenge_prompt(
            &challenger,
            &target,
            &disagreement,
            "Strong evidence supports a high score.",
            "Insufficient detail in the document.",
            &criterion,
        );

        assert!(prompt.contains("Dr. Torres"), "Should contain challenger name");
        assert!(prompt.contains("Prof. Chen"), "Should contain target name");
        assert!(prompt.contains("5"), "Should contain target score");
        assert!(prompt.contains("8"), "Should contain challenger score");
        assert!(
            prompt.contains("Knowledge and Understanding"),
            "Should contain criterion title"
        );
        assert!(
            prompt.contains("Strong evidence"),
            "Should contain challenger justification"
        );
        assert!(
            prompt.contains("Insufficient detail"),
            "Should contain target justification"
        );
    }

    #[test]
    fn test_generate_response_prompt() {
        let target = make_agent("a2", "Prof. Chen");
        let challenger = make_agent("a1", "Dr. Torres");
        let score = make_score("a2", "c1", 5.0, 1);
        let criterion = make_criterion("c1", "Knowledge and Understanding");

        let prompt = generate_response_prompt(
            &target,
            &challenger,
            "The document clearly demonstrates expertise beyond what a 5 warrants.",
            &score,
            &criterion,
        );

        assert!(prompt.contains("Prof. Chen"), "Should contain target name");
        assert!(prompt.contains("Dr. Torres"), "Should contain challenger name");
        assert!(prompt.contains("5"), "Should contain original score");
        assert!(
            prompt.contains("Knowledge and Understanding"),
            "Should contain criterion title"
        );
        assert!(
            prompt.contains("clearly demonstrates expertise"),
            "Should contain challenge argument"
        );
    }

    // ========================================================================
    // Challenge RDF recording
    // ========================================================================

    #[test]
    fn test_record_challenge() {
        let graph = GraphStore::new();
        let challenge = Challenge {
            challenger_id: "agent-1".into(),
            target_agent_id: "agent-2".into(),
            criterion_id: "crit-1".into(),
            round: 1,
            argument: "The score is too low given the strong evidence.".into(),
            response: Some("I accept the argument and will revise.".into()),
            score_change: Some((5.0, 7.0)),
        };

        let triples = record_challenge(&graph, &challenge).unwrap();
        assert!(triples > 0, "Should insert triples, got {}", triples);
    }

    #[test]
    fn test_record_challenge_without_response() {
        let graph = GraphStore::new();
        let challenge = Challenge {
            challenger_id: "agent-1".into(),
            target_agent_id: "agent-2".into(),
            criterion_id: "crit-1".into(),
            round: 2,
            argument: "Missing key evidence.".into(),
            response: None,
            score_change: None,
        };

        let triples = record_challenge(&graph, &challenge).unwrap();
        assert!(triples > 0, "Should insert triples even without response");
    }

    #[test]
    fn test_challenge_to_turtle_content() {
        let challenge = Challenge {
            challenger_id: "agent-1".into(),
            target_agent_id: "agent-2".into(),
            criterion_id: "crit-1".into(),
            round: 1,
            argument: "Evidence is weak.".into(),
            response: Some("Noted.".into()),
            score_change: Some((8.0, 7.0)),
        };

        let turtle = challenge_to_turtle(&challenge);
        assert!(turtle.contains("eval:Challenge"));
        assert!(turtle.contains("eval:challenger agent:agent_1"));
        assert!(turtle.contains("eval:target agent:agent_2"));
        assert!(turtle.contains("eval:criterion crit:crit_1"));
        assert!(turtle.contains("eval:argument"));
        assert!(turtle.contains("eval:response"));
        assert!(turtle.contains("eval:scoreFrom"));
        assert!(turtle.contains("eval:scoreTo"));
    }

    // ========================================================================
    // DebateRound builder
    // ========================================================================

    #[test]
    fn test_build_debate_round() {
        let scores = vec![make_score("a1", "c1", 7.0, 1)];
        let challenges = vec![Challenge {
            challenger_id: "a2".into(),
            target_agent_id: "a1".into(),
            criterion_id: "c1".into(),
            round: 1,
            argument: "Too high.".into(),
            response: None,
            score_change: None,
        }];

        let round = build_debate_round(1, scores.clone(), challenges.clone(), Some(1.5), false);

        assert_eq!(round.round_number, 1);
        assert_eq!(round.scores.len(), 1);
        assert_eq!(round.challenges.len(), 1);
        assert_eq!(round.drift_velocity, Some(1.5));
        assert!(!round.converged);
    }

    #[test]
    fn test_build_debate_round_converged() {
        let round = build_debate_round(3, vec![], vec![], Some(0.2), true);
        assert!(round.converged);
        assert_eq!(round.round_number, 3);
    }
}
