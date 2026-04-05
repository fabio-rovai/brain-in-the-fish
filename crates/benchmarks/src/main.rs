//! Real subagent benchmark runner.
//!
//! Runs the full evaluation pipeline with real Claude API calls through both
//! bench-naive and bench-tardygrada, comparing wall-clock time.
//!
//! Usage:
//!   ANTHROPIC_API_KEY=sk-... cargo run -p benchmarks
//!
//! Optional env vars:
//!   BRAIN_MODEL — Claude model to use (default: claude-sonnet-4-6)
//!   BENCH_MAX_ROUNDS — max debate rounds (default: 3)
//!   BENCH_AGENTS — number of agents: "small" (3) or "full" (5, default)

use std::time::Instant;

use bench_naive::llm::ClaudeClient;
use bench_naive::types as naive_types;

// ── Fixture builders ──────────────────────────────────────────────

fn build_naive_document() -> naive_types::Document {
    let mut sections = Vec::new();
    for i in 0..5 {
        sections.push(naive_types::Section {
            id: format!("sec-{i}"),
            title: format!("Section {i}: Technical Approach"),
            text: format!(
                "This section describes deliverable {i}. We propose an evidence-based \
                 methodology grounded in measurable outcomes with clear milestones. \
                 Our team has delivered similar projects achieving 15% improvement \
                 in key performance indicators across multiple sectors."
            ),
            word_count: 50,
            claims: vec![
                naive_types::Claim {
                    id: format!("claim-{i}-0"),
                    text: format!("Deliverable {i} will improve outcomes by 15%"),
                    specificity: 0.7,
                    verifiable: true,
                },
                naive_types::Claim {
                    id: format!("claim-{i}-1"),
                    text: format!("Our methodology for {i} is evidence-based"),
                    specificity: 0.5,
                    verifiable: false,
                },
            ],
            evidence: vec![naive_types::Evidence {
                id: format!("ev-{i}-0"),
                source: format!("Case study {i}"),
                evidence_type: "case_study".into(),
                text: format!("Project {i} achieved 15% KPI improvement"),
                has_quantified_outcome: true,
            }],
        });
    }

    naive_types::Document {
        id: "doc-llm-bench".into(),
        title: "LLM Benchmark Tender Response".into(),
        doc_type: "tender".into(),
        total_pages: Some(5),
        total_word_count: Some(250),
        sections,
    }
}

fn build_naive_framework() -> naive_types::Framework {
    naive_types::Framework {
        id: "fw-llm-bench".into(),
        name: "LLM Benchmark Framework".into(),
        total_weight: 1.0,
        pass_mark: Some(60.0),
        criteria: vec![
            naive_types::Criterion {
                id: "crit-0".into(),
                title: "Technical Quality".into(),
                description: Some("Quality of technical approach and methodology".into()),
                max_score: 10.0,
                weight: 0.40,
                rubric_levels: vec![
                    naive_types::RubricLevel {
                        level: "Excellent".into(),
                        score_range: "8-10".into(),
                        descriptor: "Outstanding technical approach with clear evidence".into(),
                    },
                    naive_types::RubricLevel {
                        level: "Good".into(),
                        score_range: "5-7".into(),
                        descriptor: "Solid approach with some evidence gaps".into(),
                    },
                    naive_types::RubricLevel {
                        level: "Weak".into(),
                        score_range: "0-4".into(),
                        descriptor: "Insufficient detail or unsupported claims".into(),
                    },
                ],
            },
            naive_types::Criterion {
                id: "crit-1".into(),
                title: "Delivery Approach".into(),
                description: Some("Feasibility and realism of delivery plan".into()),
                max_score: 10.0,
                weight: 0.35,
                rubric_levels: Vec::new(),
            },
            naive_types::Criterion {
                id: "crit-2".into(),
                title: "Social Value".into(),
                description: Some("Social value and community benefit".into()),
                max_score: 10.0,
                weight: 0.25,
                rubric_levels: Vec::new(),
            },
        ],
    }
}

#[tokio::main]
async fn main() {
    // ── Check prerequisites ───────────────────────────────────────
    if !ClaudeClient::available() {
        eprintln!("ERROR: ANTHROPIC_API_KEY not set.");
        eprintln!("Usage: ANTHROPIC_API_KEY=sk-... cargo run -p benchmarks");
        std::process::exit(1);
    }

    let client = ClaudeClient::from_env().expect("Failed to create Claude client");
    let max_rounds: u32 = std::env::var("BENCH_MAX_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let disagreement_threshold = 0.3;
    let convergence_threshold = 0.05;

    let model = std::env::var("BRAIN_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    println!("=== Brain-in-the-Fish Real LLM Subagent Benchmark ===");
    println!();
    println!("Model:        {model}");
    println!("Max rounds:   {max_rounds}");
    println!("Disagreement: {disagreement_threshold}");
    println!("Convergence:  {convergence_threshold}");
    println!();

    // ── Build fixtures ────────────────────────────────────────────
    let doc = build_naive_document();
    let framework = build_naive_framework();

    // ── Run bench-naive LLM pipeline ──────────────────────────────
    println!("--- bench-naive (real LLM) ---");
    println!("Spawning agents and starting parallel LLM scoring...");

    let start = Instant::now();
    let (naive_session, naive_result, naive_verdict) =
        bench_naive::orchestrator::run_full_pipeline_llm(
            &client,
            doc.clone(),
            framework.clone(),
            max_rounds,
            disagreement_threshold,
            convergence_threshold,
        )
        .await;
    let naive_time = start.elapsed();

    println!("  Completed in {naive_time:.2?}");
    println!("  Rounds:    {}", naive_session.rounds.len());
    println!("  Score:     {:.1}%", naive_result.percentage);
    println!("  Passed:    {}", naive_result.passed);
    println!("  Verdict:   {naive_verdict}");
    println!();

    // Print per-criterion scores
    for ms in &naive_session.final_scores {
        println!(
            "  {}: consensus={:.2}, mean={:.2}, std_dev={:.2}",
            ms.criterion_id, ms.consensus_score, ms.panel_mean, ms.panel_std_dev
        );
    }
    println!();

    // ── Run bench-tardygrada LLM pipeline ─────────────────────────
    println!("--- bench-tardygrada (real LLM + Tardygrada VM) ---");

    // Check if VM works
    let vm_works = std::panic::catch_unwind(|| {
        let vm = bench_tardygrada::vm_agents::TardyVm::new().ok()?;
        bench_tardygrada::pipeline::spawn_evaluator(
            &vm,
            "test",
            "evaluator",
            "test",
            bench_tardygrada::ffi::tardy_trust_t::TARDY_TRUST_DEFAULT,
        )
        .ok()?;
        Some(())
    })
    .ok()
    .flatten()
    .is_some();

    let tardy_time;
    let tardy_result_json: String;

    if vm_works {
        println!("Spawning VM agents and starting parallel LLM scoring...");

        let agent_names: Vec<&str> = vec![
            "Budget Expert",
            "Technical Evaluator",
            "Delivery Specialist",
            "Social Value Assessor",
        ];
        let criterion_ids: Vec<&str> = framework.criteria.iter().map(|c| c.id.as_str()).collect();

        // Build section texts and alignments for tardygrada
        let section_texts: Vec<(&str, &str)> = doc
            .sections
            .iter()
            .map(|s| (s.id.as_str(), s.text.as_str()))
            .collect();

        let alignments: Vec<(&str, &str, f64)> = vec![
            ("sec-0", "crit-0", 0.95),
            ("sec-1", "crit-0", 0.80),
            ("sec-2", "crit-1", 0.90),
            ("sec-3", "crit-1", 0.75),
            ("sec-4", "crit-2", 0.85),
        ];

        let start = Instant::now();
        let vm = bench_tardygrada::vm_agents::TardyVm::new()
            .expect("TardyVm creation should succeed");
        tardy_result_json = bench_tardygrada::orchestrator::run_full_pipeline_llm(
            &client,
            &vm,
            &agent_names,
            &criterion_ids,
            &alignments,
            &section_texts,
            max_rounds as usize,
            disagreement_threshold,
            convergence_threshold,
        )
        .await;
        tardy_time = start.elapsed();

        println!("  Completed in {tardy_time:.2?}");
        // Parse and display key fields from result JSON
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&tardy_result_json) {
            println!(
                "  Verdict:   {}",
                v["verdict"].as_str().unwrap_or("unknown")
            );
            println!(
                "  Overall:   {:.4}",
                v["overall_score"].as_f64().unwrap_or(0.0)
            );
            println!(
                "  Converged: {}",
                v["converged"].as_bool().unwrap_or(false)
            );
            println!("  Rounds:    {}", v["rounds"].as_u64().unwrap_or(0));
            println!(
                "  GC:        {} collected",
                v["gc_collected"].as_i64().unwrap_or(0)
            );
        }
    } else {
        println!("  SKIPPED: Tardygrada VM FFI not functional (zero UUID on spawn).");
        println!("  This is a known struct-layout mismatch — see BENCHMARK.md for details.");
        tardy_time = std::time::Duration::ZERO;
        tardy_result_json = "{}".to_string();
    }

    // ── Comparison table ──────────────────────────────────────────
    println!();
    println!("=== Benchmark Comparison ===");
    println!();
    println!(
        "{:<25} {:<15} {:<10} {:<10}",
        "Pipeline", "Wall-clock", "Rounds", "Verdict"
    );
    println!("{}", "-".repeat(60));
    println!(
        "{:<25} {:<15} {:<10} {:<10}",
        "bench-naive (LLM)",
        format!("{naive_time:.2?}"),
        naive_session.rounds.len(),
        format!("{naive_verdict}"),
    );
    if vm_works {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&tardy_result_json) {
            println!(
                "{:<25} {:<15} {:<10} {:<10}",
                "bench-tardygrada (LLM)",
                format!("{tardy_time:.2?}"),
                v["rounds"].as_u64().unwrap_or(0),
                v["verdict"].as_str().unwrap_or("unknown"),
            );
        }
    } else {
        println!(
            "{:<25} {:<15} {:<10} {:<10}",
            "bench-tardygrada (LLM)", "SKIPPED", "-", "-",
        );
    }
    println!();

    if vm_works && !tardy_time.is_zero() {
        let overhead = if naive_time.as_nanos() > 0 {
            (tardy_time.as_secs_f64() / naive_time.as_secs_f64() - 1.0) * 100.0
        } else {
            0.0
        };
        println!(
            "Tardygrada overhead: {overhead:+.1}% (cost of VM integrity, provenance, hash-verified reads)"
        );
    }

    println!();
    println!("NOTE: Wall-clock time is dominated by LLM API latency, not coordination.");
    println!("      Use `cargo bench -p benchmarks` for pure coordination overhead measurement.");
}
