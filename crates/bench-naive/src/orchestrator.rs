//! Orchestrator — runs the full evaluation pipeline end-to-end.
//!
//! `run_full_pipeline` — mock-based, deterministic, for Criterion coordination benchmarks.

use uuid::Uuid;

use crate::agent;
use crate::alignment;
use crate::argument_graph;
use crate::debate;
use crate::gate;
use crate::moderation::{self, OverallResult};
use crate::rules;
use crate::scoring;
use crate::types::{
    Document, Framework, GateWeights, Score, Session, Verdict,
};

use open_ontologies::graph::GraphStore;

/// Run the full naive evaluation pipeline with MOCK scores.
///
/// Kept for Criterion coordination benchmarks — no API calls, deterministic.
pub fn run_full_pipeline(
    doc: Document,
    framework: Framework,
    mock_scores: &[Score],
    max_rounds: u32,
    disagreement_threshold: f64,
    convergence_threshold: f64,
) -> (Session, OverallResult, Verdict) {
    // 1. Spawn agents
    let mut agents = agent::spawn_panel(&doc.title, &framework);
    agent::wire_trust_weights(&mut agents);

    // 2. Align sections to criteria
    let (alignments, gaps) = alignment::align_sections_to_criteria(&doc, &framework);

    // 3. Build argument graph
    let arg_graph = argument_graph::build_from_document(&doc);

    // 4. Debate loop
    let mut all_scores: Vec<Score> = Vec::new();
    let mut rounds = Vec::new();

    for round in 1..=max_rounds {
        // Collect scores for this round from mock data
        let round_scores: Vec<Score> = mock_scores
            .iter()
            .filter(|s| s.round == round)
            .cloned()
            .collect();

        for s in &round_scores {
            scoring::record_score(&mut all_scores, s.clone());
        }

        // Find disagreements
        let _disagreements =
            debate::find_disagreements(&round_scores, disagreement_threshold);

        // Build challenges (in real pipeline, LLM would generate these)
        let challenges = Vec::new(); // No actual LLM calls in benchmark mode

        // Calculate drift
        let drift_velocity = if round > 1 {
            let prev = scoring::get_scores_for_round(&all_scores, round - 1);
            let curr = scoring::get_scores_for_round(&all_scores, round);
            let prev_owned: Vec<Score> = prev.into_iter().cloned().collect();
            let curr_owned: Vec<Score> = curr.into_iter().cloned().collect();
            Some(debate::calculate_drift_velocity(&prev_owned, &curr_owned))
        } else {
            None
        };

        let converged = drift_velocity
            .map(|dv| debate::check_convergence(dv, convergence_threshold))
            .unwrap_or(false);

        // Update trust weights based on challenges
        debate::update_trust_weights(&mut agents, &challenges);

        rounds.push(debate::build_debate_round(
            round,
            round_scores,
            challenges,
            drift_velocity,
            converged,
        ));

        if converged {
            break;
        }
    }

    // 5. Moderation
    let moderated = moderation::calculate_moderated_scores(&all_scores, &agents);
    let overall = moderation::calculate_overall_score(&moderated, &framework);

    // 6. Rules — mine facts via GraphStore
    let graph_store = GraphStore::new();
    let facts = rules::mine_facts(&graph_store, &doc);
    let total_claims: usize = doc.sections.iter().map(|s| s.claims.len()).sum();

    // 7. Gate check
    let verdict = gate::check(
        overall.weighted_score,
        overall.max_possible,
        &arg_graph,
        &facts,
        total_claims,
        &GateWeights::default(),
    );

    // Build session
    let session = Session {
        id: Uuid::new_v4().to_string(),
        document: doc,
        framework,
        agents,
        alignments,
        gaps,
        rounds,
        final_scores: moderated,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    (session, overall, verdict)
}

