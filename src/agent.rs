//! Agent cognitive model and spawning.
//!
//! Converts evaluator agents (with Maslow needs, attitudes, trust relationships)
//! into Turtle/RDF. Agent mental states are OWL, not just JSON.

use crate::ingest::{iri_safe, turtle_escape};
use crate::types::{EvaluatorAgent, MaslowLevel, Score};
use open_ontologies::graph::GraphStore;
use std::fmt::Write;

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
}
