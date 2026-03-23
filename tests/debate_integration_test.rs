//! Integration test: debate flow with disagreements and convergence.

use brain_in_the_fish::debate;
use brain_in_the_fish::types::*;

#[test]
fn test_debate_convergence_flow() {
    // Round 1: agents disagree
    let round1 = vec![
        Score {
            agent_id: "a1".into(),
            criterion_id: "c1".into(),
            score: 8.0,
            max_score: 10.0,
            round: 1,
            justification: "Strong".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        },
        Score {
            agent_id: "a2".into(),
            criterion_id: "c1".into(),
            score: 4.0,
            max_score: 10.0,
            round: 1,
            justification: "Weak".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        },
        Score {
            agent_id: "a3".into(),
            criterion_id: "c1".into(),
            score: 7.0,
            max_score: 10.0,
            round: 1,
            justification: "Good".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        },
    ];

    let disagreements = debate::find_disagreements(&round1, 2.0);
    assert!(
        !disagreements.is_empty(),
        "Round 1 should have disagreements"
    );

    // Round 2: agents move closer after debate
    let round2 = vec![
        Score {
            agent_id: "a1".into(),
            criterion_id: "c1".into(),
            score: 7.0,
            max_score: 10.0,
            round: 2,
            justification: "Revised down".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        },
        Score {
            agent_id: "a2".into(),
            criterion_id: "c1".into(),
            score: 6.0,
            max_score: 10.0,
            round: 2,
            justification: "Revised up".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        },
        Score {
            agent_id: "a3".into(),
            criterion_id: "c1".into(),
            score: 7.0,
            max_score: 10.0,
            round: 2,
            justification: "Maintained".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        },
    ];

    let drift = debate::calculate_drift_velocity(&round1, &round2);
    assert!(drift > 0.0, "Should have some drift");

    // Round 2 should have fewer/no disagreements
    let disagreements2 = debate::find_disagreements(&round2, 2.0);
    assert!(
        disagreements2.len() < disagreements.len(),
        "Round 2 should have fewer disagreements ({} vs {})",
        disagreements2.len(),
        disagreements.len()
    );

    // Check convergence
    let converged = debate::check_convergence(drift, 2.0);
    assert!(converged, "Should converge when drift is within threshold");
}

#[test]
fn test_trust_evolution_across_rounds() {
    let mut agents = vec![
        EvaluatorAgent {
            id: "a1".into(),
            name: "Agent A".into(),
            role: "Expert".into(),
            domain: "Test".into(),
            years_experience: Some(10),
            persona_description: "Test agent".into(),
            needs: vec![],
            trust_weights: vec![TrustRelation {
                target_agent_id: "a2".into(),
                domain: "test".into(),
                trust_level: 0.5,
            }],
        },
        EvaluatorAgent {
            id: "a2".into(),
            name: "Agent B".into(),
            role: "Reviewer".into(),
            domain: "Test".into(),
            years_experience: Some(5),
            persona_description: "Test agent".into(),
            needs: vec![],
            trust_weights: vec![TrustRelation {
                target_agent_id: "a1".into(),
                domain: "test".into(),
                trust_level: 0.5,
            }],
        },
    ];

    // Successful challenge: a1 challenges a2, a2 changes score
    let challenges = vec![Challenge {
        challenger_id: "a1".into(),
        target_agent_id: "a2".into(),
        criterion_id: "c1".into(),
        round: 1,
        argument: "Your evidence is weak".into(),
        response: Some("Fair point, adjusting".into()),
        score_change: Some((4.0, 6.0)),
    }];

    debate::update_trust_weights(&mut agents, &challenges);

    // a2's trust in a1 should increase (a2 is the target, a1 is the challenger)
    let a2_trust_in_a1 = agents[1]
        .trust_weights
        .iter()
        .find(|t| t.target_agent_id == "a1")
        .unwrap()
        .trust_level;
    assert!(
        a2_trust_in_a1 > 0.5,
        "Trust should increase after successful challenge, got {}",
        a2_trust_in_a1
    );
}

#[test]
fn test_multi_round_debate_with_challenges_and_moderation() {
    use brain_in_the_fish::moderation;

    // Set up agents with mutual trust
    let agents = vec![
        EvaluatorAgent {
            id: "a1".into(),
            name: "Alice".into(),
            role: "Expert".into(),
            domain: "Technical".into(),
            years_experience: Some(15),
            persona_description: "Technical evaluator".into(),
            needs: vec![],
            trust_weights: vec![TrustRelation {
                target_agent_id: "a2".into(),
                domain: "general".into(),
                trust_level: 0.5,
            }],
        },
        EvaluatorAgent {
            id: "a2".into(),
            name: "Bob".into(),
            role: "Reviewer".into(),
            domain: "Social Value".into(),
            years_experience: Some(10),
            persona_description: "Social value reviewer".into(),
            needs: vec![],
            trust_weights: vec![TrustRelation {
                target_agent_id: "a1".into(),
                domain: "general".into(),
                trust_level: 0.5,
            }],
        },
    ];

    // Round 1: significant disagreement
    let round1_scores = vec![
        Score {
            agent_id: "a1".into(),
            criterion_id: "c1".into(),
            score: 9.0,
            max_score: 10.0,
            round: 1,
            justification: "Excellent technical depth".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        },
        Score {
            agent_id: "a2".into(),
            criterion_id: "c1".into(),
            score: 5.0,
            max_score: 10.0,
            round: 1,
            justification: "Lacks social value evidence".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        },
    ];

    let disagreements = debate::find_disagreements(&round1_scores, 2.0);
    assert_eq!(disagreements.len(), 1, "Should find one disagreement");
    assert!(
        (disagreements[0].delta - 4.0).abs() < 1e-9,
        "Delta should be 4.0"
    );

    // Build round 1
    let debate_round1 = debate::build_debate_round(1, round1_scores.clone(), vec![], None, false);
    assert!(!debate_round1.converged);

    // Round 2: agents converge
    let round2_scores = vec![
        Score {
            agent_id: "a1".into(),
            criterion_id: "c1".into(),
            score: 7.5,
            max_score: 10.0,
            round: 2,
            justification: "Revised after considering social value gaps".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        },
        Score {
            agent_id: "a2".into(),
            criterion_id: "c1".into(),
            score: 6.5,
            max_score: 10.0,
            round: 2,
            justification: "Revised up after reviewing technical evidence".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        },
    ];

    let drift = debate::calculate_drift_velocity(&round1_scores, &round2_scores);
    assert!(drift > 0.0, "Should have positive drift");

    let converged = debate::check_convergence(drift, 2.0);
    assert!(converged, "Should converge after round 2");

    let debate_round2 =
        debate::build_debate_round(2, round2_scores.clone(), vec![], Some(drift), converged);
    assert!(debate_round2.converged);

    // Moderate final scores
    let moderated = moderation::calculate_moderated_scores(&round2_scores, &agents);
    assert_eq!(moderated.len(), 1);
    assert!(
        (moderated[0].panel_mean - 7.0).abs() < 1e-9,
        "Panel mean should be 7.0, got {}",
        moderated[0].panel_mean
    );
}
