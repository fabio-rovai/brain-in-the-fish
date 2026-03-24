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
        Commands::Benchmark { dataset, ablation, output } => {
            run_benchmark(dataset, ablation, output)
        }
        Commands::History { dir } => {
            run_history(dir)
        }
        Commands::Serve { host, port } => {
            run_serve(host, port).await
        }
        Commands::Shoal { dataset, batch_size, output, max_score, intent, collect } => {
            run_shoal(dataset, batch_size, output, max_score, intent, collect)
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

    for config in &configs {
        println!("--- Config: {} ---", config.label);
        let mut predicted_scores: Vec<f64> = Vec::new();
        let mut actual_scores: Vec<f64> = Vec::new();

        for sample in &samples {
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

            predicted_scores.push(predicted);
            actual_scores.push(sample.expert_score);
            println!("  {} | predicted: {:.1} | actual: {:.1} | delta: {:.1}",
                sample.id, predicted, sample.expert_score, (predicted - sample.expert_score).abs());
        }

        let pearson_r = benchmark::pearson_correlation(&predicted_scores, &actual_scores);
        let qwk = benchmark::quadratic_weighted_kappa(&predicted_scores, &actual_scores, 0.0, 10.0);
        let mae = benchmark::mean_absolute_error(&predicted_scores, &actual_scores);
        let rmse_val = benchmark::rmse(&predicted_scores, &actual_scores);
        let mean_predicted = predicted_scores.iter().sum::<f64>() / predicted_scores.len() as f64;
        let mean_actual = actual_scores.iter().sum::<f64>() / actual_scores.len() as f64;

        let result = benchmark::BenchmarkResults {
            name: config.label.clone(),
            samples: samples.len(),
            pearson_r,
            qwk,
            mae,
            rmse: rmse_val,
            mean_predicted,
            mean_actual,
            config: config.clone(),
        };
        println!("  Pearson r: {:.3} | QWK: {:.3} | MAE: {:.2} | RMSE: {:.2}\n",
            pearson_r, qwk, mae, rmse_val);
        all_results.push(result);
    }

    println!("\n{}", benchmark::results_table(&all_results));

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
) -> anyhow::Result<()> {
    let samples = benchmark::load_dataset(&dataset)?;
    let config = shoal::ShoalConfig {
        batch_size,
        scale_description: format!("0.0-{:.1}", max_score),
        max_score,
        intent: intent.clone(),
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
        let (sub, eds, blended) = shoal::compute_blended_metrics(&all_scored, &samples, &config, &intent);

        println!("\nShoal results ({} essays):", sub.samples);
        println!("  | Method   | Pearson r | QWK   | MAE  | RMSE |");
        println!("  |----------|-----------|-------|------|------|");
        println!(
            "  | Subagent | {:.3}     | {:.3} | {:.2} | {:.2} |",
            sub.pearson_r, sub.qwk, sub.mae, sub.rmse
        );
        println!(
            "  | EDS only | {:.3}     | {:.3} | {:.2} | {:.2} |",
            eds.pearson_r, eds.qwk, eds.mae, eds.rmse
        );
        println!(
            "  | Blended  | {:.3}     | {:.3} | {:.2} | {:.2} |",
            blended.pearson_r, blended.qwk, blended.mae, blended.rmse
        );

        // Score band analysis
        let bands = shoal::score_band_analysis(&all_scored, &samples);
        print!("\n{}", shoal::format_score_band_analysis(&bands));

        shoal::save_results(&all_scored, &blended, &output)?;
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
