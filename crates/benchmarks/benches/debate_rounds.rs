//! Debate-loop benchmark: measure the debate phase in isolation.
//!
//! For naive: find_disagreements + drift_velocity + convergence check.

#[path = "fixtures/mock_data.rs"]
mod mock_data;

use criterion::{criterion_group, criterion_main, Criterion};

// -- Naive debate benchmark --------------------------------------------------

fn bench_naive_debate(c: &mut Criterion) {
    let mock_scores = mock_data::mock_scores();

    // Convert to naive types
    let naive_scores: Vec<bench_naive::types::Score> = mock_scores
        .iter()
        .map(|s| bench_naive::types::Score {
            agent_id: s.agent_id.clone(),
            criterion_id: s.criterion_id.clone(),
            score: s.score,
            max_score: s.max_score,
            round: s.round,
            justification: s.justification.clone(),
            evidence_used: Vec::new(),
            gaps_identified: Vec::new(),
        })
        .collect();

    let round1: Vec<bench_naive::types::Score> = naive_scores
        .iter()
        .filter(|s| s.round == 1)
        .cloned()
        .collect();
    let round2: Vec<bench_naive::types::Score> = naive_scores
        .iter()
        .filter(|s| s.round == 2)
        .cloned()
        .collect();
    let round3: Vec<bench_naive::types::Score> = naive_scores
        .iter()
        .filter(|s| s.round == 3)
        .cloned()
        .collect();

    c.bench_function("naive/debate_3rounds", |b| {
        b.iter(|| {
            // Round 1: find disagreements
            let _d1 = bench_naive::debate::find_disagreements(&round1, 2.0);

            // Round 1->2: drift velocity + convergence
            let drift_1_2 = bench_naive::debate::calculate_drift_velocity(&round1, &round2);
            let _conv_1_2 = bench_naive::debate::check_convergence(drift_1_2, 0.5);

            // Round 2: find disagreements
            let _d2 = bench_naive::debate::find_disagreements(&round2, 2.0);

            // Round 2->3: drift velocity + convergence
            let drift_2_3 = bench_naive::debate::calculate_drift_velocity(&round2, &round3);
            let _conv_2_3 = bench_naive::debate::check_convergence(drift_2_3, 0.5);

            // Round 3: find disagreements
            let _d3 = bench_naive::debate::find_disagreements(&round3, 2.0);
        })
    });
}

criterion_group!(benches, bench_naive_debate);
criterion_main!(benches);
