//! Deterministic mock data fixtures for benchmarks.
//!
//! Provides factory functions that produce repeatable test data sets
//! for comparing the bench-naive and bench-tardygrada pipelines.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Shared benchmark types — plain structs, no trait hierarchies
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchDocument {
    pub id: String,
    pub title: String,
    pub sections: Vec<BenchSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSection {
    pub id: String,
    pub title: String,
    pub text: String,
    pub word_count: u32,
    pub claims: Vec<BenchClaim>,
    pub evidence: Vec<BenchEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchClaim {
    pub id: String,
    pub text: String,
    pub specificity: f64,
    pub verifiable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchEvidence {
    pub id: String,
    pub source: String,
    pub evidence_type: String,
    pub text: String,
    pub has_quantified_outcome: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchFramework {
    pub id: String,
    pub name: String,
    pub criteria: Vec<BenchCriterion>,
    pub pass_mark: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchCriterion {
    pub id: String,
    pub title: String,
    pub weight: f64,
    pub max_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchAgent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub is_moderator: bool,
    pub trust_weights: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchScore {
    pub agent_id: String,
    pub criterion_id: String,
    pub round: u32,
    pub score: f64,
    pub max_score: f64,
    pub justification: String,
}

// ============================================================================
// MockLlm — deterministic score lookup
// ============================================================================

/// Deterministic LLM mock: returns a pre-computed score for each
/// (agent_id, criterion_id, round) triple.
#[derive(Debug, Clone)]
pub struct MockLlm {
    scores: HashMap<(String, String, u32), f64>,
}

impl MockLlm {
    pub fn new(scores: &[BenchScore]) -> Self {
        let mut map = HashMap::new();
        for s in scores {
            map.insert(
                (s.agent_id.clone(), s.criterion_id.clone(), s.round),
                s.score,
            );
        }
        Self { scores: map }
    }

    /// Look up a deterministic score. Returns 5.0 as fallback.
    pub fn score(&self, agent_id: &str, criterion_id: &str, round: u32) -> f64 {
        self.scores
            .get(&(agent_id.to_string(), criterion_id.to_string(), round))
            .copied()
            .unwrap_or(5.0)
    }
}

// ============================================================================
// Factory functions
// ============================================================================

/// 10 sections, each with 3 claims + 2 evidence items.
pub fn mock_document() -> BenchDocument {
    let mut sections = Vec::with_capacity(10);
    for s in 0..10 {
        let mut claims = Vec::with_capacity(3);
        for c in 0..3 {
            claims.push(BenchClaim {
                id: format!("claim-{s}-{c}"),
                text: format!("Section {s} claim {c}: the approach delivers measurable outcomes."),
                specificity: 0.6 + (c as f64) * 0.1,
                verifiable: c < 2,
            });
        }
        let mut evidence = Vec::with_capacity(2);
        for e in 0..2 {
            evidence.push(BenchEvidence {
                id: format!("ev-{s}-{e}"),
                source: format!("Case study {s}-{e}"),
                evidence_type: if e == 0 {
                    "case_study".into()
                } else {
                    "statistic".into()
                },
                text: format!("Evidence {e} for section {s}: 15% improvement in KPI."),
                has_quantified_outcome: e == 1,
            });
        }
        let word_count = 200 + (s as u32) * 30;
        sections.push(BenchSection {
            id: format!("sec-{s}"),
            title: format!("Section {s}: Technical Approach"),
            text: format!(
                "## Section {s}: Technical Approach\n\n\
                 This section describes the technical approach for deliverable {s}. \
                 We propose a methodology grounded in evidence-based practice with \
                 measurable outcomes and clear milestones. Our team has delivered \
                 similar projects achieving 15% improvement in key performance indicators. \
                 {}",
                "Lorem ipsum dolor sit amet. ".repeat(word_count as usize / 5)
            ),
            word_count,
            claims,
            evidence,
        });
    }
    BenchDocument {
        id: "doc-bench-001".into(),
        title: "Benchmark Tender Response".into(),
        sections,
    }
}

/// 5 criteria, weights sum to 1.0, pass_mark 60.0.
pub fn mock_framework() -> BenchFramework {
    BenchFramework {
        id: "fw-bench-001".into(),
        name: "Benchmark Framework".into(),
        pass_mark: 60.0,
        criteria: vec![
            BenchCriterion {
                id: "crit-0".into(),
                title: "Technical Quality".into(),
                weight: 0.30,
                max_score: 10.0,
            },
            BenchCriterion {
                id: "crit-1".into(),
                title: "Delivery Approach".into(),
                weight: 0.25,
                max_score: 10.0,
            },
            BenchCriterion {
                id: "crit-2".into(),
                title: "Social Value".into(),
                weight: 0.20,
                max_score: 10.0,
            },
            BenchCriterion {
                id: "crit-3".into(),
                title: "Risk Management".into(),
                weight: 0.15,
                max_score: 10.0,
            },
            BenchCriterion {
                id: "crit-4".into(),
                title: "Innovation".into(),
                weight: 0.10,
                max_score: 10.0,
            },
        ],
    }
}

/// 4 evaluators + 1 moderator with bidirectional trust 0.6.
pub fn mock_agents() -> Vec<BenchAgent> {
    let agent_defs = vec![
        ("agent-0", "Budget Expert", "evaluator", false),
        ("agent-1", "Technical Evaluator", "evaluator", false),
        ("agent-2", "Delivery Specialist", "evaluator", false),
        ("agent-3", "Social Value Assessor", "evaluator", false),
        ("agent-4", "Moderator", "moderator", true),
    ];

    let ids: Vec<&str> = agent_defs.iter().map(|(id, ..)| *id).collect();

    agent_defs
        .iter()
        .map(|(id, name, role, is_mod)| {
            let mut trust_weights = HashMap::new();
            for &other in &ids {
                if other != *id {
                    trust_weights.insert(other.to_string(), 0.6);
                }
            }
            BenchAgent {
                id: id.to_string(),
                name: name.to_string(),
                role: role.to_string(),
                is_moderator: *is_mod,
                trust_weights,
            }
        })
        .collect()
}

/// 3 rounds x 4 agents x 5 criteria = 60 scores.
/// Round 1: high variance (scores 4..9), converging toward 7.0 by round 3.
pub fn mock_scores() -> Vec<BenchScore> {
    let agents = mock_agents();
    let framework = mock_framework();
    let evaluators: Vec<&BenchAgent> = agents.iter().filter(|a| !a.is_moderator).collect();
    let mut scores = Vec::with_capacity(60);

    for round in 1..=3u32 {
        for (ai, agent) in evaluators.iter().enumerate() {
            for (ci, criterion) in framework.criteria.iter().enumerate() {
                // Round 1: spread from 4.0 to 9.0
                // Round 2: narrowing toward 7.0
                // Round 3: tight cluster around 7.0
                let base = 7.0;
                let spread = match round {
                    1 => 2.5,
                    2 => 1.0,
                    _ => 0.3,
                };
                // Deterministic offset based on agent and criterion index
                let offset =
                    ((ai as f64 - 1.5) * 0.7 + (ci as f64 - 2.0) * 0.3) * spread / 2.5;
                let score = (base + offset).clamp(1.0, 10.0);

                scores.push(BenchScore {
                    agent_id: agent.id.clone(),
                    criterion_id: criterion.id.clone(),
                    round,
                    score,
                    max_score: criterion.max_score,
                    justification: format!(
                        "Agent {} round {round} assessment of {}: score {score:.1}",
                        agent.name, criterion.title
                    ),
                });
            }
        }
    }
    scores
}

/// 10 section-to-criterion alignment mappings.
pub fn mock_alignments() -> Vec<(String, String, f64)> {
    vec![
        ("sec-0".into(), "crit-0".into(), 0.95),
        ("sec-1".into(), "crit-0".into(), 0.80),
        ("sec-2".into(), "crit-1".into(), 0.90),
        ("sec-3".into(), "crit-1".into(), 0.75),
        ("sec-4".into(), "crit-2".into(), 0.85),
        ("sec-5".into(), "crit-2".into(), 0.70),
        ("sec-6".into(), "crit-3".into(), 0.88),
        ("sec-7".into(), "crit-3".into(), 0.65),
        ("sec-8".into(), "crit-4".into(), 0.92),
        ("sec-9".into(), "crit-4".into(), 0.78),
    ]
}
