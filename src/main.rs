use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

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
    let (doc, _raw_sections) = ingest::ingest_pdf(&document, &intent)?;
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
            let score = Score {
                agent_id: agent_item.id.clone(),
                criterion_id: criterion.id.clone(),
                score: demo_score,
                max_score: criterion.max_score,
                round: 1,
                justification: format!("{} assessment of {}", agent_item.role, criterion.title),
                evidence_used: vec![],
                gaps_identified: vec![],
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

    Ok(())
}

/// Generate a deterministic demo score (placeholder until real subagent scoring).
fn generate_demo_score(agent: &EvaluatorAgent, criterion: &EvaluationCriterion) -> f64 {
    let hash = agent.name.len() + criterion.title.len();
    let base = (hash % 5) as f64 + 5.0;
    base.min(criterion.max_score)
}

async fn run_serve(host: String, port: u16) -> anyhow::Result<()> {
    println!("Brain in the Fish - MCP Server");
    println!("   Listening on {}:{}", host, port);
    // For now, just print and exit
    // The actual MCP server startup will be wired when server.rs is complete
    println!("   Server not yet fully implemented. Use 'evaluate' subcommand.");
    Ok(())
}
