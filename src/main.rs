use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use rmcp::ServiceExt;

use brain_in_the_fish::*;
use brain_in_the_fish::types::*;
use brain_in_the_fish::alignment;
use brain_in_the_fish::snn;
use brain_in_the_fish::memory;

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Evaluate { document, intent, criteria, output } => {
            run_evaluate(document, intent, criteria, output).await
        }
        Commands::Serve { host, port } => {
            run_serve(host, port).await
        }
    }
}

async fn run_evaluate(
    document: PathBuf,
    intent: String,
    criteria_path: Option<PathBuf>,
    output: Option<PathBuf>,
) -> anyhow::Result<()> {
    // 1. Ingest document
    println!("1. Ingesting document: {}", document.display());
    let graph = Arc::new(open_ontologies::graph::GraphStore::new());
    let (mut doc, _raw_sections) = ingest::ingest_pdf(&document, &intent)?;

    // 2. Enrich: regex claim/evidence extraction + paragraph subsections
    println!("2. Enriching document (deterministic)...");
    enrich_document(&mut doc);

    let triples = ingest::load_document_ontology(&graph, &doc)?;
    println!("   Loaded {} triples, {} sections", triples, doc.sections.len());

    // 3. Load evaluation criteria
    println!("3. Loading evaluation criteria...");
    let framework = if let Some(criteria_file) = criteria_path {
        println!("   From file: {}", criteria_file.display());
        criteria::parse_framework_from_file(&criteria_file)?
    } else {
        criteria::framework_for_intent(&intent)
    };
    let crit_triples = criteria::load_criteria_ontology(&graph, &framework)?;
    println!("   {} criteria, {} triples", framework.criteria.len(), crit_triples);

    // 4. Discover sector guidelines with provenance
    println!("4. Discovering sector guidelines...");
    let guidelines = research::built_in_guidelines(&intent);
    let guide_triples = research::load_guidelines(&graph, &guidelines)?;
    println!("   {} guidelines, {} triples", guidelines.len(), guide_triples);
    for g in &guidelines {
        println!("   - {} ({})", g.title, g.sector);
    }

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

    // 6. Spawn agent panel
    println!("6. Spawning evaluator panel...");
    let mut agents = agent::spawn_panel(&intent, &framework);
    for a in &agents {
        let agent_triples = agent::load_agent_ontology(&graph, a)?;
        println!("   {} ({}) — {} triples", a.name, a.role, agent_triples);
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

            println!("   {} -> {}: {:.1}/{:.0} (confidence {:.0}%)",
                network.agent_name, criterion_title, snn_score.snn_score, max_score,
                snn_score.confidence * 100.0);

            if !snn_score.grounded {
                println!("      LOW EVIDENCE: {}", criterion_title);
            }

            scoring::record_score(&graph, &score)?;
            round1_scores.push(score);
        }
    }

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

    // 9. Moderate: trust-weighted consensus
    println!("9. Calculating consensus scores...");
    let moderated = moderation::calculate_moderated_scores(&current_scores, &agents);
    let overall = moderation::calculate_overall_score(&moderated, &framework);

    println!("   Overall: {:.1}/{:.1} ({:.1}%)", overall.total_score, overall.max_possible, overall.percentage);
    if let Some(passed) = overall.passed {
        println!("   Result: {}", if passed { "PASS" } else { "FAIL" });
    }

    // 10. Generate report
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

    let report = report::generate_report(&session, &overall);

    let output_dir = output.unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&output_dir)?;

    let report_path = output_dir.join("evaluation-report.md");
    std::fs::write(&report_path, &report)?;
    println!("   Report: {}", report_path.display());

    // Turtle export
    let turtle_path = output_dir.join("evaluation.ttl");
    let turtle = report::session_to_turtle(&session);
    std::fs::write(&turtle_path, &turtle)?;
    println!("   Turtle: {}", turtle_path.display());

    // 11. Generate graph visualization
    println!("11. Generating graph visualization...");
    let graph_data = visualize::extract_graph_data(&session);
    let lineage = visualize::build_lineage(&session);
    let graph_html = visualize::generate_graph_html(&graph_data, &lineage, &session.document.title, &intent);
    let graph_path = output_dir.join("evaluation-graph.html");
    std::fs::write(&graph_path, &graph_html)?;
    println!("   Graph: {}", graph_path.display());

    // 12. Save to cross-evaluation memory
    println!("12. Saving to memory...");
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

    // 13. Save orchestration data for Claude subagent scoring
    println!("13. Saving orchestration tasks...");
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

async fn run_serve(_host: String, _port: u16) -> anyhow::Result<()> {
    eprintln!("Brain in the Fish MCP server starting (stdio transport)...");
    let server = server::EvalServer::new();
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
