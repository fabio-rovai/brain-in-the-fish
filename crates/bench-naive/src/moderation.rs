//! Moderation — trust-weighted consensus scoring with outlier detection.

use crate::types::{Agent, Dissent, Framework, ModeratedScore, Score};

/// The overall evaluation result after moderation.
#[derive(Debug, Clone)]
pub struct OverallResult {
    pub weighted_score: f64,
    pub max_possible: f64,
    pub percentage: f64,
    pub passed: bool,
}

/// Calculate moderated scores: trust-weighted mean with >2sigma outlier detection.
pub fn calculate_moderated_scores(
    scores: &[Score],
    agents: &[Agent],
) -> Vec<ModeratedScore> {
    // Group final-round scores by criterion
    let max_round = scores.iter().map(|s| s.round).max().unwrap_or(0);
    let final_scores: Vec<&Score> = scores.iter().filter(|s| s.round == max_round).collect();

    let mut by_criterion: std::collections::HashMap<&str, Vec<&Score>> =
        std::collections::HashMap::new();
    for s in &final_scores {
        by_criterion
            .entry(s.criterion_id.as_str())
            .or_default()
            .push(s);
    }

    let mut moderated = Vec::new();

    for (crit_id, crit_scores) in &by_criterion {
        if crit_scores.is_empty() {
            continue;
        }

        // Calculate raw mean and std dev
        let raw_scores: Vec<f64> = crit_scores.iter().map(|s| s.score).collect();
        let n = raw_scores.len() as f64;
        let mean = raw_scores.iter().sum::<f64>() / n;
        let variance = raw_scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();

        // Detect outliers (>2 sigma) and record dissents
        let mut dissents = Vec::new();
        let mut weighted_sum = 0.0;
        let mut weight_total = 0.0;

        for s in crit_scores {
            let deviation = (s.score - mean).abs();
            let is_outlier = std_dev > 0.0 && deviation > 2.0 * std_dev;

            if is_outlier {
                dissents.push(Dissent {
                    agent_id: s.agent_id.clone(),
                    score: s.score,
                    reason: format!(
                        "Outlier: {:.1} is {:.1} sigma from mean {:.1}",
                        s.score,
                        deviation / std_dev,
                        mean
                    ),
                });
            }

            // Trust weight (find the agent, use trust average; moderator
            // doesn't score so agents should be non-moderator evaluators)
            let trust = agents
                .iter()
                .find(|a| a.id == s.agent_id)
                .map(|a| {
                    if a.trust_weights.is_empty() {
                        1.0
                    } else {
                        a.trust_weights.iter().map(|t| t.trust_level).sum::<f64>()
                            / a.trust_weights.len() as f64
                    }
                })
                .unwrap_or(1.0);

            // Reduce weight of outliers
            let effective_weight = if is_outlier { trust * 0.3 } else { trust };
            weighted_sum += s.score * effective_weight;
            weight_total += effective_weight;
        }

        let consensus = if weight_total > 0.0 {
            weighted_sum / weight_total
        } else {
            mean
        };

        moderated.push(ModeratedScore {
            criterion_id: crit_id.to_string(),
            consensus_score: consensus,
            panel_mean: mean,
            panel_std_dev: std_dev,
            dissents,
        });
    }

    moderated
}

/// Calculate the overall weighted score from moderated criterion scores.
pub fn calculate_overall_score(
    moderated_scores: &[ModeratedScore],
    framework: &Framework,
) -> OverallResult {
    let mut weighted_score = 0.0;
    let mut max_possible = 0.0;

    for criterion in &framework.criteria {
        if let Some(ms) = moderated_scores
            .iter()
            .find(|m| m.criterion_id == criterion.id)
        {
            weighted_score += ms.consensus_score * criterion.weight;
            max_possible += criterion.max_score * criterion.weight;
        }
    }

    let percentage = if max_possible > 0.0 {
        (weighted_score / max_possible) * 100.0
    } else {
        0.0
    };

    let passed = framework
        .pass_mark
        .map(|pm| percentage >= pm)
        .unwrap_or(true);

    OverallResult {
        weighted_score,
        max_possible,
        percentage,
        passed,
    }
}
