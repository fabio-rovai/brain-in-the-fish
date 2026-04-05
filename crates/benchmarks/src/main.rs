//! Benchmark runner: real subagent scores, coordination timing.
//!
//! Loads `data/bench_scores.json` (real Claude subagent scores) and times
//! the naive Rust coordination pipeline, then shells out to the pure C
//! Tardygrada benchmark for comparison.

use std::time::Instant;

use serde::Deserialize;

// ============================================================================
// JSON schema matching bench_scores.json
// ============================================================================

#[derive(Debug, Deserialize)]
struct ScoresFile {
    round_1: Vec<RawScore>,
    round_2: Vec<RawScore>,
    debate: DebateEvent,
}

#[derive(Debug, Deserialize)]
struct RawScore {
    agent_id: String,
    criterion_id: String,
    score: f64,
    max_score: f64,
    round: u32,
    justification: String,
}

#[derive(Debug, Deserialize)]
struct DebateEvent {
    #[allow(dead_code)]
    round: u32,
    criterion: String,
    challenger: String,
    target: String,
    original_score: f64,
    new_score: f64,
    #[allow(dead_code)]
    score_changed: bool,
}

// ============================================================================
// Convert raw scores -> bench-naive Score types
// ============================================================================

fn to_naive_scores(raw: &[RawScore]) -> Vec<bench_naive::types::Score> {
    raw.iter()
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

// ============================================================================
// Fixture builders (10 sections, 5 criteria -- matching bench_scores.json)
// ============================================================================

fn build_document() -> bench_naive::types::Document {
    let mut sections = Vec::with_capacity(10);
    for s in 0..10 {
        let mut claims = Vec::with_capacity(3);
        for c in 0..3 {
            claims.push(bench_naive::types::Claim {
                id: format!("claim-{s}-{c}"),
                text: format!("Section {s} claim {c}: the approach delivers measurable outcomes."),
                specificity: 0.6 + (c as f64) * 0.1,
                verifiable: c < 2,
            });
        }
        let mut evidence = Vec::with_capacity(2);
        for e in 0..2 {
            evidence.push(bench_naive::types::Evidence {
                id: format!("ev-{s}-{e}"),
                source: format!("Case study {s}-{e}"),
                evidence_type: if e == 0 { "case_study".into() } else { "statistic".into() },
                text: format!("Evidence {e} for section {s}: 15% improvement in KPI."),
                has_quantified_outcome: e == 1,
            });
        }
        let word_count = 200 + (s as u32) * 30;
        sections.push(bench_naive::types::Section {
            id: format!("sec-{s}"),
            title: format!("Section {s}: Technical Approach"),
            text: format!(
                "## Section {s}: Technical Approach\n\nThis section describes the approach for deliverable {s}. {}",
                "Lorem ipsum dolor sit amet. ".repeat(word_count as usize / 5)
            ),
            word_count,
            claims,
            evidence,
        });
    }
    bench_naive::types::Document {
        id: "doc-bench-001".into(),
        title: "Benchmark Tender Response".into(),
        doc_type: "tender".into(),
        total_pages: Some(10),
        total_word_count: Some(sections.iter().map(|s| s.word_count).sum()),
        sections,
    }
}

fn build_framework() -> bench_naive::types::Framework {
    bench_naive::types::Framework {
        id: "fw-bench-001".into(),
        name: "Benchmark Framework".into(),
        total_weight: 1.0,
        pass_mark: Some(60.0),
        criteria: vec![
            bench_naive::types::Criterion {
                id: "crit-0".into(),
                title: "Technical Quality".into(),
                description: None,
                max_score: 10.0,
                weight: 0.30,
                rubric_levels: Vec::new(),
            },
            bench_naive::types::Criterion {
                id: "crit-1".into(),
                title: "Delivery Approach".into(),
                description: None,
                max_score: 10.0,
                weight: 0.25,
                rubric_levels: Vec::new(),
            },
            bench_naive::types::Criterion {
                id: "crit-2".into(),
                title: "Social Value".into(),
                description: None,
                max_score: 10.0,
                weight: 0.20,
                rubric_levels: Vec::new(),
            },
            bench_naive::types::Criterion {
                id: "crit-3".into(),
                title: "Risk Management".into(),
                description: None,
                max_score: 10.0,
                weight: 0.15,
                rubric_levels: Vec::new(),
            },
            bench_naive::types::Criterion {
                id: "crit-4".into(),
                title: "Innovation".into(),
                description: None,
                max_score: 10.0,
                weight: 0.10,
                rubric_levels: Vec::new(),
            },
        ],
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    // -- Load real subagent scores -------------------------------------------
    let scores_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/bench_scores.json");
    let raw = std::fs::read_to_string(scores_path)
        .unwrap_or_else(|e| panic!("Failed to read {scores_path}: {e}"));
    let scores_file: ScoresFile =
        serde_json::from_str(&raw).expect("Failed to parse bench_scores.json");

    // -- Print subagent scoring summary --------------------------------------
    println!("=======================================================");
    println!("  SUBAGENT SCORES (real Claude output)");
    println!("=======================================================");

    let criterion_ids = ["crit-0", "crit-1", "crit-2", "crit-3", "crit-4"];
    let criterion_names = [
        "Technical Quality",
        "Delivery Approach",
        "Social Value",
        "Risk Management",
        "Innovation",
    ];

    println!("\n{:<22} {:>8} {:>8}", "Criterion", "R1 mean", "R2 mean");
    println!("{}", "-".repeat(42));

    for (i, crit) in criterion_ids.iter().enumerate() {
        let r1: Vec<f64> = scores_file
            .round_1
            .iter()
            .filter(|s| s.criterion_id == *crit)
            .map(|s| s.score)
            .collect();
        let r2: Vec<f64> = scores_file
            .round_2
            .iter()
            .filter(|s| s.criterion_id == *crit)
            .map(|s| s.score)
            .collect();
        let mean1 = r1.iter().sum::<f64>() / r1.len() as f64;
        let mean2 = r2.iter().sum::<f64>() / r2.len() as f64;
        println!("{:<22} {:>8.2} {:>8.2}", criterion_names[i], mean1, mean2);
    }

    println!("\nDebate event: {} challenged {} on {} (score {} -> {})",
        scores_file.debate.challenger,
        scores_file.debate.target,
        scores_file.debate.criterion,
        scores_file.debate.original_score,
        scores_file.debate.new_score,
    );

    let total_scores = scores_file.round_1.len() + scores_file.round_2.len();
    println!("Total score records: {} ({} R1 + {} R2)",
        total_scores, scores_file.round_1.len(), scores_file.round_2.len());

    // -- Build fixtures ------------------------------------------------------
    let doc = build_document();
    let framework = build_framework();

    // Combine round 1 + round 2 scores for naive pipeline
    let mut all_raw: Vec<&RawScore> = Vec::new();
    all_raw.extend(scores_file.round_1.iter());
    all_raw.extend(scores_file.round_2.iter());
    let naive_scores = to_naive_scores(
        &all_raw.iter().map(|s| RawScore {
            agent_id: s.agent_id.clone(),
            criterion_id: s.criterion_id.clone(),
            score: s.score,
            max_score: s.max_score,
            round: s.round,
            justification: s.justification.clone(),
        }).collect::<Vec<_>>()
    );

    // -- Benchmark: Naive coordination ---------------------------------------
    println!("\n=======================================================");
    println!("  COORDINATION BENCHMARK");
    println!("=======================================================\n");

    let naive_start = Instant::now();
    let (naive_session, naive_overall, naive_verdict) =
        bench_naive::orchestrator::run_full_pipeline(
            doc.clone(),
            framework.clone(),
            &naive_scores,
            2,   // max_rounds (we have R1 + R2)
            2.0, // disagreement_threshold
            0.5, // convergence_threshold
        );
    let naive_elapsed = naive_start.elapsed();

    // -- Results table -------------------------------------------------------
    println!("{:<30} {:>15}", "Pipeline", "Time");
    println!("{}", "-".repeat(47));
    println!("{:<30} {:>12.3} ms", "Naive (Rust)",
        naive_elapsed.as_secs_f64() * 1000.0);

    // -- Verdicts ------------------------------------------------------------
    println!("\n=======================================================");
    println!("  VERDICTS");
    println!("=======================================================\n");

    println!("Naive verdict:       {}", naive_verdict);
    println!("  Overall score:     {:.4} / {:.4}",
        naive_overall.weighted_score, naive_overall.max_possible);
    println!("  Debate rounds:     {}", naive_session.rounds.len());
    println!("  Final criteria:    {}", naive_session.final_scores.len());

    // -- Pure C benchmark ----------------------------------------------------
    println!("\n=======================================================");
    println!("  PURE TARDYGRADA C (zero deps)");
    println!("=======================================================\n");

    let pure_output = std::process::Command::new("./crates/bench-tardygrada/bench_pure")
        .output();
    match pure_output {
        Ok(out) => print!("{}", String::from_utf8_lossy(&out.stdout)),
        Err(_) => println!("(not built -- run: cd crates/bench-tardygrada && make bench_pure_simple)"),
    }

    println!("\n=======================================================");
    println!("  DONE -- {} real scores processed", total_scores);
    println!("=======================================================");
}
