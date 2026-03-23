//! Evaluation orchestration protocol.
//!
//! Defines the step-by-step protocol for running an evaluation.
//! This can be executed by:
//! - A Claude subagent calling MCP tools (eval_ingest, eval_score_prompt, etc.)
//! - The CLI directly via llm.rs
//! - The MCP server's eval_report tool (full auto mode)
//!
//! The protocol is the same regardless of execution path.

use crate::scoring;
use crate::types::*;

/// The orchestration protocol — each step returns what the next step needs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum EvalStep {
    /// Step 1: Document ingested, sections extracted
    Ingested {
        document_id: String,
        section_count: usize,
        triple_count: usize,
    },
    /// Step 2: Criteria loaded
    CriteriaLoaded {
        framework_name: String,
        criteria_count: usize,
    },
    /// Step 3: Guidelines discovered
    GuidelinesLoaded {
        guideline_count: usize,
        sectors: Vec<String>,
    },
    /// Step 4: Sections aligned to criteria
    Aligned {
        alignment_count: usize,
        gap_count: usize,
        gaps: Vec<String>,
    },
    /// Step 5: Agent panel spawned
    PanelSpawned {
        agent_count: usize,
        agent_names: Vec<String>,
    },
    /// Step 6: Ready for scoring — returns prompts for each agent/criterion pair
    ScoringReady {
        prompts: Vec<ScoringTask>,
    },
    /// Step 7: Scores recorded, SNN verified
    ScoresVerified {
        score_count: usize,
        hallucination_warnings: usize,
    },
    /// Step 8: Debate round complete
    DebateRoundComplete {
        round: u32,
        disagreements: usize,
        challenges: usize,
        drift_velocity: f64,
        converged: bool,
    },
    /// Step 9: Evaluation complete
    Complete {
        overall_score: f64,
        max_possible: f64,
        percentage: f64,
        passed: Option<bool>,
    },
}

/// A scoring task for a subagent to execute.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScoringTask {
    pub agent_index: usize,
    pub agent_name: String,
    pub criterion_index: usize,
    pub criterion_title: String,
    pub prompt: String,
}

/// Generate all scoring tasks for a panel.
///
/// Produces one `ScoringTask` per (agent, criterion) pair. Each task contains
/// the full scoring prompt with aligned document sections inlined — ready to
/// be dispatched to a Claude subagent or executed directly via LLM call.
pub fn generate_scoring_tasks(
    agents: &[EvaluatorAgent],
    framework: &EvaluationFramework,
    doc: &EvalDocument,
    alignments: &[AlignmentMapping],
) -> Vec<ScoringTask> {
    let mut tasks = Vec::new();

    for (ai, agent) in agents.iter().enumerate() {
        for (ci, criterion) in framework.criteria.iter().enumerate() {
            // Get aligned sections for this criterion
            let matched_sections: Vec<scoring::SectionMatch> =
                crate::alignment::sections_for_criterion(alignments, &criterion.id, doc)
                    .into_iter()
                    .map(|(s, _conf)| scoring::SectionMatch {
                        section_iri: s.id.clone(),
                        title: s.title.clone(),
                        text: s.text.clone(),
                        word_count: s.word_count,
                    })
                    .collect();

            let prompt =
                scoring::generate_scoring_prompt(agent, criterion, &matched_sections, 1);

            tasks.push(ScoringTask {
                agent_index: ai,
                agent_name: agent.name.clone(),
                criterion_index: ci,
                criterion_title: criterion.title.clone(),
                prompt,
            });
        }
    }

    tasks
}

/// Generate a subagent system prompt for an evaluator agent.
///
/// This is the system prompt you give to a Claude subagent that will call
/// MCP eval_* tools to score a document. It tells the subagent who it is,
/// what tools are available, and the protocol to follow.
pub fn subagent_system_prompt(agent: &EvaluatorAgent) -> String {
    format!(
        r#"You are {name}, a {role} with expertise in {domain}.

{persona}

You are part of an evaluation panel assessing a document. You have access to MCP tools:
- eval_score_prompt: Get your scoring prompt for a specific criterion
- eval_record_score: Submit your score
- eval_debate_status: Check if other agents disagree with your scores
- eval_challenge_prompt: Get a challenge prompt if you disagree with another agent

Your job:
1. For each criterion you're assigned, call eval_score_prompt to get the prompt
2. Read the document content in the prompt carefully
3. Score the document against the criterion
4. Call eval_record_score with your score, justification, evidence used, and gaps identified
5. After all scores are submitted, check eval_debate_status for disagreements
6. If challenged, defend or adjust your scores

Be rigorous. Reference specific rubric levels. Cite specific evidence from the document.
Do not hallucinate evidence that isn't in the provided text."#,
        name = agent.name,
        role = agent.role,
        domain = agent.domain,
        persona = agent.persona_description,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent;
    use crate::criteria;

    #[test]
    fn test_generate_scoring_tasks() {
        let framework = criteria::academic_essay_framework();
        let agents = agent::spawn_panel("mark this essay", &framework);
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(100),
            sections: vec![Section {
                id: "s1".into(),
                title: "Intro".into(),
                text: "Content".into(),
                word_count: 10,
                page_range: None,
                claims: vec![],
                evidence: vec![],
                subsections: vec![],
            }],
        };
        let alignments = vec![AlignmentMapping {
            section_id: "s1".into(),
            criterion_id: framework.criteria[0].id.clone(),
            confidence: 0.8,
        }];

        let tasks = generate_scoring_tasks(&agents, &framework, &doc, &alignments);
        assert_eq!(tasks.len(), agents.len() * framework.criteria.len());
        assert!(tasks[0].prompt.contains(&agents[0].name));
    }

    #[test]
    fn test_scoring_tasks_cover_all_pairs() {
        let framework = criteria::generic_quality_framework();
        let agents = agent::spawn_panel("evaluate this policy", &framework);
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Policy".into(),
            doc_type: "policy".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![
                Section {
                    id: "s1".into(),
                    title: "Executive Summary".into(),
                    text: "Summary content here".into(),
                    word_count: 50,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                },
                Section {
                    id: "s2".into(),
                    title: "Methodology".into(),
                    text: "Methods used".into(),
                    word_count: 100,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                },
            ],
        };
        let alignments = vec![];

        let tasks = generate_scoring_tasks(&agents, &framework, &doc, &alignments);
        // Every agent x criterion pair should produce a task
        assert_eq!(tasks.len(), agents.len() * framework.criteria.len());

        // Each agent_index should appear framework.criteria.len() times
        for ai in 0..agents.len() {
            let count = tasks.iter().filter(|t| t.agent_index == ai).count();
            assert_eq!(count, framework.criteria.len());
        }
    }

    #[test]
    fn test_subagent_system_prompt() {
        let agent = EvaluatorAgent {
            id: "a1".into(),
            name: "Dr. Test".into(),
            role: "Expert".into(),
            domain: "Economics".into(),
            years_experience: Some(10),
            persona_description: "Test persona".into(),
            needs: vec![],
            trust_weights: vec![],
        };
        let prompt = subagent_system_prompt(&agent);
        assert!(prompt.contains("Dr. Test"));
        assert!(prompt.contains("eval_score_prompt"));
        assert!(prompt.contains("eval_record_score"));
        assert!(prompt.contains("Do not hallucinate"));
        assert!(prompt.contains("Economics"));
    }

    #[test]
    fn test_eval_step_serialization() {
        let step = EvalStep::Complete {
            overall_score: 72.5,
            max_possible: 100.0,
            percentage: 72.5,
            passed: Some(true),
        };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("72.5"));
        let deserialized: EvalStep = serde_json::from_str(&json).unwrap();
        match deserialized {
            EvalStep::Complete { percentage, .. } => assert!((percentage - 72.5).abs() < 0.01),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_scoring_task_serialization() {
        let task = ScoringTask {
            agent_index: 0,
            agent_name: "Dr. Test".into(),
            criterion_index: 1,
            criterion_title: "Analysis".into(),
            prompt: "Score this".into(),
        };
        let json = serde_json::to_string(&task).unwrap();
        let back: ScoringTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back.agent_name, "Dr. Test");
        assert_eq!(back.criterion_index, 1);
    }
}
