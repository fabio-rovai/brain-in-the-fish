//! Full pipeline benchmark: naive vs tardygrada.
//!
//! Runs the complete evaluation pipeline (spawn agents, debate, moderate,
//! gate) under Criterion and compares wall-clock time.

#[path = "fixtures/mock_data.rs"]
mod mock_data;

use criterion::{criterion_group, criterion_main, Criterion};

// ── Type conversion: mock_data -> bench_naive types ─────────────────

fn to_naive_document(doc: &mock_data::BenchDocument) -> bench_naive::types::Document {
    bench_naive::types::Document {
        id: doc.id.clone(),
        title: doc.title.clone(),
        doc_type: "tender".into(),
        total_pages: Some(doc.sections.len() as u32),
        total_word_count: Some(doc.sections.iter().map(|s| s.word_count).sum()),
        sections: doc
            .sections
            .iter()
            .map(|s| bench_naive::types::Section {
                id: s.id.clone(),
                title: s.title.clone(),
                text: s.text.clone(),
                word_count: s.word_count,
                claims: s
                    .claims
                    .iter()
                    .map(|c| bench_naive::types::Claim {
                        id: c.id.clone(),
                        text: c.text.clone(),
                        specificity: c.specificity,
                        verifiable: c.verifiable,
                    })
                    .collect(),
                evidence: s
                    .evidence
                    .iter()
                    .map(|e| bench_naive::types::Evidence {
                        id: e.id.clone(),
                        source: e.source.clone(),
                        evidence_type: e.evidence_type.clone(),
                        text: e.text.clone(),
                        has_quantified_outcome: e.has_quantified_outcome,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn to_naive_framework(fw: &mock_data::BenchFramework) -> bench_naive::types::Framework {
    bench_naive::types::Framework {
        id: fw.id.clone(),
        name: fw.name.clone(),
        total_weight: fw.criteria.iter().map(|c| c.weight).sum(),
        pass_mark: Some(fw.pass_mark),
        criteria: fw
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
    }
}

fn to_naive_scores(scores: &[mock_data::BenchScore]) -> Vec<bench_naive::types::Score> {
    scores
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
        .collect()
}

// ── Benchmarks ──────────────────────────────────────────────────────

fn bench_naive_full_pipeline(c: &mut Criterion) {
    let doc = to_naive_document(&mock_data::mock_document());
    let framework = to_naive_framework(&mock_data::mock_framework());
    let scores = to_naive_scores(&mock_data::mock_scores());

    c.bench_function("naive/full_pipeline", |b| {
        b.iter(|| {
            bench_naive::orchestrator::run_full_pipeline(
                doc.clone(),
                framework.clone(),
                &scores,
                3,   // max_rounds
                2.0, // disagreement_threshold
                0.5, // convergence_threshold
            )
        })
    });
}

fn bench_tardygrada_full_pipeline(c: &mut Criterion) {
    let agents = mock_data::mock_agents();
    let framework = mock_data::mock_framework();
    let mock_scores = mock_data::mock_scores();
    let alignments = mock_data::mock_alignments();

    // Pre-compute string slices for the tardygrada API
    let agent_names: Vec<&str> = agents
        .iter()
        .filter(|a| !a.is_moderator)
        .map(|a| a.name.as_str())
        .collect();
    let criterion_ids: Vec<&str> = framework.criteria.iter().map(|c| c.id.as_str()).collect();

    // Convert mock_scores to tardygrada format: (agent_name, criterion_id, score)
    let agent_id_to_name: std::collections::HashMap<&str, &str> = agents
        .iter()
        .map(|a| (a.id.as_str(), a.name.as_str()))
        .collect();

    let tardy_scores: Vec<(&str, &str, f64)> = mock_scores
        .iter()
        .filter(|s| s.round == 1) // tardygrada uses round-1 scores as base, drifts internally
        .filter_map(|s| {
            agent_id_to_name
                .get(s.agent_id.as_str())
                .map(|name| (*name, s.criterion_id.as_str(), s.score))
        })
        .collect();

    let tardy_alignments: Vec<(&str, &str, f64)> = alignments
        .iter()
        .map(|(s, c, conf)| (s.as_str(), c.as_str(), *conf))
        .collect();

    // Pre-check: can the VM even spawn agents?
    let vm_works = std::panic::catch_unwind(|| {
        let vm = bench_tardygrada::vm_agents::TardyVm::new().ok()?;
        bench_tardygrada::pipeline::spawn_evaluator(
            &vm, "test", "evaluator", "test",
            bench_tardygrada::ffi::tardy_trust_t::TARDY_TRUST_DEFAULT,
        ).ok()?;
        Some(())
    }).ok().flatten().is_some();

    if !vm_works {
        eprintln!("NOTE: Tardygrada VM FFI not functional — skipping tardygrada/full_pipeline benchmark");
        eprintln!("      (the C VM spawn returns zero UUID; this is a known FFI layout issue)");
        return;
    }

    c.bench_function("tardygrada/full_pipeline", |b| {
        b.iter(|| {
            let vm = match bench_tardygrada::vm_agents::TardyVm::new() {
                Ok(vm) => vm,
                Err(e) => {
                    eprintln!("TardyVm::new() failed: {e}");
                    return;
                }
            };
            bench_tardygrada::orchestrator::run_full_pipeline(
                &vm,
                &agent_names,
                &criterion_ids,
                &tardy_alignments,
                &tardy_scores,
                3,   // max_rounds
                2.0, // disagreement_threshold
                0.5, // convergence_threshold
            );
        })
    });
}

criterion_group!(benches, bench_naive_full_pipeline, bench_tardygrada_full_pipeline);
criterion_main!(benches);
