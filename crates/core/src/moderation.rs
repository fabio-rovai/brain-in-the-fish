//! Consensus moderation and convergence detection.
//!
//! After debate rounds converge, the moderator agent synthesises final scores
//! using trust-weighted means, outlier detection, and dissent recording.

use std::collections::HashMap;

use crate::types::*;

/// Overall evaluation result after moderation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverallResult {
    pub total_score: f64,
    pub max_possible: f64,
    pub percentage: f64,
    pub pass_mark: Option<f64>,
    /// None if no pass mark defined.
    pub passed: Option<bool>,
    /// Criteria scoring highest (by weighted percentage).
    pub top_strengths: Vec<String>,
    /// Criteria scoring lowest (by weighted percentage).
    pub top_weaknesses: Vec<String>,
}

/// Calculate moderated scores from the final round of agent scores.
///
/// Groups scores by criterion, computes trust-weighted consensus, panel
/// statistics, and identifies outlier dissents (> 2 std dev from mean).
pub fn calculate_moderated_scores(
    scores: &[Score],
    agents: &[EvaluatorAgent],
) -> Vec<ModeratedScore> {
    // Group scores by criterion_id
    let mut by_criterion: HashMap<String, Vec<&Score>> = HashMap::new();
    for s in scores {
        by_criterion
            .entry(s.criterion_id.clone())
            .or_default()
            .push(s);
    }

    let mut results: Vec<ModeratedScore> = Vec::new();

    for (criterion_id, criterion_scores) in &by_criterion {
        let raw_scores: Vec<f64> = criterion_scores.iter().map(|s| s.score).collect();

        // Panel mean
        let panel_mean = if raw_scores.is_empty() {
            0.0
        } else {
            raw_scores.iter().sum::<f64>() / raw_scores.len() as f64
        };

        // Panel std deviation (population)
        let panel_std_dev = if raw_scores.len() < 2 {
            0.0
        } else {
            let variance =
                raw_scores.iter().map(|s| (s - panel_mean).powi(2)).sum::<f64>()
                    / raw_scores.len() as f64;
            variance.sqrt()
        };

        // Trust-weighted mean
        let agent_scores: Vec<(String, f64)> = criterion_scores
            .iter()
            .map(|s| (s.agent_id.clone(), s.score))
            .collect();
        let consensus_score = trust_weighted_mean(&agent_scores, agents);

        // Identify outliers (> 2 std from panel mean)
        let mut dissents = Vec::new();
        if panel_std_dev > 0.0 {
            for s in criterion_scores {
                let deviation = (s.score - panel_mean).abs();
                if deviation > 2.0 * panel_std_dev {
                    dissents.push(Dissent {
                        agent_id: s.agent_id.clone(),
                        score: s.score,
                        reason: format!(
                            "Score {:.2} is {:.1} std devs from panel mean {:.2}",
                            s.score,
                            deviation / panel_std_dev,
                            panel_mean
                        ),
                    });
                }
            }
        }

        results.push(ModeratedScore {
            criterion_id: criterion_id.clone(),
            consensus_score,
            panel_mean,
            panel_std_dev,
            dissents,
        });
    }

    // Sort by criterion_id for deterministic output
    results.sort_by(|a, b| a.criterion_id.cmp(&b.criterion_id));
    results
}

/// Calculate trust-weighted mean for a set of scores.
///
/// Weight for each agent = average trust level that *other* agents assign to it.
/// If an agent has no inbound trust data, weight defaults to 1.0.
fn trust_weighted_mean(
    criterion_scores: &[(String, f64)], // (agent_id, score)
    agents: &[EvaluatorAgent],
) -> f64 {
    if criterion_scores.is_empty() {
        return 0.0;
    }

    // Build map: agent_id -> average inbound trust
    let mut inbound_trust: HashMap<&str, Vec<f64>> = HashMap::new();
    for agent in agents {
        for tr in &agent.trust_weights {
            inbound_trust
                .entry(&tr.target_agent_id)
                .or_default()
                .push(tr.trust_level);
        }
    }

    let mut weighted_sum = 0.0;
    let mut weight_total = 0.0;

    for (agent_id, score) in criterion_scores {
        let weight = inbound_trust
            .get(agent_id.as_str())
            .map(|levels| {
                if levels.is_empty() {
                    1.0
                } else {
                    levels.iter().sum::<f64>() / levels.len() as f64
                }
            })
            .unwrap_or(1.0);

        weighted_sum += score * weight;
        weight_total += weight;
    }

    if weight_total == 0.0 {
        0.0
    } else {
        weighted_sum / weight_total
    }
}

/// Calculate the overall evaluation result from moderated scores and framework.
pub fn calculate_overall_score(
    moderated_scores: &[ModeratedScore],
    framework: &EvaluationFramework,
) -> OverallResult {
    // Build criterion lookup: id -> (weight, max_score)
    let criterion_map: HashMap<String, &EvaluationCriterion> = collect_criteria(&framework.criteria);

    let mut total_score = 0.0;
    let mut max_possible = 0.0;

    // Track (criterion_id, weighted_percentage) for strengths/weaknesses
    let mut scored_criteria: Vec<(String, f64)> = Vec::new();

    for ms in moderated_scores {
        if let Some(criterion) = criterion_map.get(&ms.criterion_id) {
            let weighted_score = ms.consensus_score * criterion.weight;
            let weighted_max = criterion.max_score * criterion.weight;
            total_score += weighted_score;
            max_possible += weighted_max;

            let pct = if criterion.max_score > 0.0 {
                ms.consensus_score / criterion.max_score
            } else {
                0.0
            };
            scored_criteria.push((ms.criterion_id.clone(), pct));
        }
    }

    let percentage = if max_possible > 0.0 {
        (total_score / max_possible) * 100.0
    } else {
        0.0
    };

    let passed = framework.pass_mark.map(|pm| percentage >= pm);

    // Sort for strengths (highest pct first) and weaknesses (lowest first)
    scored_criteria.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let top_n = 3.min(scored_criteria.len());
    let top_strengths: Vec<String> = scored_criteria[..top_n]
        .iter()
        .map(|(id, _)| id.clone())
        .collect();

    let top_weaknesses: Vec<String> = scored_criteria
        .iter()
        .rev()
        .take(top_n)
        .map(|(id, _)| id.clone())
        .collect();

    OverallResult {
        total_score,
        max_possible,
        percentage,
        pass_mark: framework.pass_mark,
        passed,
        top_strengths,
        top_weaknesses,
    }
}

/// Recursively collect all criteria into a flat map.
fn collect_criteria(criteria: &[EvaluationCriterion]) -> HashMap<String, &EvaluationCriterion> {
    let mut map = HashMap::new();
    for c in criteria {
        map.insert(c.id.clone(), c);
        map.extend(collect_criteria(&c.sub_criteria));
    }
    map
}

/// Generate the moderation prompt for the moderator subagent.
///
/// The prompt includes all agents' scores for a criterion and asks the
/// moderator to identify outliers and request justification.
pub fn generate_moderation_prompt(
    moderator: &EvaluatorAgent,
    scores: &[Score],
    agents: &[EvaluatorAgent],
    criterion: &EvaluationCriterion,
) -> String {
    let agent_map: HashMap<&str, &EvaluatorAgent> =
        agents.iter().map(|a| (a.id.as_str(), a)).collect();

    let mut score_lines = String::new();
    for s in scores {
        let agent_name = agent_map
            .get(s.agent_id.as_str())
            .map(|a| a.name.as_str())
            .unwrap_or("Unknown");
        score_lines.push_str(&format!(
            "- {} ({}): {:.1}/{:.1} — \"{}\"\n",
            agent_name, s.agent_id, s.score, s.max_score, s.justification
        ));
    }

    let raw_scores: Vec<f64> = scores.iter().map(|s| s.score).collect();
    let mean = if raw_scores.is_empty() {
        0.0
    } else {
        raw_scores.iter().sum::<f64>() / raw_scores.len() as f64
    };

    format!(
        "You are {name}, acting as moderator for this evaluation panel.\n\
         \n\
         ## Criterion: {title}\n\
         {desc}\n\
         Max score: {max:.1}\n\
         \n\
         ## Panel Scores\n\
         {scores}\
         \n\
         Panel mean: {mean:.2}\n\
         \n\
         ## Your Task\n\
         1. Identify any outlier scores (more than 2 standard deviations from the mean).\n\
         2. For each outlier, request justification from the scoring agent.\n\
         3. Determine a fair consensus score using trust-weighted averaging.\n\
         4. Record any unresolved dissents.\n\
         \n\
         Respond with your moderated score and reasoning.",
        name = moderator.name,
        title = criterion.title,
        desc = criterion.description.as_deref().unwrap_or("No description."),
        max = criterion.max_score,
        scores = score_lines,
        mean = mean,
    )
}

/// Identify which criteria had the most disagreement during debate.
///
/// For each criterion, calculates total challenge count plus average score
/// drift across rounds. Returns sorted by contentiousness (descending).
pub fn most_contested_criteria(rounds: &[DebateRound]) -> Vec<(String, f64)> {
    // Per criterion: (challenge_count, total_drift, drift_samples)
    let mut stats: HashMap<String, (usize, f64, usize)> = HashMap::new();

    for round in rounds {
        // Count challenges per criterion
        for challenge in &round.challenges {
            let entry = stats
                .entry(challenge.criterion_id.clone())
                .or_insert((0, 0.0, 0));
            entry.0 += 1;

            // Track score drift from challenges
            if let Some((from, to)) = challenge.score_change {
                entry.1 += (to - from).abs();
                entry.2 += 1;
            }
        }
    }

    let mut result: Vec<(String, f64)> = stats
        .into_iter()
        .map(|(criterion_id, (challenge_count, total_drift, _samples))| {
            // Contentiousness = challenge_count + total_drift
            let contentiousness = challenge_count as f64 + total_drift;
            (criterion_id, contentiousness)
        })
        .collect();

    result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a minimal agent with optional trust weights.
    fn make_agent(id: &str, trust: Vec<(&str, f64)>) -> EvaluatorAgent {
        EvaluatorAgent {
            id: id.to_string(),
            name: format!("Agent {}", id),
            role: "evaluator".to_string(),
            domain: "general".to_string(),
            years_experience: Some(5),
            persona_description: "Test agent".to_string(),
            needs: vec![],
            trust_weights: trust
                .into_iter()
                .map(|(target, level)| TrustRelation {
                    target_agent_id: target.to_string(),
                    domain: "general".to_string(),
                    trust_level: level,
                })
                .collect(),
        }
    }

    fn make_score(agent_id: &str, criterion_id: &str, score: f64) -> Score {
        Score {
            agent_id: agent_id.to_string(),
            criterion_id: criterion_id.to_string(),
            score,
            max_score: 10.0,
            round: 1,
            justification: format!("Score {} by {}", score, agent_id),
            evidence_used: vec![],
            gaps_identified: vec![],
        }
    }

    fn make_criterion(id: &str, max_score: f64, weight: f64) -> EvaluationCriterion {
        EvaluationCriterion {
            id: id.to_string(),
            title: format!("Criterion {}", id),
            description: Some(format!("Description for {}", id)),
            max_score,
            weight,
            rubric_levels: vec![],
            sub_criteria: vec![],
        }
    }

    fn make_framework(criteria: Vec<EvaluationCriterion>, pass_mark: Option<f64>) -> EvaluationFramework {
        let total_weight: f64 = criteria.iter().map(|c| c.weight).sum();
        EvaluationFramework {
            id: "fw-1".to_string(),
            name: "Test Framework".to_string(),
            total_weight,
            pass_mark,
            criteria,
        }
    }

    #[test]
    fn test_moderated_scores_basic() {
        // 3 agents scoring one criterion → correct mean, std dev
        let agents = vec![
            make_agent("a1", vec![]),
            make_agent("a2", vec![]),
            make_agent("a3", vec![]),
        ];
        let scores = vec![
            make_score("a1", "c1", 7.0),
            make_score("a2", "c1", 8.0),
            make_score("a3", "c1", 9.0),
        ];

        let result = calculate_moderated_scores(&scores, &agents);
        assert_eq!(result.len(), 1);

        let ms = &result[0];
        assert_eq!(ms.criterion_id, "c1");
        assert!((ms.panel_mean - 8.0).abs() < 1e-9);
        // std dev of [7,8,9] = sqrt(2/3) ≈ 0.8165
        assert!((ms.panel_std_dev - (2.0_f64 / 3.0).sqrt()).abs() < 1e-4);
        assert!(ms.dissents.is_empty());
    }

    #[test]
    fn test_trust_weighted_mean_basic() {
        // Agent a1 is highly trusted, a2 less so
        // a1 trusts a2 at 0.3, a2 trusts a1 at 0.9
        let agents = vec![
            make_agent("a1", vec![("a2", 0.3)]),
            make_agent("a2", vec![("a1", 0.9)]),
        ];

        let scores = vec![
            ("a1".to_string(), 8.0),
            ("a2".to_string(), 4.0),
        ];

        let result = trust_weighted_mean(&scores, &agents);
        // a1 weight = avg inbound trust = 0.9 (from a2)
        // a2 weight = avg inbound trust = 0.3 (from a1)
        // weighted mean = (8*0.9 + 4*0.3) / (0.9+0.3) = (7.2+1.2)/1.2 = 8.4/1.2 = 7.0
        assert!((result - 7.0).abs() < 1e-9);
    }

    #[test]
    fn test_equal_trust_gives_simple_mean() {
        // When all trust weights are equal (or absent), result is simple average
        let agents = vec![
            make_agent("a1", vec![]),
            make_agent("a2", vec![]),
            make_agent("a3", vec![]),
        ];

        let scores = vec![
            ("a1".to_string(), 6.0),
            ("a2".to_string(), 8.0),
            ("a3".to_string(), 10.0),
        ];

        let result = trust_weighted_mean(&scores, &agents);
        // All weights default to 1.0 → simple mean = 8.0
        assert!((result - 8.0).abs() < 1e-9);
    }

    #[test]
    fn test_outlier_detection() {
        // Scores at exactly 2 std devs from mean are NOT outliers (strictly >)
        let agents = vec![
            make_agent("a1", vec![]),
            make_agent("a2", vec![]),
            make_agent("a3", vec![]),
            make_agent("a4", vec![]),
            make_agent("a5", vec![]),
        ];
        // mean = 6.0, std = 2.0, a5 deviation = 4.0 = exactly 2*std → no dissent
        let scores = vec![
            make_score("a1", "c1", 7.0),
            make_score("a2", "c1", 7.0),
            make_score("a3", "c1", 7.0),
            make_score("a4", "c1", 7.0),
            make_score("a5", "c1", 2.0),
        ];

        let result = calculate_moderated_scores(&scores, &agents);
        assert_eq!(result.len(), 1);
        assert!((result[0].panel_mean - 6.0).abs() < 1e-9);
        assert!((result[0].panel_std_dev - 2.0).abs() < 1e-9);
        assert!(result[0].dissents.is_empty());
    }

    #[test]
    fn test_outlier_detection_with_clear_outlier() {
        // 6 agents: 5 agree on 9, one scores 0 → clear outlier
        let agents = vec![
            make_agent("a1", vec![]),
            make_agent("a2", vec![]),
            make_agent("a3", vec![]),
            make_agent("a4", vec![]),
            make_agent("a5", vec![]),
            make_agent("a6", vec![]),
        ];
        let scores = vec![
            make_score("a1", "c1", 9.0),
            make_score("a2", "c1", 9.0),
            make_score("a3", "c1", 9.0),
            make_score("a4", "c1", 9.0),
            make_score("a5", "c1", 9.0),
            make_score("a6", "c1", 0.0),
        ];
        // mean = 7.5, std ≈ 3.354, threshold ≈ 6.708
        // a6 deviation = 7.5 > 6.708 → outlier
        let result = calculate_moderated_scores(&scores, &agents);
        let ms = &result[0];
        assert_eq!(ms.dissents.len(), 1);
        assert_eq!(ms.dissents[0].agent_id, "a6");
        assert!((ms.dissents[0].score - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_overall_score() {
        let criteria = vec![
            make_criterion("c1", 10.0, 0.6),
            make_criterion("c2", 10.0, 0.4),
        ];
        let framework = make_framework(criteria, None);

        let moderated = vec![
            ModeratedScore {
                criterion_id: "c1".to_string(),
                consensus_score: 8.0,
                panel_mean: 8.0,
                panel_std_dev: 0.5,
                dissents: vec![],
            },
            ModeratedScore {
                criterion_id: "c2".to_string(),
                consensus_score: 6.0,
                panel_mean: 6.0,
                panel_std_dev: 1.0,
                dissents: vec![],
            },
        ];

        let result = calculate_overall_score(&moderated, &framework);
        // total = 8*0.6 + 6*0.4 = 4.8 + 2.4 = 7.2
        // max = 10*0.6 + 10*0.4 = 6 + 4 = 10
        // pct = 72%
        assert!((result.total_score - 7.2).abs() < 1e-9);
        assert!((result.max_possible - 10.0).abs() < 1e-9);
        assert!((result.percentage - 72.0).abs() < 1e-9);
        assert!(result.passed.is_none());
    }

    #[test]
    fn test_pass_fail_above() {
        let criteria = vec![make_criterion("c1", 10.0, 1.0)];
        let framework = make_framework(criteria, Some(60.0));

        let moderated = vec![ModeratedScore {
            criterion_id: "c1".to_string(),
            consensus_score: 7.0,
            panel_mean: 7.0,
            panel_std_dev: 0.0,
            dissents: vec![],
        }];

        let result = calculate_overall_score(&moderated, &framework);
        assert_eq!(result.passed, Some(true));
        assert!((result.percentage - 70.0).abs() < 1e-9);
    }

    #[test]
    fn test_pass_fail_below() {
        let criteria = vec![make_criterion("c1", 10.0, 1.0)];
        let framework = make_framework(criteria, Some(60.0));

        let moderated = vec![ModeratedScore {
            criterion_id: "c1".to_string(),
            consensus_score: 5.0,
            panel_mean: 5.0,
            panel_std_dev: 0.0,
            dissents: vec![],
        }];

        let result = calculate_overall_score(&moderated, &framework);
        assert_eq!(result.passed, Some(false));
        assert!((result.percentage - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_most_contested() {
        let rounds = vec![
            DebateRound {
                round_number: 1,
                scores: vec![],
                challenges: vec![
                    Challenge {
                        challenger_id: "a1".to_string(),
                        target_agent_id: "a2".to_string(),
                        criterion_id: "c1".to_string(),
                        round: 1,
                        argument: "Too high".to_string(),
                        response: None,
                        score_change: Some((8.0, 6.0)),
                    },
                    Challenge {
                        challenger_id: "a2".to_string(),
                        target_agent_id: "a1".to_string(),
                        criterion_id: "c1".to_string(),
                        round: 1,
                        argument: "Too low".to_string(),
                        response: None,
                        score_change: Some((5.0, 6.0)),
                    },
                    Challenge {
                        challenger_id: "a1".to_string(),
                        target_agent_id: "a3".to_string(),
                        criterion_id: "c2".to_string(),
                        round: 1,
                        argument: "Disagree".to_string(),
                        response: None,
                        score_change: None,
                    },
                ],
                drift_velocity: Some(1.0),
                converged: false,
            },
        ];

        let result = most_contested_criteria(&rounds);
        // c1: 2 challenges + drift |8-6|+|5-6| = 2+1 = 3, total = 2+3 = 5
        // c2: 1 challenge + 0 drift = 1
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "c1");
        assert!((result[0].1 - 5.0).abs() < 1e-9);
        assert_eq!(result[1].0, "c2");
        assert!((result[1].1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_moderation_prompt_contains_scores() {
        let moderator = make_agent("mod", vec![]);
        let agents = vec![
            make_agent("a1", vec![]),
            make_agent("a2", vec![]),
        ];
        let scores = vec![
            make_score("a1", "c1", 7.0),
            make_score("a2", "c1", 9.0),
        ];
        let criterion = make_criterion("c1", 10.0, 1.0);

        let prompt = generate_moderation_prompt(&moderator, &scores, &agents, &criterion);

        assert!(prompt.contains("Agent mod"));
        assert!(prompt.contains("Criterion c1"));
        assert!(prompt.contains("Agent a1"));
        assert!(prompt.contains("7.0"));
        assert!(prompt.contains("9.0"));
        assert!(prompt.contains("Panel mean"));
    }

    #[test]
    fn test_multiple_criteria_moderation() {
        let agents = vec![
            make_agent("a1", vec![]),
            make_agent("a2", vec![]),
        ];
        let scores = vec![
            make_score("a1", "c1", 7.0),
            make_score("a2", "c1", 9.0),
            make_score("a1", "c2", 5.0),
            make_score("a2", "c2", 5.0),
        ];

        let result = calculate_moderated_scores(&scores, &agents);
        assert_eq!(result.len(), 2);

        let c1 = result.iter().find(|ms| ms.criterion_id == "c1").unwrap();
        let c2 = result.iter().find(|ms| ms.criterion_id == "c2").unwrap();

        assert!((c1.panel_mean - 8.0).abs() < 1e-9);
        assert!((c2.panel_mean - 5.0).abs() < 1e-9);
        assert!((c2.panel_std_dev - 0.0).abs() < 1e-9);
    }
}
