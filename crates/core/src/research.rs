//! Sector-specific guideline research and provenance tracking.
//!
//! When evaluating a document, this module discovers relevant standards,
//! guidelines, and frameworks from the sector. Each discovered resource
//! includes provenance metadata for the knowledge graph.

use serde::{Deserialize, Serialize};

/// A discovered guideline or standard relevant to the evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guideline {
    pub id: String,
    pub title: String,
    pub source: String,
    pub url: Option<String>,
    pub sector: String,
    pub relevance: f64,
    pub summary: String,
    pub key_criteria: Vec<String>,
    pub provenance: Provenance,
}

/// Where this guideline was discovered and when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub discovered_via: String,
    pub discovered_at: String,
    pub confidence: f64,
    pub reasoning: String,
}

/// Built-in sector guidelines database.
/// These are standards that are well-known and don't need web search.
pub fn built_in_guidelines(intent: &str) -> Vec<Guideline> {
    let lower = intent.to_lowercase();
    let now = chrono::Utc::now().to_rfc3339();
    let mut guidelines = Vec::new();

    if matches_education(&lower) {
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Ofsted Education Inspection Framework (EIF)".into(),
            source: "Ofsted".into(),
            url: Some("https://www.gov.uk/government/publications/education-inspection-framework".into()),
            sector: "education".into(),
            relevance: 0.9,
            summary: "Framework for inspecting schools and further education. Covers quality of education, behaviour and attitudes, personal development, leadership and management.".into(),
            key_criteria: vec![
                "Quality of education: intent, implementation, impact".into(),
                "Behaviour and attitudes: expectations, attendance, bullying".into(),
                "Personal development: character, SMSC, careers".into(),
                "Leadership and management: vision, CPD, governance".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.95,
                reasoning: "Intent matches education sector — Ofsted EIF is the primary quality framework for English education.".into(),
            },
        });
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "AQA Assessment Objectives".into(),
            source: "AQA Exam Board".into(),
            url: Some("https://www.aqa.org.uk/subjects".into()),
            sector: "education".into(),
            relevance: if lower.contains("a-level") || lower.contains("gcse") { 0.95 } else { 0.6 },
            summary: "Assessment objectives for GCSE and A-Level qualifications. AO1: Knowledge and understanding. AO2: Application. AO3: Analysis and evaluation. AO4: Communication.".into(),
            key_criteria: vec![
                "AO1: Demonstrate knowledge and understanding".into(),
                "AO2: Apply knowledge to contexts".into(),
                "AO3: Analyse and evaluate, make judgements".into(),
                "AO4: Communicate using specialist vocabulary".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.90,
                reasoning: "Assessment objectives are the standard marking framework for UK qualifications.".into(),
            },
        });
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Bloom's Taxonomy of Educational Objectives".into(),
            source: "Academic framework".into(),
            url: None,
            sector: "education".into(),
            relevance: 0.7,
            summary: "Hierarchical framework for classifying educational learning objectives. Remember → Understand → Apply → Analyse → Evaluate → Create.".into(),
            key_criteria: vec![
                "Lower order: Remember, Understand, Apply".into(),
                "Higher order: Analyse, Evaluate, Create".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.85,
                reasoning: "Bloom's taxonomy underpins assessment design across all education sectors.".into(),
            },
        });
    }

    if matches_healthcare(&lower) {
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "CQC Fundamental Standards".into(),
            source: "Care Quality Commission".into(),
            url: Some("https://www.cqc.org.uk/guidance-providers/regulations/fundamental-standards".into()),
            sector: "healthcare".into(),
            relevance: 0.9,
            summary: "Standards below which care must never fall. Covers person-centred care, dignity and respect, consent, safety, safeguarding, nutrition, premises and equipment, complaints, good governance, staffing, fit and proper persons, duty of candour, and display of ratings.".into(),
            key_criteria: vec![
                "Regulation 9: Person-centred care".into(),
                "Regulation 10: Dignity and respect".into(),
                "Regulation 12: Safe care and treatment".into(),
                "Regulation 13: Safeguarding".into(),
                "Regulation 17: Good governance".into(),
                "Regulation 20: Duty of candour".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.95,
                reasoning: "CQC standards are the legal minimum for all regulated health and social care providers in England.".into(),
            },
        });
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "NICE Quality Standards".into(),
            source: "National Institute for Health and Care Excellence".into(),
            url: Some("https://www.nice.org.uk/standards-and-indicators/quality-standards".into()),
            sector: "healthcare".into(),
            relevance: 0.8,
            summary: "Concise sets of prioritised statements designed to drive measurable improvements in healthcare quality. Cover clinical effectiveness, patient safety, and patient experience.".into(),
            key_criteria: vec![
                "Evidence-based clinical practice".into(),
                "Measurable quality improvement".into(),
                "Patient-centred outcome measures".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.85,
                reasoning: "NICE quality standards are the benchmark for clinical effectiveness in the NHS.".into(),
            },
        });
    }

    if matches_government(&lower) {
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "HM Treasury Green Book".into(),
            source: "HM Treasury".into(),
            url: Some("https://www.gov.uk/government/publications/the-green-book-appraisal-and-evaluation-in-central-government".into()),
            sector: "government".into(),
            relevance: 0.9,
            summary: "Central government guidance on appraisal and evaluation. The ROAMEF cycle: Rationale, Objectives, Appraisal, Monitoring, Evaluation, Feedback. Mandatory for all government spending proposals.".into(),
            key_criteria: vec![
                "ROAMEF cycle compliance".into(),
                "Options appraisal with counterfactual".into(),
                "Cost-benefit analysis (CBA)".into(),
                "Distributional analysis".into(),
                "Risk and uncertainty assessment".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.95,
                reasoning: "The Green Book is mandatory guidance for all UK government policy and spending decisions.".into(),
            },
        });
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Magenta Book: Guidance for Evaluation".into(),
            source: "HM Treasury".into(),
            url: Some("https://www.gov.uk/government/publications/the-magenta-book".into()),
            sector: "government".into(),
            relevance: 0.8,
            summary: "Guidance on what to consider when designing an evaluation. Covers theory-based evaluation, experimental and quasi-experimental methods, process evaluation, and value for money assessment.".into(),
            key_criteria: vec![
                "Theory of change".into(),
                "Counterfactual design".into(),
                "Mixed methods approach".into(),
                "Proportionate evaluation".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.85,
                reasoning: "The Magenta Book complements the Green Book with specific evaluation methodology guidance.".into(),
            },
        });
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Government Functional Standard GovS 013: Counter Fraud".into(),
            source: "Cabinet Office".into(),
            url: Some("https://www.gov.uk/government/publications/government-functional-standard-govs-013-counter-fraud".into()),
            sector: "government".into(),
            relevance: 0.5,
            summary: "Standards for preventing, detecting, and responding to fraud in government. Covers governance, risk assessment, prevention, detection, investigation, sanctions, and redress.".into(),
            key_criteria: vec![
                "Counter fraud governance".into(),
                "Fraud risk assessment".into(),
                "Prevention and detection controls".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.6,
                reasoning: "Relevant if the document involves government spending or procurement, less so for pure policy analysis.".into(),
            },
        });
    }

    if matches_legal(&lower) {
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "UK GDPR and Data Protection Act 2018".into(),
            source: "ICO / UK Government".into(),
            url: Some("https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/".into()),
            sector: "legal".into(),
            relevance: 0.8,
            summary: "Data protection requirements for any document involving personal data processing. Covers lawful basis, rights of data subjects, DPIAs, international transfers, and breach notification.".into(),
            key_criteria: vec![
                "Lawful basis for processing".into(),
                "Data minimisation and purpose limitation".into(),
                "Data subject rights".into(),
                "Data Protection Impact Assessment".into(),
                "International transfer safeguards".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.85,
                reasoning: "GDPR compliance is required for any document that involves personal data processing.".into(),
            },
        });
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Unfair Contract Terms Act 1977 / Consumer Rights Act 2015".into(),
            source: "UK Legislation".into(),
            url: Some("https://www.legislation.gov.uk/ukpga/2015/15/contents".into()),
            sector: "legal".into(),
            relevance: 0.7,
            summary: "Legislation governing fairness of contract terms. Key tests: transparency, prominence, fair balance, good faith. Relevant for reviewing standard terms and conditions.".into(),
            key_criteria: vec![
                "Transparency of terms".into(),
                "Fair balance between parties".into(),
                "No exclusion of statutory rights".into(),
                "Reasonable limitation clauses".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.75,
                reasoning: "Contract terms must comply with consumer protection legislation.".into(),
            },
        });
    }

    if matches_research(&lower) {
        guidelines.push(Guideline {
            id: uuid::Uuid::new_v4().to_string(),
            title: "ESRC Research Ethics Framework".into(),
            source: "Economic and Social Research Council".into(),
            url: Some("https://www.ukri.org/councils/esrc/guidance-for-applicants/research-ethics-guidance/".into()),
            sector: "research".into(),
            relevance: 0.85,
            summary: "Ethical framework for social science research. Covers informed consent, confidentiality, harm avoidance, independence, and transparency.".into(),
            key_criteria: vec![
                "Informed consent".into(),
                "Anonymisation and confidentiality".into(),
                "Avoidance of harm".into(),
                "Research independence".into(),
                "Data management plan".into(),
            ],
            provenance: Provenance {
                discovered_via: "built-in".into(),
                discovered_at: now.clone(),
                confidence: 0.85,
                reasoning: "ESRC framework is the standard for social science research ethics in the UK.".into(),
            },
        });
    }

    guidelines
}

fn matches_education(intent: &str) -> bool {
    intent.contains("essay")
        || intent.contains("mark")
        || intent.contains("grade")
        || intent.contains("coursework")
        || intent.contains("assignment")
        || intent.contains("a-level")
        || intent.contains("gcse")
        || intent.contains("education")
        || intent.contains("school")
        || intent.contains("ofsted")
        || intent.contains("teaching")
        || intent.contains("curriculum")
        || intent.contains("student")
}

fn matches_healthcare(intent: &str) -> bool {
    intent.contains("nhs")
        || intent.contains("clinical")
        || intent.contains("health")
        || intent.contains("patient")
        || intent.contains("care")
        || intent.contains("medical")
        || intent.contains("nursing")
        || intent.contains("hospital")
        || intent.contains("cqc")
}

fn matches_government(intent: &str) -> bool {
    intent.contains("policy")
        || intent.contains("government")
        || intent.contains("green book")
        || intent.contains("impact assessment")
        || intent.contains("public sector")
        || intent.contains("civil service")
        || intent.contains("whitehall")
        || intent.contains("legislation")
        || intent.contains("regulation")
        || intent.contains("strategy")
        || intent.contains("cabinet")
}

fn matches_legal(intent: &str) -> bool {
    intent.contains("contract")
        || intent.contains("legal")
        || intent.contains("law")
        || intent.contains("compliance")
        || intent.contains("gdpr")
        || intent.contains("terms")
        || intent.contains("agreement")
        || intent.contains("licence")
        || intent.contains("liability")
}

fn matches_research(intent: &str) -> bool {
    intent.contains("survey")
        || intent.contains("research")
        || intent.contains("methodology")
        || intent.contains("questionnaire")
        || intent.contains("study")
        || intent.contains("data collection")
        || intent.contains("sample")
}

/// Convert guidelines into Turtle for the knowledge graph.
/// Creates a provenance chain: Guideline -> discovered_via -> source.
pub fn guidelines_to_turtle(guidelines: &[Guideline]) -> String {
    use crate::ingest::{iri_safe, turtle_escape};

    let mut turtle = String::from(
        "@prefix guide: <http://brain-in-the-fish.dev/guideline/> .\n\
         @prefix eval: <http://brain-in-the-fish.dev/eval/> .\n\
         @prefix prov: <http://www.w3.org/ns/prov#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n",
    );

    for g in guidelines {
        let gid = iri_safe(&g.id);
        turtle.push_str(&format!(
            "guide:{gid} a eval:Guideline ;\n\
             \teval:title \"{}\" ;\n\
             \teval:source \"{}\" ;\n\
             \teval:sector \"{}\" ;\n\
             \teval:relevance \"{}\"^^xsd:decimal ;\n\
             \teval:summary \"{}\" ;\n\
             \tprov:wasDiscoveredBy \"{}\" ;\n\
             \tprov:generatedAtTime \"{}\"^^xsd:dateTime ;\n\
             \tprov:confidence \"{}\"^^xsd:decimal ;\n\
             \tprov:reasoning \"{}\" .\n\n",
            turtle_escape(&g.title),
            turtle_escape(&g.source),
            turtle_escape(&g.sector),
            g.relevance,
            turtle_escape(&g.summary),
            turtle_escape(&g.provenance.discovered_via),
            turtle_escape(&g.provenance.discovered_at),
            g.provenance.confidence,
            turtle_escape(&g.provenance.reasoning),
        ));

        for (i, kc) in g.key_criteria.iter().enumerate() {
            let kc_id = format!("{gid}_{i}");
            turtle.push_str(&format!(
                "guide:{kc_id} a eval:GuidelineCriterion ;\n\
                 \teval:text \"{}\" ;\n\
                 \teval:partOf guide:{gid} .\n\n",
                turtle_escape(kc),
            ));
        }
    }

    turtle
}

/// Load guidelines into the graph store.
pub fn load_guidelines(
    graph: &open_ontologies::graph::GraphStore,
    guidelines: &[Guideline],
) -> anyhow::Result<usize> {
    if guidelines.is_empty() {
        return Ok(0);
    }
    let turtle = guidelines_to_turtle(guidelines);
    graph.load_turtle(&turtle, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_education_guidelines() {
        let guides = built_in_guidelines("mark this essay for A-level economics");
        assert!(
            guides.len() >= 2,
            "Should find education guidelines, got {}",
            guides.len()
        );
        assert!(guides.iter().any(|g| g.title.contains("Ofsted")));
        assert!(guides.iter().any(|g| g.title.contains("AQA")));
    }

    #[test]
    fn test_healthcare_guidelines() {
        let guides = built_in_guidelines("assess this NHS clinical governance report");
        assert!(guides.iter().any(|g| g.title.contains("CQC")));
        assert!(guides.iter().any(|g| g.title.contains("NICE")));
    }

    #[test]
    fn test_government_guidelines() {
        let guides =
            built_in_guidelines("evaluate this government policy impact assessment");
        assert!(guides.iter().any(|g| g.title.contains("Green Book")));
    }

    #[test]
    fn test_legal_guidelines() {
        let guides = built_in_guidelines("review this contract for legal compliance");
        assert!(guides.iter().any(|g| g.title.contains("GDPR")));
    }

    #[test]
    fn test_research_guidelines() {
        let guides = built_in_guidelines("audit this survey research methodology");
        assert!(guides.iter().any(|g| g.title.contains("ESRC")));
    }

    #[test]
    fn test_provenance_present() {
        let guides = built_in_guidelines("mark this essay");
        for g in &guides {
            assert!(!g.provenance.discovered_via.is_empty());
            assert!(!g.provenance.reasoning.is_empty());
            assert!(g.provenance.confidence > 0.0);
        }
    }

    #[test]
    fn test_guidelines_to_turtle() {
        let guides = built_in_guidelines("mark this essay for A-level");
        let turtle = guidelines_to_turtle(&guides);
        assert!(turtle.contains("eval:Guideline"));
        assert!(turtle.contains("prov:wasDiscoveredBy"));
        assert!(turtle.contains("prov:reasoning"));
        assert!(turtle.contains("eval:GuidelineCriterion"));
    }

    #[test]
    fn test_load_guidelines() {
        let graph = open_ontologies::graph::GraphStore::new();
        let guides = built_in_guidelines("assess this NHS patient safety report");
        let triples = load_guidelines(&graph, &guides).unwrap();
        assert!(triples > 0, "Should load guideline triples");
    }

    #[test]
    fn test_multi_sector_intent() {
        let guides = built_in_guidelines(
            "evaluate this NHS policy document for clinical governance compliance",
        );
        assert!(guides.iter().any(|g| g.sector == "healthcare"));
        assert!(guides.iter().any(|g| g.sector == "government"));
    }
}
