//! Integration test: full evaluation pipeline from document to report.

use brain_in_the_fish_core::agent;
use brain_in_the_fish_core::criteria;
use brain_in_the_fish_core::debate;
use brain_in_the_fish_core::ingest;
use brain_in_the_fish_core::moderation;
use brain_in_the_fish_core::report;
use brain_in_the_fish_core::scoring;
use brain_in_the_fish_core::types::*;
use open_ontologies::graph::GraphStore;

#[test]
fn test_full_pipeline_essay_evaluation() {
    // 1. Create a test document (inline, not from PDF)
    let doc = EvalDocument {
        id: "test-doc".into(),
        title: "Test Essay".into(),
        doc_type: "essay".into(),
        total_pages: Some(5),
        total_word_count: Some(1000),
        sections: vec![
            Section {
                id: "sec-intro".into(),
                title: "Introduction".into(),
                text: "This essay examines the impact of quantitative easing on inflation.".into(),
                word_count: 12,
                page_range: Some("1".into()),
                claims: vec![Claim {
                    id: "claim-1".into(),
                    text: "QE had limited direct impact on CPI inflation".into(),
                    specificity: 0.8,
                    verifiable: true,
                }],
                evidence: vec![Evidence {
                    id: "ev-1".into(),
                    source: "Bank of England, 2023".into(),
                    evidence_type: "primary_data".into(),
                    text: "BoE data shows M4 growth of 12% during QE periods".into(),
                    has_quantified_outcome: true,
                }],
                subsections: vec![],
            },
            Section {
                id: "sec-analysis".into(),
                title: "Analysis".into(),
                text: "Our analysis shows that transmission mechanisms were disrupted.".into(),
                word_count: 10,
                page_range: Some("2-3".into()),
                claims: vec![],
                evidence: vec![],
                subsections: vec![],
            },
        ],
    };

    // 2. Load into graph store
    let graph = GraphStore::new();
    let doc_triples = ingest::load_document_ontology(&graph, &doc).unwrap();
    assert!(doc_triples > 0, "Document should produce triples");

    // 3. Load criteria
    let framework = criteria::academic_essay_framework();
    let crit_triples = criteria::load_criteria_ontology(&graph, &framework).unwrap();
    assert!(crit_triples > 0, "Criteria should produce triples");

    // 4. Spawn agents
    let agents = agent::spawn_panel("mark this essay", &framework);
    assert!(
        agents.len() >= 4,
        "Academic panel should have at least 4 agents, got {}",
        agents.len()
    );
    assert!(agents.iter().any(|a| a.role == "Panel Moderator"));

    for a in &agents {
        let a_triples = agent::load_agent_ontology(&graph, a).unwrap();
        assert!(a_triples > 0, "Agent {} should produce triples", a.name);
    }

    // 5. Score (round 1)
    let mut round1_scores = Vec::new();
    for a in &agents {
        for c in &framework.criteria {
            let score = Score {
                agent_id: a.id.clone(),
                criterion_id: c.id.clone(),
                score: 6.0,
                max_score: c.max_score,
                round: 1,
                justification: format!("{} scored {}", a.name, c.title),
                evidence_used: vec!["ev-1".into()],
                gaps_identified: vec![],
            };
            scoring::record_score(&graph, &score).unwrap();
            round1_scores.push(score);
        }
    }
    assert!(!round1_scores.is_empty());

    // 6. Check disagreements
    let disagreements = debate::find_disagreements(&round1_scores, 2.0);
    // All scores are 6.0 so no disagreements expected
    assert!(
        disagreements.is_empty(),
        "Same scores should produce no disagreements"
    );

    // 7. Moderation
    let moderated = moderation::calculate_moderated_scores(&round1_scores, &agents);
    assert!(!moderated.is_empty());

    let overall = moderation::calculate_overall_score(&moderated, &framework);
    assert!(overall.percentage > 0.0);

    // 8. Report
    let session = EvaluationSession {
        rounds: vec![debate::build_debate_round(
            1,
            round1_scores,
            vec![],
            None,
            true,
        )],
        final_scores: moderated,
        ..EvaluationSession::new(doc, framework, agents)
    };

    let report_text = report::generate_report(&session, &overall);
    assert!(report_text.contains("Evaluation Report"));
    assert!(report_text.contains("Scorecard"));
    assert!(report_text.contains("Executive Summary"));
}

#[test]
fn test_full_pipeline_tender_evaluation() {
    let doc = EvalDocument {
        id: "bid-doc".into(),
        title: "Acme Corp Bid".into(),
        doc_type: "bid".into(),
        total_pages: Some(40),
        total_word_count: Some(15000),
        sections: vec![Section {
            id: "sec-tech".into(),
            title: "Technical Approach".into(),
            text: "We propose using machine learning for automated triage.".into(),
            word_count: 10,
            page_range: None,
            claims: vec![],
            evidence: vec![],
            subsections: vec![],
        }],
    };

    let graph = GraphStore::new();
    ingest::load_document_ontology(&graph, &doc).unwrap();

    let framework = criteria::generic_quality_framework();
    criteria::load_criteria_ontology(&graph, &framework).unwrap();

    let agents = agent::spawn_panel("score this tender bid", &framework);
    assert!(
        agents.len() >= 5,
        "Tender panel should have at least 5 agents, got {}",
        agents.len()
    );

    // Verify agents loaded into graph
    for a in &agents {
        agent::load_agent_ontology(&graph, a).unwrap();
    }

    // Score all criteria, run moderation, generate report
    let mut scores = Vec::new();
    for a in &agents {
        for c in &framework.criteria {
            let score = Score {
                agent_id: a.id.clone(),
                criterion_id: c.id.clone(),
                score: 7.0,
                max_score: c.max_score,
                round: 1,
                justification: format!("{} evaluates {}", a.name, c.title),
                evidence_used: vec![],
                gaps_identified: vec![],
            };
            scoring::record_score(&graph, &score).unwrap();
            scores.push(score);
        }
    }

    let moderated = moderation::calculate_moderated_scores(&scores, &agents);
    let overall = moderation::calculate_overall_score(&moderated, &framework);
    assert!(
        overall.percentage > 0.0,
        "Tender evaluation should produce a non-zero percentage"
    );

    let session = EvaluationSession {
        rounds: vec![debate::build_debate_round(1, scores, vec![], None, true)],
        final_scores: moderated,
        ..EvaluationSession::new(doc, framework, agents)
    };

    let report_text = report::generate_report(&session, &overall);
    assert!(report_text.contains("Evaluation Report"));
    assert!(report_text.contains("Acme Corp Bid"));
}
