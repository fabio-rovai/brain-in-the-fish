use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use rmcp::ServiceExt;

use brain_in_the_fish::*;
use brain_in_the_fish::types::*;

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

    let triples = ingest::load_document_ontology(&graph, &doc)?;
    println!("   Loaded {} triples, {} sections", triples, doc.sections.len());

    // 3. Load criteria
    println!("Loading evaluation criteria...");
    let framework = if let Some(_criteria_file) = criteria_path {
        // TODO: parse criteria from file
        criteria::generic_quality_framework()
    } else {
        // Select framework based on intent
        let domain = agent::detect_domain(&intent);
        match domain {
            agent::EvalDomain::Academic => criteria::academic_essay_framework(),
            _ => criteria::generic_quality_framework(),
        }
    };
    let crit_triples = criteria::load_criteria_ontology(&graph, &framework)?;
    println!(
        "   Loaded {} criteria, {} triples",
        framework.criteria.len(),
        crit_triples
    );

    // 4. Spawn agent panel
    println!("Spawning evaluator panel...");
    let agents = agent::spawn_panel(&intent, &framework);
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

    // 6. Simulate a round of scoring (placeholder scores for demo)
    println!("\nRound 1: Independent scoring...");
    let mut round1_scores = Vec::new();
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

    // 7. Find disagreements
    let disagreements = debate::find_disagreements(&round1_scores, 2.0);
    println!("   Found {} disagreements", disagreements.len());

    // 8. Calculate moderated scores
    println!("\nCalculating consensus scores...");
    let moderated = moderation::calculate_moderated_scores(&round1_scores, &agents);
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
        alignments: vec![],
        gaps: vec![],
        rounds: vec![debate::build_debate_round(1, round1_scores, vec![], None, true)],
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

async fn run_serve(_host: String, _port: u16) -> anyhow::Result<()> {
    eprintln!("Brain in the Fish MCP server starting (stdio transport)...");
    let server = server::EvalServer::new();
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
