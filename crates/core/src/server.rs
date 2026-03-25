//! MCP server with eval_* tools.

use std::sync::{Arc, Mutex};

use rmcp::{
    ServerHandler,
    tool, tool_handler, tool_router,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo, Tool},
};
use rmcp::schemars;
use open_ontologies::graph::GraphStore;

use crate::types::*;

// ============================================================================
// Input structs
// ============================================================================

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalIngestInput {
    /// Path to the PDF file to ingest.
    pub path: String,
    /// Evaluation intent (what to assess).
    pub intent: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalCriteriaInput {
    /// Built-in framework name: "generic", "academic", "policy", "clinical", "legal", or auto-detected from intent.
    pub framework: Option<String>,
    /// Free-text intent to auto-select framework.
    pub intent: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalSpawnInput {
    /// Evaluation intent — determines agent panel composition.
    pub intent: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalAlignInput {
    /// Optional: restrict alignment to a specific criterion ID.
    pub criterion_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalScorePromptInput {
    /// Index of the agent in the panel (0-based).
    pub agent_index: usize,
    /// Index of the criterion in the framework (0-based).
    pub criterion_index: usize,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalRecordScoreInput {
    /// Agent ID.
    pub agent_id: String,
    /// Criterion ID.
    pub criterion_id: String,
    /// Score awarded.
    pub score: f64,
    /// Maximum possible score.
    pub max_score: f64,
    /// Scoring round number.
    pub round: u32,
    /// Justification for the score.
    pub justification: String,
    /// Evidence references used.
    pub evidence_used: Vec<String>,
    /// Gaps identified.
    pub gaps_identified: Vec<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalChallengePromptInput {
    /// Index of the challenger agent in the panel (0-based).
    pub challenger_index: usize,
    /// Index of the target agent in the panel (0-based).
    pub target_index: usize,
    /// Criterion ID for the challenge.
    pub criterion_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalWhatIfInput {
    /// The section ID to modify.
    pub section_id: String,
    /// The new text content for the section.
    pub new_text: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalScoringTasksInput {
    /// Optional: restrict to a specific agent index.
    pub agent_index: Option<usize>,
    /// Optional: restrict to a specific criterion index.
    pub criterion_index: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalPredictInput {
    /// Mode: "prompt" returns the extraction prompt for subagent use, "extract" runs rule-based extraction directly.
    pub mode: Option<String>,
    /// Optional section index to restrict extraction to. If omitted, all sections are used.
    pub section_index: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalHistoryInput {
    /// Optional path to history directory. Defaults to ~/.brain-in-the-fish/history/
    pub dir: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdsFeedInput {
    /// Agent ID (from eval_spawn).
    pub agent_id: String,
    /// Criterion ID to feed evidence into.
    pub criterion_id: String,
    /// Structured evidence items extracted by the subagent.
    pub evidence: Vec<EdsFeedEvidence>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdsFeedEvidence {
    /// Source identifier for audit trail.
    pub source_id: String,
    /// Evidence type: "claim", "evidence", "quantified_data", "citation", "alignment".
    pub evidence_type: String,
    /// Strength/quality of this evidence (0.0-1.0).
    pub strength: f64,
    /// The evidence text (for audit trail).
    pub text: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdsScoreInput {
    /// Agent ID. Returns scores for this agent's SNN network.
    pub agent_id: String,
    /// Optional criterion ID. If omitted, returns scores for all criteria.
    pub criterion_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdsChallengeInput {
    /// Agent ID of the challenger.
    pub challenger_agent_id: String,
    /// Agent ID of the target whose score is being challenged.
    pub target_agent_id: String,
    /// Criterion being challenged.
    pub criterion_id: String,
    /// Counter-evidence supporting the challenge.
    pub counter_evidence: Vec<EdsFeedEvidence>,
}

// ============================================================================
// Session state
// ============================================================================

/// Mutable session state built up incrementally by tool calls.
#[derive(Debug, Clone)]
struct SessionState {
    document: Option<EvalDocument>,
    framework: Option<EvaluationFramework>,
    agents: Vec<EvaluatorAgent>,
    alignments: Vec<AlignmentMapping>,
    rounds: Vec<DebateRound>,
    current_round: u32,
    intent: String,
    snn_networks: Vec<crate::snn::AgentNetwork>,
    snn_config: crate::snn::SNNConfig,
}

impl SessionState {
    fn new() -> Self {
        Self {
            document: None,
            framework: None,
            agents: Vec::new(),
            alignments: Vec::new(),
            rounds: Vec::new(),
            current_round: 0,
            intent: String::new(),
            snn_networks: Vec::new(),
            snn_config: crate::snn::SNNConfig::default(),
        }
    }
}

// ============================================================================
// EvalServer
// ============================================================================

/// MCP server exposing eval_* tools for document evaluation.
#[derive(Clone)]
pub struct EvalServer {
    tool_router: ToolRouter<Self>,
    graph: Arc<GraphStore>,
    session: Arc<Mutex<SessionState>>,
}

impl Default for EvalServer {
    fn default() -> Self {
        Self::new()
    }
}

impl EvalServer {
    /// Create a new EvalServer with a fresh graph store.
    pub fn new() -> Self {
        Self::with_graph(Arc::new(GraphStore::new()))
    }

    /// Create a new EvalServer sharing an existing graph store.
    pub fn with_graph(graph: Arc<GraphStore>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            graph,
            session: Arc::new(Mutex::new(SessionState::new())),
        }
    }

    /// Return the list of all registered tool definitions.
    pub fn list_tool_definitions(&self) -> Vec<Tool> {
        self.tool_router.list_all()
    }
}

// ============================================================================
// Tool definitions
// ============================================================================

#[tool_router]
impl EvalServer {

    // ── Status ──────────────────────────────────────────────────────────────

    #[tool(name = "eval_status", description = "Returns server status, version, and current session state")]
    fn eval_status(&self) -> String {
        let session = self.session.lock().unwrap();
        let tool_count = self.tool_router.list_all().len();
        let triple_count = self.graph.triple_count();
        serde_json::json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
            "tools": tool_count,
            "triples_loaded": triple_count,
            "has_document": session.document.is_some(),
            "has_framework": session.framework.is_some(),
            "agent_count": session.agents.len(),
            "current_round": session.current_round,
            "rounds_completed": session.rounds.len(),
        })
        .to_string()
    }

    // ── Ingest ──────────────────────────────────────────────────────────────

    #[tool(name = "eval_ingest", description = "Ingest a PDF file and build the document ontology. Returns document summary.")]
    async fn eval_ingest(&self, Parameters(input): Parameters<EvalIngestInput>) -> String {
        let path = std::path::Path::new(&input.path);

        let (doc, raw_sections) = match crate::ingest::ingest_pdf(path, &input.intent) {
            Ok(result) => result,
            Err(e) => return format!(r#"{{"error":"{}"}}"#, e),
        };

        // Load document ontology into graph store
        match crate::ingest::load_document_ontology(&self.graph, &doc) {
            Ok(triples) => {
                let mut session = self.session.lock().unwrap();
                session.document = Some(doc.clone());
                session.intent = input.intent.clone();

                serde_json::json!({
                    "ok": true,
                    "document_id": doc.id,
                    "sections": raw_sections.len(),
                    "total_words": doc.total_word_count,
                    "triples_loaded": triples,
                    "intent": input.intent,
                })
                .to_string()
            }
            Err(e) => format!(r#"{{"error":"{}"}}"#, e),
        }
    }

    // ── Criteria ────────────────────────────────────────────────────────────

    #[tool(name = "eval_criteria", description = "Load a built-in evaluation framework by name (generic, academic, tender) or auto-select from intent.")]
    async fn eval_criteria(&self, Parameters(input): Parameters<EvalCriteriaInput>) -> String {
        let framework_name = input.framework
            .or_else(|| {
                input.intent.as_ref().map(|intent| {
                    let domain = crate::agent::detect_domain(intent);
                    match domain {
                        crate::agent::EvalDomain::Academic => "academic".to_string(),
                        crate::agent::EvalDomain::Tender => "tender".to_string(),
                        _ => "generic".to_string(),
                    }
                })
            })
            .unwrap_or_else(|| "generic".to_string());

        let framework = match framework_name.as_str() {
            "academic" => crate::criteria::academic_essay_framework(),
            _ => crate::criteria::generic_quality_framework(),
        };

        // Load criteria ontology into graph store
        match crate::criteria::load_criteria_ontology(&self.graph, &framework) {
            Ok(triples) => {
                let criteria_summary: Vec<serde_json::Value> = framework.criteria.iter().map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "title": c.title,
                        "max_score": c.max_score,
                        "weight": c.weight,
                    })
                }).collect();

                let mut session = self.session.lock().unwrap();
                session.framework = Some(framework.clone());

                serde_json::json!({
                    "ok": true,
                    "framework": framework.name,
                    "framework_id": framework.id,
                    "criteria_count": framework.criteria.len(),
                    "criteria": criteria_summary,
                    "triples_loaded": triples,
                    "pass_mark": framework.pass_mark,
                })
                .to_string()
            }
            Err(e) => format!(r#"{{"error":"{}"}}"#, e),
        }
    }

    // ── Align ───────────────────────────────────────────────────────────────

    #[tool(name = "eval_align", description = "Run ontology alignment between document sections and evaluation criteria. Returns mappings and gaps.")]
    async fn eval_align(&self, Parameters(_input): Parameters<EvalAlignInput>) -> String {
        let session = self.session.lock().unwrap();

        let doc = match &session.document {
            Some(d) => d.clone(),
            None => return r#"{"error":"No document ingested. Call eval_ingest first."}"#.to_string(),
        };
        let framework = match &session.framework {
            Some(f) => f.clone(),
            None => return r#"{"error":"No framework loaded. Call eval_criteria first."}"#.to_string(),
        };
        drop(session);

        // Simple keyword-based alignment: match section titles/content against criterion titles/descriptions
        let mut alignments = Vec::new();
        let mut gaps = Vec::new();

        for criterion in &framework.criteria {
            let crit_terms: Vec<String> = criterion.title.to_lowercase()
                .split_whitespace()
                .filter(|w| w.len() > 3)
                .map(|s| s.to_string())
                .collect();

            let mut best_confidence = 0.0f64;
            let mut best_section_id = None;

            for section in &doc.sections {
                let section_lower = format!("{} {}", section.title, section.text).to_lowercase();
                let matches: usize = crit_terms.iter()
                    .filter(|term| section_lower.contains(term.as_str()))
                    .count();

                let confidence = if crit_terms.is_empty() {
                    0.0
                } else {
                    matches as f64 / crit_terms.len() as f64
                };

                if confidence > best_confidence {
                    best_confidence = confidence;
                    best_section_id = Some(section.id.clone());
                }
            }

            if best_confidence >= 0.3 {
                if let Some(section_id) = best_section_id {
                    alignments.push(AlignmentMapping {
                        section_id,
                        criterion_id: criterion.id.clone(),
                        confidence: best_confidence,
                    });
                }
            } else {
                gaps.push(Gap {
                    criterion_id: criterion.id.clone(),
                    criterion_title: criterion.title.clone(),
                    best_partial_match: best_section_id.map(|sid| AlignmentMapping {
                        section_id: sid,
                        criterion_id: criterion.id.clone(),
                        confidence: best_confidence,
                    }),
                });
            }
        }

        let alignment_count = alignments.len();
        let gap_count = gaps.len();

        // Store alignments in session for later use by eval_scoring_tasks
        let mut session = self.session.lock().unwrap();
        session.alignments = alignments.clone();
        drop(session);

        serde_json::json!({
            "ok": true,
            "alignments": alignment_count,
            "gaps": gap_count,
            "alignment_details": alignments,
            "gap_details": gaps,
        })
        .to_string()
    }

    // ── Spawn ───────────────────────────────────────────────────────────────

    #[tool(name = "eval_spawn", description = "Generate an evaluator agent panel from the evaluation intent. Returns panel composition.")]
    async fn eval_spawn(&self, Parameters(input): Parameters<EvalSpawnInput>) -> String {
        let session = self.session.lock().unwrap();
        let framework = match &session.framework {
            Some(f) => f.clone(),
            None => return r#"{"error":"No framework loaded. Call eval_criteria first."}"#.to_string(),
        };
        drop(session);

        let agents = crate::agent::spawn_panel(&input.intent, &framework);

        // Load agent ontologies into graph store
        let mut total_triples = 0usize;
        for agent in &agents {
            match crate::agent::load_agent_ontology(&self.graph, agent) {
                Ok(t) => total_triples += t,
                Err(e) => return format!(r#"{{"error":"Failed to load agent {}: {}"}}"#, agent.name, e),
            }
        }

        let agent_summary: Vec<serde_json::Value> = agents.iter().enumerate().map(|(i, a)| {
            serde_json::json!({
                "index": i,
                "id": a.id,
                "name": a.name,
                "role": a.role,
                "domain": a.domain,
            })
        }).collect();

        let mut session = self.session.lock().unwrap();
        session.agents = agents;
        session.current_round = 1;

        // Create SNN networks for each agent
        let snn_networks: Vec<crate::snn::AgentNetwork> = session.agents.iter()
            .map(|a| crate::snn::AgentNetwork::new(a, &framework.criteria))
            .collect();
        session.snn_networks = snn_networks;

        serde_json::json!({
            "ok": true,
            "agent_count": session.agents.len(),
            "agents": agent_summary,
            "triples_loaded": total_triples,
            "snn_neurons_per_agent": framework.criteria.len(),
        })
        .to_string()
    }

    // ── Score Prompt ────────────────────────────────────────────────────────

    #[tool(name = "eval_score_prompt", description = "Generate a scoring prompt for a subagent to score a specific criterion. Returns the prompt text.")]
    async fn eval_score_prompt(&self, Parameters(input): Parameters<EvalScorePromptInput>) -> String {
        let session = self.session.lock().unwrap();

        if input.agent_index >= session.agents.len() {
            return format!(
                r#"{{"error":"agent_index {} out of range (panel has {} agents)"}}"#,
                input.agent_index, session.agents.len()
            );
        }

        let framework = match &session.framework {
            Some(f) => f.clone(),
            None => return r#"{"error":"No framework loaded. Call eval_criteria first."}"#.to_string(),
        };

        if input.criterion_index >= framework.criteria.len() {
            return format!(
                r#"{{"error":"criterion_index {} out of range (framework has {} criteria)"}}"#,
                input.criterion_index, framework.criteria.len()
            );
        }

        let agent = session.agents[input.agent_index].clone();
        let criterion = framework.criteria[input.criterion_index].clone();
        let round = session.current_round;
        drop(session);

        // Query document sections from graph
        let sections = crate::scoring::query_sections_for_criterion(&self.graph, &criterion.id)
            .unwrap_or_default();

        let prompt = crate::scoring::generate_scoring_prompt(&agent, &criterion, &sections, round);

        serde_json::json!({
            "ok": true,
            "agent_name": agent.name,
            "criterion_title": criterion.title,
            "round": round,
            "prompt": prompt,
        })
        .to_string()
    }

    // ── Record Score ────────────────────────────────────────────────────────

    #[tool(name = "eval_record_score", description = "Record a score from an agent for a criterion. Stores in the graph store.")]
    async fn eval_record_score(&self, Parameters(input): Parameters<EvalRecordScoreInput>) -> String {
        let evidence_refs = input.evidence_used.clone();
        let score = Score {
            agent_id: input.agent_id.clone(),
            criterion_id: input.criterion_id.clone(),
            score: input.score,
            max_score: input.max_score,
            round: input.round,
            justification: input.justification,
            evidence_used: input.evidence_used,
            gaps_identified: input.gaps_identified,
        };

        match crate::scoring::record_score(&self.graph, &score) {
            Ok(triples) => {
                // Auto-feed evidence into EDS if networks exist
                let eds_spikes_fed = {
                    let mut session = self.session.lock().unwrap();
                    let config = session.snn_config.clone();
                    if let Some(network) = session.snn_networks.iter_mut()
                        .find(|n| n.agent_id == input.agent_id)
                    {
                        if let Some(neuron) = network.neurons.iter_mut()
                            .find(|n| n.criterion_id == input.criterion_id)
                        {
                            for (i, ev_ref) in evidence_refs.iter().enumerate() {
                                if i > 0 && i as u32 % config.refractory_period == 0 {
                                    neuron.clear_refractory();
                                }
                                neuron.receive_spike(
                                    crate::snn::Spike {
                                        source_id: ev_ref.clone(),
                                        strength: 0.7,
                                        spike_type: crate::snn::SpikeType::Evidence,
                                        timestep: i as u32 % config.timesteps,
                                    },
                                    &config,
                                );
                            }
                            evidence_refs.len()
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                };

                serde_json::json!({
                    "ok": true,
                    "agent_id": input.agent_id,
                    "criterion_id": input.criterion_id,
                    "score": input.score,
                    "round": input.round,
                    "triples_inserted": triples,
                    "eds_spikes_fed": eds_spikes_fed,
                })
                .to_string()
            }
            Err(e) => format!(r#"{{"error":"{}"}}"#, e),
        }
    }

    // ── Debate Status ───────────────────────────────────────────────────────

    #[tool(name = "eval_debate_status", description = "Returns current disagreements, drift velocity, and convergence status for the active round.")]
    async fn eval_debate_status(&self) -> String {
        let session = self.session.lock().unwrap();
        let round = session.current_round;
        drop(session);

        // Get scores for the current round
        let current_scores = crate::scoring::get_scores_for_round(&self.graph, round)
            .unwrap_or_default();

        // Find disagreements (threshold 2.0 on a 10-point scale)
        let disagreements = crate::debate::find_disagreements(&current_scores, 2.0);

        // Calculate drift velocity if we have a previous round
        let drift_velocity = if round > 1 {
            let prev_scores = crate::scoring::get_scores_for_round(&self.graph, round - 1)
                .unwrap_or_default();
            Some(crate::debate::calculate_drift_velocity(&prev_scores, &current_scores))
        } else {
            None
        };

        let converged = drift_velocity
            .map(|dv| crate::debate::check_convergence(dv, 0.5))
            .unwrap_or(false);

        let disagreement_details: Vec<serde_json::Value> = disagreements.iter().map(|d| {
            serde_json::json!({
                "criterion_id": d.criterion_id,
                "agent_a": d.agent_a_id,
                "score_a": d.agent_a_score,
                "agent_b": d.agent_b_id,
                "score_b": d.agent_b_score,
                "delta": d.delta,
            })
        }).collect();

        serde_json::json!({
            "round": round,
            "scores_recorded": current_scores.len(),
            "disagreements": disagreement_details.len(),
            "disagreement_details": disagreement_details,
            "drift_velocity": drift_velocity,
            "converged": converged,
        })
        .to_string()
    }

    // ── Challenge Prompt ────────────────────────────────────────────────────

    #[tool(name = "eval_challenge_prompt", description = "Generate a challenge prompt for one agent to challenge another's score on a criterion.")]
    async fn eval_challenge_prompt(&self, Parameters(input): Parameters<EvalChallengePromptInput>) -> String {
        let session = self.session.lock().unwrap();

        let agents = &session.agents;
        if input.challenger_index >= agents.len() {
            return format!(
                r#"{{"error":"challenger_index {} out of range (panel has {} agents)"}}"#,
                input.challenger_index, agents.len()
            );
        }
        if input.target_index >= agents.len() {
            return format!(
                r#"{{"error":"target_index {} out of range (panel has {} agents)"}}"#,
                input.target_index, agents.len()
            );
        }

        let challenger = agents[input.challenger_index].clone();
        let target = agents[input.target_index].clone();

        let framework = match &session.framework {
            Some(f) => f.clone(),
            None => return r#"{"error":"No framework loaded. Call eval_criteria first."}"#.to_string(),
        };

        let round = session.current_round;
        drop(session);

        // Find the criterion
        let criterion = match framework.criteria.iter().find(|c| c.id == input.criterion_id) {
            Some(c) => c,
            None => return format!(r#"{{"error":"Criterion '{}' not found in framework"}}"#, input.criterion_id),
        };

        // Get scores for both agents on this criterion
        let round_scores = crate::scoring::get_scores_for_round(&self.graph, round)
            .unwrap_or_default();

        let challenger_score = round_scores.iter()
            .find(|s| s.agent_id == challenger.id && s.criterion_id == input.criterion_id);
        let target_score = round_scores.iter()
            .find(|s| s.agent_id == target.id && s.criterion_id == input.criterion_id);

        let (challenger_justification, target_justification, disagreement) = match (challenger_score, target_score) {
            (Some(cs), Some(ts)) => {
                let d = crate::debate::Disagreement {
                    criterion_id: input.criterion_id.clone(),
                    agent_a_id: challenger.id.clone(),
                    agent_a_score: cs.score,
                    agent_b_id: target.id.clone(),
                    agent_b_score: ts.score,
                    delta: (cs.score - ts.score).abs(),
                };
                (cs.justification.clone(), ts.justification.clone(), d)
            }
            _ => return r#"{"error":"Both agents must have scores recorded for this criterion and round. Call eval_record_score first."}"#.to_string(),
        };

        let prompt = crate::debate::generate_challenge_prompt(
            &challenger,
            &target,
            &disagreement,
            &challenger_justification,
            &target_justification,
            criterion,
        );

        serde_json::json!({
            "ok": true,
            "challenger": challenger.name,
            "target": target.name,
            "criterion": criterion.title,
            "round": round,
            "prompt": prompt,
        })
        .to_string()
    }

    // ── Scoring Tasks ───────────────────────────────────────────────────────

    #[tool(name = "eval_scoring_tasks", description = "Generate all scoring tasks for the agent panel. Returns one task per (agent, criterion) pair with the full prompt. An orchestrator can dispatch each task to a Claude subagent.")]
    async fn eval_scoring_tasks(&self, Parameters(input): Parameters<EvalScoringTasksInput>) -> String {
        let session = self.session.lock().unwrap();

        let doc = match &session.document {
            Some(d) => d.clone(),
            None => return r#"{"error":"No document ingested. Call eval_ingest first."}"#.to_string(),
        };
        let framework = match &session.framework {
            Some(f) => f.clone(),
            None => return r#"{"error":"No framework loaded. Call eval_criteria first."}"#.to_string(),
        };
        let agents = session.agents.clone();
        let alignments = session.alignments.clone();
        drop(session);

        if agents.is_empty() {
            return r#"{"error":"No agents spawned. Call eval_spawn first."}"#.to_string();
        }

        let mut tasks = crate::orchestrator::generate_scoring_tasks(
            &agents, &framework, &doc, &alignments,
        );

        // Apply optional filters
        if let Some(ai) = input.agent_index {
            tasks.retain(|t| t.agent_index == ai);
        }
        if let Some(ci) = input.criterion_index {
            tasks.retain(|t| t.criterion_index == ci);
        }

        // Generate subagent system prompts
        let agent_prompts: Vec<serde_json::Value> = agents.iter().enumerate().map(|(i, a)| {
            serde_json::json!({
                "agent_index": i,
                "agent_name": a.name.clone(),
                "system_prompt": crate::orchestrator::subagent_system_prompt(a),
            })
        }).collect();

        serde_json::json!({
            "ok": true,
            "task_count": tasks.len(),
            "agent_count": agents.len(),
            "criteria_count": framework.criteria.len(),
            "tasks": tasks,
            "agent_system_prompts": agent_prompts,
        })
        .to_string()
    }

    // ── What-If ─────────────────────────────────────────────────────────────

    #[tool(name = "eval_whatif", description = "Simulate a text change and estimate how it would affect scores. Uses LLM re-scoring when ANTHROPIC_API_KEY is set, otherwise falls back to text-metric heuristics.")]
    async fn eval_whatif(&self, Parameters(input): Parameters<EvalWhatIfInput>) -> String {
        let (doc, framework, agents, alignments, current_round) = {
            let session = self.session.lock().unwrap();

            let doc = match &session.document {
                Some(d) => d.clone(),
                None => return r#"{"error":"No document ingested. Call eval_ingest first."}"#.to_string(),
            };
            let framework = match &session.framework {
                Some(f) => f.clone(),
                None => return r#"{"error":"No framework loaded. Call eval_criteria first."}"#.to_string(),
            };
            let agents = session.agents.clone();
            let alignments = session.alignments.clone();
            let current_round = session.current_round;
            (doc, framework, agents, alignments, current_round)
        };

        // Build a lightweight EvaluationSession for the scoring functions
        let latest_scores = crate::scoring::get_scores_for_round(&self.graph, current_round)
            .unwrap_or_default();

        let moderated = if !latest_scores.is_empty() && !agents.is_empty() {
            crate::moderation::calculate_moderated_scores(&latest_scores, &agents)
        } else {
            Vec::new()
        };

        let eval_session = EvaluationSession {
            id: uuid::Uuid::new_v4().to_string(),
            document: doc,
            framework,
            agents,
            alignments: alignments.clone(),
            gaps: Vec::new(),
            rounds: Vec::new(),
            final_scores: moderated,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        // Try LLM re-scoring, fall back to heuristic
        let results = if crate::llm::ClaudeClient::available() && !eval_session.agents.is_empty() {
            match crate::scoring::what_if_rescore_llm(
                &eval_session,
                &input.section_id,
                &input.new_text,
                &alignments,
            )
            .await
            {
                Ok(r) => r,
                Err(_) => crate::scoring::what_if_rescore(
                    &eval_session,
                    &input.section_id,
                    &input.new_text,
                    &alignments,
                ),
            }
        } else {
            crate::scoring::what_if_rescore(
                &eval_session,
                &input.section_id,
                &input.new_text,
                &alignments,
            )
        };

        let used_llm = crate::llm::ClaudeClient::available() && !eval_session.agents.is_empty();

        serde_json::json!({
            "ok": true,
            "section_id": input.section_id,
            "method": if used_llm { "llm" } else { "heuristic" },
            "results": results,
        })
        .to_string()
    }

    // ── Report ──────────────────────────────────────────────────────────────

    #[tool(name = "eval_report", description = "Generate the full evaluation report. Runs moderation, calculates overall score, and returns Markdown.")]
    async fn eval_report(&self) -> String {
        let session = self.session.lock().unwrap();

        let doc = match &session.document {
            Some(d) => d.clone(),
            None => return r#"{"error":"No document ingested. Call eval_ingest first."}"#.to_string(),
        };
        let framework = match &session.framework {
            Some(f) => f.clone(),
            None => return r#"{"error":"No framework loaded. Call eval_criteria first."}"#.to_string(),
        };
        let agents = session.agents.clone();
        let rounds = session.rounds.clone();
        let current_round = session.current_round;
        drop(session);

        if agents.is_empty() {
            return r#"{"error":"No agents spawned. Call eval_spawn first."}"#.to_string();
        }

        // Get the latest round scores
        let latest_scores = crate::scoring::get_scores_for_round(&self.graph, current_round)
            .unwrap_or_default();

        if latest_scores.is_empty() {
            return r#"{"error":"No scores recorded. Call eval_record_score first."}"#.to_string();
        }

        // Run moderation
        let moderated = crate::moderation::calculate_moderated_scores(&latest_scores, &agents);
        let overall = crate::moderation::calculate_overall_score(&moderated, &framework);

        // Build a session for the report
        let eval_session = EvaluationSession {
            id: uuid::Uuid::new_v4().to_string(),
            document: doc,
            framework,
            agents,
            alignments: Vec::new(),
            gaps: Vec::new(),
            rounds,
            final_scores: moderated,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let report = crate::report::generate_report(&eval_session, &overall);
        let turtle = crate::report::session_to_turtle(&eval_session);

        serde_json::json!({
            "ok": true,
            "overall_score": overall.total_score,
            "max_possible": overall.max_possible,
            "percentage": overall.percentage,
            "passed": overall.passed,
            "report_markdown": report,
            "report_turtle": turtle,
        })
        .to_string()
    }

    // ── Predict ────────────────────────────────────────────────────────────

    #[tool(name = "eval_predict", description = "Extract and verify predictions from the ingested document. Mode 'prompt' returns an extraction prompt for subagent use. Mode 'extract' (default) runs rule-based extraction with SNN verification.")]
    async fn eval_predict(&self, Parameters(input): Parameters<EvalPredictInput>) -> String {
        let session = self.session.lock().unwrap();

        let doc = match &session.document {
            Some(d) => d.clone(),
            None => return r#"{"error":"No document ingested. Call eval_ingest first."}"#.to_string(),
        };
        drop(session);

        let mode = input.mode.unwrap_or_else(|| "extract".to_string());

        match mode.as_str() {
            "prompt" => {
                // Return the extraction prompt for subagent use
                let text = if let Some(idx) = input.section_index {
                    if idx < doc.sections.len() {
                        doc.sections[idx].text.clone()
                    } else {
                        return format!(r#"{{"error":"section_index {} out of range (document has {} sections)"}}"#,
                            idx, doc.sections.len());
                    }
                } else {
                    // Concatenate all section text
                    doc.sections.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("\n\n")
                };

                let prompt = crate::predict::extraction_prompt(&text);

                serde_json::json!({
                    "ok": true,
                    "mode": "prompt",
                    "prompt": prompt,
                    "section_count": doc.sections.len(),
                })
                .to_string()
            }
            _ => {
                // Run rule-based extraction with SNN verification
                let mut predictions = crate::predict::extract_predictions(&doc);
                crate::predict::assess_credibility(&mut predictions, &doc);

                // Run extraction for SNN verification
                let all_text: String = doc.sections.iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let extracted_items = crate::extract::extract_all(&all_text);

                // Verify predictions against extracted evidence
                let verifications = crate::predict::verify_all(&predictions, &doc, &extracted_items);

                // Generate report
                let report = crate::predict::prediction_report_with_verification(&predictions, &verifications);

                let pred_summary: Vec<serde_json::Value> = predictions.iter().zip(verifications.iter()).map(|(p, v)| {
                    serde_json::json!({
                        "text": crate::predict::extraction_prompt("").len().min(p.text.len()), // just use the text
                        "prediction_text": if p.text.len() > 100 { format!("{}...", &p.text[..100]) } else { p.text.clone() },
                        "type": format!("{:?}", p.prediction_type),
                        "credibility": p.credibility.score,
                        "verdict": format!("{:?}", p.credibility.verdict),
                        "snn_verified": v.verified,
                        "verification_score": v.verification_score,
                        "flag": v.flag,
                    })
                }).collect();

                serde_json::json!({
                    "ok": true,
                    "mode": "extract",
                    "prediction_count": predictions.len(),
                    "predictions": pred_summary,
                    "report_markdown": report,
                })
                .to_string()
            }
        }
    }

    // ── History ─────────────────────────────────────────────────────────────

    #[tool(name = "eval_history", description = "Returns cross-evaluation history report as JSON: score distribution, trends, weakest criteria, and common weaknesses across all past evaluations.")]
    async fn eval_history(&self, Parameters(input): Parameters<EvalHistoryInput>) -> String {
        let history_dir = if let Some(dir) = &input.dir {
            std::path::PathBuf::from(dir)
        } else {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".brain-in-the-fish")
                .join("history")
        };

        match crate::memory::cross_evaluation_summary(&history_dir) {
            Ok(summary) => serde_json::json!({
                "ok": true,
                "evaluations": summary.evaluations,
                "mean_score": (summary.mean_score * 10.0).round() / 10.0,
                "std_dev": (summary.std_dev * 10.0).round() / 10.0,
                "min_score": (summary.min_score * 10.0).round() / 10.0,
                "max_score": (summary.max_score * 10.0).round() / 10.0,
                "trend": summary.trend,
                "weakest_criteria": summary.weakest_criteria,
                "common_weaknesses": summary.common_weaknesses,
                "by_document_type": summary.by_document_type,
            })
            .to_string(),
            Err(e) => format!(r#"{{"error":"{}"}}"#, e),
        }
    }

    // ── EDS Feed ───────────────────────────────────────────────────────────

    #[tool(name = "eds_feed", description = "Push structured evidence into the SNN for a specific agent and criterion. Returns updated neuron state.")]
    async fn eds_feed(&self, Parameters(input): Parameters<EdsFeedInput>) -> String {
        let mut session = self.session.lock().unwrap();
        let config = session.snn_config.clone();

        let network = match session.snn_networks.iter_mut()
            .find(|n| n.agent_id == input.agent_id)
        {
            Some(n) => n,
            None => return format!(
                r#"{{"error":"No SNN network for agent_id '{}'. Call eval_spawn first."}}"#,
                input.agent_id
            ),
        };

        let neuron = match network.neurons.iter_mut()
            .find(|n| n.criterion_id == input.criterion_id)
        {
            Some(n) => n,
            None => return format!(
                r#"{{"error":"No neuron for criterion_id '{}'"}}"#,
                input.criterion_id
            ),
        };

        for (i, ev) in input.evidence.iter().enumerate() {
            let spike_type = match ev.evidence_type.as_str() {
                "quantified_data" => crate::snn::SpikeType::QuantifiedData,
                "evidence" => crate::snn::SpikeType::Evidence,
                "citation" => crate::snn::SpikeType::Citation,
                "alignment" => crate::snn::SpikeType::Alignment,
                _ => crate::snn::SpikeType::Claim,
            };

            if i > 0 && i as u32 % config.refractory_period == 0 {
                neuron.clear_refractory();
            }

            neuron.receive_spike(
                crate::snn::Spike {
                    source_id: ev.source_id.clone(),
                    strength: ev.strength.clamp(0.0, 1.0),
                    spike_type,
                    timestep: i as u32 % config.timesteps,
                },
                &config,
            );
        }

        serde_json::json!({
            "ok": true,
            "agent_id": input.agent_id,
            "criterion_id": input.criterion_id,
            "spikes_fed": input.evidence.len(),
            "membrane_potential": neuron.membrane_potential,
            "fire_count": neuron.fire_count,
            "total_spikes": neuron.total_spikes,
            "bayesian_confidence": neuron.bayesian_confidence,
            "refractory": neuron.refractory,
        })
        .to_string()
    }

    // ── EDS Score ─────────────────────────────────────────────────────────

    #[tool(name = "eds_score", description = "Get SNN scores for an agent. Returns score, confidence, firing rate, and low-confidence criteria.")]
    async fn eds_score(&self, Parameters(input): Parameters<EdsScoreInput>) -> String {
        let session = self.session.lock().unwrap();

        let network = match session.snn_networks.iter()
            .find(|n| n.agent_id == input.agent_id)
        {
            Some(n) => n,
            None => return format!(
                r#"{{"error":"No SNN network for agent_id '{}'"}}"#,
                input.agent_id
            ),
        };

        let framework = match &session.framework {
            Some(f) => f,
            None => return r#"{"error":"No framework loaded"}"#.to_string(),
        };

        let scores = network.compute_scores(&framework.criteria, &session.snn_config);

        let filtered: Vec<_> = if let Some(ref cid) = input.criterion_id {
            scores.into_iter().filter(|(id, _)| id == cid).collect()
        } else {
            scores
        };

        let score_details: Vec<serde_json::Value> = filtered.iter().map(|(cid, s)| {
            serde_json::json!({
                "criterion_id": cid,
                "snn_score": s.snn_score,
                "confidence": s.confidence,
                "bayesian_confidence": s.bayesian_confidence,
                "firing_rate": s.firing_rate,
                "evidence_count": s.evidence_count,
                "spike_quality": s.spike_quality,
                "grounded": s.grounded,
                "falsification_checked": s.falsification_checked,
                "confidence_interval": [s.confidence_interval.0, s.confidence_interval.1],
                "explanation": s.explanation,
            })
        }).collect();

        let low_confidence: Vec<String> = filtered.iter()
            .filter(|(_, s)| s.bayesian_confidence < 0.6 && s.grounded)
            .map(|(cid, _)| cid.clone())
            .collect();

        serde_json::json!({
            "ok": true,
            "agent_id": input.agent_id,
            "criteria_scored": score_details.len(),
            "scores": score_details,
            "low_confidence_criteria": low_confidence,
        })
        .to_string()
    }

    // ── EDS Challenge ─────────────────────────────────────────────────────

    #[tool(name = "eds_challenge", description = "Challenge another agent's SNN score on a criterion. Applies lateral inhibition and feeds counter-evidence.")]
    async fn eds_challenge(&self, Parameters(input): Parameters<EdsChallengeInput>) -> String {
        let mut session = self.session.lock().unwrap();
        let config = session.snn_config.clone();
        let framework = match &session.framework {
            Some(f) => f.clone(),
            None => return r#"{"error":"No framework loaded"}"#.to_string(),
        };

        let target_network = match session.snn_networks.iter_mut()
            .find(|n| n.agent_id == input.target_agent_id)
        {
            Some(n) => n,
            None => return format!(
                r#"{{"error":"No SNN network for target agent '{}'"}}"#,
                input.target_agent_id
            ),
        };

        let avg_strength = if input.counter_evidence.is_empty() {
            0.0
        } else {
            input.counter_evidence.iter().map(|e| e.strength).sum::<f64>()
                / input.counter_evidence.len() as f64
        };
        let inhibition_amount = avg_strength * config.inhibition_strength;

        target_network.inhibit(&input.criterion_id, inhibition_amount);

        let scores = target_network.compute_scores(&framework.criteria, &config);
        let updated = scores.iter()
            .find(|(cid, _)| *cid == input.criterion_id)
            .map(|(_, s)| serde_json::json!({
                "snn_score": s.snn_score,
                "confidence": s.confidence,
                "bayesian_confidence": s.bayesian_confidence,
            }));

        serde_json::json!({
            "ok": true,
            "challenger": input.challenger_agent_id,
            "target": input.target_agent_id,
            "criterion_id": input.criterion_id,
            "inhibition_applied": inhibition_amount,
            "counter_evidence_count": input.counter_evidence.len(),
            "updated_score": updated,
        })
        .to_string()
    }

    // ── EDS Consensus ────────────────────────────────────────────────────

    #[tool(name = "eds_consensus", description = "Check if agents' SNN scores have converged. Returns drift velocity and per-criterion variance.")]
    async fn eds_consensus(&self) -> String {
        let session = self.session.lock().unwrap();

        let framework = match &session.framework {
            Some(f) => f,
            None => return r#"{"error":"No framework loaded"}"#.to_string(),
        };

        if session.snn_networks.is_empty() {
            return r#"{"error":"No SNN networks. Call eval_spawn first."}"#.to_string();
        }

        let config = &session.snn_config;

        let mut criterion_stats: Vec<serde_json::Value> = Vec::new();
        let mut total_variance = 0.0;

        for criterion in &framework.criteria {
            let mut scores_for_criterion: Vec<f64> = Vec::new();

            for network in &session.snn_networks {
                let scores = network.compute_scores(&framework.criteria, config);
                if let Some((_, s)) = scores.iter().find(|(cid, _)| *cid == criterion.id) {
                    scores_for_criterion.push(s.snn_score);
                }
            }

            if scores_for_criterion.len() < 2 {
                continue;
            }

            let mean = scores_for_criterion.iter().sum::<f64>()
                / scores_for_criterion.len() as f64;
            let variance = scores_for_criterion.iter()
                .map(|s| (s - mean).powi(2))
                .sum::<f64>() / scores_for_criterion.len() as f64;

            total_variance += variance;

            criterion_stats.push(serde_json::json!({
                "criterion_id": criterion.id,
                "criterion_title": criterion.title,
                "mean_score": mean,
                "variance": variance,
                "agent_scores": scores_for_criterion,
                "converged": variance < 1.0,
            }));
        }

        let avg_variance = if criterion_stats.is_empty() {
            0.0
        } else {
            total_variance / criterion_stats.len() as f64
        };

        let all_converged = criterion_stats.iter()
            .all(|c| c["converged"].as_bool().unwrap_or(false));

        serde_json::json!({
            "ok": true,
            "agent_count": session.snn_networks.len(),
            "criteria_count": criterion_stats.len(),
            "average_variance": avg_variance,
            "converged": all_converged,
            "per_criterion": criterion_stats,
        })
        .to_string()
    }
}

// ============================================================================
// Test helpers — public wrappers around tool methods for integration testing
// ============================================================================

impl EvalServer {
    /// Wrapper for eval_status tool.
    pub fn test_status(&self) -> String {
        self.eval_status()
    }

    /// Wrapper for eval_ingest tool.
    pub async fn test_ingest(&self, path: &str, intent: &str) -> String {
        self.eval_ingest(Parameters(EvalIngestInput {
            path: path.to_string(),
            intent: intent.to_string(),
        }))
        .await
    }

    /// Wrapper for eval_criteria tool.
    pub async fn test_criteria(&self, framework: Option<&str>, intent: Option<&str>) -> String {
        self.eval_criteria(Parameters(EvalCriteriaInput {
            framework: framework.map(|s| s.to_string()),
            intent: intent.map(|s| s.to_string()),
        }))
        .await
    }

    /// Wrapper for eval_align tool.
    pub async fn test_align(&self) -> String {
        self.eval_align(Parameters(EvalAlignInput {
            criterion_id: None,
        }))
        .await
    }

    /// Wrapper for eval_spawn tool.
    pub async fn test_spawn(&self, intent: &str) -> String {
        self.eval_spawn(Parameters(EvalSpawnInput {
            intent: intent.to_string(),
        }))
        .await
    }

    /// Wrapper for eval_record_score tool.
    pub async fn test_record_score(
        &self,
        agent_id: &str,
        criterion_id: &str,
        score: f64,
        max_score: f64,
        round: u32,
        justification: &str,
    ) -> String {
        self.eval_record_score(Parameters(EvalRecordScoreInput {
            agent_id: agent_id.to_string(),
            criterion_id: criterion_id.to_string(),
            score,
            max_score,
            round,
            justification: justification.to_string(),
            evidence_used: vec!["test evidence".to_string()],
            gaps_identified: vec![],
        }))
        .await
    }

    /// Wrapper for eval_scoring_tasks tool.
    pub async fn test_scoring_tasks(&self) -> String {
        self.eval_scoring_tasks(Parameters(EvalScoringTasksInput {
            agent_index: None,
            criterion_index: None,
        }))
        .await
    }

    /// Wrapper for eval_debate_status tool.
    pub async fn test_debate_status(&self) -> String {
        self.eval_debate_status().await
    }

    /// Wrapper for eval_report tool.
    pub async fn test_report(&self) -> String {
        self.eval_report().await
    }
}

// ============================================================================
// ServerHandler
// ============================================================================

#[tool_handler]
impl ServerHandler for EvalServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Brain in the Fish: Universal document evaluation engine. \
                 Spawns a panel of AI agents that independently score, debate, \
                 and reach consensus on document quality."
            )
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_structs_derive_json_schema() {
        // Verify JsonSchema can be generated for all input types
        let _schema = schemars::schema_for!(EvalIngestInput);
        let _schema = schemars::schema_for!(EvalCriteriaInput);
        let _schema = schemars::schema_for!(EvalSpawnInput);
        let _schema = schemars::schema_for!(EvalAlignInput);
        let _schema = schemars::schema_for!(EvalScorePromptInput);
        let _schema = schemars::schema_for!(EvalRecordScoreInput);
        let _schema = schemars::schema_for!(EvalChallengePromptInput);
        let _schema = schemars::schema_for!(EvalScoringTasksInput);
        let _schema = schemars::schema_for!(EvalWhatIfInput);
        let _schema = schemars::schema_for!(EvalPredictInput);
    }

    #[test]
    fn test_server_construction() {
        let server = EvalServer::new();
        let tools = server.list_tool_definitions();
        assert!(tools.len() >= 10, "Should have at least 10 tools, got {}", tools.len());
    }

    #[test]
    fn test_server_with_shared_graph() {
        let graph = Arc::new(GraphStore::new());
        let server = EvalServer::with_graph(graph.clone());
        assert_eq!(graph.triple_count(), 0);
        let tools = server.list_tool_definitions();
        assert!(!tools.is_empty());
    }

    #[test]
    fn test_eval_status() {
        let server = EvalServer::new();
        let status = server.eval_status();
        let parsed: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["has_document"], false);
        assert_eq!(parsed["has_framework"], false);
        assert_eq!(parsed["agent_count"], 0);
    }

    #[test]
    fn test_tool_names() {
        let server = EvalServer::new();
        let tools = server.list_tool_definitions();
        let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(names.contains(&"eval_status".to_string()));
        assert!(names.contains(&"eval_ingest".to_string()));
        assert!(names.contains(&"eval_criteria".to_string()));
        assert!(names.contains(&"eval_align".to_string()));
        assert!(names.contains(&"eval_spawn".to_string()));
        assert!(names.contains(&"eval_score_prompt".to_string()));
        assert!(names.contains(&"eval_record_score".to_string()));
        assert!(names.contains(&"eval_debate_status".to_string()));
        assert!(names.contains(&"eval_challenge_prompt".to_string()));
        assert!(names.contains(&"eval_scoring_tasks".to_string()));
        assert!(names.contains(&"eval_whatif".to_string()));
        assert!(names.contains(&"eval_report".to_string()));
        assert!(names.contains(&"eval_predict".to_string()));
        assert!(names.contains(&"eds_feed".to_string()));
        assert!(names.contains(&"eds_score".to_string()));
        assert!(names.contains(&"eds_consensus".to_string()));
        assert!(names.contains(&"eds_challenge".to_string()));
    }

    #[test]
    fn test_session_has_snn_networks() {
        let state = SessionState::new();
        assert!(state.snn_networks.is_empty());
        assert_eq!(state.snn_config.timesteps, 10);
    }
}
