//! Debate-loop benchmark: measure the debate phase in isolation.
//!
//! For naive: find_disagreements + drift_velocity + convergence check.
//! For tardygrada: spawn agents + record scores + read verified + send challenges + GC.

#[path = "fixtures/mock_data.rs"]
mod mock_data;

use criterion::{criterion_group, criterion_main, Criterion};

// ── Naive debate benchmark ──────────────────────────────────────────

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

// ── Tardygrada debate benchmark ─────────────────────────────────────

fn bench_tardygrada_debate(c: &mut Criterion) {
    let agents = mock_data::mock_agents();
    let framework = mock_data::mock_framework();
    let mock_scores = mock_data::mock_scores();

    let evaluators: Vec<&mock_data::BenchAgent> =
        agents.iter().filter(|a| !a.is_moderator).collect();
    let agent_names: Vec<&str> = evaluators.iter().map(|a| a.name.as_str()).collect();
    let criterion_ids: Vec<&str> = framework.criteria.iter().map(|c| c.id.as_str()).collect();

    let agent_id_to_name: std::collections::HashMap<&str, &str> = agents
        .iter()
        .map(|a| (a.id.as_str(), a.name.as_str()))
        .collect();

    // Round 1 scores in tardygrada format
    let round1_scores: Vec<(&str, &str, f64)> = mock_scores
        .iter()
        .filter(|s| s.round == 1)
        .filter_map(|s| {
            agent_id_to_name
                .get(s.agent_id.as_str())
                .map(|name| (*name, s.criterion_id.as_str(), s.score))
        })
        .collect();

    // Pre-check: can the VM spawn agents?
    let vm_works = std::panic::catch_unwind(|| {
        let vm = bench_tardygrada::vm_agents::TardyVm::new().ok()?;
        bench_tardygrada::pipeline::spawn_evaluator(
            &vm, "test", "evaluator", "test",
            bench_tardygrada::ffi::tardy_trust_t::TARDY_TRUST_DEFAULT,
        ).ok()?;
        Some(())
    }).ok().flatten().is_some();

    if !vm_works {
        eprintln!("NOTE: Tardygrada VM FFI not functional — skipping tardygrada/debate_3rounds benchmark");
        return;
    }

    c.bench_function("tardygrada/debate_3rounds", |b| {
        b.iter(|| {
            let vm = match bench_tardygrada::vm_agents::TardyVm::new() {
                Ok(vm) => vm,
                Err(e) => {
                    eprintln!("TardyVm::new() failed: {e}");
                    return;
                }
            };

            // Spawn agents
            let mut agent_ids = Vec::new();
            for &name in &agent_names {
                match bench_tardygrada::pipeline::spawn_evaluator(
                    &vm,
                    name,
                    "evaluator",
                    "benchmark",
                    bench_tardygrada::ffi::tardy_trust_t::TARDY_TRUST_DEFAULT,
                ) {
                    Ok(id) => agent_ids.push((name, id)),
                    Err(_) => continue,
                }
            }

            // 3 rounds of debate
            for round in 0..3usize {
                // Record/mutate scores
                for &(agent_name, criterion, score) in &round1_scores {
                    if let Some((_, agent_id)) = agent_ids.iter().find(|(n, _)| *n == agent_name) {
                        let drifted = if round == 0 {
                            score
                        } else {
                            // Simple convergence drift
                            let current = bench_tardygrada::pipeline::read_score(&vm, *agent_id, criterion)
                                .unwrap_or(score);
                            current + (7.0 - current) * 0.3
                        };

                        if round == 0 {
                            let justification = format!("round_{round}_score_{drifted:.2}");
                            let _ = bench_tardygrada::pipeline::record_score(
                                &vm, *agent_id, criterion, drifted, &justification,
                            );
                        } else {
                            let score_name = format!("score_{criterion}");
                            let _ = vm.mutate_float(*agent_id, &score_name, drifted);
                        }
                    }
                }

                // Read verified scores + find disagreements
                let mut max_disagreement: f64 = 0.0;
                for &criterion in &criterion_ids {
                    let scores: Vec<f64> = agent_ids
                        .iter()
                        .filter_map(|(_, id)| bench_tardygrada::pipeline::read_score(&vm, *id, criterion))
                        .collect();
                    if scores.len() >= 2 {
                        let min = scores.iter().cloned().fold(f64::INFINITY, f64::min);
                        let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        let spread = max - min;
                        if spread > max_disagreement {
                            max_disagreement = spread;
                        }
                    }
                }

                // Send challenges if disagreement exceeds threshold
                if max_disagreement > 2.0 {
                    for i in 0..agent_ids.len() {
                        for j in (i + 1)..agent_ids.len() {
                            let challenge = format!("round_{round}_challenge");
                            let _ = bench_tardygrada::pipeline::send_challenge(
                                &vm,
                                agent_ids[i].1,
                                agent_ids[j].1,
                                &challenge,
                            );
                        }
                    }
                }

                // Drain responses
                for (_, agent_id) in &agent_ids {
                    while bench_tardygrada::pipeline::recv_response(&vm, *agent_id).is_some() {}
                }
            }

            // GC
            let _ = vm.gc();
        })
    });
}

criterion_group!(benches, bench_naive_debate, bench_tardygrada_debate);
criterion_main!(benches);
