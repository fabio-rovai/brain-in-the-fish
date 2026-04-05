//! Scaling benchmark: agent count sweep.
//!
//! Tests how each implementation scales with agent count: 5, 50, 500, 5000.
//! For naive: spawn agents + wire trust + score + moderate.
//! For tardygrada: TardyVm spawn + record + read verified + GC.

#[path = "fixtures/mock_data.rs"]
mod mock_data;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

const AGENT_COUNTS: &[usize] = &[5, 50, 500, 5000];

// ── Naive scaling ───────────────────────────────────────────────────

fn bench_naive_scaling(c: &mut Criterion) {
    let framework = mock_data::mock_framework();
    let naive_fw = bench_naive::types::Framework {
        id: framework.id.clone(),
        name: framework.name.clone(),
        total_weight: framework.criteria.iter().map(|c| c.weight).sum(),
        pass_mark: Some(framework.pass_mark),
        criteria: framework
            .criteria
            .iter()
            .map(|c| bench_naive::types::Criterion {
                id: c.id.clone(),
                title: c.title.clone(),
                description: None,
                max_score: c.max_score,
                weight: c.weight,
                rubric_levels: Vec::new(),
            })
            .collect(),
    };

    let mut group = c.benchmark_group("naive/scaling_agents");

    for &n_agents in AGENT_COUNTS {
        group.bench_with_input(BenchmarkId::from_parameter(n_agents), &n_agents, |b, &n| {
            b.iter(|| {
                // 1. Spawn agents
                let mut agents = bench_naive::agent::spawn_panel("tender benchmark", &naive_fw);

                // Extend to n agents by cloning and re-IDing
                while agents.len() < n {
                    let mut cloned = agents[agents.len() % 5].clone();
                    cloned.id = format!("agent-extra-{}", agents.len());
                    cloned.name = format!("Extra Agent {}", agents.len());
                    agents.push(cloned);
                }
                agents.truncate(n);

                // 2. Wire trust
                bench_naive::agent::wire_trust_weights(&mut agents);

                // 3. Score: one round, all agents x all criteria
                let mut scores = Vec::with_capacity(n * naive_fw.criteria.len());
                for (ai, agent) in agents.iter().enumerate() {
                    for (ci, criterion) in naive_fw.criteria.iter().enumerate() {
                        let base = 7.0;
                        let offset = ((ai % 4) as f64 - 1.5) * 0.7
                            + (ci as f64 - 2.0) * 0.3;
                        let score = (base + offset).clamp(1.0, 10.0);
                        scores.push(bench_naive::types::Score {
                            agent_id: agent.id.clone(),
                            criterion_id: criterion.id.clone(),
                            score,
                            max_score: criterion.max_score,
                            round: 1,
                            justification: format!("agent_{ai}_crit_{ci}"),
                            evidence_used: Vec::new(),
                            gaps_identified: Vec::new(),
                        });
                    }
                }

                // 4. Moderate
                let moderated = bench_naive::moderation::calculate_moderated_scores(&scores, &agents);
                let _overall = bench_naive::moderation::calculate_overall_score(&moderated, &naive_fw);
            })
        });
    }
    group.finish();
}

// ── Tardygrada scaling ──────────────────────────────────────────────

fn bench_tardygrada_scaling(c: &mut Criterion) {
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
        eprintln!("NOTE: Tardygrada VM FFI not functional — skipping tardygrada/scaling_agents benchmark");
        return;
    }

    let framework = mock_data::mock_framework();
    let criterion_ids: Vec<&str> = framework.criteria.iter().map(|c| c.id.as_str()).collect();

    let mut group = c.benchmark_group("tardygrada/scaling_agents");

    for &n_agents in AGENT_COUNTS {
        group.bench_with_input(BenchmarkId::from_parameter(n_agents), &n_agents, |b, &n| {
            // Pre-generate agent names
            let names: Vec<String> = (0..n).map(|i| format!("agent_{i}")).collect();

            b.iter(|| {
                let vm = match bench_tardygrada::vm_agents::TardyVm::new() {
                    Ok(vm) => vm,
                    Err(e) => {
                        eprintln!("TardyVm::new() failed: {e}");
                        return;
                    }
                };

                // 1. Spawn agents
                let mut agent_ids = Vec::with_capacity(n);
                for name in &names {
                    match bench_tardygrada::pipeline::spawn_evaluator(
                        &vm,
                        name,
                        "evaluator",
                        "benchmark",
                        bench_tardygrada::ffi::tardy_trust_t::TARDY_TRUST_DEFAULT,
                    ) {
                        Ok(id) => agent_ids.push((name.as_str(), id)),
                        Err(_) => break, // VM agent limit reached
                    }
                }

                // 2. Record scores for each agent x criterion
                for (ai, (_, agent_id)) in agent_ids.iter().enumerate() {
                    for (ci, &criterion) in criterion_ids.iter().enumerate() {
                        let base = 7.0;
                        let offset = ((ai % 4) as f64 - 1.5) * 0.7
                            + (ci as f64 - 2.0) * 0.3;
                        let score = (base + offset).clamp(1.0, 10.0);
                        let justification = format!("agent_{ai}_crit_{ci}");
                        let _ = bench_tardygrada::pipeline::record_score(
                            &vm, *agent_id, criterion, score, &justification,
                        );
                    }
                }

                // 3. Read verified scores
                for &criterion in &criterion_ids {
                    let _scores: Vec<f64> = agent_ids
                        .iter()
                        .filter_map(|(_, id)| {
                            bench_tardygrada::pipeline::read_score(&vm, *id, criterion)
                        })
                        .collect();
                }

                // 4. GC
                let _ = vm.gc();
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_naive_scaling, bench_tardygrada_scaling);
criterion_main!(benches);
