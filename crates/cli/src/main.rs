use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use rmcp::ServiceExt;

use brain_in_the_fish_core::*;
use brain_in_the_fish_core::types::*;
use brain_in_the_fish_core::alignment;
use brain_in_the_fish_core::snn;
use brain_in_the_fish_core::memory;
use brain_in_the_fish_core::semantic;
use brain_in_the_fish_core::validate;
use brain_in_the_fish_core::predict;
use brain_in_the_fish_core::benchmark;
use brain_in_the_fish_core::shoal;

#[derive(Parser)]
#[command(name = "brain-in-the-fish", version, about = "Universal document evaluation engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Evaluate a document (deterministic pipeline + SNN scoring)
    Evaluate {
        /// Path to the document to evaluate
        document: PathBuf,

        /// Evaluation intent (what to assess)
        #[arg(long)]
        intent: String,

        /// Path to a criteria file (YAML/JSON ontology)
        #[arg(long)]
        criteria: Option<PathBuf>,

        /// Output directory for the evaluation report
        #[arg(long)]
        output: Option<PathBuf>,

        /// Open the graph visualization in browser after evaluation
        #[arg(long)]
        open: bool,

        /// Enable prediction credibility assessment
        #[arg(long)]
        predict: bool,

        /// Enable deep validation (all 15 checks). Default uses 8 core checks.
        #[arg(long)]
        deep_validate: bool,

        /// Generate Claude subagent orchestration tasks
        #[arg(long)]
        orchestrate: bool,
    },
    /// Open the knowledge graph visualization
    Graph {
        /// Path to an evaluation output directory (defaults to most recent)
        path: Option<PathBuf>,
    },
    /// Run benchmarks against labeled datasets
    Benchmark {
        /// Path to labeled dataset (JSON)
        #[arg(long)]
        dataset: Option<PathBuf>,

        /// Run ablation experiments
        #[arg(long)]
        ablation: bool,

        /// Run benchmark across all available datasets for cross-domain comparison
        #[arg(long)]
        multi_dataset: bool,

        /// Path to LLM-extracted evidence JSON (produced by Claude subagent)
        #[arg(long)]
        extractions: Option<PathBuf>,

        /// Path to graph node scores JSON (Option B: subagent scores nodes, SNN aggregates)
        #[arg(long)]
        graph_scores: Option<PathBuf>,

        /// Path to reference evaluation ontology (Turtle) for onto_align
        #[arg(long)]
        reference_ontology: Option<PathBuf>,

        /// Self-calibrate SNN weights via Nelder-Mead optimization
        #[arg(long)]
        calibrate: bool,

        /// Output directory for results
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// View cross-evaluation history and trends
    History {
        /// History directory (default: ~/.brain-in-the-fish/history/)
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Start the MCP server for Claude subagent orchestration
    Serve {
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port to listen on
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Batch-score a dataset using Claude subagents (shoal mode)
    Shoal {
        /// Path to labeled dataset (JSON)
        dataset: PathBuf,

        /// Batch size (essays per subagent)
        #[arg(long, default_value_t = 10)]
        batch_size: usize,

        /// Output directory
        #[arg(long, default_value = "/tmp/shoal")]
        output: PathBuf,

        /// Score scale max
        #[arg(long, default_value_t = 5.0)]
        max_score: f64,

        /// Evaluation intent (drives rubric, persona, criteria selection)
        #[arg(long, default_value = "evaluate this essay")]
        intent: String,

        /// Collect results and compute metrics (run after scoring)
        #[arg(long)]
        collect: bool,

        /// Path to anchor essays JSON for ontology-grounded calibration
        #[arg(long)]
        anchors: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Evaluate { document, intent, criteria, output, open, predict, deep_validate, orchestrate } => {
            run_evaluate(document, intent, criteria, output, open, predict, deep_validate, orchestrate).await
        }
        Commands::Graph { path } => {
            run_graph(path)
        }
        Commands::Benchmark { dataset, ablation, multi_dataset, extractions, graph_scores, reference_ontology, calibrate, output } => {
            if let Some(gs_path) = graph_scores {
                if calibrate {
                    run_calibrate_graph(dataset, gs_path, output)
                } else {
                    run_benchmark_graph(dataset, gs_path, reference_ontology, output)
                }
            } else if let Some(ext_path) = extractions {
                run_benchmark_with_extractions(dataset, ext_path, output)
            } else {
                run_benchmark(dataset, ablation, multi_dataset, output)
            }
        }
        Commands::History { dir } => {
            run_history(dir)
        }
        Commands::Serve { host, port } => {
            run_serve(host, port).await
        }
        Commands::Shoal { dataset, batch_size, output, max_score, intent, collect, anchors } => {
            run_shoal(dataset, batch_size, output, max_score, intent, collect, anchors)
        }
    }
}

fn run_graph(path: Option<PathBuf>) -> anyhow::Result<()> {
    let graph_file = if let Some(dir) = path {
        let f = dir.join("evaluation-graph.html");
        if !f.exists() {
            // Maybe they passed the HTML file directly
            if dir.extension().map(|e| e == "html").unwrap_or(false) && dir.exists() {
                dir
            } else {
                anyhow::bail!("No graph found at {}", f.display());
            }
        } else {
            f
        }
    } else {
        // Find the most recent evaluation from memory store
        if let Ok(store) = memory::MemoryStore::open() {
            if let Ok(records) = store.load_all() {
                if let Some(last) = records.last() {
                    // Check common output locations
                    let candidates = [
                        PathBuf::from(".").join("evaluation-graph.html"),
                        PathBuf::from("/tmp").join(&last.id).join("evaluation-graph.html"),
                    ];
                    if let Some(found) = candidates.iter().find(|p| p.exists()) {
                        found.clone()
                    } else {
                        // Search /tmp for any evaluation-graph.html
                        let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;
                        if let Ok(entries) = std::fs::read_dir("/tmp") {
                            for entry in entries.flatten() {
                                let gpath = entry.path().join("evaluation-graph.html");
                                if gpath.exists()
                                    && let Ok(meta) = gpath.metadata()
                                    && let Ok(modified) = meta.modified()
                                    && latest.as_ref().is_none_or(|(_, t)| modified > *t)
                                {
                                    latest = Some((gpath, modified));
                                }
                            }
                        }
                        if let Some((found, _)) = latest {
                            found
                        } else {
                            anyhow::bail!("No graph found. Run 'brain-in-the-fish evaluate' first.");
                        }
                    }
                } else {
                    anyhow::bail!("No evaluations in memory. Run 'brain-in-the-fish evaluate' first.");
                }
            } else {
                anyhow::bail!("Could not read evaluation history.");
            }
        } else {
            anyhow::bail!("Could not open memory store. Run 'brain-in-the-fish evaluate' first.");
        }
    };

    println!("Opening graph: {}", graph_file.display());
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open").arg(&graph_file).spawn(); }
    #[cfg(target_os = "linux")]
    { let _ = std::process::Command::new("xdg-open").arg(&graph_file).spawn(); }
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("cmd").args(["/c", "start"]).arg(&graph_file).spawn(); }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_evaluate(
    document: PathBuf,
    intent: String,
    criteria_path: Option<PathBuf>,
    output: Option<PathBuf>,
    open_graph: bool,
    predict: bool,
    deep_validate: bool,
    orchestrate: bool,
) -> anyhow::Result<()> {
    // Resolve output_dir early (needed for state DB)
    let output_dir = output.unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&output_dir)?;

    // Initialize ontology lifecycle (versioning, lineage, drift)
    let state_db_path = output_dir.join(".brain-state.db");
    let state_db = open_ontologies::state::StateDb::open(&state_db_path)?;
    let onto_lineage = open_ontologies::lineage::LineageLog::new(state_db.clone());
    let session_id = onto_lineage.new_session();

    // 1. Ingest document
    println!("1. Ingesting document: {}", document.display());
    let graph = Arc::new(open_ontologies::graph::GraphStore::new());
    let (mut doc, _raw_sections) = ingest::ingest_pdf(&document, &intent)?;

    // 2. Enrich: regex claim/evidence extraction + paragraph subsections
    println!("2. Enriching document (deterministic)...");
    enrich_document(&mut doc);

    // 2.5 Hybrid extraction
    println!("   Running hybrid extraction...");
    for section in &mut doc.sections {
        let rule_items = extract::extract_all(&section.text);
        let (claims, evidence) = extract::to_claims_and_evidence(&rule_items);
        // Replace regex-extracted with rule-extracted (higher quality)
        if !claims.is_empty() || !evidence.is_empty() {
            section.claims = claims;
            section.evidence = evidence;
        }
        // Also process subsections
        for sub in &mut section.subsections {
            let sub_items = extract::extract_all(&sub.text);
            let (sub_claims, sub_evidence) = extract::to_claims_and_evidence(&sub_items);
            if !sub_claims.is_empty() || !sub_evidence.is_empty() {
                sub.claims = sub_claims;
                sub.evidence = sub_evidence;
            }
        }
    }

    let triples = ingest::load_document_ontology(&graph, &doc)?;
    println!("   Loaded {} triples, {} sections", triples, doc.sections.len());
    onto_lineage.record(&session_id, "I", "ingest", &format!("{} sections, {} triples", doc.sections.len(), triples));

    // 2.5 Run OWL-RL reasoning to infer new triples
    println!("   Running OWL-RL reasoning...");
    match open_ontologies::reason::Reasoner::run(&graph, "owl-rl", true) {
        Ok(reason_result) => {
            let reason_json: serde_json::Value = serde_json::from_str(&reason_result).unwrap_or_default();
            let inferred = reason_json.get("inferred_triples").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("   Inferred {} new triples via OWL-RL", inferred);
            onto_lineage.record(&session_id, "A", "reason", &format!("{} triples inferred", inferred));
        }
        Err(e) => {
            println!("   Warning: reasoning failed: {}", e);
        }
    }

    // 3. Validate document (deterministic fact-checking)
    let prelim_framework = criteria::framework_for_intent(&intent);
    let validation_signals = if deep_validate {
        println!("3. Validating (deep, 15 checks)...");
        validate::validate_deep(&doc, &prelim_framework)
    } else {
        println!("3. Validating (core, 8 checks)...");
        validate::validate_core(&doc, &prelim_framework)
    };
    let val_triples = validate::load_signals(&graph, &validation_signals)?;
    let warnings = validation_signals
        .iter()
        .filter(|s| s.severity == validate::Severity::Warning)
        .count();
    let errors = validation_signals
        .iter()
        .filter(|s| s.severity == validate::Severity::Error)
        .count();
    println!(
        "   {} signals ({} warnings, {} errors), {} triples",
        validation_signals.len(),
        warnings,
        errors,
        val_triples
    );
    for signal in &validation_signals {
        if signal.severity != validate::Severity::Info {
            println!(
                "   {}: {}",
                if signal.severity == validate::Severity::Error {
                    "ERROR"
                } else {
                    "WARN"
                },
                signal.title
            );
        }
    }
    onto_lineage.record(
        &session_id,
        "A",
        "validate",
        &format!(
            "{} signals, {} warnings, {} errors",
            validation_signals.len(),
            warnings,
            errors
        ),
    );

    // Predictions (opt-in via --predict)
    let predictions = if predict {
        println!("   Extracting predictions...");
        let mut predictions = predict::extract_predictions(&doc);
        predict::assess_credibility(&mut predictions, &doc);
        if !predictions.is_empty() {
            let pred_triples = graph.load_turtle(&predict::predictions_to_turtle(&predictions), None).unwrap_or(0);
            println!("   {} predictions found, {} triples", predictions.len(), pred_triples);
            for pred in &predictions {
                let icon = match pred.credibility.verdict {
                    predict::CredibilityVerdict::WellSupported => "\u{2713}",
                    predict::CredibilityVerdict::PartiallySupported => "~",
                    predict::CredibilityVerdict::Aspirational => "?",
                    predict::CredibilityVerdict::Unsupported => "\u{2717}",
                    predict::CredibilityVerdict::OverClaimed => "!",
                };
                let display_text = if pred.text.len() > 60 { &pred.text[..60] } else { &pred.text };
                println!("   {} {:?}: {} (credibility {:.0}%)",
                    icon, pred.prediction_type,
                    display_text,
                    pred.credibility.score * 100.0);
            }
        }
        predictions
    } else {
        Vec::new()
    };

    // 4. Load evaluation criteria
    println!("4. Loading evaluation criteria...");
    let framework = if let Some(criteria_file) = criteria_path {
        println!("   From file: {}", criteria_file.display());
        criteria::parse_framework_from_file(&criteria_file)?
    } else {
        criteria::framework_for_intent(&intent)
    };
    let crit_triples = criteria::load_criteria_ontology(&graph, &framework)?;
    println!("   {} criteria, {} triples", framework.criteria.len(), crit_triples);
    onto_lineage.record(&session_id, "I", "criteria", &format!("{} criteria loaded", framework.criteria.len()));

    // Discover sector guidelines with provenance
    println!("   Discovering sector guidelines...");
    let guidelines = research::built_in_guidelines(&intent);
    let guide_triples = research::load_guidelines(&graph, &guidelines)?;
    println!("   {} guidelines, {} triples", guidelines.len(), guide_triples);
    for g in &guidelines {
        println!("   - {} ({})", g.title, g.sector);
    }
    onto_lineage.record(&session_id, "I", "guidelines", &format!("{} guidelines discovered", guidelines.len()));

    // 5. Align sections to criteria (ontology alignment with 7 signals)
    println!("5. Aligning document to criteria...");
    let (alignments, gaps) = match alignment::align_via_ontology(&graph, &doc, &framework) {
        Ok(result) => {
            println!("   (ontology alignment with 7 structural signals)");
            result
        }
        Err(_) => {
            println!("   (keyword-based alignment fallback)");
            alignment::align_sections_to_criteria(&doc, &framework)
        }
    };
    let align_triples = alignment::load_alignments(&graph, &alignments)?;
    println!("   {} alignments, {} gaps, {} triples", alignments.len(), gaps.len(), align_triples);
    for gap in &gaps {
        println!("   GAP: No content for '{}'", gap.criterion_title);
    }
    onto_lineage.record(&session_id, "A", "align", &format!("{} alignments, {} gaps", alignments.len(), gaps.len()));

    // 6. Spawn agent panel
    println!("6. Spawning evaluator panel...");
    let mut agents = agent::spawn_panel(&intent, &framework);
    for a in &agents {
        let agent_triples = agent::load_agent_ontology(&graph, a)?;
        println!("   {} ({}) — {} triples", a.name, a.role, agent_triples);
    }
    onto_lineage.record(&session_id, "A", "spawn", &format!("{} agents spawned", agents.len()));

    // 6.5 Semantic embeddings (optional — requires models)
    if semantic::models_available() {
        println!("   Generating semantic embeddings...");
        match semantic::embed_graph(&graph, &output_dir) {
            Ok(count) => {
                println!("   Embedded {} entities", count);
                onto_lineage.record(&session_id, "A", "embed", &format!("{} entities embedded", count));
            }
            Err(e) => {
                println!("   Embeddings skipped: {}", e);
            }
        }
    } else {
        println!("   Embeddings skipped (run 'open-ontologies init' to enable)");
    }

    // 7. SNN scoring (deterministic — no LLM needed)
    println!("7. SNN scoring (deterministic)...");
    let snn_config = snn::SNNConfig::default();
    let mut snn_networks: Vec<snn::AgentNetwork> = Vec::new();

    for agent_item in &agents {
        let mut network = snn::AgentNetwork::new(agent_item, &framework.criteria);
        network.feed_evidence(&doc, &alignments, &snn_config);
        snn_networks.push(network);
    }

    // Boost SNN with semantic similarity signals
    if semantic::models_available() {
        for network in &mut snn_networks {
            for neuron in &mut network.neurons {
                for alignment in &alignments {
                    if alignment.criterion_id == neuron.criterion_id
                        && let Ok(sim) = semantic::semantic_similarity(
                            &alignment.section_id,
                            &alignment.criterion_id,
                            &output_dir,
                        )
                        && sim > 0.3
                    {
                        neuron.receive_spike(snn::Spike {
                            source_id: format!("semantic_{}", alignment.section_id),
                            strength: sim.min(1.0),
                            spike_type: snn::SpikeType::Alignment,
                            timestep: 0,
                            source_text: None,
                            justification: None,
                        }, &snn_config);
                    }
                }
            }
        }
    }

    // Feed validation signals into SNN
    for signal in &validation_signals {
        if signal.spike_effect.abs() > 0.01 {
            for network in &mut snn_networks {
                for neuron in &mut network.neurons {
                    // Apply to all neurons (document-level signals) or matching criterion
                    let matches = signal.criterion_id.is_none()
                        || signal.criterion_id.as_deref() == Some(&neuron.criterion_id);
                    if matches {
                        neuron.receive_spike(
                            snn::Spike {
                                source_id: signal.id.clone(),
                                strength: signal.spike_effect.abs(),
                                spike_type: if signal.spike_effect > 0.0 {
                                    snn::SpikeType::Evidence
                                } else {
                                    snn::SpikeType::Claim
                                },
                                timestep: 0,
                                source_text: None,
                                justification: None,
                            },
                            &snn_config,
                        );
                        if signal.spike_effect < 0.0 {
                            neuron.apply_inhibition(signal.spike_effect.abs() * 0.2); // gentler inhibition
                        }
                    }
                }
            }
        }
    }

    // SNN scores ARE the actual scores
    let mut round1_scores: Vec<Score> = Vec::new();
    for network in &snn_networks {
        let snn_scores = network.compute_scores(&framework.criteria, &snn_config);
        for (criterion_id, snn_score) in &snn_scores {
            let criterion = framework.criteria.iter().find(|c| c.id == *criterion_id);
            let max_score = criterion.map(|c| c.max_score).unwrap_or(10.0);
            let criterion_title = criterion.map(|c| c.title.as_str()).unwrap_or(criterion_id);

            let score = Score {
                agent_id: network.agent_id.clone(),
                criterion_id: criterion_id.clone(),
                score: snn_score.snn_score,
                max_score,
                round: 1,
                justification: snn_score.explanation.clone(),
                evidence_used: network.neurons.iter()
                    .find(|n| n.criterion_id == *criterion_id)
                    .map(|n| n.spike_log.iter()
                        .map(|s| format!("{} ({:?}, strength {:.2})", s.source_id, s.spike_type, s.strength))
                        .collect::<Vec<_>>())
                    .unwrap_or_default(),
                gaps_identified: if !snn_score.grounded {
                    vec!["Insufficient evidence in the knowledge graph".into()]
                } else {
                    vec![]
                },
            };

            println!("   {} -> {}: {:.1}/{:.0} (CI: {:.1}-{:.1}, Bayesian: {:.0}%, falsification: {})",
                network.agent_name, criterion_title, snn_score.snn_score, max_score,
                snn_score.confidence_interval.0, snn_score.confidence_interval.1,
                snn_score.bayesian_confidence * 100.0,
                if snn_score.falsification_checked { "passed" } else { "NOT CHECKED" });

            if !snn_score.grounded {
                println!("      LOW EVIDENCE: {}", criterion_title);
            }

            scoring::record_score(&graph, &score)?;
            round1_scores.push(score);
        }
    }

    onto_lineage.record(&session_id, "A", "snn_score", &format!("{} scores computed", round1_scores.len()));

    // Version snapshot before debate (for drift detection after)
    let pre_debate_turtle = match graph.serialize("turtle") {
        Ok(t) => Some(t),
        Err(e) => {
            println!("   Warning: could not snapshot pre-debate graph: {}", e);
            None
        }
    };

    // 8. Build debate rounds from SNN score disagreements
    println!("8. Debate (deterministic convergence)...");
    let mut all_rounds = vec![debate::build_debate_round(1, round1_scores.clone(), vec![], None, false)];
    let mut current_scores = round1_scores;
    let max_rounds = 5;

    for round_num in 2..=max_rounds {
        let disagreements = debate::find_disagreements(&current_scores, 2.0);
        if disagreements.is_empty() {
            println!("   No disagreements — converged at round {}", round_num - 1);
            if let Some(last) = all_rounds.last_mut() {
                last.converged = true;
            }
            break;
        }

        println!("   Round {}: {} disagreements...", round_num, disagreements.len());
        let mut challenges = Vec::new();
        let mut new_scores = current_scores.clone();

        for disagreement in &disagreements {
            let challenger = agents.iter().find(|a| a.id == disagreement.agent_a_id);
            let target = agents.iter().find(|a| a.id == disagreement.agent_b_id);
            let criterion = framework.criteria.iter().find(|c| c.id == disagreement.criterion_id);

            if let (Some(challenger), Some(target), Some(criterion)) = (challenger, target, criterion) {
                // Higher scorer challenges lower scorer
                let (actual_challenger, actual_target) = if disagreement.agent_a_score > disagreement.agent_b_score {
                    (challenger, target)
                } else {
                    (target, challenger)
                };

                // Also apply SNN lateral inhibition for the debate
                for network in &mut snn_networks {
                    if network.agent_id == actual_target.id {
                        network.inhibit(&criterion.id, snn_config.inhibition_strength);
                    }
                }

                // Mechanical convergence: move target 30% toward challenger
                if let Some(target_score) = new_scores.iter_mut().find(|s|
                    s.agent_id == actual_target.id && s.criterion_id == criterion.id
                ) {
                    let challenger_score_val = current_scores.iter()
                        .find(|s| s.agent_id == actual_challenger.id && s.criterion_id == criterion.id)
                        .map(|s| s.score)
                        .unwrap_or(target_score.score);

                    let old_score = target_score.score;
                    let adjustment = (challenger_score_val - old_score) * 0.3;
                    target_score.score = (old_score + adjustment).min(target_score.max_score).max(0.0);
                    target_score.round = round_num;
                    target_score.justification = format!(
                        "{} [R{}: adjusted after challenge from {}]",
                        target_score.justification, round_num, actual_challenger.name
                    );

                    println!("      {} challenges {} on '{}': {:.1} -> {:.1}",
                        actual_challenger.name, actual_target.name,
                        criterion.title, old_score, target_score.score);

                    challenges.push(Challenge {
                        challenger_id: actual_challenger.id.clone(),
                        target_agent_id: actual_target.id.clone(),
                        criterion_id: criterion.id.clone(),
                        round: round_num,
                        argument: format!("Score delta of {:.1} — challenging based on evidence assessment", disagreement.delta),
                        response: Some(format!("Adjusted from {:.1} to {:.1}", old_score, target_score.score)),
                        score_change: Some((old_score, target_score.score)),
                    });
                }
            }
        }

        debate::update_trust_weights(&mut agents, &challenges);

        let drift = debate::calculate_drift_velocity(&current_scores, &new_scores);
        let converged = debate::check_convergence(drift, 0.5);
        println!("   Drift: {:.2}, Converged: {}", drift, converged);

        current_scores = new_scores;
        all_rounds.push(debate::build_debate_round(round_num, current_scores.clone(), challenges, Some(drift), converged));

        if converged {
            println!("   Debate converged!");
            break;
        }
    }

    // Ontology drift detection: compare graph before and after debate
    if let Some(ref pre_turtle) = pre_debate_turtle {
        match graph.serialize("turtle") {
            Ok(post_debate_turtle) => {
                let drift_detector = open_ontologies::drift::DriftDetector::new(state_db.clone());
                match drift_detector.detect(pre_turtle, &post_debate_turtle) {
                    Ok(drift_result) => {
                        let drift_json: serde_json::Value = serde_json::from_str(&drift_result).unwrap_or_default();
                        let drift_velocity = drift_json.get("drift_velocity").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        println!("   Ontology drift: velocity={:.3}", drift_velocity);
                        onto_lineage.record(&session_id, "D", "drift", &format!("velocity={:.3}", drift_velocity));
                    }
                    Err(e) => println!("   Warning: drift detection failed: {}", e),
                }
            }
            Err(e) => println!("   Warning: could not snapshot post-debate graph: {}", e),
        }
    }

    // 9. Moderate: trust-weighted consensus
    println!("9. Calculating consensus scores...");
    let moderated = moderation::calculate_moderated_scores(&current_scores, &agents);
    let overall = moderation::calculate_overall_score(&moderated, &framework);

    println!("   Overall: {:.1}/{:.1} ({:.1}%)", overall.total_score, overall.max_possible, overall.percentage);
    if let Some(passed) = overall.passed {
        println!("   Result: {}", if passed { "PASS" } else { "FAIL" });
    }

    // 10. Generate report + outputs
    println!("10. Generating report...");
    let session = EvaluationSession {
        id: uuid::Uuid::new_v4().to_string(),
        document: doc,
        framework,
        agents: agents.clone(),
        alignments: alignments.clone(),
        gaps,
        rounds: all_rounds,
        final_scores: moderated,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let mut report = report::generate_report(&session, &overall);
    report.push_str(&predict::prediction_report(&predictions));

    let report_path = output_dir.join("evaluation-report.md");
    std::fs::write(&report_path, &report)?;
    println!("   Report: {}", report_path.display());

    // Turtle export
    let turtle_path = output_dir.join("evaluation.ttl");
    let turtle = report::session_to_turtle(&session);
    std::fs::write(&turtle_path, &turtle)?;
    println!("   Turtle: {}", turtle_path.display());

    // Generate graph visualization
    println!("   Generating graph visualization...");
    let graph_data = visualize::extract_graph_data(&session);
    let lineage = visualize::build_lineage(&session);
    let graph_html = visualize::generate_graph_html(&graph_data, &lineage, &session.document.title, &intent);
    let graph_path = output_dir.join("evaluation-graph.html");
    std::fs::write(&graph_path, &graph_html)?;
    println!("   Graph: {}", graph_path.display());
    if open_graph {
        let _ = std::process::Command::new("open").arg(&graph_path).spawn();
    }

    // Save to cross-evaluation memory
    println!("   Saving to memory...");
    if let Ok(store) = memory::MemoryStore::open() {
        let record = memory::build_record(&session, &overall, &intent);
        if let Ok(Some(comp)) = store.compare(&record) {
            println!("   Historical: {} previous | This: {:.1}% | Mean: {:.1}% | Percentile: {}th",
                comp.total_compared, comp.current_percentage, comp.historical_mean, comp.percentile);
        }
        if let Ok(path) = store.save(&record) {
            println!("   Memory: {}", path.display());
        }
    }

    // Orchestration (opt-in via --orchestrate)
    if orchestrate {
        println!("   [--orchestrate] Saving orchestration tasks...");
        let tasks = orchestrator::generate_scoring_tasks(&agents, &session.framework, &session.document, &alignments);
        let orchestration = serde_json::json!({
            "session_id": session.id,
            "document": session.document.title,
            "intent": intent,
            "mcp_server": "brain-in-the-fish serve",
            "tasks": tasks.len(),
            "scoring_tasks": tasks,
            "instructions": "Start the MCP server, then dispatch one Claude subagent per scoring task. Each subagent should call eval_record_score with their assessment.",
        });
        let orch_path = output_dir.join("orchestration.json");
        std::fs::write(&orch_path, serde_json::to_string_pretty(&orchestration)?)?;
        println!("   Orchestration tasks: {}", orch_path.display());
    }

    // Quality gate: enforce evaluation patterns
    println!("   Enforcing evaluation quality rules...");
    let enforcer = open_ontologies::enforce::Enforcer::new(state_db.clone(), graph.clone());
    enforcer.add_custom_rule(
        "eval_every_criterion_scored",
        "eval",
        "PREFIX eval: <http://brain-in-the-fish.dev/eval/> SELECT ?c WHERE { ?c a eval:EvaluationCriterion . FILTER NOT EXISTS { ?s eval:criterion ?c . ?s a eval:Score } }",
        "error",
        "Criterion has no scores",
    );
    enforcer.add_custom_rule(
        "eval_agent_has_needs",
        "eval",
        "PREFIX eval: <http://brain-in-the-fish.dev/eval/> PREFIX cog: <http://brain-in-the-fish.dev/cognition/> SELECT ?a WHERE { ?a a eval:EvaluatorAgent . FILTER NOT EXISTS { ?n cog:agentRef ?a } }",
        "warning",
        "Agent has no cognitive needs defined",
    );
    match enforcer.enforce("eval") {
        Ok(enforce_result) => {
            let enforce_json: serde_json::Value = serde_json::from_str(&enforce_result).unwrap_or_default();
            let violations = enforce_json.get("violations").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            println!("   Enforce: {} violations", violations);
            onto_lineage.record(&session_id, "E", "enforce", &format!("{} violations", violations));
        }
        Err(e) => {
            println!("   Warning: enforce failed: {}", e);
        }
    }

    // Print lineage trail
    let compact_lineage = onto_lineage.get_compact(&session_id);
    println!("\nLineage trail (onto_lineage):\n{}", compact_lineage);

    println!("\nTo enhance with LLM scoring:");
    println!("   1. Start MCP server: brain-in-the-fish serve");
    println!("   2. Connect Claude and dispatch subagents from orchestration.json");

    Ok(())
}

/// Enrich a document with paragraph-level subsections, claims, and evidence.
///
/// Splits each section's text into paragraphs and extracts:
/// - Claims: sentences with strong assertion patterns
/// - Evidence: sentences citing sources, data, statistics
fn enrich_document(doc: &mut EvalDocument) {
    if doc.title.is_empty() && !doc.sections.is_empty() {
        doc.title = format!("Document: {}", doc.sections[0].title);
    }
    if doc.doc_type.is_empty() {
        doc.doc_type = "document".into();
    }

    for section in &mut doc.sections {
        let paragraphs: Vec<&str> = section.text.split("\n\n")
            .map(|p| p.trim())
            .filter(|p| !p.is_empty() && p.split_whitespace().count() > 5)
            .collect();

        if paragraphs.len() <= 1 {
            extract_claims_and_evidence(section);
            continue;
        }

        for (i, para) in paragraphs.iter().enumerate() {
            let para_title = extract_first_phrase(para, 8);
            let mut subsection = Section {
                id: uuid::Uuid::new_v4().to_string(),
                title: format!("{}:{} — {}", section.title, i + 1, para_title),
                text: para.to_string(),
                word_count: para.split_whitespace().count() as u32,
                page_range: None,
                claims: vec![],
                evidence: vec![],
                subsections: vec![],
            };
            extract_claims_and_evidence(&mut subsection);
            section.subsections.push(subsection);
        }
    }
}

fn extract_claims_and_evidence(section: &mut Section) {
    let sentences: Vec<&str> = section.text.split('.')
        .map(|s| s.trim())
        .filter(|s| s.len() > 15)
        .collect();

    for sentence in &sentences {
        let lower = sentence.to_lowercase();

        let is_claim = lower.contains("argue")
            || lower.contains("suggest")
            || lower.contains("indicate")
            || lower.contains("shows that")
            || lower.contains("demonstrates")
            || lower.contains("supports the")
            || lower.contains("effective")
            || lower.contains("significant")
            || lower.contains("important")
            || lower.contains("should");

        let is_evidence = sentence.contains('(') && sentence.contains(')')
            && (lower.contains("et al")
                || lower.contains("20")
                || lower.contains("found")
                || lower.contains("estimate"))
            || lower.contains("%")
            || lower.contains("\u{00a3}")
            || lower.contains("$")
            || lower.contains("billion")
            || lower.contains("million");

        if is_claim {
            let specificity = if sentence.contains('%') || sentence.contains('\u{00a3}') { 0.9 }
                else if sentence.len() > 80 { 0.7 }
                else { 0.5 };
            section.claims.push(types::Claim {
                id: uuid::Uuid::new_v4().to_string(),
                text: sentence.to_string(),
                specificity,
                verifiable: is_evidence,
            });
        }

        if is_evidence {
            let source = if let Some(start) = sentence.find('(') {
                if let Some(end) = sentence[start..].find(')') {
                    sentence[start + 1..start + end].to_string()
                } else {
                    "inline data".into()
                }
            } else {
                "inline data".into()
            };
            let has_quantified = lower.contains('%') || lower.contains('\u{00a3}')
                || lower.contains("billion") || lower.contains("million")
                || lower.contains("basis point");
            section.evidence.push(types::Evidence {
                id: uuid::Uuid::new_v4().to_string(),
                source,
                evidence_type: if has_quantified { "statistical".into() } else { "citation".into() },
                text: sentence.to_string(),
                has_quantified_outcome: has_quantified,
            });
        }
    }
}

fn extract_first_phrase(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().take(max_words).collect();
    let phrase = words.join(" ");
    if phrase.len() > 50 { format!("{}...", &phrase[..47]) } else { phrase }
}

fn run_benchmark(
    dataset_path: Option<PathBuf>,
    ablation: bool,
    multi_dataset: bool,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    // Multi-dataset mode: run across all available datasets and compare
    if multi_dataset {
        let data_dir = PathBuf::from("data");
        let dataset_names = ["ellipse-sample.json", "asap-set1.json", "asap-stratified-100.json"];
        let mut cross_results: Vec<benchmark::BenchmarkResults> = Vec::new();

        for name in &dataset_names {
            let path = data_dir.join(name);
            if !path.exists() {
                println!("Skipping {} (not found)", path.display());
                continue;
            }
            println!("\n========== Dataset: {} ==========", name);
            let result = run_single_benchmark(&path, false)?;
            for mut r in result {
                r.name = format!("{} ({})", r.name, name);
                cross_results.push(r);
            }
        }

        if cross_results.is_empty() {
            println!("No datasets found in data/ directory. Running synthetic benchmark instead.");
            return run_single_benchmark_full(None, ablation, output);
        }

        println!("\n\n===== Cross-Dataset Comparison =====\n");
        println!("{}", benchmark::results_table(&cross_results));

        if let Some(output_dir) = output {
            std::fs::create_dir_all(&output_dir)?;
            let results_path = output_dir.join("multi-dataset-results.json");
            std::fs::write(&results_path, serde_json::to_string_pretty(&cross_results)?)?;
            println!("Results saved to: {}", results_path.display());
        }

        return Ok(());
    }

    run_single_benchmark_full(dataset_path, ablation, output)
}

/// Run benchmark on a single dataset path, returning results (no output/ablation).
fn run_single_benchmark(
    dataset_path: &std::path::Path,
    _ablation: bool,
) -> anyhow::Result<Vec<benchmark::BenchmarkResults>> {
    let samples = benchmark::load_dataset(dataset_path)?;
    println!("Loaded {} samples", samples.len());

    let configs = vec![benchmark::BenchmarkConfig::default()];
    let mut all_results = Vec::new();

    for config in &configs {
        let (result, _predicted) = run_benchmark_config(&samples, config)?;
        all_results.push(result);
    }
    Ok(all_results)
}

fn run_single_benchmark_full(
    dataset_path: Option<PathBuf>,
    ablation: bool,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    let samples = if let Some(ref path) = dataset_path {
        println!("Loading dataset: {}", path.display());
        benchmark::load_dataset(path)?
    } else {
        println!("Using synthetic benchmark dataset (10 samples)");
        benchmark::synthetic_dataset()
    };
    println!("Loaded {} samples\n", samples.len());

    let configs = if ablation {
        println!("Running ablation experiments (6 configs)...\n");
        benchmark::ablation_configs()
    } else {
        vec![benchmark::BenchmarkConfig::default()]
    };

    let mut all_results = Vec::new();
    let mut last_predicted: Vec<f64> = Vec::new();

    for config in &configs {
        let (result, predicted) = run_benchmark_config(&samples, config)?;
        println!("  Pearson r: {:.3} | QWK: {:.3} | MAE: {:.2} | NMAE: {:.3} | RMSE: {:.2} | Halluc: {} ({:.1}%)\n",
            result.pearson_r, result.qwk, result.mae, result.nmae, result.rmse,
            result.hallucination_count, result.hallucination_rate * 100.0);
        last_predicted = predicted;
        all_results.push(result);
    }

    println!("\n{}", benchmark::results_table(&all_results));

    // Per-rubric breakdown (Fix 2)
    if !last_predicted.is_empty() {
        let per_group = benchmark::per_group_results(&samples, &last_predicted);
        if per_group.len() > 1 {
            println!("\n===== Per-Rubric Breakdown =====\n");
            println!("{}", benchmark::results_table(&per_group));
        }
    }

    if let Some(output_dir) = output {
        std::fs::create_dir_all(&output_dir)?;
        let results_path = output_dir.join("benchmark-results.json");
        std::fs::write(&results_path, serde_json::to_string_pretty(&all_results)?)?;
        println!("Results saved to: {}", results_path.display());
        let table_path = output_dir.join("benchmark-results.md");
        std::fs::write(&table_path, benchmark::results_table(&all_results))?;
        println!("Table saved to: {}", table_path.display());
    }

    Ok(())
}

/// Run a single benchmark configuration against samples, returning results and predicted scores.
fn run_benchmark_config(
    samples: &[benchmark::LabeledSample],
    config: &benchmark::BenchmarkConfig,
) -> anyhow::Result<(benchmark::BenchmarkResults, Vec<f64>)> {
    println!("--- Config: {} ---", config.label);
    let mut predicted_scores: Vec<f64> = Vec::new();
    let mut actual_scores: Vec<f64> = Vec::new();
    let mut hallucination_count: usize = 0;

    for sample in samples {
        let mut doc = EvalDocument::new(
            format!("Benchmark: {}", sample.id),
            "essay".into(),
        );
        let word_count = sample.text.split_whitespace().count() as u32;
        let mut section = Section {
            id: uuid::Uuid::new_v4().to_string(),
            title: sample.id.clone(),
            text: sample.text.clone(),
            word_count,
            page_range: None,
            claims: vec![],
            evidence: vec![],
            subsections: vec![],
        };
        // Use hybrid extraction for better evidence detection
        let extracted = extract::extract_all(&section.text);
        let (claims, evidence) = extract::to_claims_and_evidence(&extracted);
        if !claims.is_empty() || !evidence.is_empty() {
            section.claims = claims;
            section.evidence = evidence;
        } else {
            extract_claims_and_evidence(&mut section);
        }
        doc.sections.push(section);
        doc.total_word_count = Some(word_count);

        let intent = if sample.domain.is_empty() {
            "academic essay evaluation".to_string()
        } else {
            format!("{} essay evaluation", sample.domain)
        };
        let framework = criteria::framework_for_intent(&intent);

        let (alignments, _gaps) = if config.use_ontology_alignment {
            alignment::align_sections_to_criteria(&doc, &framework)
        } else {
            (vec![], vec![])
        };

        let validation_signals = if config.use_validation {
            validate::validate_document(&doc, &framework)
        } else {
            vec![]
        };

        let agents = agent::spawn_panel(&intent, &framework);

        let predicted = if config.use_snn {
            let snn_config = snn::SNNConfig::default();
            let mut snn_networks: Vec<snn::AgentNetwork> = Vec::new();
            for agent_item in &agents {
                let mut network = snn::AgentNetwork::new(agent_item, &framework.criteria);
                network.feed_evidence(&doc, &alignments, &snn_config);
                snn_networks.push(network);
            }

            if config.use_validation {
                for signal in &validation_signals {
                    if signal.spike_effect.abs() > 0.01 {
                        for network in &mut snn_networks {
                            for neuron in &mut network.neurons {
                                let matches = signal.criterion_id.is_none()
                                    || signal.criterion_id.as_deref() == Some(&neuron.criterion_id);
                                if matches {
                                    neuron.receive_spike(
                                        snn::Spike {
                                            source_id: signal.id.clone(),
                                            strength: signal.spike_effect.abs(),
                                            spike_type: if signal.spike_effect > 0.0 {
                                                snn::SpikeType::Evidence
                                            } else {
                                                snn::SpikeType::Claim
                                            },
                                            timestep: 0,
                                            source_text: None,
                                            justification: None,
                                        },
                                        &snn_config,
                                    );
                                    if signal.spike_effect < 0.0 {
                                        neuron.apply_inhibition(signal.spike_effect.abs() * 0.2); // gentler inhibition
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let mut round1_scores: Vec<Score> = Vec::new();
            for network in &snn_networks {
                let snn_scores = network.compute_scores(&framework.criteria, &snn_config);
                for (criterion_id, snn_score) in &snn_scores {
                    let criterion = framework.criteria.iter().find(|c| c.id == *criterion_id);
                    let max_score = criterion.map(|c| c.max_score).unwrap_or(10.0);
                    round1_scores.push(Score {
                        agent_id: network.agent_id.clone(),
                        criterion_id: criterion_id.clone(),
                        score: snn_score.snn_score,
                        max_score,
                        round: 1,
                        justification: snn_score.explanation.clone(),
                        evidence_used: vec![],
                        gaps_identified: vec![],
                    });
                }
            }

            let final_scores = if config.use_debate && round1_scores.len() > 1 {
                let mut current_scores = round1_scores;
                for _round_num in 2..=5 {
                    let disagreements = debate::find_disagreements(&current_scores, 2.0);
                    if disagreements.is_empty() { break; }
                    let mut new_scores = current_scores.clone();
                    for disagreement in &disagreements {
                        if let Some(target_score) = new_scores.iter_mut().find(|s| {
                            s.agent_id == disagreement.agent_b_id
                                && s.criterion_id == disagreement.criterion_id
                        }) {
                            let challenger_val = current_scores.iter()
                                .find(|s| s.agent_id == disagreement.agent_a_id
                                    && s.criterion_id == disagreement.criterion_id)
                                .map(|s| s.score)
                                .unwrap_or(target_score.score);
                            let adjustment = (challenger_val - target_score.score) * 0.3;
                            target_score.score = (target_score.score + adjustment)
                                .min(target_score.max_score).max(0.0);
                        }
                    }
                    let drift = debate::calculate_drift_velocity(&current_scores, &new_scores);
                    current_scores = new_scores;
                    if debate::check_convergence(drift, 0.5) { break; }
                }
                current_scores
            } else {
                round1_scores
            };

            let moderated = moderation::calculate_moderated_scores(&final_scores, &agents);
            let overall = moderation::calculate_overall_score(&moderated, &framework);
            overall.percentage / 100.0 * sample.max_score
        } else {
            sample.max_score * 0.5
        };

        // Hallucination detection (Fix 3): predicted deviates > 30% from actual on normalized scale
        if sample.max_score > 0.0 {
            let normalized_pred = predicted / sample.max_score;
            let normalized_actual = sample.expert_score / sample.max_score;
            if (normalized_pred - normalized_actual).abs() > 0.3 {
                hallucination_count += 1;
            }
        }

        predicted_scores.push(predicted);
        actual_scores.push(sample.expert_score);
        println!("  {} | predicted: {:.1} | actual: {:.1} | delta: {:.1}",
            sample.id, predicted, sample.expert_score, (predicted - sample.expert_score).abs());
    }

    // Fix 1: Dynamic QWK range — use 0.0 and the max of all sample max_scores
    let max_score_val = samples.iter().map(|s| s.max_score).fold(0.0f64, f64::max);

    let pearson_r = benchmark::pearson_correlation(&predicted_scores, &actual_scores);
    let qwk = benchmark::quadratic_weighted_kappa(&predicted_scores, &actual_scores, 0.0, max_score_val);
    let mae = benchmark::mean_absolute_error(&predicted_scores, &actual_scores);
    let rmse_val = benchmark::rmse(&predicted_scores, &actual_scores);
    let mean_predicted = predicted_scores.iter().sum::<f64>() / predicted_scores.len() as f64;
    let mean_actual = actual_scores.iter().sum::<f64>() / actual_scores.len() as f64;

    // Fix 7: Normalized MAE — divide by score range for cross-set comparability
    let score_range = max_score_val; // min is always 0
    let nmae = if score_range > 0.0 { mae / score_range } else { 0.0 };

    let hallucination_rate = if samples.is_empty() {
        0.0
    } else {
        hallucination_count as f64 / samples.len() as f64
    };

    let result = benchmark::BenchmarkResults {
        name: config.label.clone(),
        samples: samples.len(),
        pearson_r,
        qwk,
        mae,
        nmae,
        rmse: rmse_val,
        mean_predicted,
        mean_actual,
        hallucination_count,
        hallucination_rate,
        config: config.clone(),
    };

    Ok((result, predicted_scores))
}

/// Subagent-produced extraction for a single sample.
/// Format: Claude extracts evidence, saves as JSON, benchmark loads and feeds through SNN.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SubagentExtraction {
    id: String,
    claims: Vec<SubagentClaim>,
    evidence: Vec<SubagentEvidence>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SubagentClaim {
    text: String,
    specificity: f64,
    verifiable: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SubagentEvidence {
    source: String,
    #[serde(rename = "type")]
    evidence_type: String,
    text: String,
    quantified: bool,
}

/// Run benchmark using subagent-produced extractions.
/// The extractions JSON is produced by Claude (the subagent) and contains
/// structured evidence for each essay. The benchmark feeds this through the SNN.
///
/// Usage: cargo run -- benchmark --dataset data/asap-set1.json --extractions data/asap-set1-extractions.json
fn run_benchmark_with_extractions(
    dataset_path: Option<PathBuf>,
    extractions_path: PathBuf,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    println!("Running subagent extraction → SNN benchmark");

    let samples = if let Some(ref path) = dataset_path {
        println!("Loading dataset: {}", path.display());
        benchmark::load_dataset(path)?
    } else {
        println!("Using synthetic benchmark dataset (10 samples)");
        benchmark::synthetic_dataset()
    };

    // Load subagent extractions
    println!("Loading extractions: {}", extractions_path.display());
    let ext_content = std::fs::read_to_string(&extractions_path)?;
    let extractions: Vec<SubagentExtraction> = serde_json::from_str(&ext_content)?;
    let ext_map: std::collections::HashMap<String, &SubagentExtraction> =
        extractions.iter().map(|e| (e.id.clone(), e)).collect();
    println!("Loaded {} extractions for {} samples\n", extractions.len(), samples.len());

    let mut predicted_scores: Vec<f64> = Vec::new();
    let mut actual_scores: Vec<f64> = Vec::new();
    let mut hallucination_count: usize = 0;
    let mut matched = 0usize;
    let mut unmatched = 0usize;

    for (i, sample) in samples.iter().enumerate() {
        let intent = if sample.domain.is_empty() {
            "academic essay evaluation".to_string()
        } else {
            format!("{} essay evaluation", sample.domain)
        };

        let framework = criteria::framework_for_intent(&intent);
        let agents = agent::spawn_panel(&intent, &framework);
        let snn_config = snn::SNNConfig::default();

        let predicted = if let Some(extraction) = ext_map.get(&sample.id) {
            matched += 1;

            let mut snn_networks: Vec<snn::AgentNetwork> = Vec::new();
            for agent_item in &agents {
                let mut network = snn::AgentNetwork::new(agent_item, &framework.criteria);

                // Feed subagent-extracted claims as spikes
                for claim in &extraction.claims {
                    let spike_type = if claim.verifiable {
                        snn::SpikeType::Evidence
                    } else {
                        snn::SpikeType::Claim
                    };
                    for neuron in &mut network.neurons {
                        neuron.receive_spike(
                            snn::Spike {
                                source_id: format!("sub_claim_{}", uuid::Uuid::new_v4()),
                                strength: claim.specificity.clamp(0.0, 1.0),
                                spike_type: spike_type.clone(),
                                timestep: 0,
                                source_text: Some(claim.text.clone()),
                                justification: Some(format!(
                                    "Subagent-extracted claim, specificity={:.2}, verifiable={}",
                                    claim.specificity, claim.verifiable
                                )),
                            },
                            &snn_config,
                        );
                    }
                }

                // Feed subagent-extracted evidence as typed spikes
                for ev in &extraction.evidence {
                    let spike_type = match ev.evidence_type.as_str() {
                        "statistical" => snn::SpikeType::QuantifiedData,
                        "citation" => snn::SpikeType::Citation,
                        _ => if ev.quantified {
                            snn::SpikeType::QuantifiedData
                        } else {
                            snn::SpikeType::Evidence
                        },
                    };
                    let strength = if ev.quantified { 0.85 } else { 0.7 };
                    for neuron in &mut network.neurons {
                        neuron.receive_spike(
                            snn::Spike {
                                source_id: format!("sub_ev_{}", uuid::Uuid::new_v4()),
                                strength,
                                spike_type: spike_type.clone(),
                                timestep: 0,
                                source_text: Some(ev.text.clone()),
                                justification: Some(format!(
                                    "Subagent-extracted {}, source={}, quantified={}",
                                    ev.evidence_type, ev.source, ev.quantified
                                )),
                            },
                            &snn_config,
                        );
                    }
                }

                snn_networks.push(network);
            }

            // Compute SNN scores
            let mut round1_scores: Vec<Score> = Vec::new();
            for network in &snn_networks {
                let snn_scores = network.compute_scores(&framework.criteria, &snn_config);
                for (criterion_id, snn_score) in &snn_scores {
                    let criterion = framework.criteria.iter().find(|c| c.id == *criterion_id);
                    let max_score = criterion.map(|c| c.max_score).unwrap_or(10.0);
                    round1_scores.push(Score {
                        agent_id: network.agent_id.clone(),
                        criterion_id: criterion_id.clone(),
                        score: snn_score.snn_score,
                        max_score,
                        round: 1,
                        justification: snn_score.explanation.clone(),
                        evidence_used: vec![],
                        gaps_identified: vec![],
                    });
                }
            }

            let moderated = moderation::calculate_moderated_scores(&round1_scores, &agents);
            let overall = moderation::calculate_overall_score(&moderated, &framework);
            overall.percentage / 100.0 * sample.max_score
        } else {
            // No extraction for this sample — fall back to regex extraction
            unmatched += 1;
            let mut doc = EvalDocument::new(format!("Benchmark: {}", sample.id), "essay".into());
            let word_count = sample.text.split_whitespace().count() as u32;
            let mut section = Section {
                id: uuid::Uuid::new_v4().to_string(),
                title: sample.id.clone(),
                text: sample.text.clone(),
                word_count,
                page_range: None,
                claims: vec![],
                evidence: vec![],
                subsections: vec![],
            };
            let extracted = extract::extract_all(&section.text);
            let (claims, evidence) = extract::to_claims_and_evidence(&extracted);
            if !claims.is_empty() || !evidence.is_empty() {
                section.claims = claims;
                section.evidence = evidence;
            } else {
                extract_claims_and_evidence(&mut section);
            }
            doc.sections.push(section);
            doc.total_word_count = Some(word_count);

            let (alignments, _) = alignment::align_sections_to_criteria(&doc, &framework);
            let mut snn_networks: Vec<snn::AgentNetwork> = Vec::new();
            for agent_item in &agents {
                let mut network = snn::AgentNetwork::new(agent_item, &framework.criteria);
                network.feed_evidence(&doc, &alignments, &snn_config);
                snn_networks.push(network);
            }
            let mut round1_scores: Vec<Score> = Vec::new();
            for network in &snn_networks {
                let snn_scores = network.compute_scores(&framework.criteria, &snn_config);
                for (criterion_id, snn_score) in &snn_scores {
                    let criterion = framework.criteria.iter().find(|c| c.id == *criterion_id);
                    let max_score = criterion.map(|c| c.max_score).unwrap_or(10.0);
                    round1_scores.push(Score {
                        agent_id: network.agent_id.clone(),
                        criterion_id: criterion_id.clone(),
                        score: snn_score.snn_score,
                        max_score,
                        round: 1,
                        justification: snn_score.explanation.clone(),
                        evidence_used: vec![],
                        gaps_identified: vec![],
                    });
                }
            }
            let moderated = moderation::calculate_moderated_scores(&round1_scores, &agents);
            let overall = moderation::calculate_overall_score(&moderated, &framework);
            overall.percentage / 100.0 * sample.max_score
        };

        // Hallucination detection
        if sample.max_score > 0.0 {
            let normalized_pred = predicted / sample.max_score;
            let normalized_actual = sample.expert_score / sample.max_score;
            if (normalized_pred - normalized_actual).abs() > 0.3 {
                hallucination_count += 1;
            }
        }

        predicted_scores.push(predicted);
        actual_scores.push(sample.expert_score);
        if (i + 1) % 50 == 0 || i == samples.len() - 1 {
            println!("  [{}/{}] {} | predicted: {:.1} | actual: {:.1} | delta: {:.1}",
                i + 1, samples.len(), sample.id, predicted, sample.expert_score,
                (predicted - sample.expert_score).abs());
        }
    }

    println!("\nMatched extractions: {} | Fell back to regex: {}", matched, unmatched);

    // Compute metrics
    let max_score_val = samples.iter().map(|s| s.max_score).fold(0.0f64, f64::max);
    let pearson_r = benchmark::pearson_correlation(&predicted_scores, &actual_scores);
    let qwk = benchmark::quadratic_weighted_kappa(&predicted_scores, &actual_scores, 0.0, max_score_val);
    let mae = benchmark::mean_absolute_error(&predicted_scores, &actual_scores);
    let rmse_val = benchmark::rmse(&predicted_scores, &actual_scores);
    let mean_predicted = predicted_scores.iter().sum::<f64>() / predicted_scores.len() as f64;
    let mean_actual = actual_scores.iter().sum::<f64>() / actual_scores.len() as f64;
    let nmae = if max_score_val > 0.0 { mae / max_score_val } else { 0.0 };
    let hallucination_rate = if samples.is_empty() {
        0.0
    } else {
        hallucination_count as f64 / samples.len() as f64
    };

    let result = benchmark::BenchmarkResults {
        name: "subagent_extract_snn".to_string(),
        samples: samples.len(),
        pearson_r,
        qwk,
        mae,
        nmae,
        rmse: rmse_val,
        mean_predicted,
        mean_actual,
        hallucination_count,
        hallucination_rate,
        config: benchmark::BenchmarkConfig {
            use_snn: true,
            use_llm_extraction: true,
            label: "subagent_extract_snn".into(),
            ..Default::default()
        },
    };

    println!("\n  Pearson r: {:.3} | QWK: {:.3} | MAE: {:.2} | NMAE: {:.3} | RMSE: {:.2} | Halluc: {} ({:.1}%)\n",
        result.pearson_r, result.qwk, result.mae, result.nmae, result.rmse,
        result.hallucination_count, result.hallucination_rate * 100.0);
    println!("{}", benchmark::results_table(std::slice::from_ref(&result)));

    // Per-rubric breakdown
    let per_group = benchmark::per_group_results(&samples, &predicted_scores);
    if per_group.len() > 1 {
        println!("\n===== Per-Rubric Breakdown =====\n");
        println!("{}", benchmark::results_table(&per_group));
    }

    if let Some(output_dir) = output {
        std::fs::create_dir_all(&output_dir)?;
        let results_path = output_dir.join("subagent-benchmark-results.json");
        std::fs::write(&results_path, serde_json::to_string_pretty(&[&result])?)?;
        println!("Results saved to: {}", results_path.display());
    }

    Ok(())
}

/// Run benchmark using Option B: argument graph + subagent node scores → SNN aggregation.
/// The subagent builds the graph, scores each node, saves as JSON.
/// The benchmark loads node scores, builds the graph, feeds through graph-SNN.
fn run_benchmark_graph(
    dataset_path: Option<PathBuf>,
    graph_scores_path: PathBuf,
    reference_ontology_path: Option<PathBuf>,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    use brain_in_the_fish_core::argument_graph;

    println!("Running Option B benchmark: argument graph → node scores → SNN aggregation");

    // Load reference ontology if provided
    let reference_turtle = if let Some(ref path) = reference_ontology_path {
        println!("Loading reference ontology: {}", path.display());
        let ttl = std::fs::read_to_string(path)?;
        println!("Reference ontology: {} chars", ttl.len());
        Some(ttl)
    } else {
        None
    };

    let samples = if let Some(ref path) = dataset_path {
        println!("Loading dataset: {}", path.display());
        benchmark::load_dataset(path)?
    } else {
        println!("Using synthetic benchmark dataset (10 samples)");
        benchmark::synthetic_dataset()
    };

    // Load subagent graph scores
    println!("Loading graph scores: {}", graph_scores_path.display());
    let gs_content = std::fs::read_to_string(&graph_scores_path)?;
    let graph_score_entries: Vec<argument_graph::GraphScoreEntry> = serde_json::from_str(&gs_content)?;
    let gs_map: std::collections::HashMap<String, &argument_graph::GraphScoreEntry> =
        graph_score_entries.iter().map(|e| (e.id.clone(), e)).collect();
    println!("Loaded {} graph score entries for {} samples\n", graph_score_entries.len(), samples.len());

    let mut predicted_scores: Vec<f64> = Vec::new();
    let mut actual_scores: Vec<f64> = Vec::new();
    let mut hallucination_count: usize = 0;
    let mut matched = 0usize;

    for (i, sample) in samples.iter().enumerate() {
        let intent = if sample.domain.is_empty() {
            "academic essay evaluation".to_string()
        } else {
            format!("{} essay evaluation", sample.domain)
        };

        let framework = criteria::framework_for_intent(&intent);
        let agents = agent::spawn_panel(&intent, &framework);
        let snn_config = snn::SNNConfig::default();

        // Build argument graph: prefer Turtle (full /sketch), then node scores, then regex
        let graph = if let Some(gs_entry) = gs_map.get(&sample.id) {
            matched += 1;
            if let Some(ref turtle) = gs_entry.turtle {
                // Full /sketch mode: load OWL Turtle into GraphStore
                match argument_graph::build_from_turtle(&sample.id, turtle, &gs_entry.node_scores) {
                    Ok(g) => {
                        if i < 5 {
                            eprintln!("  [DEBUG] {} Turtle: {} nodes, {} edges from GraphStore",
                                sample.id, g.nodes.len(), g.edges.len());
                        }
                        g
                    }
                    Err(e) => {
                        eprintln!("  Turtle parse failed for {}: {}, falling back to node scores", sample.id, e);
                        argument_graph::build_from_node_scores(&sample.id, &gs_entry.node_scores)
                    }
                }
            } else {
                // Node scores only (no Turtle)
                argument_graph::build_from_node_scores(&sample.id, &gs_entry.node_scores)
            }
        } else {
            // No subagent data — fall back to regex extraction
            argument_graph::build_from_text(&sample.text, &sample.id)
        };

        // Compute alignment against reference ontology if Turtle is available
        let alignment_candidates = if let (Some(ref_ttl), Some(gs_entry)) = (&reference_turtle, gs_map.get(&sample.id)) {
            if let Some(ref essay_ttl) = gs_entry.turtle {
                argument_graph::align_to_reference(essay_ttl, ref_ttl, 0.3).unwrap_or_default()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        // Feed graph through SNN (with alignment signals when available)
        let mut snn_networks: Vec<snn::AgentNetwork> = Vec::new();
        for agent_item in &agents {
            let mut network = snn::AgentNetwork::new(agent_item, &framework.criteria);
            network.feed_argument_graph_with_alignment(&graph, &snn_config, &alignment_candidates);
            snn_networks.push(network);
        }

        // Compute scores
        let mut round1_scores: Vec<Score> = Vec::new();
        for network in &snn_networks {
            let snn_scores = network.compute_scores(&framework.criteria, &snn_config);
            for (criterion_id, snn_score) in &snn_scores {
                let criterion = framework.criteria.iter().find(|c| c.id == *criterion_id);
                let max_score = criterion.map(|c| c.max_score).unwrap_or(10.0);
                round1_scores.push(Score {
                    agent_id: network.agent_id.clone(),
                    criterion_id: criterion_id.clone(),
                    score: snn_score.snn_score,
                    max_score,
                    round: 1,
                    justification: snn_score.explanation.clone(),
                    evidence_used: vec![],
                    gaps_identified: vec![],
                });
            }
        }

        let moderated = moderation::calculate_moderated_scores(&round1_scores, &agents);
        let overall = moderation::calculate_overall_score(&moderated, &framework);
        let predicted = overall.percentage / 100.0 * sample.max_score;

        // Hallucination detection
        if sample.max_score > 0.0 {
            let normalized_pred = predicted / sample.max_score;
            let normalized_actual = sample.expert_score / sample.max_score;
            if (normalized_pred - normalized_actual).abs() > 0.3 {
                hallucination_count += 1;
            }
        }

        predicted_scores.push(predicted);
        actual_scores.push(sample.expert_score);

        let metrics = argument_graph::compute_metrics(&graph);
        if (i + 1) % 25 == 0 || i == samples.len() - 1 {
            println!("  [{}/{}] {} | pred: {:.1} | actual: {:.1} | delta: {:.1} | nodes: {} edges: {} depth: {}",
                i + 1, samples.len(), sample.id, predicted, sample.expert_score,
                (predicted - sample.expert_score).abs(),
                metrics.node_count, metrics.edge_count, metrics.max_depth);
        }
    }

    println!("\nMatched graph scores: {} / {}", matched, samples.len());

    // Compute metrics
    let max_score_val = samples.iter().map(|s| s.max_score).fold(0.0f64, f64::max);
    let pearson_r = benchmark::pearson_correlation(&predicted_scores, &actual_scores);
    let qwk = benchmark::quadratic_weighted_kappa(&predicted_scores, &actual_scores, 0.0, max_score_val);
    let mae = benchmark::mean_absolute_error(&predicted_scores, &actual_scores);
    let rmse_val = benchmark::rmse(&predicted_scores, &actual_scores);
    let mean_predicted = predicted_scores.iter().sum::<f64>() / predicted_scores.len() as f64;
    let mean_actual = actual_scores.iter().sum::<f64>() / actual_scores.len() as f64;
    let nmae = if max_score_val > 0.0 { mae / max_score_val } else { 0.0 };
    let hallucination_rate = if samples.is_empty() { 0.0 } else { hallucination_count as f64 / samples.len() as f64 };

    let result = benchmark::BenchmarkResults {
        name: "graph_snn".to_string(),
        samples: samples.len(),
        pearson_r,
        qwk,
        mae,
        nmae,
        rmse: rmse_val,
        mean_predicted,
        mean_actual,
        hallucination_count,
        hallucination_rate,
        config: benchmark::BenchmarkConfig {
            use_snn: true,
            use_llm_extraction: true,
            label: "graph_snn".into(),
            ..Default::default()
        },
    };

    println!("\n  Pearson r: {:.3} | QWK: {:.3} | MAE: {:.2} | NMAE: {:.3} | RMSE: {:.2} | Halluc: {} ({:.1}%)\n",
        result.pearson_r, result.qwk, result.mae, result.nmae, result.rmse,
        result.hallucination_count, result.hallucination_rate * 100.0);
    println!("{}", benchmark::results_table(std::slice::from_ref(&result)));

    // Per-rubric breakdown
    let per_group = benchmark::per_group_results(&samples, &predicted_scores);
    if per_group.len() > 1 {
        println!("\n===== Per-Rubric Breakdown =====\n");
        println!("{}", benchmark::results_table(&per_group));
    }

    if let Some(output_dir) = output {
        std::fs::create_dir_all(&output_dir)?;
        let results_path = output_dir.join("graph-snn-results.json");
        std::fs::write(&results_path, serde_json::to_string_pretty(&[&result])?)?;
        println!("Results saved to: {}", results_path.display());
    }

    Ok(())
}

/// Self-calibrate SNN weights for graph-SNN scoring via Nelder-Mead.
/// Optimizes the 9 ScoreWeights parameters to maximize Pearson r against expert scores.
fn run_calibrate_graph(
    dataset_path: Option<PathBuf>,
    graph_scores_path: PathBuf,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    use brain_in_the_fish_core::argument_graph;
    use brain_in_the_fish_core::optimize;

    println!("Self-calibrating SNN weights for graph-SNN scoring...\n");

    let samples = if let Some(ref path) = dataset_path {
        benchmark::load_dataset(path)?
    } else {
        benchmark::synthetic_dataset()
    };

    let gs_content = std::fs::read_to_string(&graph_scores_path)?;
    let graph_score_entries: Vec<argument_graph::GraphScoreEntry> = serde_json::from_str(&gs_content)?;
    let gs_map: std::collections::HashMap<String, &argument_graph::GraphScoreEntry> =
        graph_score_entries.iter().map(|e| (e.id.clone(), e)).collect();

    println!("Loaded {} samples, {} graph scores", samples.len(), graph_score_entries.len());

    // Pre-build all argument graphs (expensive, do once)
    let graphs: Vec<argument_graph::ArgumentGraph> = samples.iter().map(|sample| {
        if let Some(gs_entry) = gs_map.get(&sample.id) {
            if let Some(ref turtle) = gs_entry.turtle {
                argument_graph::build_from_turtle(&sample.id, turtle, &gs_entry.node_scores)
                    .unwrap_or_else(|_| argument_graph::build_from_node_scores(&sample.id, &gs_entry.node_scores))
            } else {
                argument_graph::build_from_node_scores(&sample.id, &gs_entry.node_scores)
            }
        } else {
            argument_graph::build_from_text(&sample.text, &sample.id)
        }
    }).collect();

    let actual: Vec<f64> = samples.iter().map(|s| s.expert_score).collect();

    // Default weights as starting point
    let default_weights = snn::ScoreWeights::default();
    let initial = default_weights.to_params();
    println!("Initial weights: {:?}", initial);

    // Objective: minimize negative Pearson (= maximize Pearson)
    let objective = |params: &[f64]| -> f64 {
        let weights = snn::ScoreWeights::from_params(params);
        let config = snn::SNNConfig { weights, ..Default::default() };

        let mut predicted: Vec<f64> = Vec::with_capacity(samples.len());
        for (i, sample) in samples.iter().enumerate() {
            let intent = if sample.domain.is_empty() {
                "academic essay evaluation".to_string()
            } else {
                format!("{} essay evaluation", sample.domain)
            };
            let framework = criteria::framework_for_intent(&intent);
            let agents = agent::spawn_panel(&intent, &framework);

            let mut snn_networks: Vec<snn::AgentNetwork> = Vec::new();
            for agent_item in &agents {
                let mut network = snn::AgentNetwork::new(agent_item, &framework.criteria);
                network.feed_argument_graph(&graphs[i], &config);
                snn_networks.push(network);
            }

            let mut round1_scores: Vec<Score> = Vec::new();
            for network in &snn_networks {
                let snn_scores = network.compute_scores(&framework.criteria, &config);
                for (criterion_id, snn_score) in &snn_scores {
                    let criterion = framework.criteria.iter().find(|c| c.id == *criterion_id);
                    let max_score = criterion.map(|c| c.max_score).unwrap_or(10.0);
                    round1_scores.push(Score {
                        agent_id: network.agent_id.clone(),
                        criterion_id: criterion_id.clone(),
                        score: snn_score.snn_score,
                        max_score,
                        round: 1,
                        justification: String::new(),
                        evidence_used: vec![],
                        gaps_identified: vec![],
                    });
                }
            }

            let moderated = moderation::calculate_moderated_scores(&round1_scores, &agents);
            let overall = moderation::calculate_overall_score(&moderated, &framework);
            predicted.push(overall.percentage / 100.0 * sample.max_score);
        }

        let pearson = benchmark::pearson_correlation(&predicted, &actual);
        let mae = benchmark::mean_absolute_error(&predicted, &actual);

        // Multi-objective: maximize Pearson, minimize MAE
        // Weight Pearson more heavily since it's the primary metric
        -pearson + mae * 0.01
    };

    // Run before
    let before_loss = objective(&initial);
    println!("Before calibration: loss={:.4} (Pearson~{:.3})\n", before_loss, -before_loss);

    println!("Running Nelder-Mead optimization (500 iterations)...");
    let (best_params, best_loss) = optimize::nelder_mead(&objective, &initial, 500, 1e-8);

    let best_weights = snn::ScoreWeights::from_params(&best_params);
    println!("\nCalibrated weights:");
    println!("  w_saturation:  {:.4}", best_weights.w_saturation);
    println!("  w_quality:     {:.4}", best_weights.w_quality);
    println!("  w_firing:      {:.4}", best_weights.w_firing);
    println!("  saturation_base: {:.2}", best_weights.saturation_base);
    println!("  lr_quantified: {:.3}", best_weights.lr_quantified);
    println!("  lr_evidence:   {:.3}", best_weights.lr_evidence);
    println!("  lr_citation:   {:.3}", best_weights.lr_citation);
    println!("  lr_alignment:  {:.3}", best_weights.lr_alignment);
    println!("  lr_claim:      {:.3}", best_weights.lr_claim);
    println!("\nBest loss: {:.4} (Pearson~{:.3})", best_loss, -best_loss);
    println!("Improvement: {:.4} → {:.4}", before_loss, best_loss);

    // Run final benchmark with calibrated weights
    println!("\n--- Final benchmark with calibrated weights ---");
    let config = snn::SNNConfig { weights: best_weights, ..Default::default() };

    let mut predicted: Vec<f64> = Vec::new();
    let mut hallucination_count = 0usize;
    for (i, sample) in samples.iter().enumerate() {
        let intent = if sample.domain.is_empty() {
            "academic essay evaluation".to_string()
        } else {
            format!("{} essay evaluation", sample.domain)
        };
        let framework = criteria::framework_for_intent(&intent);
        let agents = agent::spawn_panel(&intent, &framework);

        let mut snn_networks: Vec<snn::AgentNetwork> = Vec::new();
        for agent_item in &agents {
            let mut network = snn::AgentNetwork::new(agent_item, &framework.criteria);
            network.feed_argument_graph(&graphs[i], &config);
            snn_networks.push(network);
        }
        let mut round1_scores: Vec<Score> = Vec::new();
        for network in &snn_networks {
            let snn_scores = network.compute_scores(&framework.criteria, &config);
            for (criterion_id, snn_score) in &snn_scores {
                let criterion = framework.criteria.iter().find(|c| c.id == *criterion_id);
                let max_score = criterion.map(|c| c.max_score).unwrap_or(10.0);
                round1_scores.push(Score {
                    agent_id: network.agent_id.clone(),
                    criterion_id: criterion_id.clone(),
                    score: snn_score.snn_score,
                    max_score,
                    round: 1,
                    justification: String::new(),
                    evidence_used: vec![],
                    gaps_identified: vec![],
                });
            }
        }
        let moderated = moderation::calculate_moderated_scores(&round1_scores, &agents);
        let overall = moderation::calculate_overall_score(&moderated, &framework);
        let pred = overall.percentage / 100.0 * sample.max_score;

        if sample.max_score > 0.0 {
            let np = pred / sample.max_score;
            let na = sample.expert_score / sample.max_score;
            if (np - na).abs() > 0.3 { hallucination_count += 1; }
        }
        predicted.push(pred);
    }

    let max_score_val = samples.iter().map(|s| s.max_score).fold(0.0f64, f64::max);
    let pearson_r = benchmark::pearson_correlation(&predicted, &actual);
    let qwk = benchmark::quadratic_weighted_kappa(&predicted, &actual, 0.0, max_score_val);
    let mae = benchmark::mean_absolute_error(&predicted, &actual);
    let rmse_val = benchmark::rmse(&predicted, &actual);
    let nmae = if max_score_val > 0.0 { mae / max_score_val } else { 0.0 };
    let hr = hallucination_count as f64 / samples.len() as f64;

    println!("\n  Pearson r: {:.3} | QWK: {:.3} | MAE: {:.2} | NMAE: {:.3} | RMSE: {:.2} | Halluc: {} ({:.1}%)",
        pearson_r, qwk, mae, nmae, rmse_val, hallucination_count, hr * 100.0);

    if let Some(output_dir) = output {
        std::fs::create_dir_all(&output_dir)?;
        let weights_path = output_dir.join("calibrated-weights.json");
        std::fs::write(&weights_path, serde_json::to_string_pretty(&best_params)?)?;
        println!("Calibrated weights saved to: {}", weights_path.display());
    }

    Ok(())
}

fn run_history(dir: Option<PathBuf>) -> anyhow::Result<()> {
    let history_dir = if let Some(d) = dir {
        d
    } else {
        // Use the same default as MemoryStore::open()
        let store = memory::MemoryStore::open()?;
        store.dir().to_path_buf()
    };

    if !history_dir.exists() {
        println!("No history directory found at: {}", history_dir.display());
        println!("Run 'brain-in-the-fish evaluate' to create evaluation history.");
        return Ok(());
    }

    let report = memory::cross_evaluation_report(&history_dir)?;
    println!("{report}");
    Ok(())
}

async fn run_serve(_host: String, _port: u16) -> anyhow::Result<()> {
    eprintln!("Brain in the Fish MCP server starting (stdio transport)...");
    let server = brain_in_the_fish_core::server::EvalServer::new();
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn run_shoal(
    dataset: PathBuf,
    batch_size: usize,
    output: PathBuf,
    max_score: f64,
    intent: String,
    collect: bool,
    anchors_path: Option<PathBuf>,
) -> anyhow::Result<()> {
    let samples = benchmark::load_dataset(&dataset)?;

    // Load anchor essays if provided
    let anchors: Option<Vec<brain_in_the_fish_core::calibrate::AnchorEssay>> =
        if let Some(path) = &anchors_path {
            let content = std::fs::read_to_string(path)?;
            let loaded: Vec<brain_in_the_fish_core::calibrate::AnchorEssay> =
                serde_json::from_str(&content)?;
            println!("Loaded {} anchor essays for calibration", loaded.len());
            Some(loaded)
        } else {
            None
        };

    let config = shoal::ShoalConfig {
        batch_size,
        scale_description: format!("0.0-{:.1}", max_score),
        max_score,
        intent: intent.clone(),
        anchors: anchors.clone(),
    };

    if collect {
        // Read all batch_N_scores.json files from output dir
        let mut all_scored = Vec::new();
        for entry in std::fs::read_dir(&output)? {
            let path = entry?.path();
            if path
                .file_name()
                .is_some_and(|n| {
                    n.to_str().is_some_and(|s| {
                        s.starts_with("batch_") && s.ends_with("_scores.json")
                    })
                })
            {
                let content = std::fs::read_to_string(&path)?;
                let scored: Vec<shoal::ScoredEssay> = serde_json::from_str(&content)?;
                all_scored.extend(scored);
            }
        }
        println!("Running EDS pipeline on {} scored essays...", all_scored.len());
        let bm = shoal::compute_blended_metrics_full(&all_scored, &samples, &config, &intent);

        println!("\nShoal results ({} essays):", bm.subagent.samples);
        println!("  | Method     | Pearson r | QWK   | MAE  | RMSE |");
        println!("  |------------|-----------|-------|------|------|");
        println!(
            "  | Subagent   | {:.3}     | {:.3} | {:.2} | {:.2} |",
            bm.subagent.pearson_r, bm.subagent.qwk, bm.subagent.mae, bm.subagent.rmse
        );
        println!(
            "  | EDS only   | {:.3}     | {:.3} | {:.2} | {:.2} |",
            bm.eds.pearson_r, bm.eds.qwk, bm.eds.mae, bm.eds.rmse
        );
        println!(
            "  | Blended    | {:.3}     | {:.3} | {:.2} | {:.2} |",
            bm.blended.pearson_r, bm.blended.qwk, bm.blended.mae, bm.blended.rmse
        );
        if let Some(cal) = &bm.calibrated {
            println!(
                "  | Calibrated | {:.3}     | {:.3} | {:.2} | {:.2} |",
                cal.pearson_r, cal.qwk, cal.mae, cal.rmse
            );
        }

        // Score band analysis
        let bands = shoal::score_band_analysis(&all_scored, &samples);
        print!("\n{}", shoal::format_score_band_analysis(&bands));

        shoal::save_results(&all_scored, &bm.blended, &output)?;
    } else {
        // Generate batch prompts
        std::fs::create_dir_all(&output)?;
        let batches = shoal::split_batches(&samples, batch_size);
        println!(
            "Shoal: {} essays -> {} batches of up to {}",
            samples.len(),
            batches.len(),
            batch_size
        );
        if anchors_path.is_some() {
            println!("  Calibration anchors loaded — prompts will include anchor references");
        }
        for (i, batch) in batches.iter().enumerate() {
            let prompt = shoal::batch_scoring_prompt(batch, &config);
            let prompt_path = output.join(format!("batch_{}_prompt.txt", i));
            std::fs::write(&prompt_path, &prompt)?;
            println!(
                "  Batch {}: {} essays -> {}",
                i,
                batch.len(),
                prompt_path.display()
            );
        }
        println!("\nNext steps:");
        println!("  1. Dispatch each prompt to a Claude subagent");
        println!(
            "  2. Save each response as batch_N_scores.json in {}",
            output.display()
        );
        println!(
            "  3. Run: brain-in-the-fish shoal {} --collect --output {}",
            dataset.display(),
            output.display()
        );
    }
    Ok(())
}
