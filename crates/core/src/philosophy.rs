//! Philosophical evaluation frameworks.
//!
//! Applies ethical and philosophical lenses to document evaluation:
//! - Kantian: universalizability, treating people as ends not means
//! - Utilitarian: greatest good for greatest number
//! - Virtue ethics: excellence, character, practical wisdom
//! - Care ethics: relationships, responsibility, context

use crate::types::*;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PhilosophicalAssessment {
    pub framework: PhilosophicalFramework,
    pub signals: Vec<PhilosophicalSignal>,
    pub overall_alignment: f64,  // 0.0-1.0
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum PhilosophicalFramework {
    Kantian,
    Utilitarian,
    VirtueEthics,
    CareEthics,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhilosophicalSignal {
    pub principle: String,
    pub finding: String,
    pub alignment: f64,  // -1.0 to 1.0
    pub evidence: String,
}

/// Apply Kantian analysis to the document.
/// Checks: universalizability, human dignity, duty-based reasoning.
pub fn kantian_analysis(doc: &EvalDocument) -> PhilosophicalAssessment {
    let mut signals = Vec::new();
    let text = full_text(doc).to_lowercase();

    // Categorical Imperative 1: Universalizability
    // "Would this be acceptable if everyone did it?"
    let universalizable_markers = ["all", "every", "universal", "standard", "always", "principle", "rule", "framework", "consistent"];
    let exception_markers = ["except", "unless", "special case", "exempt", "waiver", "override"];
    let universal_count = universalizable_markers.iter().filter(|m| text.contains(*m)).count();
    let exception_count = exception_markers.iter().filter(|m| text.contains(*m)).count();

    signals.push(PhilosophicalSignal {
        principle: "Categorical Imperative (Universalizability)".into(),
        finding: if universal_count > exception_count {
            "The document proposes standards that appear universally applicable.".into()
        } else {
            "The document contains exceptions or special cases that may undermine universal applicability.".into()
        },
        alignment: if universal_count > exception_count { 0.6 } else { -0.3 },
        evidence: format!("{} universalising markers, {} exception markers", universal_count, exception_count),
    });

    // Categorical Imperative 2: Human dignity
    // "Does this treat people as ends, not merely as means?"
    let dignity_markers = ["rights", "dignity", "consent", "welfare", "wellbeing", "well-being", "person", "citizen", "patient", "student", "individual", "autonomy", "respect"];
    let instrumentalising_markers = ["resource", "asset", "headcount", "fte", "human capital", "leverage", "utilise", "exploit"];
    let dignity_count = dignity_markers.iter().filter(|m| text.contains(*m)).count();
    let instrumental_count = instrumentalising_markers.iter().filter(|m| text.contains(*m)).count();

    signals.push(PhilosophicalSignal {
        principle: "Human Dignity (Persons as Ends)".into(),
        finding: if dignity_count > instrumental_count * 2 {
            "The document consistently references human welfare, rights, and dignity.".into()
        } else if instrumental_count > dignity_count {
            "The document may instrumentalise people — treating them as resources rather than ends in themselves.".into()
        } else {
            "Mixed: some references to human dignity alongside instrumental language.".into()
        },
        alignment: ((dignity_count as f64 - instrumental_count as f64) / (dignity_count + instrumental_count + 1) as f64).clamp(-1.0, 1.0),
        evidence: format!("{} dignity markers, {} instrumentalising markers", dignity_count, instrumental_count),
    });

    // Duty-based reasoning
    let duty_markers = ["must", "shall", "obligation", "duty", "required", "mandate", "responsible", "accountable"];
    let duty_count = duty_markers.iter().filter(|m| text.contains(*m)).count();

    signals.push(PhilosophicalSignal {
        principle: "Duty-Based Reasoning".into(),
        finding: format!("Document contains {} duty/obligation markers, indicating {} deontological grounding.",
            duty_count, if duty_count > 5 { "strong" } else if duty_count > 2 { "moderate" } else { "weak" }),
        alignment: (duty_count as f64 / 10.0).min(1.0),
        evidence: format!("{} duty markers found", duty_count),
    });

    let overall = signals.iter().map(|s| s.alignment).sum::<f64>() / signals.len().max(1) as f64;
    PhilosophicalAssessment { framework: PhilosophicalFramework::Kantian, signals, overall_alignment: overall.clamp(-1.0, 1.0) }
}

/// Apply utilitarian analysis to the document.
/// Checks: cost-benefit reasoning, outcome focus, stakeholder impact.
pub fn utilitarian_analysis(doc: &EvalDocument) -> PhilosophicalAssessment {
    let mut signals = Vec::new();
    let text = full_text(doc).to_lowercase();

    // Consequentialist reasoning
    let outcome_markers = ["outcome", "result", "impact", "effect", "benefit", "consequence", "improvement", "saving", "reduction"];
    let process_markers = ["process", "procedure", "methodology", "approach", "framework", "system"];
    let outcome_count = outcome_markers.iter().filter(|m| text.contains(*m)).count();
    let process_count = process_markers.iter().filter(|m| text.contains(*m)).count();

    signals.push(PhilosophicalSignal {
        principle: "Consequentialism (Outcome Focus)".into(),
        finding: format!("Document shows {} outcome-oriented language vs {} process-oriented. {}",
            outcome_count, process_count,
            if outcome_count > process_count { "Outcome-focused — aligned with utilitarian reasoning." }
            else { "Process-focused — may not sufficiently justify outcomes." }),
        alignment: ((outcome_count as f64 - process_count as f64 * 0.5) / (outcome_count + process_count + 1) as f64).clamp(-1.0, 1.0),
        evidence: format!("{} outcome markers, {} process markers", outcome_count, process_count),
    });

    // Greatest good for greatest number
    let breadth_markers = ["all", "everyone", "community", "public", "population", "society", "widespread", "inclusive"];
    let narrow_markers = ["specific", "targeted", "selected", "elite", "exclusive", "niche"];
    let breadth_count = breadth_markers.iter().filter(|m| text.contains(*m)).count();
    let narrow_count = narrow_markers.iter().filter(|m| text.contains(*m)).count();

    signals.push(PhilosophicalSignal {
        principle: "Greatest Good for Greatest Number".into(),
        finding: if breadth_count > narrow_count { "Document focuses on broad public benefit.".into() }
            else { "Document may serve narrow interests over wider public good.".into() },
        alignment: ((breadth_count as f64 - narrow_count as f64) / (breadth_count + narrow_count + 1) as f64).clamp(-1.0, 1.0),
        evidence: format!("{} breadth markers, {} narrow markers", breadth_count, narrow_count),
    });

    // Cost-benefit analysis presence
    let cba_markers = ["cost", "benefit", "value for money", "return on", "savings", "efficiency", "budget"];
    let cba_count = cba_markers.iter().filter(|m| text.contains(*m)).count();

    signals.push(PhilosophicalSignal {
        principle: "Cost-Benefit Reasoning".into(),
        finding: format!("{} cost-benefit markers. {}", cba_count,
            if cba_count >= 3 { "Strong utilitarian cost-benefit grounding." }
            else if cba_count >= 1 { "Some cost-benefit consideration." }
            else { "No explicit cost-benefit analysis — utilitarian justification is weak." }),
        alignment: (cba_count as f64 / 5.0).min(1.0),
        evidence: format!("{} CBA markers", cba_count),
    });

    let overall = signals.iter().map(|s| s.alignment).sum::<f64>() / signals.len().max(1) as f64;
    PhilosophicalAssessment { framework: PhilosophicalFramework::Utilitarian, signals, overall_alignment: overall.clamp(-1.0, 1.0) }
}

/// Apply virtue ethics analysis.
/// Checks: excellence, practical wisdom, character.
pub fn virtue_ethics_analysis(doc: &EvalDocument) -> PhilosophicalAssessment {
    let mut signals = Vec::new();
    let text = full_text(doc).to_lowercase();

    let excellence_markers = ["excellent", "outstanding", "best practice", "exemplary", "innovative", "rigorous", "thorough", "comprehensive"];
    let mediocrity_markers = ["adequate", "sufficient", "minimum", "basic", "acceptable", "satisfactory"];
    let exc_count = excellence_markers.iter().filter(|m| text.contains(*m)).count();
    let med_count = mediocrity_markers.iter().filter(|m| text.contains(*m)).count();

    signals.push(PhilosophicalSignal {
        principle: "Arete (Excellence)".into(),
        finding: format!("{} excellence markers vs {} mediocrity markers. {}",
            exc_count, med_count,
            if exc_count > med_count { "Document aspires to excellence." }
            else { "Document aims for adequacy rather than excellence." }),
        alignment: ((exc_count as f64 - med_count as f64) / (exc_count + med_count + 1) as f64).clamp(-1.0, 1.0),
        evidence: format!("{} excellence, {} mediocrity markers", exc_count, med_count),
    });

    let wisdom_markers = ["judgement", "judgment", "balanced", "nuanced", "considered", "deliberate", "proportionate", "context", "trade-off"];
    let wisdom_count = wisdom_markers.iter().filter(|m| text.contains(*m)).count();

    signals.push(PhilosophicalSignal {
        principle: "Phronesis (Practical Wisdom)".into(),
        finding: format!("{} practical wisdom markers. {}",
            wisdom_count,
            if wisdom_count >= 3 { "Document demonstrates nuanced, contextual reasoning." }
            else { "Document may lack nuanced deliberation." }),
        alignment: (wisdom_count as f64 / 5.0).min(1.0),
        evidence: format!("{} wisdom markers", wisdom_count),
    });

    let overall = signals.iter().map(|s| s.alignment).sum::<f64>() / signals.len().max(1) as f64;
    PhilosophicalAssessment { framework: PhilosophicalFramework::VirtueEthics, signals, overall_alignment: overall.clamp(-1.0, 1.0) }
}

/// Run all philosophical analyses appropriate for the document type.
pub fn analyse(doc: &EvalDocument) -> Vec<PhilosophicalAssessment> {
    let mut assessments = vec![
        kantian_analysis(doc),
        utilitarian_analysis(doc),
        virtue_ethics_analysis(doc),
    ];

    // Filter to most relevant based on doc type
    match doc.doc_type.as_str() {
        "policy" | "government" => {} // Keep all — policy needs all lenses
        "essay" => { assessments.retain(|a| a.framework != PhilosophicalFramework::Utilitarian || a.overall_alignment.abs() > 0.1); }
        "contract" | "legal" => { assessments.retain(|a| a.framework == PhilosophicalFramework::Kantian); }
        _ => {}
    }

    assessments
}

/// Convert philosophical assessments to Turtle.
pub fn assessments_to_turtle(assessments: &[PhilosophicalAssessment]) -> String {
    use crate::ingest::{iri_safe, turtle_escape};

    let mut t = String::from(
        "@prefix phil: <http://brain-in-the-fish.dev/philosophy/> .\n\
         @prefix eval: <http://brain-in-the-fish.dev/eval/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n"
    );

    for assessment in assessments {
        let fw_name = format!("{:?}", assessment.framework);
        let fw_id = iri_safe(&fw_name);
        t.push_str(&format!(
            "phil:{} a phil:PhilosophicalAssessment ;\n\
             \tphil:framework \"{}\" ;\n\
             \tphil:overallAlignment \"{}\"^^xsd:decimal .\n\n",
            fw_id, fw_name, assessment.overall_alignment,
        ));

        for (i, signal) in assessment.signals.iter().enumerate() {
            let sid = format!("{}_{}", fw_id, i);
            t.push_str(&format!(
                "phil:{} a phil:PhilosophicalSignal ;\n\
                 \tphil:assessment phil:{} ;\n\
                 \tphil:principle \"{}\" ;\n\
                 \tphil:finding \"{}\" ;\n\
                 \tphil:alignment \"{}\"^^xsd:decimal ;\n\
                 \tphil:evidence \"{}\" .\n\n",
                sid, fw_id,
                turtle_escape(&signal.principle),
                turtle_escape(&signal.finding),
                signal.alignment,
                turtle_escape(&signal.evidence),
            ));
        }
    }

    t
}

fn full_text(doc: &EvalDocument) -> String {
    let mut text = String::new();
    for section in &doc.sections {
        text.push_str(&section.text);
        text.push('\n');
        for sub in &section.subsections {
            text.push_str(&sub.text);
            text.push('\n');
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_doc() -> EvalDocument {
        EvalDocument {
            id: "p1".into(), title: "AI Policy".into(), doc_type: "policy".into(),
            total_pages: None, total_word_count: Some(1000),
            sections: vec![Section {
                id: "s1".into(), title: "Policy".into(),
                text: "This policy shall ensure all citizens have rights to dignity and welfare. The cost-benefit analysis shows savings. Mandatory impact assessments must be universal. Every individual has the right to a human review. This outcome-focused approach will benefit the entire community.".into(),
                word_count: 50, page_range: None, claims: vec![], evidence: vec![], subsections: vec![],
            }],
        }
    }

    #[test]
    fn test_kantian_analysis() {
        let doc = policy_doc();
        let result = kantian_analysis(&doc);
        assert!(!result.signals.is_empty());
        // Policy with "rights", "dignity", "universal" should score well
        assert!(result.overall_alignment > 0.0, "Policy with dignity language should align: {}", result.overall_alignment);
    }

    #[test]
    fn test_utilitarian_analysis() {
        let doc = policy_doc();
        let result = utilitarian_analysis(&doc);
        assert!(!result.signals.is_empty());
        // Policy with "cost-benefit", "savings", "outcome" should score well
        assert!(result.overall_alignment > 0.0, "Policy with CBA should align: {}", result.overall_alignment);
    }

    #[test]
    fn test_virtue_ethics_analysis() {
        let doc = policy_doc();
        let result = virtue_ethics_analysis(&doc);
        assert!(!result.signals.is_empty());
    }

    #[test]
    fn test_analyse_all() {
        let doc = policy_doc();
        let results = analyse(&doc);
        assert!(results.len() >= 2, "Policy should get multiple frameworks");
    }

    #[test]
    fn test_assessments_to_turtle() {
        let doc = policy_doc();
        let results = analyse(&doc);
        let turtle = assessments_to_turtle(&results);
        assert!(turtle.contains("phil:PhilosophicalAssessment"));
        assert!(turtle.contains("Kantian"));
    }
}
