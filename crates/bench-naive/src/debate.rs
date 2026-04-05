//! Debate — disagreement detection, challenge generation, convergence checking.

use crate::types::{Agent, Challenge, Criterion, DebateRound, Score};

/// A disagreement between two agents on a criterion.
#[derive(Debug, Clone)]
pub struct Disagreement {
    pub agent_a_id: String,
    pub agent_b_id: String,
    pub criterion_id: String,
    pub score_a: f64,
    pub score_b: f64,
    pub delta: f64,
}

/// Find disagreements where score difference exceeds the threshold.
pub fn find_disagreements(scores: &[Score], threshold: f64) -> Vec<Disagreement> {
    let mut disagreements = Vec::new();

    // Group scores by criterion
    let mut by_criterion: std::collections::HashMap<&str, Vec<&Score>> =
        std::collections::HashMap::new();
    for s in scores {
        by_criterion
            .entry(s.criterion_id.as_str())
            .or_default()
            .push(s);
    }

    for (_crit_id, crit_scores) in &by_criterion {
        for (i, a) in crit_scores.iter().enumerate() {
            for b in crit_scores.iter().skip(i + 1) {
                let delta = (a.score - b.score).abs();
                if delta >= threshold {
                    disagreements.push(Disagreement {
                        agent_a_id: a.agent_id.clone(),
                        agent_b_id: b.agent_id.clone(),
                        criterion_id: a.criterion_id.clone(),
                        score_a: a.score,
                        score_b: b.score,
                        delta,
                    });
                }
            }
        }
    }

    disagreements
}

/// Generate a challenge prompt from one agent to another.
pub fn generate_challenge_prompt(
    challenger: &Agent,
    target: &Agent,
    disagreement: &Disagreement,
    criterion: &Criterion,
) -> String {
    format!(
        "You are {challenger_name} challenging {target_name}'s score on \"{crit}\".\n\
         \n\
         Your score: {score_c:.1}/{max}\n\
         Their score: {score_t:.1}/{max}\n\
         Difference: {delta:.1}\n\
         \n\
         Explain why your assessment differs. Reference specific evidence or gaps \
         that justify your score. Be constructive but rigorous.\n",
        challenger_name = challenger.name,
        target_name = target.name,
        crit = criterion.title,
        score_c = if disagreement.agent_a_id == challenger.id {
            disagreement.score_a
        } else {
            disagreement.score_b
        },
        score_t = if disagreement.agent_a_id == target.id {
            disagreement.score_a
        } else {
            disagreement.score_b
        },
        max = criterion.max_score,
        delta = disagreement.delta,
    )
}

/// Generate a response prompt for the target of a challenge.
pub fn generate_response_prompt(
    target: &Agent,
    challenger: &Agent,
    challenge_argument: &str,
    score: &Score,
    criterion: &Criterion,
) -> String {
    format!(
        "You are {target_name} responding to {challenger_name}'s challenge on \"{crit}\".\n\
         \n\
         Your original score: {s:.1}/{max}\n\
         \n\
         Their challenge:\n{challenge}\n\
         \n\
         You may:\n\
         1. Maintain your score with stronger justification\n\
         2. Adjust your score if the challenge raises valid points\n\
         3. Partially adjust with explanation\n",
        target_name = target.name,
        challenger_name = challenger.name,
        crit = criterion.title,
        s = score.score,
        max = criterion.max_score,
        challenge = challenge_argument,
    )
}

/// Calculate drift velocity between two rounds of scores.
///
/// Drift = mean absolute score change per agent-criterion pair.
pub fn calculate_drift_velocity(round_a: &[Score], round_b: &[Score]) -> f64 {
    if round_a.is_empty() || round_b.is_empty() {
        return f64::MAX;
    }

    let mut total_drift = 0.0;
    let mut count = 0u32;

    for sa in round_a {
        if let Some(sb) = round_b.iter().find(|sb| {
            sb.agent_id == sa.agent_id && sb.criterion_id == sa.criterion_id
        }) {
            total_drift += (sa.score - sb.score).abs();
            count += 1;
        }
    }

    if count == 0 {
        f64::MAX
    } else {
        total_drift / count as f64
    }
}

/// Check if the debate has converged (drift below threshold).
pub fn check_convergence(drift_velocity: f64, threshold: f64) -> bool {
    drift_velocity <= threshold
}

/// Update trust weights based on challenge outcomes.
///
/// Agents who successfully challenge (score_change happened) gain trust;
/// agents whose challenges were rejected lose a small amount.
pub fn update_trust_weights(agents: &mut [Agent], challenges: &[Challenge]) {
    for challenge in challenges {
        let gained = challenge.score_change.is_some();
        for agent in agents.iter_mut() {
            for tw in agent.trust_weights.iter_mut() {
                if tw.target_agent_id == challenge.challenger_id && gained {
                    // Challenger was effective — slight trust increase
                    tw.trust_level = (tw.trust_level + 0.05).min(1.0);
                }
                if tw.target_agent_id == challenge.challenger_id && !gained {
                    // Challenger's argument was rejected — slight trust decrease
                    tw.trust_level = (tw.trust_level - 0.02).max(0.1);
                }
            }
        }
    }
}

/// Build a DebateRound struct from its components.
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
