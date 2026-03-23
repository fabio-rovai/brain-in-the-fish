//! Agent cognitive model and spawning.
//!
//! Converts evaluator agents (with Maslow needs, attitudes, trust relationships)
//! into Turtle/RDF. Agent mental states are OWL, not just JSON.

use crate::ingest::{iri_safe, turtle_escape};
use crate::types::{EvaluationFramework, EvaluatorAgent, MaslowLevel, MaslowNeed, Score, TrustRelation};
use open_ontologies::graph::GraphStore;
use std::fmt::Write;
use uuid::Uuid;

/// Helper to convert MaslowLevel to its OWL class name.
fn maslow_class(level: &MaslowLevel) -> &'static str {
    match level {
        MaslowLevel::Physiological => "cog:PhysiologicalNeed",
        MaslowLevel::Safety => "cog:SafetyNeed",
        MaslowLevel::Belonging => "cog:BelongingNeed",
        MaslowLevel::Esteem => "cog:EsteemNeed",
        MaslowLevel::SelfActualisation => "cog:SelfActualisationNeed",
    }
}

/// Convert an EvaluatorAgent into Turtle RDF.
///
/// Uses prefixes:
///   @prefix agent: <http://brain-in-the-fish.dev/agent/> .
///   @prefix cog: <http://brain-in-the-fish.dev/cognition/> .
///   @prefix eval: <http://brain-in-the-fish.dev/eval/> .
pub fn agent_to_turtle(agent: &EvaluatorAgent) -> String {
    let mut out = String::new();

    // Prefixes
    let _ = writeln!(out, "@prefix agent: <http://brain-in-the-fish.dev/agent/> .");
    let _ = writeln!(out, "@prefix cog: <http://brain-in-the-fish.dev/cognition/> .");
    let _ = writeln!(out, "@prefix eval: <http://brain-in-the-fish.dev/eval/> .");
    let _ = writeln!(out, "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .");
    let _ = writeln!(out);

    let agent_id = iri_safe(&agent.id);

    // Agent identity
    let _ = writeln!(out, "agent:{agent_id} a eval:EvaluatorAgent ;");
    let _ = writeln!(out, "    eval:name \"{}\" ;", turtle_escape(&agent.name));
    let _ = writeln!(out, "    eval:role \"{}\" ;", turtle_escape(&agent.role));
    let _ = writeln!(out, "    eval:domain \"{}\" ;", turtle_escape(&agent.domain));
    let _ = writeln!(
        out,
        "    eval:yearsExperience \"{}\"^^xsd:integer ;",
        agent.years_experience.unwrap_or(0)
    );
    let _ = writeln!(
        out,
        "    eval:personaDescription \"{}\" .",
        turtle_escape(&agent.persona_description)
    );
    let _ = writeln!(out);

    // Maslow needs
    for (i, need) in agent.needs.iter().enumerate() {
        let need_id = format!("{}_Need_{}", agent_id, i);
        let class = maslow_class(&need.need_type);
        let _ = writeln!(out, "agent:{need_id} a {class} ;");
        let _ = writeln!(out, "    cog:agentRef agent:{agent_id} ;");
        let _ = writeln!(out, "    cog:expression \"{}\" ;", turtle_escape(&need.expression));
        let _ = writeln!(out, "    cog:salience \"{}\"^^xsd:decimal ;", need.salience);
        let _ = writeln!(
            out,
            "    cog:satisfied \"{}\"^^xsd:boolean .",
            need.satisfied
        );
        let _ = writeln!(out);
    }

    // Trust relationships
    for trust in &agent.trust_weights {
        let target_id = iri_safe(&trust.target_agent_id);
        let trust_node = format!("{}_Trust_{}", agent_id, target_id);
        let _ = writeln!(out, "agent:{trust_node} a cog:TrustRelation ;");
        let _ = writeln!(out, "    cog:truster agent:{agent_id} ;");
        let _ = writeln!(out, "    cog:trustee agent:{target_id} ;");
        let _ = writeln!(out, "    cog:trustDomain \"{}\" ;", turtle_escape(&trust.domain));
        let _ = writeln!(
            out,
            "    cog:trustLevel \"{}\"^^xsd:decimal .",
            trust.trust_level
        );
        let _ = writeln!(out);
    }

    out
}

/// Convert a Score into Turtle RDF triples.
///
/// Scores are linked to the agent and criterion.
pub fn score_to_turtle(score: &Score) -> String {
    let mut out = String::new();

    // Prefixes
    let _ = writeln!(out, "@prefix agent: <http://brain-in-the-fish.dev/agent/> .");
    let _ = writeln!(out, "@prefix crit: <http://brain-in-the-fish.dev/criteria/> .");
    let _ = writeln!(out, "@prefix eval: <http://brain-in-the-fish.dev/eval/> .");
    let _ = writeln!(out, "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .");
    let _ = writeln!(out);

    let agent_id = iri_safe(&score.agent_id);
    let criterion_id = iri_safe(&score.criterion_id);
    let score_node = format!("{}_Score_{}_R{}", agent_id, criterion_id, score.round);

    let _ = writeln!(out, "agent:{score_node} a eval:Score ;");
    let _ = writeln!(out, "    eval:agent agent:{agent_id} ;");
    let _ = writeln!(out, "    eval:criterion crit:{criterion_id} ;");
    let _ = writeln!(out, "    eval:score \"{}\"^^xsd:decimal ;", score.score);
    let _ = writeln!(out, "    eval:maxScore \"{}\"^^xsd:decimal ;", score.max_score);
    let _ = writeln!(out, "    eval:round \"{}\"^^xsd:integer ;", score.round);
    let _ = writeln!(
        out,
        "    eval:justification \"{}\" .",
        turtle_escape(&score.justification)
    );
    let _ = writeln!(out);

    out
}

/// Load an agent's ontology into the graph store.
pub fn load_agent_ontology(
    graph: &GraphStore,
    agent: &EvaluatorAgent,
) -> anyhow::Result<usize> {
    let turtle = agent_to_turtle(agent);
    let triples = graph.load_turtle(&turtle, None)?;
    Ok(triples)
}

/// Load a score into the graph store.
pub fn load_score(
    graph: &GraphStore,
    score: &Score,
) -> anyhow::Result<usize> {
    let turtle = score_to_turtle(score);
    let triples = graph.load_turtle(&turtle, None)?;
    Ok(triples)
}

// ============================================================================
// Agent panel spawning
// ============================================================================

/// Evaluation domain detected from intent keywords.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalDomain {
    Academic,
    Tender,
    Policy,
    Survey,
    Legal,
    Generic,
}

/// Detect the evaluation domain from an intent string using keyword matching.
pub fn detect_domain(intent: &str) -> EvalDomain {
    let lower = intent.to_lowercase();
    // Order matters: check more specific domains first
    if ["essay", "mark", "thesis", "coursework", "assignment", "grade", "dissertation"]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        EvalDomain::Academic
    } else if ["bid", "tender", "proposal", "itt", "procurement", "score"]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        EvalDomain::Tender
    } else if ["policy", "strategy", "impact", "assessment"]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        EvalDomain::Policy
    } else if ["survey", "research", "methodology", "questionnaire"]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        EvalDomain::Survey
    } else if ["contract", "legal", "compliance", "terms"]
        .iter()
        .any(|kw| lower.contains(kw))
    {
        EvalDomain::Legal
    } else {
        EvalDomain::Generic
    }
}

/// Generate an evaluator panel based on the evaluation intent and criteria.
/// Returns 3-5 agents appropriate for the domain, plus always a Moderator.
pub fn spawn_panel(intent: &str, _framework: &EvaluationFramework) -> Vec<EvaluatorAgent> {
    let domain = detect_domain(intent);
    let mut agents = match domain {
        EvalDomain::Academic => spawn_academic_agents(),
        EvalDomain::Tender => spawn_tender_agents(),
        EvalDomain::Policy => spawn_policy_agents(),
        EvalDomain::Survey => spawn_survey_agents(),
        EvalDomain::Legal => spawn_legal_agents(),
        EvalDomain::Generic => spawn_generic_agents(),
    };

    // Always add the moderator
    agents.push(make_moderator());

    // Wire up trust weights between all agents
    wire_trust_weights(&mut agents);

    agents
}

/// Build a single agent with the given archetype details.
fn make_agent(
    name: &str,
    role: &str,
    domain: &str,
    persona: &str,
    needs: Vec<MaslowNeed>,
) -> EvaluatorAgent {
    EvaluatorAgent {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        role: role.to_string(),
        domain: domain.to_string(),
        years_experience: Some(10),
        persona_description: persona.to_string(),
        needs,
        trust_weights: vec![], // wired up after all agents are created
    }
}

fn make_need(level: MaslowLevel, expression: &str, salience: f64) -> MaslowNeed {
    MaslowNeed {
        need_type: level,
        expression: expression.to_string(),
        salience,
        satisfied: false,
    }
}

fn make_moderator() -> EvaluatorAgent {
    make_agent(
        "Dr. Carla Mendez",
        "Panel Moderator",
        "Evaluation Moderation",
        "Experienced panel chair who synthesises diverse expert perspectives into \
         balanced, defensible consensus scores. Challenges outlier opinions with \
         evidence-based reasoning and ensures every criterion receives fair attention.",
        vec![
            make_need(
                MaslowLevel::SelfActualisation,
                "Achieving fair and balanced consensus",
                0.9,
            ),
            make_need(
                MaslowLevel::Safety,
                "Ensuring all criteria are addressed",
                0.85,
            ),
        ],
    )
}

fn spawn_academic_agents() -> Vec<EvaluatorAgent> {
    vec![
        make_agent(
            "Prof. Eleanor Harding",
            "Subject Expert",
            "Academic Subject Matter",
            "Professor with deep domain knowledge across multiple disciplines. \
             Focuses on accuracy of claims, depth of understanding, and appropriate \
             use of primary and secondary sources.",
            vec![
                make_need(MaslowLevel::Esteem, "Recognition of scholarly rigour", 0.8),
                make_need(MaslowLevel::SelfActualisation, "Advancing knowledge in the field", 0.7),
            ],
        ),
        make_agent(
            "Dr. Marcus Chen",
            "Academic Writing Specialist",
            "Academic Writing",
            "Expert in academic writing conventions, essay structure, and clear \
             argumentation. Evaluates logical flow, paragraph cohesion, and \
             whether the writing meets the expected academic register.",
            vec![
                make_need(MaslowLevel::Esteem, "Maintaining high writing standards", 0.8),
                make_need(MaslowLevel::Belonging, "Upholding academic community norms", 0.6),
            ],
        ),
        make_agent(
            "Dr. Amira Osei",
            "Critical Thinking Assessor",
            "Critical Analysis",
            "Specialist in evaluating higher-order thinking skills: analysis, \
             synthesis, and evaluation. Looks for original thought, logical \
             coherence, and well-supported argumentation beyond mere description.",
            vec![
                make_need(MaslowLevel::SelfActualisation, "Identifying genuine original insight", 0.85),
                make_need(MaslowLevel::Esteem, "Distinguishing surface from deep analysis", 0.7),
            ],
        ),
    ]
}

fn spawn_tender_agents() -> Vec<EvaluatorAgent> {
    vec![
        make_agent(
            "Sarah Mitchell",
            "Procurement Lead",
            "Procurement & Compliance",
            "Senior procurement professional who ensures submissions comply with \
             all mandatory requirements, documentation standards, and process \
             rules. Flags non-compliant bids before quality scoring begins.",
            vec![
                make_need(MaslowLevel::Safety, "Ensuring process compliance and audit trail", 0.9),
                make_need(MaslowLevel::Physiological, "All mandatory documents present and complete", 0.85),
            ],
        ),
        make_agent(
            "Dr. James Okonkwo",
            "Domain Expert",
            "Technical Delivery",
            "Technical specialist who assesses feasibility, methodology soundness, \
             and whether the proposed approach will actually deliver the required \
             outcomes. Looks for realistic timelines and proven methods.",
            vec![
                make_need(MaslowLevel::Esteem, "Recognising technically sound approaches", 0.8),
                make_need(MaslowLevel::Safety, "Identifying delivery risks early", 0.75),
            ],
        ),
        make_agent(
            "Priya Sharma",
            "Social Value Champion",
            "Social Value & Sustainability",
            "Specialist in evaluating community impact, TOMs measures, and \
             sustainability commitments. Ensures social value proposals are \
             measurable, deliverable, and genuinely additional.",
            vec![
                make_need(MaslowLevel::Belonging, "Ensuring community voices are represented", 0.85),
                make_need(MaslowLevel::SelfActualisation, "Driving meaningful social change through procurement", 0.8),
            ],
        ),
        make_agent(
            "Robert Tanaka",
            "Finance Assessor",
            "Financial Analysis",
            "Chartered accountant who scrutinises pricing models, cost breakdowns, \
             and value-for-money calculations. Identifies hidden costs, unsustainable \
             pricing, and commercial risk in financial submissions.",
            vec![
                make_need(MaslowLevel::Safety, "Protecting against financial risk and unsustainable bids", 0.85),
                make_need(MaslowLevel::Esteem, "Ensuring transparent and defensible pricing", 0.7),
            ],
        ),
    ]
}

fn spawn_policy_agents() -> Vec<EvaluatorAgent> {
    vec![
        make_agent(
            "Dr. Helena Voss",
            "Policy Analyst",
            "Policy Analysis",
            "Senior policy analyst who evaluates strategic coherence, evidence base, \
             and alignment with stated objectives. Examines whether policy proposals \
             are grounded in data and achievable within constraints.",
            vec![
                make_need(MaslowLevel::Esteem, "Producing rigorous, evidence-based analysis", 0.85),
                make_need(MaslowLevel::SelfActualisation, "Contributing to better public outcomes", 0.7),
            ],
        ),
        make_agent(
            "Maria Santos",
            "Stakeholder Representative",
            "Community & Stakeholder Engagement",
            "Advocate for affected communities and stakeholder groups. Ensures that \
             policy proposals consider diverse perspectives, address equity concerns, \
             and include meaningful engagement mechanisms.",
            vec![
                make_need(MaslowLevel::Belonging, "Ensuring affected groups have a voice", 0.9),
                make_need(MaslowLevel::Safety, "Protecting vulnerable populations from harm", 0.8),
            ],
        ),
        make_agent(
            "Tom Brennan",
            "Implementation Expert",
            "Programme Delivery",
            "Experienced programme manager who assesses delivery feasibility, \
             resource requirements, risk mitigation, and implementation timelines. \
             Focuses on whether proposals can move from paper to practice.",
            vec![
                make_need(MaslowLevel::Safety, "Identifying implementation risks before they materialise", 0.85),
                make_need(MaslowLevel::Esteem, "Ensuring realistic and deliverable plans", 0.7),
            ],
        ),
    ]
}

fn spawn_survey_agents() -> Vec<EvaluatorAgent> {
    vec![
        make_agent(
            "Prof. Yuki Nakamura",
            "Statistician",
            "Statistical Methods",
            "Expert in sampling design, statistical validity, and reliability analysis. \
             Evaluates whether the methodology will produce robust, generalisable \
             findings with appropriate confidence levels.",
            vec![
                make_need(MaslowLevel::Esteem, "Ensuring methodological soundness", 0.85),
                make_need(MaslowLevel::SelfActualisation, "Advancing best practice in research methods", 0.7),
            ],
        ),
        make_agent(
            "Dr. Fiona Adebayo",
            "Domain Expert",
            "Research Design",
            "Research design specialist who evaluates question construction, survey \
             flow, and relevance to research objectives. Identifies leading questions, \
             gaps in coverage, and response burden issues.",
            vec![
                make_need(MaslowLevel::Esteem, "Crafting questions that yield actionable data", 0.8),
                make_need(MaslowLevel::Belonging, "Representing respondent experience accurately", 0.7),
            ],
        ),
        make_agent(
            "Dr. Isaac Lowe",
            "Ethics Reviewer",
            "Research Ethics",
            "Research ethics specialist who evaluates consent procedures, data \
             protection, representation, and potential for bias or harm. Ensures \
             the research meets ethical standards and protects participants.",
            vec![
                make_need(MaslowLevel::Safety, "Protecting research participants from harm", 0.9),
                make_need(MaslowLevel::Belonging, "Ensuring fair representation of all groups", 0.8),
            ],
        ),
    ]
}

fn spawn_legal_agents() -> Vec<EvaluatorAgent> {
    vec![
        make_agent(
            "Catherine Park",
            "Legal Reviewer",
            "Legal Analysis",
            "Senior solicitor who analyses contractual clauses, identifies legal \
             risks, and assesses enforceability. Examines liability allocation, \
             indemnities, and dispute resolution mechanisms.",
            vec![
                make_need(MaslowLevel::Safety, "Identifying and mitigating legal risk", 0.9),
                make_need(MaslowLevel::Esteem, "Providing precise and defensible legal analysis", 0.8),
            ],
        ),
        make_agent(
            "David Okoye",
            "Compliance Officer",
            "Regulatory Compliance",
            "Regulatory compliance expert who checks alignment with applicable \
             laws, standards, and industry regulations. Ensures documents meet \
             all statutory and sector-specific requirements.",
            vec![
                make_need(MaslowLevel::Safety, "Ensuring full regulatory alignment", 0.9),
                make_need(MaslowLevel::Physiological, "All mandatory compliance elements present", 0.8),
            ],
        ),
        make_agent(
            "Laura Fleming",
            "Commercial Analyst",
            "Commercial Terms",
            "Commercial analyst who evaluates financial terms, liability caps, \
             payment structures, and overall commercial balance. Identifies \
             terms that create undue exposure or imbalance.",
            vec![
                make_need(MaslowLevel::Safety, "Protecting against commercial exposure", 0.85),
                make_need(MaslowLevel::Esteem, "Achieving fair and balanced commercial terms", 0.7),
            ],
        ),
    ]
}

fn spawn_generic_agents() -> Vec<EvaluatorAgent> {
    vec![
        make_agent(
            "Dr. Rachel Kim",
            "Quality Assessor",
            "Quality Evaluation",
            "Generalist quality assessor who evaluates overall clarity, completeness, \
             and fitness for purpose. Provides a holistic view of document quality \
             across structure, content, and presentation.",
            vec![
                make_need(MaslowLevel::Esteem, "Maintaining consistent quality standards", 0.8),
                make_need(MaslowLevel::Safety, "Ensuring completeness and accuracy", 0.75),
            ],
        ),
        make_agent(
            "Dr. Samuel Grant",
            "Evidence Reviewer",
            "Evidence Assessment",
            "Evidence specialist who evaluates specificity, data quality, and \
             citation practices. Distinguishes between assertion and evidence, \
             and checks that claims are properly supported.",
            vec![
                make_need(MaslowLevel::Esteem, "Upholding evidence-based standards", 0.85),
                make_need(MaslowLevel::SelfActualisation, "Distinguishing genuine evidence from rhetoric", 0.7),
            ],
        ),
        make_agent(
            "Prof. Nadia Petrova",
            "Domain Expert",
            "Subject Matter Expertise",
            "Broad subject matter expert who assesses accuracy of domain-specific \
             claims, appropriate use of terminology, and depth of subject knowledge \
             demonstrated in the document.",
            vec![
                make_need(MaslowLevel::Esteem, "Recognition of deep domain knowledge", 0.8),
                make_need(MaslowLevel::SelfActualisation, "Identifying insights that advance the field", 0.7),
            ],
        ),
    ]
}

/// Wire up trust weights between all agents in the panel.
/// Moderator gets 0.6 trust toward all others; non-moderators get 0.5 toward all others.
fn wire_trust_weights(agents: &mut [EvaluatorAgent]) {
    let ids_and_roles: Vec<(String, String)> = agents
        .iter()
        .map(|a| (a.id.clone(), a.role.clone()))
        .collect();

    for agent in agents.iter_mut() {
        let is_moderator = agent.role == "Panel Moderator";
        let trust_level = if is_moderator { 0.6 } else { 0.5 };

        agent.trust_weights = ids_and_roles
            .iter()
            .filter(|(id, _)| *id != agent.id)
            .map(|(id, role)| TrustRelation {
                target_agent_id: id.clone(),
                domain: role.clone(),
                trust_level,
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MaslowNeed, TrustRelation};

    fn make_test_agent() -> EvaluatorAgent {
        EvaluatorAgent {
            id: "agent-1".into(),
            name: "Dr. Torres".into(),
            role: "Subject Expert".into(),
            domain: "Economics".into(),
            years_experience: Some(15),
            persona_description: "Deep knowledge of macroeconomics".into(),
            needs: vec![MaslowNeed {
                need_type: MaslowLevel::Esteem,
                expression: "Recognition of thorough analysis".into(),
                salience: 0.8,
                satisfied: false,
            }],
            trust_weights: vec![TrustRelation {
                target_agent_id: "agent-2".into(),
                domain: "Academic writing".into(),
                trust_level: 0.7,
            }],
        }
    }

    fn make_test_score() -> Score {
        Score {
            agent_id: "agent-1".into(),
            criterion_id: "crit-knowledge".into(),
            score: 7.0,
            max_score: 8.0,
            round: 1,
            justification: "Strong primary sources".into(),
            evidence_used: vec!["ev-1".into()],
            gaps_identified: vec!["velocity of money".into()],
        }
    }

    #[test]
    fn test_agent_to_turtle() {
        let agent = make_test_agent();
        let turtle = agent_to_turtle(&agent);
        assert!(turtle.contains("eval:EvaluatorAgent"));
        assert!(turtle.contains("Dr. Torres"));
        assert!(turtle.contains("cog:EsteemNeed"));
        assert!(turtle.contains("cog:TrustRelation"));
        assert!(turtle.contains("0.7"));
    }

    #[test]
    fn test_agent_turtle_contains_all_fields() {
        let agent = make_test_agent();
        let turtle = agent_to_turtle(&agent);
        assert!(turtle.contains("eval:name \"Dr. Torres\""));
        assert!(turtle.contains("eval:role \"Subject Expert\""));
        assert!(turtle.contains("eval:domain \"Economics\""));
        assert!(turtle.contains("eval:yearsExperience \"15\"^^xsd:integer"));
        assert!(turtle.contains("eval:personaDescription \"Deep knowledge of macroeconomics\""));
        assert!(turtle.contains("cog:agentRef agent:agent_1"));
        assert!(turtle.contains("cog:expression \"Recognition of thorough analysis\""));
        assert!(turtle.contains("cog:salience \"0.8\"^^xsd:decimal"));
        assert!(turtle.contains("cog:satisfied \"false\"^^xsd:boolean"));
        assert!(turtle.contains("cog:truster agent:agent_1"));
        assert!(turtle.contains("cog:trustee agent:agent_2"));
        assert!(turtle.contains("cog:trustDomain \"Academic writing\""));
        assert!(turtle.contains("cog:trustLevel \"0.7\"^^xsd:decimal"));
    }

    #[test]
    fn test_score_to_turtle() {
        let score = make_test_score();
        let turtle = score_to_turtle(&score);
        assert!(turtle.contains("eval:Score"));
        assert!(turtle.contains("7"));
        assert!(turtle.contains("Strong primary sources"));
    }

    #[test]
    fn test_score_turtle_contains_all_fields() {
        let score = make_test_score();
        let turtle = score_to_turtle(&score);
        assert!(turtle.contains("eval:agent agent:agent_1"));
        assert!(turtle.contains("eval:criterion crit:crit_knowledge"));
        assert!(turtle.contains("eval:score \"7\"^^xsd:decimal"));
        assert!(turtle.contains("eval:maxScore \"8\"^^xsd:decimal"));
        assert!(turtle.contains("eval:round \"1\"^^xsd:integer"));
        assert!(turtle.contains("eval:justification \"Strong primary sources\""));
    }

    #[test]
    fn test_maslow_class() {
        assert_eq!(maslow_class(&MaslowLevel::Physiological), "cog:PhysiologicalNeed");
        assert_eq!(maslow_class(&MaslowLevel::Safety), "cog:SafetyNeed");
        assert_eq!(maslow_class(&MaslowLevel::Belonging), "cog:BelongingNeed");
        assert_eq!(maslow_class(&MaslowLevel::Esteem), "cog:EsteemNeed");
        assert_eq!(maslow_class(&MaslowLevel::SelfActualisation), "cog:SelfActualisationNeed");
    }

    #[test]
    fn test_load_agent_ontology() {
        let graph = GraphStore::new();
        let agent = make_test_agent();
        let triples = load_agent_ontology(&graph, &agent).unwrap();
        // Agent: 6 triples (a, name, role, domain, yearsExperience, personaDescription)
        // Need: 5 triples (a, agentRef, expression, salience, satisfied)
        // Trust: 5 triples (a, truster, trustee, trustDomain, trustLevel)
        // Total: 16
        assert!(triples > 0, "Should load triples, got {}", triples);
        assert_eq!(triples, 16, "Expected 16 triples, got {}", triples);
    }

    #[test]
    fn test_load_score() {
        let graph = GraphStore::new();
        let score = make_test_score();
        let triples = load_score(&graph, &score).unwrap();
        // Score: 7 triples (a, agent, criterion, score, maxScore, round, justification)
        assert!(triples > 0, "Should load triples, got {}", triples);
        assert_eq!(triples, 7, "Expected 7 triples, got {}", triples);
    }

    #[test]
    fn test_agent_turtle_escaping() {
        let agent = EvaluatorAgent {
            id: "agent-esc".into(),
            name: "Dr. \"Quotes\" O'Brien".into(),
            role: "Lead\nReviewer".into(),
            domain: "AI & ML".into(),
            years_experience: None,
            persona_description: "Handles \"edge cases\"\nwell".into(),
            needs: vec![],
            trust_weights: vec![],
        };
        let turtle = agent_to_turtle(&agent);
        assert!(turtle.contains("Dr. \\\"Quotes\\\" O'Brien"));
        assert!(turtle.contains("Lead\\nReviewer"));
        assert!(turtle.contains("Handles \\\"edge cases\\\"\\nwell"));
    }

    // ========================================================================
    // Agent spawner tests
    // ========================================================================

    #[test]
    fn test_spawn_academic_panel() {
        let framework = crate::criteria::academic_essay_framework();
        let panel = spawn_panel("mark this essay", &framework);
        assert!(panel.len() >= 4, "Academic panel should have >= 4 agents, got {}", panel.len());
        assert!(panel.iter().any(|a| a.role == "Panel Moderator"));
    }

    #[test]
    fn test_spawn_tender_panel() {
        let framework = crate::criteria::generic_quality_framework();
        let panel = spawn_panel("score this tender bid against ITT criteria", &framework);
        assert!(panel.len() >= 5, "Tender panel should have >= 5 agents, got {}", panel.len());
        assert!(panel.iter().any(|a| a.role == "Panel Moderator"));
    }

    #[test]
    fn test_detect_domain() {
        assert_eq!(detect_domain("mark this essay for A-level economics"), EvalDomain::Academic);
        assert_eq!(detect_domain("score this tender bid"), EvalDomain::Tender);
        assert_eq!(detect_domain("audit this policy document"), EvalDomain::Policy);
        assert_eq!(detect_domain("review this survey methodology"), EvalDomain::Survey);
        assert_eq!(detect_domain("check this contract for issues"), EvalDomain::Legal);
        assert_eq!(detect_domain("evaluate this random thing"), EvalDomain::Generic);
    }

    #[test]
    fn test_panel_has_trust_weights() {
        let framework = crate::criteria::generic_quality_framework();
        let panel = spawn_panel("evaluate this", &framework);
        for agent in &panel {
            if agent.role != "Panel Moderator" {
                assert!(!agent.trust_weights.is_empty(),
                    "Agent {} should have trust weights", agent.name);
            }
        }
    }

    #[test]
    fn test_panel_agents_have_needs() {
        let framework = crate::criteria::generic_quality_framework();
        let panel = spawn_panel("evaluate this", &framework);
        for agent in &panel {
            assert!(!agent.needs.is_empty(), "Agent {} should have needs", agent.name);
        }
    }

    #[test]
    fn test_moderator_trust_level() {
        let framework = crate::criteria::generic_quality_framework();
        let panel = spawn_panel("evaluate this", &framework);
        let moderator = panel.iter().find(|a| a.role == "Panel Moderator").unwrap();
        for tw in &moderator.trust_weights {
            assert!((tw.trust_level - 0.6).abs() < 1e-10,
                "Moderator trust should be 0.6, got {}", tw.trust_level);
        }
    }

    #[test]
    fn test_non_moderator_trust_level() {
        let framework = crate::criteria::generic_quality_framework();
        let panel = spawn_panel("evaluate this", &framework);
        for agent in panel.iter().filter(|a| a.role != "Panel Moderator") {
            for tw in &agent.trust_weights {
                assert!((tw.trust_level - 0.5).abs() < 1e-10,
                    "Non-moderator trust should be 0.5, got {}", tw.trust_level);
            }
        }
    }

    #[test]
    fn test_all_agents_have_unique_ids() {
        let framework = crate::criteria::generic_quality_framework();
        let panel = spawn_panel("mark this essay", &framework);
        let ids: Vec<&str> = panel.iter().map(|a| a.id.as_str()).collect();
        for (i, id) in ids.iter().enumerate() {
            for (j, other) in ids.iter().enumerate() {
                if i != j {
                    assert_ne!(id, other, "Agent IDs must be unique");
                }
            }
        }
    }
}
