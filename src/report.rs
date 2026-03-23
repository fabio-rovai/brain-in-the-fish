//! Evaluation report generation.
//!
//! Produces structured Markdown reports from evaluation results.
//! Pure string generation — no graph store operations.

use crate::types::*;
pub use crate::moderation::OverallResult;

/// Generate a full evaluation report as Markdown.
pub fn generate_report(session: &EvaluationSession, overall: &OverallResult) -> String {
    let mut report = String::new();

    report.push_str(&generate_header(session));
    report.push_str(&generate_executive_summary(overall, session));
    report.push_str(&generate_scorecard(session));
    report.push_str(&generate_gap_analysis(session));
    report.push_str(&generate_debate_trail(session));
    report.push_str(&generate_recommendations(session, overall));
    report.push_str(&generate_panel_summary(session));

    report
}

/// Title block with document metadata.
fn generate_header(session: &EvaluationSession) -> String {
    format!(
        "# Evaluation Report\n\n\
         **Document:** {}\n\
         **Type:** {}\n\
         **Evaluation ID:** {}\n\
         **Date:** {}\n\n---\n\n",
        session.document.title,
        session.document.doc_type,
        session.id,
        session.created_at,
    )
}

/// High-level pass/fail, score, strengths and weaknesses.
fn generate_executive_summary(overall: &OverallResult, session: &EvaluationSession) -> String {
    let mut s = String::from("## Executive Summary\n\n");

    s.push_str(&format!(
        "**Overall Score:** {:.1} / {:.1} ({:.1}%)\n\n",
        overall.total_score, overall.max_possible, overall.percentage,
    ));

    if let (Some(pass_mark), Some(passed)) = (overall.pass_mark, overall.passed) {
        let verdict = if passed { "PASS" } else { "FAIL" };
        s.push_str(&format!(
            "**Result:** {} (pass mark: {:.1}%)\n\n",
            verdict, pass_mark,
        ));
    }

    s.push_str(&format!(
        "**Criteria evaluated:** {}  \n**Debate rounds:** {}\n\n",
        session.final_scores.len(),
        session.rounds.len(),
    ));

    if !overall.top_strengths.is_empty() {
        s.push_str("### Top Strengths\n\n");
        for strength in &overall.top_strengths {
            s.push_str(&format!("- {}\n", strength));
        }
        s.push('\n');
    }

    if !overall.top_weaknesses.is_empty() {
        s.push_str("### Top Weaknesses\n\n");
        for weakness in &overall.top_weaknesses {
            s.push_str(&format!("- {}\n", weakness));
        }
        s.push('\n');
    }

    s.push_str("---\n\n");
    s
}

/// Markdown table of moderated scores per criterion.
pub fn generate_scorecard(session: &EvaluationSession) -> String {
    let mut s = String::from("## Scorecard\n\n");

    if session.final_scores.is_empty() {
        s.push_str("No scores recorded.\n\n");
        return s;
    }

    s.push_str("| Criterion | Score | Max | Weight | Weighted | Dissent |\n");
    s.push_str("|-----------|-------|-----|--------|----------|---------|\n");

    for ms in &session.final_scores {
        let criterion = session
            .framework
            .criteria
            .iter()
            .find(|c| c.id == ms.criterion_id);

        let title = criterion
            .map(|c| c.title.as_str())
            .unwrap_or(&ms.criterion_id);
        let max = criterion.map(|c| c.max_score).unwrap_or(0.0);
        let weight = criterion.map(|c| c.weight).unwrap_or(1.0);
        let weighted = ms.consensus_score * weight;
        let dissent_count = ms.dissents.len();

        s.push_str(&format!(
            "| {} | {:.1} | {:.1} | {:.1} | {:.1} | {} |\n",
            title, ms.consensus_score, max, weight, weighted, dissent_count,
        ));
    }

    s.push('\n');
    s.push_str("---\n\n");
    s
}

/// List gaps where criteria have no or partial document coverage.
pub fn generate_gap_analysis(session: &EvaluationSession) -> String {
    let mut s = String::from("## Gap Analysis\n\n");

    if session.gaps.is_empty() {
        s.push_str("No gaps identified.\n\n");
        s.push_str("---\n\n");
        return s;
    }

    for gap in &session.gaps {
        match &gap.best_partial_match {
            Some(mapping) => {
                s.push_str(&format!(
                    "- **{}**: Partial match (confidence: {:.0}%)\n",
                    gap.criterion_title,
                    mapping.confidence * 100.0,
                ));
            }
            None => {
                s.push_str(&format!(
                    "- **{}**: NO matching content\n",
                    gap.criterion_title,
                ));
            }
        }
    }

    s.push('\n');
    s.push_str("---\n\n");
    s
}

/// Per-round debate history: scores, challenges, convergence.
pub fn generate_debate_trail(session: &EvaluationSession) -> String {
    let mut s = String::from("## Debate Trail\n\n");

    if session.rounds.is_empty() {
        s.push_str("No debate rounds recorded.\n\n");
        s.push_str("---\n\n");
        return s;
    }

    for round in &session.rounds {
        s.push_str(&format!("### Round {}\n\n", round.round_number));

        // Score summary table
        if !round.scores.is_empty() {
            s.push_str("| Agent | Criterion | Score | Max |\n");
            s.push_str("|-------|-----------|-------|-----|\n");
            for score in &round.scores {
                s.push_str(&format!(
                    "| {} | {} | {:.1} | {:.1} |\n",
                    score.agent_id, score.criterion_id, score.score, score.max_score,
                ));
            }
            s.push('\n');
        }

        // Challenges
        if !round.challenges.is_empty() {
            s.push_str("**Challenges:**\n\n");
            for ch in &round.challenges {
                s.push_str(&format!(
                    "- {} challenged {} on *{}*: {}\n",
                    ch.challenger_id, ch.target_agent_id, ch.criterion_id, ch.argument,
                ));
                if let Some(resp) = &ch.response {
                    s.push_str(&format!("  - Response: {}\n", resp));
                }
                if let Some((from, to)) = ch.score_change {
                    s.push_str(&format!("  - Score moved: {:.1} -> {:.1}\n", from, to));
                }
            }
            s.push('\n');
        }

        // Drift and convergence
        if let Some(dv) = round.drift_velocity {
            s.push_str(&format!("**Drift velocity:** {:.3}\n\n", dv));
        }
        let converged_label = if round.converged { "Yes" } else { "No" };
        s.push_str(&format!("**Converged:** {}\n\n", converged_label));
    }

    s.push_str("---\n\n");
    s
}

/// Recommendations for criteria scoring below 70% of their max.
pub fn generate_recommendations(
    session: &EvaluationSession,
    _overall: &OverallResult,
) -> String {
    let mut s = String::from("## Improvement Recommendations\n\n");
    let mut found_any = false;

    for ms in &session.final_scores {
        let criterion = session
            .framework
            .criteria
            .iter()
            .find(|c| c.id == ms.criterion_id);

        let max = criterion.map(|c| c.max_score).unwrap_or(0.0);
        if max == 0.0 {
            continue;
        }

        let pct = ms.consensus_score / max;
        if pct >= 0.7 {
            continue;
        }

        found_any = true;
        let title = criterion
            .map(|c| c.title.as_str())
            .unwrap_or(&ms.criterion_id);

        s.push_str(&format!(
            "### {} ({:.1}/{:.1} — {:.0}%)\n\n",
            title, ms.consensus_score, max, pct * 100.0,
        ));

        // Collect gaps from individual agent scores in the last round
        let gaps: Vec<&str> = session
            .rounds
            .last()
            .map(|r| {
                r.scores
                    .iter()
                    .filter(|sc| sc.criterion_id == ms.criterion_id)
                    .flat_map(|sc| sc.gaps_identified.iter().map(|g| g.as_str()))
                    .collect()
            })
            .unwrap_or_default();

        if !gaps.is_empty() {
            s.push_str("**Gaps identified:**\n\n");
            for gap in gaps {
                s.push_str(&format!("- {}\n", gap));
            }
            s.push('\n');
        }

        // Suggest next rubric level
        if let Some(crit) = criterion
            && !crit.rubric_levels.is_empty() {
                // Find the first rubric level above current score conceptually
                // We list the next level up as a target
                s.push_str("**Target rubric level:**\n\n");
                for level in &crit.rubric_levels {
                    s.push_str(&format!(
                        "- {} ({}): {}\n",
                        level.level, level.score_range, level.descriptor,
                    ));
                }
                s.push('\n');
            }
    }

    if !found_any {
        s.push_str("All criteria scored at or above 70%. No immediate improvements needed.\n\n");
    }

    s.push_str("---\n\n");
    s
}

/// Summary of the agent panel: names, roles, domains.
fn generate_panel_summary(session: &EvaluationSession) -> String {
    let mut s = String::from("## Evaluation Panel\n\n");

    if session.agents.is_empty() {
        s.push_str("No agents recorded.\n\n");
        return s;
    }

    for agent in &session.agents {
        s.push_str(&format!(
            "- **{}** — {} ({})\n",
            agent.name, agent.role, agent.domain,
        ));
        if !agent.persona_description.is_empty() {
            s.push_str(&format!("  {}\n", agent.persona_description));
        }
    }

    s.push('\n');
    s
}

/// Export session as minimal Turtle (RDF) for cross-evaluation analysis.
pub fn session_to_turtle(session: &EvaluationSession) -> String {
    let mut t = String::new();

    t.push_str("@prefix eval: <http://brain-in-the-fish.dev/eval/> .\n");
    t.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n");

    // Session node
    t.push_str(&format!(
        "eval:{} a eval:EvaluationSession ;\n",
        sanitise_turtle_id(&session.id),
    ));
    t.push_str(&format!(
        "    eval:document \"{}\" ;\n",
        escape_turtle_string(&session.document.title),
    ));
    t.push_str(&format!(
        "    eval:framework \"{}\" ;\n",
        escape_turtle_string(&session.framework.name),
    ));
    t.push_str(&format!(
        "    eval:createdAt \"{}\"^^xsd:dateTime ;\n",
        session.created_at,
    ));

    // Final scores
    for (i, ms) in session.final_scores.iter().enumerate() {
        let last = i == session.final_scores.len() - 1;
        let sep = if last { " ." } else { " ;" };
        t.push_str(&format!(
            "    eval:score [ eval:criterion \"{}\" ; eval:value \"{}\"^^xsd:decimal ]{}\n",
            escape_turtle_string(&ms.criterion_id),
            ms.consensus_score,
            sep,
        ));
    }

    if session.final_scores.is_empty() {
        // Close the node
        t.push_str("    eval:status \"no-scores\" .\n");
    }

    t.push('\n');
    t
}

/// Make a string safe for use as a Turtle local name.
fn sanitise_turtle_id(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Escape special characters in a Turtle string literal.
fn escape_turtle_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal but complete EvaluationSession for testing.
    fn create_test_session() -> EvaluationSession {
        let doc = EvalDocument {
            id: "doc-1".into(),
            title: "Test Proposal".into(),
            doc_type: "tender_response".into(),
            total_pages: Some(20),
            total_word_count: Some(5000),
            sections: vec![],
        };

        let framework = EvaluationFramework {
            id: "fw-1".into(),
            name: "Quality Framework".into(),
            total_weight: 100.0,
            pass_mark: Some(60.0),
            criteria: vec![
                EvaluationCriterion {
                    id: "c-1".into(),
                    title: "Evidence Quality".into(),
                    description: Some("Quality of evidence provided".into()),
                    max_score: 10.0,
                    weight: 5.0,
                    rubric_levels: vec![
                        RubricLevel {
                            level: "Excellent".into(),
                            score_range: "9-10".into(),
                            descriptor: "Outstanding evidence with quantified outcomes".into(),
                        },
                        RubricLevel {
                            level: "Good".into(),
                            score_range: "7-8".into(),
                            descriptor: "Strong evidence with some metrics".into(),
                        },
                    ],
                    sub_criteria: vec![],
                },
                EvaluationCriterion {
                    id: "c-2".into(),
                    title: "Social Value".into(),
                    description: Some("Social value commitments".into()),
                    max_score: 10.0,
                    weight: 3.0,
                    rubric_levels: vec![
                        RubricLevel {
                            level: "Excellent".into(),
                            score_range: "9-10".into(),
                            descriptor: "Transformative social value".into(),
                        },
                    ],
                    sub_criteria: vec![],
                },
            ],
        };

        let agents = vec![
            EvaluatorAgent {
                id: "agent-1".into(),
                name: "Alice".into(),
                role: "Lead Evaluator".into(),
                domain: "Technical".into(),
                years_experience: Some(15),
                persona_description: "Senior technical evaluator with deep domain expertise.".into(),
                needs: vec![],
                trust_weights: vec![],
            },
            EvaluatorAgent {
                id: "agent-2".into(),
                name: "Bob".into(),
                role: "Social Value Assessor".into(),
                domain: "Social Value".into(),
                years_experience: Some(10),
                persona_description: "Specialist in community impact assessment.".into(),
                needs: vec![],
                trust_weights: vec![],
            },
        ];

        let rounds = vec![
            DebateRound {
                round_number: 1,
                scores: vec![
                    Score {
                        agent_id: "agent-1".into(),
                        criterion_id: "c-1".into(),
                        score: 8.0,
                        max_score: 10.0,
                        round: 1,
                        justification: "Good evidence throughout".into(),
                        evidence_used: vec!["case-study-1".into()],
                        gaps_identified: vec![],
                    },
                    Score {
                        agent_id: "agent-2".into(),
                        criterion_id: "c-2".into(),
                        score: 5.0,
                        max_score: 10.0,
                        round: 1,
                        justification: "Weak social value section".into(),
                        evidence_used: vec![],
                        gaps_identified: vec!["No TOMs measures cited".into()],
                    },
                ],
                challenges: vec![Challenge {
                    challenger_id: "agent-1".into(),
                    target_agent_id: "agent-2".into(),
                    criterion_id: "c-2".into(),
                    round: 1,
                    argument: "The social value section mentions community benefits".into(),
                    response: Some("Those are generic claims without evidence".into()),
                    score_change: Some((5.0, 5.5)),
                }],
                drift_velocity: Some(0.15),
                converged: false,
            },
            DebateRound {
                round_number: 2,
                scores: vec![
                    Score {
                        agent_id: "agent-1".into(),
                        criterion_id: "c-1".into(),
                        score: 7.5,
                        max_score: 10.0,
                        round: 2,
                        justification: "Revised slightly after debate".into(),
                        evidence_used: vec!["case-study-1".into()],
                        gaps_identified: vec![],
                    },
                    Score {
                        agent_id: "agent-2".into(),
                        criterion_id: "c-2".into(),
                        score: 5.5,
                        max_score: 10.0,
                        round: 2,
                        justification: "Slightly improved but still weak".into(),
                        evidence_used: vec![],
                        gaps_identified: vec!["No TOMs measures cited".into()],
                    },
                ],
                challenges: vec![],
                drift_velocity: Some(0.03),
                converged: true,
            },
        ];

        let final_scores = vec![
            ModeratedScore {
                criterion_id: "c-1".into(),
                consensus_score: 7.5,
                panel_mean: 7.75,
                panel_std_dev: 0.35,
                dissents: vec![],
            },
            ModeratedScore {
                criterion_id: "c-2".into(),
                consensus_score: 5.5,
                panel_mean: 5.25,
                panel_std_dev: 0.5,
                dissents: vec![Dissent {
                    agent_id: "agent-1".into(),
                    score: 6.0,
                    reason: "I believe the community mentions warrant a slightly higher score".into(),
                }],
            },
        ];

        EvaluationSession {
            id: "session-001".into(),
            document: doc,
            framework,
            agents,
            alignments: vec![],
            gaps: vec![],
            rounds,
            final_scores,
            created_at: "2026-03-23T10:00:00Z".into(),
        }
    }

    /// Session with gaps in coverage.
    fn create_test_session_with_gaps() -> EvaluationSession {
        let mut session = create_test_session();
        session.gaps = vec![
            Gap {
                criterion_id: "c-2".into(),
                criterion_title: "Social Value".into(),
                best_partial_match: Some(AlignmentMapping {
                    section_id: "sec-3".into(),
                    criterion_id: "c-2".into(),
                    confidence: 0.35,
                }),
            },
            Gap {
                criterion_id: "c-3".into(),
                criterion_title: "Risk Management".into(),
                best_partial_match: None,
            },
        ];
        session
    }

    fn make_overall() -> OverallResult {
        OverallResult {
            total_score: 67.0,
            max_possible: 100.0,
            percentage: 67.0,
            pass_mark: Some(60.0),
            passed: Some(true),
            top_strengths: vec!["Evidence quality".into()],
            top_weaknesses: vec!["Social value".into()],
        }
    }

    #[test]
    fn test_generate_report_basic() {
        let session = create_test_session();
        let overall = make_overall();

        let report = generate_report(&session, &overall);
        assert!(report.contains("# Evaluation Report"));
        assert!(report.contains("Executive Summary"));
        assert!(report.contains("Scorecard"));
        assert!(report.contains("67"));
        assert!(report.contains("PASS"));
    }

    #[test]
    fn test_scorecard_has_table() {
        let session = create_test_session();
        let scorecard = generate_scorecard(&session);
        assert!(scorecard.contains("|"));
        assert!(scorecard.contains("Criterion"));
        assert!(scorecard.contains("Evidence Quality"));
        assert!(scorecard.contains("Social Value"));
    }

    #[test]
    fn test_gap_analysis_no_gaps() {
        let session = create_test_session();
        let gaps = generate_gap_analysis(&session);
        assert!(gaps.contains("No gaps identified"));
    }

    #[test]
    fn test_gap_analysis_with_gaps() {
        let session = create_test_session_with_gaps();
        let gaps = generate_gap_analysis(&session);
        assert!(gaps.contains("Gap"));
        assert!(gaps.contains("Social Value"));
        assert!(gaps.contains("Partial match"));
        assert!(gaps.contains("Risk Management"));
        assert!(gaps.contains("NO matching content"));
    }

    #[test]
    fn test_debate_trail() {
        let session = create_test_session();
        let trail = generate_debate_trail(&session);
        assert!(trail.contains("Round 1"));
        assert!(trail.contains("Round 2"));
        assert!(trail.contains("Converged:** Yes"));
        assert!(trail.contains("Challenges"));
    }

    #[test]
    fn test_recommendations_for_low_scores() {
        let session = create_test_session();
        let overall = make_overall();
        let recs = generate_recommendations(&session, &overall);
        // Social Value scores 5.5/10 = 55%, below 70% threshold
        assert!(recs.contains("Social Value"));
        assert!(recs.contains("5.5"));
        // Evidence Quality scores 7.5/10 = 75%, above threshold — should NOT appear
        assert!(!recs.contains("Evidence Quality"));
    }

    #[test]
    fn test_recommendations_all_good() {
        let mut session = create_test_session();
        // Set all scores above 70%
        for ms in &mut session.final_scores {
            ms.consensus_score = 8.0;
        }
        let overall = make_overall();
        let recs = generate_recommendations(&session, &overall);
        assert!(recs.contains("No immediate improvements needed"));
    }

    #[test]
    fn test_panel_summary() {
        let session = create_test_session();
        let panel = generate_panel_summary(&session);
        assert!(panel.contains("Alice"));
        assert!(panel.contains("Bob"));
        assert!(panel.contains("Lead Evaluator"));
    }

    #[test]
    fn test_session_to_turtle() {
        let session = create_test_session();
        let turtle = session_to_turtle(&session);
        assert!(turtle.contains("eval:"));
        assert!(turtle.contains("EvaluationSession"));
        assert!(turtle.contains("Test Proposal"));
        assert!(turtle.contains("xsd:decimal"));
    }

    #[test]
    fn test_header_contains_metadata() {
        let session = create_test_session();
        let header = generate_header(&session);
        assert!(header.contains("Test Proposal"));
        assert!(header.contains("tender_response"));
        assert!(header.contains("session-001"));
    }

    #[test]
    fn test_executive_summary_no_pass_mark() {
        let session = create_test_session();
        let overall = OverallResult {
            total_score: 50.0,
            max_possible: 100.0,
            percentage: 50.0,
            pass_mark: None,
            passed: None,
            top_strengths: vec![],
            top_weaknesses: vec![],
        };
        let summary = generate_executive_summary(&overall, &session);
        assert!(summary.contains("50.0"));
        assert!(!summary.contains("PASS"));
        assert!(!summary.contains("FAIL"));
    }
}
