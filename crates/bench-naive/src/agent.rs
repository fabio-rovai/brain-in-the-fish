//! Agent spawning and domain detection — plain functions, no trait dispatch.

use uuid::Uuid;

use crate::types::{Agent, Framework, MaslowLevel, MaslowNeed, TrustRelation};

/// Detect the evaluation domain from an intent string via keyword matching.
pub fn detect_domain(intent: &str) -> &'static str {
    let lower = intent.to_lowercase();
    let keywords: &[(&[&str], &str)] = &[
        (&["tender", "procurement", "bid", "itb", "itt", "rfp", "rfq"], "tender"),
        (&["academic", "essay", "thesis", "dissertation", "paper", "journal"], "academic"),
        (&["policy", "regulation", "legislation", "government", "public sector"], "policy"),
        (&["clinical", "medical", "patient", "health", "nhs", "trial"], "clinical"),
        (&["legal", "contract", "law", "statute", "compliance", "litigation"], "legal"),
        (&["survey", "questionnaire", "poll", "feedback", "response rate"], "survey"),
    ];
    for (kws, domain) in keywords {
        if kws.iter().any(|kw| lower.contains(kw)) {
            return domain;
        }
    }
    "generic"
}

/// Spawn a panel of agents appropriate for the detected domain.
pub fn spawn_panel(intent: &str, framework: &Framework) -> Vec<Agent> {
    let domain = detect_domain(intent);

    let panel_specs: Vec<(&str, &str, &str, MaslowLevel)> = match domain {
        "tender" => vec![
            ("Budget Expert", "evaluator", "finance", MaslowLevel::Safety),
            ("Technical Evaluator", "evaluator", "technical", MaslowLevel::Esteem),
            ("Delivery Specialist", "evaluator", "delivery", MaslowLevel::Physiological),
            ("Social Value Assessor", "evaluator", "social_value", MaslowLevel::Belonging),
            ("Moderator", "moderator", "general", MaslowLevel::SelfActualisation),
        ],
        "academic" => vec![
            ("Methodology Reviewer", "evaluator", "methodology", MaslowLevel::Esteem),
            ("Subject Expert", "evaluator", "subject", MaslowLevel::Esteem),
            ("Writing Quality Assessor", "evaluator", "writing", MaslowLevel::Belonging),
            ("Citation Checker", "evaluator", "references", MaslowLevel::Safety),
            ("Moderator", "moderator", "general", MaslowLevel::SelfActualisation),
        ],
        "policy" => vec![
            ("Policy Analyst", "evaluator", "policy", MaslowLevel::Safety),
            ("Impact Assessor", "evaluator", "impact", MaslowLevel::Esteem),
            ("Stakeholder Reviewer", "evaluator", "stakeholders", MaslowLevel::Belonging),
            ("Evidence Reviewer", "evaluator", "evidence", MaslowLevel::Physiological),
            ("Moderator", "moderator", "general", MaslowLevel::SelfActualisation),
        ],
        _ => vec![
            ("Domain Expert", "evaluator", "general", MaslowLevel::Esteem),
            ("Quality Assessor", "evaluator", "quality", MaslowLevel::Safety),
            ("Evidence Reviewer", "evaluator", "evidence", MaslowLevel::Physiological),
            ("Compliance Checker", "evaluator", "compliance", MaslowLevel::Belonging),
            ("Moderator", "moderator", "general", MaslowLevel::SelfActualisation),
        ],
    };

    let criteria_desc = framework
        .criteria
        .iter()
        .map(|c| c.title.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    panel_specs
        .into_iter()
        .map(|(name, role, spec_domain, primary_need)| Agent {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            role: role.to_string(),
            domain: spec_domain.to_string(),
            years_experience: Some(10),
            persona_description: format!(
                "{name} ({domain} domain) evaluating against: {criteria_desc}"
            ),
            needs: vec![MaslowNeed {
                need_type: primary_need,
                expression: format!("{name}'s primary evaluation concern"),
                salience: 0.8,
                satisfied: false,
            }],
            trust_weights: Vec::new(), // wired separately
        })
        .collect()
}

/// Wire bidirectional trust weights at 0.6 between all agents.
pub fn wire_trust_weights(agents: &mut [Agent]) {
    let ids: Vec<String> = agents.iter().map(|a| a.id.clone()).collect();
    for agent in agents.iter_mut() {
        agent.trust_weights = ids
            .iter()
            .filter(|id| *id != &agent.id)
            .map(|id| TrustRelation {
                target_agent_id: id.clone(),
                domain: agent.domain.clone(),
                trust_level: 0.6,
            })
            .collect();
    }
}

/// Serialize an agent to RDF Turtle.
pub fn agent_to_turtle(agent: &Agent) -> String {
    let mut ttl = String::new();
    ttl.push_str("@prefix eval: <http://brain-in-the-fish.dev/eval/> .\n");
    ttl.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n");

    let iri = format!("eval:{}", agent.id);
    ttl.push_str(&format!(
        "{iri} a eval:Agent ;\n    rdfs:label \"{}\" ;\n    eval:role \"{}\" ;\n    eval:domain \"{}\" .\n",
        agent.name, agent.role, agent.domain
    ));

    for tw in &agent.trust_weights {
        ttl.push_str(&format!(
            "\n{iri} eval:trusts [ eval:target eval:{} ; eval:level {:.2} ] .\n",
            tw.target_agent_id, tw.trust_level
        ));
    }

    ttl
}
