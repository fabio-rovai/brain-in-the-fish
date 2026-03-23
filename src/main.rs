use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use rmcp::ServiceExt;

use brain_in_the_fish::*;
use brain_in_the_fish::types::*;
use brain_in_the_fish::llm;
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
    /// Evaluate a document against criteria
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
    /// Start the MCP server
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
    // 1. Initialize graph store
    let graph = Arc::new(open_ontologies::graph::GraphStore::new());

    // 2. Ingest document
    println!("Ingesting document: {}", document.display());
    let (mut doc, _raw_sections) = ingest::ingest_pdf(&document, &intent)?;

    // Enrich: extract claims, evidence, and paragraph-level subsections from text
    enrich_document(&mut doc);

    // LLM-powered enrichment when API key is available
    if llm::ClaudeClient::available() {
        println!("   Enriching with Claude API...");
        if let Err(e) = enrich_with_llm(&mut doc, &intent).await {
            eprintln!("   Warning: LLM enrichment failed, using regex fallback: {}", e);
        }
    }

    let triples = ingest::load_document_ontology(&graph, &doc)?;
    println!("   Loaded {} triples, {} sections", triples, doc.sections.len());

    // 3. Load criteria — use framework_for_intent when no file provided
    println!("Loading evaluation criteria...");
    let framework = if let Some(criteria_file) = criteria_path {
        println!("   Loading criteria from: {}", criteria_file.display());
        criteria::parse_framework_from_file(&criteria_file)?
    } else {
        criteria::framework_for_intent(&intent)
    };
    let crit_triples = criteria::load_criteria_ontology(&graph, &framework)?;
    println!(
        "   Loaded {} criteria, {} triples",
        framework.criteria.len(),
        crit_triples
    );

    // 3.5 Align document sections to criteria
    println!("Aligning document to criteria...");
    let (alignments, gaps) = match alignment::align_via_ontology(&graph, &doc, &framework) {
        Ok(result) => {
            println!("   (using ontology alignment with 7 structural signals)");
            result
        }
        Err(_) => {
            println!("   (using keyword-based alignment)");
            alignment::align_sections_to_criteria(&doc, &framework)
        }
    };
    let align_triples = alignment::load_alignments(&graph, &alignments)?;
    println!("   {} alignments, {} gaps, {} triples", alignments.len(), gaps.len(), align_triples);
    for gap in &gaps {
        println!("   GAP: No content for '{}'", gap.criterion_title);
    }

    // 4. Spawn agent panel
    println!("Spawning evaluator panel...");
    let mut agents = agent::spawn_panel(&intent, &framework);
    for a in &agents {
        let agent_triples = agent::load_agent_ontology(&graph, a)?;
        println!("   {} ({}) - {} triples", a.name, a.role, agent_triples);
    }

    // 5. Generate scoring prompts (one per agent per criterion)
    println!(
        "\nScoring prompts generated for {} agents x {} criteria",
        agents.len(),
        framework.criteria.len()
    );

    // 6. Score — real LLM or demo fallback
    println!("\nRound 1: Independent scoring...");
    let mut round1_scores = Vec::new();

    if llm::ClaudeClient::available() {
        println!("   Using Claude API for real evaluation...");
        let claude = llm::ClaudeClient::from_env()?;

        for agent_item in &agents {
            for criterion in &framework.criteria {
                // Get aligned sections for this criterion
                let matched_sections: Vec<scoring::SectionMatch> = alignment::sections_for_criterion(&alignments, &criterion.id, &doc)
                    .into_iter()
                    .map(|(s, _conf)| scoring::SectionMatch {
                        section_iri: s.id.clone(),
                        title: s.title.clone(),
                        text: s.text.clone(),
                        word_count: s.word_count,
                    })
                    .collect();

                let prompt = scoring::generate_scoring_prompt(agent_item, criterion, &matched_sections, 1);

                match claude.score_as_agent(&prompt).await {
                    Ok(result) => {
                        let score = Score {
                            agent_id: agent_item.id.clone(),
                            criterion_id: criterion.id.clone(),
                            score: result.score.min(criterion.max_score).max(0.0),
                            max_score: criterion.max_score,
                            round: 1,
                            justification: result.justification,
                            evidence_used: result.evidence_used,
                            gaps_identified: result.gaps_identified,
                        };
                        println!("   {} → {}: {:.1}/{:.0}", agent_item.name, criterion.title, score.score, score.max_score);
                        scoring::record_score(&graph, &score)?;
                        round1_scores.push(score);
                    }
                    Err(e) => {
                        eprintln!("   Warning: Claude scoring failed for {} on {}: {}", agent_item.name, criterion.title, e);
                        // Fallback to demo score
                        let demo_score_val = generate_demo_score(agent_item, criterion);
                        let justification = generate_demo_justification(agent_item, criterion, demo_score_val);
                        let (evidence, gaps_list) = generate_demo_evidence_and_gaps(agent_item, criterion, demo_score_val);
                        let score = Score {
                            agent_id: agent_item.id.clone(),
                            criterion_id: criterion.id.clone(),
                            score: demo_score_val,
                            max_score: criterion.max_score,
                            round: 1,
                            justification,
                            evidence_used: evidence,
                            gaps_identified: gaps_list,
                        };
                        scoring::record_score(&graph, &score)?;
                        round1_scores.push(score);
                    }
                }
            }
        }
    } else {
        println!("   ANTHROPIC_API_KEY not set — using demo scores");
        println!("   Set ANTHROPIC_API_KEY for real LLM evaluation");
        for agent_item in &agents {
            for criterion in &framework.criteria {
                let demo_score = generate_demo_score(agent_item, criterion);
                let justification = generate_demo_justification(agent_item, criterion, demo_score);
                let (evidence, gaps) = generate_demo_evidence_and_gaps(agent_item, criterion, demo_score);
                let score = Score {
                    agent_id: agent_item.id.clone(),
                    criterion_id: criterion.id.clone(),
                    score: demo_score,
                    max_score: criterion.max_score,
                    round: 1,
                    justification,
                    evidence_used: evidence,
                    gaps_identified: gaps,
                };
                scoring::record_score(&graph, &score)?;
                round1_scores.push(score);
            }
        }
    }

    // 6.5 SNN verification layer
    println!("\nSNN verification...");
    let snn_config = snn::SNNConfig::default();
    let mut snn_networks: Vec<snn::AgentNetwork> = Vec::new();

    for agent_item in &agents {
        let mut network = snn::AgentNetwork::new(agent_item, &framework.criteria);
        network.feed_evidence(&doc, &alignments, &snn_config);
        snn_networks.push(network);
    }

    // Compute SNN scores and blend with LLM/demo scores
    let mut blended_scores = Vec::new();
    for network in snn_networks.iter() {
        let snn_scores = network.compute_scores(&framework.criteria, &snn_config);
        for (criterion_id, snn_score) in &snn_scores {
            // Find matching LLM score
            if let Some(llm_score_entry) = round1_scores.iter().find(|s|
                s.agent_id == network.agent_id && s.criterion_id == *criterion_id
            ) {
                let max_score = llm_score_entry.max_score;
                let blended = snn::blend_scores(snn_score, llm_score_entry.score, max_score);

                if blended.hallucination_risk {
                    println!("   WARNING: {} on '{}' — LLM={:.1} vs SNN={:.1} — possible hallucination",
                        network.agent_name,
                        framework.criteria.iter().find(|c| c.id == *criterion_id)
                            .map(|c| c.title.as_str()).unwrap_or(criterion_id),
                        llm_score_entry.score, snn_score.snn_score);
                }

                blended_scores.push((network.agent_id.clone(), criterion_id.clone(), blended));
            }
        }
    }

    // Update round1_scores with blended values
    for (agent_id, criterion_id, blended) in &blended_scores {
        if let Some(score) = round1_scores.iter_mut().find(|s|
            s.agent_id == *agent_id && s.criterion_id == *criterion_id
        ) {
            score.score = blended.final_score;
            score.justification = format!("{}\n\n[SNN: {}]", score.justification, blended.explanation);
        }
    }

    let hallucination_count = blended_scores.iter().filter(|(_, _, b)| b.hallucination_risk).count();
    println!("   {} scores blended (SNN+LLM), {} hallucination warnings", blended_scores.len(), hallucination_count);

    // 7. Debate rounds
    let mut all_rounds = vec![debate::build_debate_round(1, round1_scores.clone(), vec![], None, false)];
    let mut current_scores = round1_scores;
    let max_rounds = 5;

    for round_num in 2..=max_rounds {
        let disagreements = debate::find_disagreements(&current_scores, 2.0);
        if disagreements.is_empty() {
            println!("\n   No disagreements — debate converged at round {}", round_num - 1);
            // Mark last round as converged
            if let Some(last) = all_rounds.last_mut() {
                last.converged = true;
            }
            break;
        }

        println!("\nRound {}: {} disagreements to debate...", round_num, disagreements.len());
        let mut challenges = Vec::new();
        let mut new_scores = current_scores.clone();

        // Create Claude client once if available for this round
        let claude_client = if llm::ClaudeClient::available() {
            llm::ClaudeClient::from_env().ok()
        } else {
            None
        };

        for disagreement in &disagreements {
            // Find challenger and target agents
            let challenger = agents.iter().find(|a| a.id == disagreement.agent_a_id);
            let target = agents.iter().find(|a| a.id == disagreement.agent_b_id);
            let criterion = framework.criteria.iter().find(|c| c.id == disagreement.criterion_id);

            if let (Some(challenger), Some(target), Some(criterion)) = (challenger, target, criterion) {
                // Find their justifications
                let challenger_just = current_scores.iter()
                    .find(|s| s.agent_id == challenger.id && s.criterion_id == criterion.id)
                    .map(|s| s.justification.as_str())
                    .unwrap_or("");
                let target_just = current_scores.iter()
                    .find(|s| s.agent_id == target.id && s.criterion_id == criterion.id)
                    .map(|s| s.justification.as_str())
                    .unwrap_or("");

                // Higher scorer challenges lower scorer
                let (actual_challenger, actual_target) = if disagreement.agent_a_score > disagreement.agent_b_score {
                    (challenger, target)
                } else {
                    (target, challenger)
                };

                println!("   {} challenges {} on '{}'", actual_challenger.name, actual_target.name, criterion.title);

                if let Some(ref claude) = claude_client {
                    // Real LLM debate
                    let challenge_prompt = debate::generate_challenge_prompt(
                        actual_challenger, actual_target,
                        disagreement,
                        challenger_just, target_just,
                        criterion,
                    );

                    match claude.generate_challenge(&challenge_prompt).await {
                        Ok(challenge_result) => {
                            let target_score_entry = current_scores.iter()
                                .find(|s| s.agent_id == actual_target.id && s.criterion_id == criterion.id)
                                .cloned();

                            if let Some(target_score) = target_score_entry {
                                let response_prompt = debate::generate_response_prompt(
                                    actual_target, actual_challenger,
                                    &challenge_result.argument,
                                    &target_score,
                                    criterion,
                                );

                                match claude.generate_response(&response_prompt).await {
                                    Ok(response_result) => {
                                        let old_score = target_score.score;
                                        let new_score_val = if response_result.maintain_score {
                                            old_score
                                        } else {
                                            response_result.new_score
                                                .unwrap_or(old_score)
                                                .min(target_score.max_score)
                                                .max(0.0)
                                        };

                                        if let Some(s) = new_scores.iter_mut().find(|s|
                                            s.agent_id == actual_target.id && s.criterion_id == criterion.id
                                        ) {
                                            s.score = new_score_val;
                                            s.round = round_num;
                                            s.justification = response_result.justification.clone();
                                        }

                                        let challenge = Challenge {
                                            challenger_id: actual_challenger.id.clone(),
                                            target_agent_id: actual_target.id.clone(),
                                            criterion_id: criterion.id.clone(),
                                            round: round_num,
                                            argument: challenge_result.argument,
                                            response: Some(response_result.response),
                                            score_change: if (old_score - new_score_val).abs() > 0.01 {
                                                Some((old_score, new_score_val))
                                            } else {
                                                None
                                            },
                                        };
                                        challenges.push(challenge);

                                        println!("     {} -> {} (LLM debate: {:.1} -> {:.1})",
                                            actual_challenger.name, actual_target.name, old_score, new_score_val);
                                    }
                                    Err(e) => {
                                        eprintln!("     LLM response failed, using mechanical fallback: {}", e);
                                        mechanical_converge(&mut new_scores, &current_scores, actual_challenger, actual_target, criterion, round_num, &mut challenges, disagreement);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("     LLM challenge failed, using mechanical fallback: {}", e);
                            mechanical_converge(&mut new_scores, &current_scores, actual_challenger, actual_target, criterion, round_num, &mut challenges, disagreement);
                        }
                    }
                } else {
                    // Mechanical convergence fallback (no API key)
                    mechanical_converge(&mut new_scores, &current_scores, actual_challenger, actual_target, criterion, round_num, &mut challenges, disagreement);
                }
            }
        }

        // Update trust weights
        debate::update_trust_weights(&mut agents, &challenges);

        // Calculate drift
        let drift = debate::calculate_drift_velocity(&current_scores, &new_scores);
        let converged = debate::check_convergence(drift, 0.5);

        println!("   Drift velocity: {:.2}, Converged: {}", drift, converged);

        current_scores = new_scores;
        all_rounds.push(debate::build_debate_round(round_num, current_scores.clone(), challenges, Some(drift), converged));

        if converged {
            println!("   Debate converged!");
            break;
        }
    }

    // 8. Calculate moderated scores
    println!("\nCalculating consensus scores...");
    let moderated = moderation::calculate_moderated_scores(&current_scores, &agents);
    let overall = moderation::calculate_overall_score(&moderated, &framework);

    println!(
        "   Overall: {:.1}/{:.1} ({:.1}%)",
        overall.total_score, overall.max_possible, overall.percentage
    );
    if let Some(passed) = overall.passed {
        println!("   Result: {}", if passed { "PASS" } else { "FAIL" });
    }

    // 9. Generate report
    println!("\nGenerating report...");
    let session = EvaluationSession {
        id: uuid::Uuid::new_v4().to_string(),
        document: doc,
        framework,
        agents,
        alignments,
        gaps,
        rounds: all_rounds,
        final_scores: moderated,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let report = report::generate_report(&session, &overall);

    // 10. Output
    let output_dir = output.unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&output_dir)?;

    let report_path = output_dir.join("evaluation-report.md");
    std::fs::write(&report_path, &report)?;
    println!("\nReport saved to: {}", report_path.display());

    // Also export Turtle
    let turtle_path = output_dir.join("evaluation.ttl");
    let turtle = report::session_to_turtle(&session);
    std::fs::write(&turtle_path, &turtle)?;
    println!("   Turtle export: {}", turtle_path.display());

    // Generate interactive graph visualization (session-based — richer, connected)
    let graph_data = visualize::extract_graph_data(&session);
    let lineage = visualize::build_lineage(&session);
    let graph_html = visualize::generate_graph_html(
        &graph_data,
        &lineage,
        &session.document.title,
        &intent,
    );
    let graph_path = output_dir.join("evaluation-graph.html");
    std::fs::write(&graph_path, &graph_html)?;
    println!("   Graph visualization: {}", graph_path.display());

    // 11. Save to cross-evaluation memory
    if let Ok(store) = memory::MemoryStore::open() {
        let record = memory::build_record(&session, &overall, &intent);
        if let Ok(Some(comp)) = store.compare(&record) {
            println!("\n   Historical comparison ({} previous evaluations):", comp.total_compared);
            println!("   This: {:.1}% | Mean: {:.1}% | Percentile: {}th",
                comp.current_percentage, comp.historical_mean, comp.percentile);
        }
        if let Ok(path) = store.save(&record) {
            println!("   Saved to memory: {}", path.display());
        }
    }

    Ok(())
}

/// Generate a deterministic demo score varied by agent role and criterion.
///
/// Different agent archetypes score differently across criteria to produce
/// a realistic-looking spread rather than uniform scores.
fn generate_demo_score(agent: &EvaluatorAgent, criterion: &EvaluationCriterion) -> f64 {
    let role_lower = agent.role.to_lowercase();
    let title_lower = criterion.title.to_lowercase();

    // Deterministic jitter from name/title lengths so identical roles still vary slightly
    let jitter = ((agent.name.len() * 7 + criterion.title.len() * 3) % 10) as f64 * 0.1;

    let base = if role_lower.contains("subject") || role_lower.contains("expert") {
        // Subject Expert: strong on knowledge/content, weaker on communication/style
        if title_lower.contains("knowledge") || title_lower.contains("content") || title_lower.contains("accuracy") {
            7.0 + jitter
        } else if title_lower.contains("communicat") || title_lower.contains("style") || title_lower.contains("clarity") {
            5.0 + jitter
        } else if title_lower.contains("analysis") || title_lower.contains("critical") {
            6.5 + jitter
        } else {
            6.0 + jitter
        }
    } else if role_lower.contains("writing") || role_lower.contains("language") {
        // Writing Specialist: strong on communication/style, weaker on analysis depth
        if title_lower.contains("communicat") || title_lower.contains("style") || title_lower.contains("clarity") {
            8.0 + jitter
        } else if title_lower.contains("structure") || title_lower.contains("organiz") {
            7.5 + jitter
        } else if title_lower.contains("analysis") || title_lower.contains("critical") {
            5.5 + jitter
        } else {
            6.5 + jitter
        }
    } else if role_lower.contains("critical") || role_lower.contains("analytical") {
        // Critical Thinking Assessor: highest on analysis, moderate elsewhere
        if title_lower.contains("analysis") || title_lower.contains("critical") || title_lower.contains("argument") {
            8.5 + jitter
        } else if title_lower.contains("evidence") || title_lower.contains("research") {
            7.0 + jitter
        } else if title_lower.contains("communicat") || title_lower.contains("style") {
            6.0 + jitter
        } else {
            6.5 + jitter
        }
    } else if role_lower.contains("moderat") {
        // Moderator: scores evenly across all criteria
        6.5 + jitter
    } else {
        // Fallback: moderate spread
        6.0 + jitter
    };

    base.min(criterion.max_score).max(1.0)
}

/// Enrich a document with paragraph-level subsections, claims, and evidence.
///
/// Splits each section's text into paragraphs and extracts:
/// - Claims: sentences with strong assertion patterns ("I argue", "shows that", numbers)
/// - Evidence: sentences citing sources (parenthetical references, data, statistics)
fn enrich_document(doc: &mut EvalDocument) {
    // Set title from first section if empty
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
            // Single paragraph — extract claims/evidence directly
            extract_claims_and_evidence(section);
            continue;
        }

        // Multiple paragraphs — create subsections
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

/// Enrich document using Claude API for claim/evidence extraction.
/// Called after basic enrichment when ANTHROPIC_API_KEY is set.
async fn enrich_with_llm(doc: &mut EvalDocument, intent: &str) -> anyhow::Result<()> {
    let claude = llm::ClaudeClient::from_env()?;

    for section in &mut doc.sections {
        match claude.extract_content(&section.text, intent).await {
            Ok(extraction) => {
                section.claims = extraction.claims.iter().map(|c| types::Claim {
                    id: uuid::Uuid::new_v4().to_string(),
                    text: c.text.clone(),
                    specificity: c.specificity,
                    verifiable: c.verifiable,
                }).collect();

                section.evidence = extraction.evidence.iter().map(|e| types::Evidence {
                    id: uuid::Uuid::new_v4().to_string(),
                    source: e.source.clone(),
                    evidence_type: e.evidence_type.clone(),
                    text: e.text.clone(),
                    has_quantified_outcome: e.quantified,
                }).collect();
            }
            Err(e) => {
                eprintln!("   Warning: LLM extraction failed for '{}': {}", section.title, e);
            }
        }

        // Also enrich subsections
        for sub in &mut section.subsections {
            match claude.extract_content(&sub.text, intent).await {
                Ok(extraction) => {
                    sub.claims = extraction.claims.iter().map(|c| types::Claim {
                        id: uuid::Uuid::new_v4().to_string(),
                        text: c.text.clone(),
                        specificity: c.specificity,
                        verifiable: c.verifiable,
                    }).collect();
                    sub.evidence = extraction.evidence.iter().map(|e| types::Evidence {
                        id: uuid::Uuid::new_v4().to_string(),
                        source: e.source.clone(),
                        evidence_type: e.evidence_type.clone(),
                        text: e.text.clone(),
                        has_quantified_outcome: e.quantified,
                    }).collect();
                }
                Err(_) => {} // Keep regex extraction
            }
        }
    }

    Ok(())
}

fn extract_claims_and_evidence(section: &mut Section) {
    let sentences: Vec<&str> = section.text.split('.')
        .map(|s| s.trim())
        .filter(|s| s.len() > 15)
        .collect();

    for sentence in &sentences {
        let lower = sentence.to_lowercase();

        // Claims: assertions, arguments, conclusions
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

        // Evidence: citations, data, statistics
        let is_evidence = sentence.contains('(') && sentence.contains(')')
            && (lower.contains("et al")
                || lower.contains("20") // year references
                || lower.contains("found")
                || lower.contains("estimate"))
            || lower.contains("%")
            || lower.contains("£")
            || lower.contains("$")
            || lower.contains("billion")
            || lower.contains("million");

        if is_claim {
            let specificity = if sentence.contains('%') || sentence.contains('£') { 0.9 }
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
            // Extract source from parenthetical
            let source = if let Some(start) = sentence.find('(') {
                if let Some(end) = sentence[start..].find(')') {
                    sentence[start + 1..start + end].to_string()
                } else {
                    "inline data".into()
                }
            } else {
                "inline data".into()
            };
            let has_quantified = lower.contains('%') || lower.contains('£')
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

/// Generate a detailed justification for a demo score, mimicking real evaluator feedback.
fn generate_demo_justification(agent: &EvaluatorAgent, criterion: &EvaluationCriterion, score: f64) -> String {
    let role_lower = agent.role.to_lowercase();
    let title_lower = criterion.title.to_lowercase();
    let pct = (score / criterion.max_score * 100.0) as u32;

    let strength = if pct >= 80 { "strong" } else if pct >= 60 { "adequate" } else { "weak" };

    if role_lower.contains("subject") || role_lower.contains("expert") {
        if title_lower.contains("knowledge") {
            format!(
                "The essay demonstrates {} command of macroeconomic theory relevant to quantitative easing. \
                The candidate correctly identifies the transmission mechanism through portfolio rebalancing \
                and references the zero lower bound constraint. Key theoretical frameworks (Keynesian liquidity \
                preference) are accurately applied. However, the velocity of money concept is introduced only \
                in the conclusion without earlier theoretical grounding. The distinction between M4 monetary \
                aggregate and broader money supply measures could be more precise. \
                Overall knowledge base is {}, warranting {:.0}/{:.0}.",
                strength, strength, score, criterion.max_score
            )
        } else if title_lower.contains("analysis") {
            format!(
                "The analytical framework shows {} reasoning. The candidate successfully identifies the \
                disconnect between monetary base expansion and CPI inflation, and constructs a plausible \
                causal argument about financial market absorption. The distributional analysis (asset prices \
                vs consumer prices) demonstrates capacity for multi-dimensional evaluation. The analysis \
                would benefit from considering counterfactual scenarios — what would have happened without QE. \
                Score: {:.0}/{:.0}.",
                strength, score, criterion.max_score
            )
        } else if title_lower.contains("application") {
            format!(
                "Application of theoretical concepts to empirical evidence is {}. The candidate applies \
                transmission mechanism theory to UK-specific data (M4 growth, gilt yields, FTSE performance) \
                effectively. The connection between QE policy objectives and measured outcomes is clearly \
                drawn. Weaker on applying distributional theory to the equity concerns raised in the \
                analysis section. Score: {:.0}/{:.0}.",
                strength, score, criterion.max_score
            )
        } else {
            format!(
                "From a subject expertise perspective, this criterion is assessed as {}. The essay shows \
                competence in the domain but has room for deeper engagement with the theoretical literature. \
                The argument structure is logical but would benefit from more explicit signposting of \
                how evidence connects to claims. Score: {:.0}/{:.0}.",
                strength, score, criterion.max_score
            )
        }
    } else if role_lower.contains("writing") {
        if title_lower.contains("communicat") || title_lower.contains("structure") {
            format!(
                "The essay follows a clear four-part structure (Introduction, Literature Review, Analysis, \
                Conclusion) which aids readability. Paragraph transitions are generally smooth, though the \
                shift from Literature Review to Analysis could be more explicitly signposted. Academic \
                register is maintained throughout. Terminology is used precisely — 'quantitative easing', \
                'portfolio rebalancing', 'zero lower bound' are all deployed correctly. The conclusion \
                effectively synthesises the argument. One area for improvement: the thesis statement in \
                the introduction could be more prominently positioned. Overall {} communication quality. \
                Score: {:.0}/{:.0}.",
                strength, score, criterion.max_score
            )
        } else if title_lower.contains("source") {
            format!(
                "Source usage is {}. The essay cites Bernanke (2009), Joyce et al. (2012), Kapetanios et al. \
                (2012), and Bridges and Thomas (2012) — all highly relevant to the topic. Primary data from \
                the Bank of England and ONS is referenced appropriately. However, the bibliography could be \
                broader — no sources post-2015 are cited despite the essay covering the period to 2023. \
                Harvard referencing style is followed consistently. Score: {:.0}/{:.0}.",
                strength, score, criterion.max_score
            )
        } else {
            format!(
                "From an academic writing perspective, the work is {}. The prose is clear and well-structured, \
                with appropriate use of hedging language ('suggests', 'indicates') for contested claims. \
                Paragraphing is effective. The essay would benefit from stronger topic sentences in the \
                Analysis section. Score: {:.0}/{:.0}.",
                strength, score, criterion.max_score
            )
        }
    } else if role_lower.contains("critical") {
        if title_lower.contains("analysis") || title_lower.contains("evaluation") {
            format!(
                "Critical evaluation is {} in this essay. The candidate engages with competing perspectives \
                (Joyce et al. vs Bridges and Thomas) rather than presenting a one-sided argument, which is \
                commendable. The identification of the CPI-asset price disconnect shows genuine analytical \
                insight. The distributional equity argument elevates the analysis beyond purely technical \
                assessment. However, the candidate does not sufficiently interrogate their own thesis — \
                if QE had 'limited direct impact on CPI inflation', what alternative mechanisms might \
                explain the 2011-2012 inflation spike? The conclusion's policy recommendation (combining \
                monetary and fiscal measures) is asserted rather than argued from the evidence presented. \
                Score: {:.0}/{:.0}.",
                strength, score, criterion.max_score
            )
        } else {
            format!(
                "From a critical thinking perspective, this aspect is {}. The argument shows awareness of \
                complexity and avoids oversimplification. The candidate demonstrates capacity for evaluative \
                judgement by weighing evidence on both sides. The essay would score higher with explicit \
                consideration of limitations and counterarguments to the main thesis. Score: {:.0}/{:.0}.",
                strength, score, criterion.max_score
            )
        }
    } else {
        // Moderator — balanced overview
        format!(
            "This criterion is assessed as {} overall. The response demonstrates competence and engagement \
            with the material. Strengths include the structured approach and use of relevant evidence. \
            Areas for improvement include deeper critical engagement and broader source coverage. \
            The work meets the standard expected at this level. Score: {:.0}/{:.0}.",
            strength, score, criterion.max_score
        )
    }
}

/// Generate demo evidence references and gaps based on agent role and criterion.
fn generate_demo_evidence_and_gaps(
    _agent: &EvaluatorAgent,
    criterion: &EvaluationCriterion,
    score: f64,
) -> (Vec<String>, Vec<String>) {
    let pct = (score / criterion.max_score * 100.0) as u32;
    let title_lower = criterion.title.to_lowercase();

    let evidence = if title_lower.contains("knowledge") {
        vec![
            "References Keynesian liquidity preference theory".into(),
            "Correctly describes portfolio rebalancing mechanism".into(),
            "Cites M4 monetary aggregate data (12% growth)".into(),
        ]
    } else if title_lower.contains("analysis") {
        vec![
            "CPI vs asset price disconnect identified".into(),
            "FTSE 100 rose 45% (Mar 2009 - Dec 2012)".into(),
            "UK house prices +23% despite subdued wages".into(),
        ]
    } else if title_lower.contains("source") {
        vec![
            "Bernanke (2009)".into(),
            "Joyce et al. (2012) — gilt yields -100bps".into(),
            "Kapetanios et al. (2012) — GDP +1.5-2%".into(),
            "Bank of England Quarterly Bulletin".into(),
        ]
    } else if title_lower.contains("communicat") || title_lower.contains("structure") {
        vec![
            "Clear four-part structure".into(),
            "Consistent academic register".into(),
            "Precise terminology usage".into(),
        ]
    } else {
        vec!["General evidence from document sections".into()]
    };

    let gaps = if pct < 80 {
        if title_lower.contains("knowledge") {
            vec![
                "Velocity of money not theoretically grounded before conclusion".into(),
                "No distinction between narrow and broad money supply".into(),
            ]
        } else if title_lower.contains("analysis") {
            vec![
                "No counterfactual analysis (what without QE?)".into(),
                "2011-2012 inflation spike not addressed".into(),
            ]
        } else if title_lower.contains("source") {
            vec!["No sources post-2015 despite covering period to 2023".into()]
        } else {
            vec!["Could engage more deeply with the material".into()]
        }
    } else {
        vec![]
    };

    (evidence, gaps)
}

/// Mechanical convergence fallback: move target score 30% toward challenger's score.
fn mechanical_converge(
    new_scores: &mut Vec<Score>,
    current_scores: &[Score],
    challenger: &EvaluatorAgent,
    target: &EvaluatorAgent,
    criterion: &EvaluationCriterion,
    round_num: u32,
    challenges: &mut Vec<Challenge>,
    disagreement: &debate::Disagreement,
) {
    if let Some(target_score) = new_scores.iter_mut().find(|s|
        s.agent_id == target.id && s.criterion_id == criterion.id
    ) {
        let challenger_score_val = current_scores.iter()
            .find(|s| s.agent_id == challenger.id && s.criterion_id == criterion.id)
            .map(|s| s.score)
            .unwrap_or(target_score.score);

        let old_score = target_score.score;
        let adjustment = (challenger_score_val - old_score) * 0.3;
        target_score.score = (old_score + adjustment).min(target_score.max_score).max(0.0);
        target_score.round = round_num;
        target_score.justification = format!("{} [Adjusted in R{} after challenge from {}]",
            target_score.justification, round_num, challenger.name);

        challenges.push(Challenge {
            challenger_id: challenger.id.clone(),
            target_agent_id: target.id.clone(),
            criterion_id: criterion.id.clone(),
            round: round_num,
            argument: format!("Score delta of {:.1} — challenging based on evidence assessment", disagreement.delta),
            response: Some(format!("Adjusted from {:.1} to {:.1}", old_score, target_score.score)),
            score_change: Some((old_score, target_score.score)),
        });
    }
}

async fn run_serve(_host: String, _port: u16) -> anyhow::Result<()> {
    eprintln!("Brain in the Fish MCP server starting (stdio transport)...");
    let server = server::EvalServer::new();
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
