//! Prediction credibility assessment.
//!
//! Extracts predictions, forecasts, targets, and commitments from documents
//! and assesses their credibility based on the evidence presented.
//!
//! This is NOT forecasting (MiroFish-style hallucination). It evaluates
//! whether predictions WITHIN the document are supported by evidence
//! WITHIN the document. Grounded, deterministic, auditable.

use crate::types::*;
use serde::Serialize;

/// A prediction or forecast found in the document.
#[derive(Debug, Clone, Serialize)]
pub struct Prediction {
    pub id: String,
    pub text: String,
    pub section_id: String,
    pub section_title: String,
    pub prediction_type: PredictionType,
    pub target_value: Option<String>,    // "50%", "£45M", "100%"
    pub timeframe: Option<String>,       // "24 months", "by March 2027"
    pub credibility: CredibilityAssessment,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum PredictionType {
    QuantitativeTarget,   // "reduce complaints by 50%"
    QualitativeGoal,      // "improve patient experience"
    Timeline,             // "within 18 months"
    CostEstimate,         // "£45M over 5 years"
    ComparisonClaim,      // "achieves 85% of the risk reduction"
    Commitment,           // "we will hire 2 apprentices"
}

/// How credible is this prediction based on document evidence?
#[derive(Debug, Clone, Serialize)]
pub struct CredibilityAssessment {
    pub score: f64,              // 0.0-1.0
    pub confidence: f64,         // 0.0-1.0 (how confident we are in OUR assessment)
    pub supporting_evidence: Vec<String>,
    pub risk_factors: Vec<String>,
    pub verdict: CredibilityVerdict,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum CredibilityVerdict {
    WellSupported,      // Multiple evidence items, quantified basis, realistic timeframe
    PartiallySupported, // Some evidence but gaps
    Aspirational,       // No direct evidence, plausible but unproven
    Unsupported,        // No evidence or contradicted by evidence
    OverClaimed,        // Claims exceed what evidence supports
}

/// Extract predictions from a document.
pub fn extract_predictions(doc: &EvalDocument) -> Vec<Prediction> {
    let mut predictions = Vec::new();

    for section in all_sections(&doc.sections) {
        let sentences = split_sentences(&section.text);

        for sentence in &sentences {
            let lower = sentence.to_lowercase();

            // Quantitative targets: "reduce X by Y%", "achieve X%", "save £Y"
            if let Some(pred) = detect_quantitative_target(sentence, &lower, section) {
                predictions.push(pred);
                continue;
            }

            // Cost estimates: "£Xm over Y years", "estimated cost of £X"
            if let Some(pred) = detect_cost_estimate(sentence, &lower, section) {
                predictions.push(pred);
                continue;
            }

            // Timeline commitments: "within X months", "by [date]"
            if let Some(pred) = detect_timeline(sentence, &lower, section) {
                predictions.push(pred);
                continue;
            }

            // Comparison claims: "achieves X% of Y", "compared to Z"
            if let Some(pred) = detect_comparison(sentence, &lower, section) {
                predictions.push(pred);
                continue;
            }

            // Qualitative goals: "will improve", "aims to enhance"
            if let Some(pred) = detect_qualitative_goal(sentence, &lower, section) {
                predictions.push(pred);
                continue;
            }

            // Commitments: "we will", "we commit to"
            if let Some(pred) = detect_commitment(sentence, &lower, section) {
                predictions.push(pred);
            }
        }
    }

    predictions
}

fn detect_quantitative_target(sentence: &str, lower: &str, section: &Section) -> Option<Prediction> {
    let target_patterns = [
        "reduce", "increase", "achieve", "improve by", "grow by",
        "decrease", "cut by", "raise to", "target of", "goal of",
    ];

    let has_target = target_patterns.iter().any(|p| lower.contains(p));
    let has_number = sentence.chars().any(|c| c.is_ascii_digit())
        && (lower.contains('%') || lower.contains('\u{00a3}') || lower.contains('$'));

    if has_target && has_number {
        let target_value = extract_number_with_unit(sentence);
        let timeframe = extract_timeframe(lower);

        Some(Prediction {
            id: uuid::Uuid::new_v4().to_string(),
            text: sentence.to_string(),
            section_id: section.id.clone(),
            section_title: section.title.clone(),
            prediction_type: PredictionType::QuantitativeTarget,
            target_value,
            timeframe,
            credibility: CredibilityAssessment::default(),
        })
    } else {
        None
    }
}

fn detect_cost_estimate(sentence: &str, lower: &str, section: &Section) -> Option<Prediction> {
    let cost_patterns = ["estimated", "cost of", "budget of", "investment of", "\u{00a3}", "$"];
    let has_cost = cost_patterns.iter().any(|p| lower.contains(p));
    let has_large_number = lower.contains("million") || lower.contains("billion")
        || lower.contains('\u{00a3}') || lower.contains('$');
    let has_timeframe = lower.contains("over") || lower.contains("per year") || lower.contains("annual");

    if has_cost && has_large_number && has_timeframe {
        Some(Prediction {
            id: uuid::Uuid::new_v4().to_string(),
            text: sentence.to_string(),
            section_id: section.id.clone(),
            section_title: section.title.clone(),
            prediction_type: PredictionType::CostEstimate,
            target_value: extract_number_with_unit(sentence),
            timeframe: extract_timeframe(lower),
            credibility: CredibilityAssessment::default(),
        })
    } else {
        None
    }
}

fn detect_timeline(sentence: &str, lower: &str, section: &Section) -> Option<Prediction> {
    let timeline_patterns = [
        "within", "by march", "by april", "by may", "by june", "by july",
        "by august", "by september", "by october", "by november", "by december",
        "by january", "by february", "by 20", "months", "phase 1", "phase 2", "phase 3",
    ];
    let has_timeline = timeline_patterns.iter().any(|p| lower.contains(p));
    let has_action = lower.contains("will") || lower.contains("shall") || lower.contains("plan to")
        || lower.contains("aim to") || lower.contains("deliver");

    if has_timeline && has_action && sentence.len() > 30 {
        Some(Prediction {
            id: uuid::Uuid::new_v4().to_string(),
            text: sentence.to_string(),
            section_id: section.id.clone(),
            section_title: section.title.clone(),
            prediction_type: PredictionType::Timeline,
            target_value: None,
            timeframe: extract_timeframe(lower),
            credibility: CredibilityAssessment::default(),
        })
    } else {
        None
    }
}

fn detect_comparison(sentence: &str, lower: &str, section: &Section) -> Option<Prediction> {
    let compare_patterns = [
        "achieves", "compared to", "better than", "worse than",
        "more effective", "less effective", "outperforms", "% of the",
    ];
    let has_compare = compare_patterns.iter().any(|p| lower.contains(p));
    let has_number = sentence.chars().any(|c| c.is_ascii_digit());

    if has_compare && has_number {
        Some(Prediction {
            id: uuid::Uuid::new_v4().to_string(),
            text: sentence.to_string(),
            section_id: section.id.clone(),
            section_title: section.title.clone(),
            prediction_type: PredictionType::ComparisonClaim,
            target_value: extract_number_with_unit(sentence),
            timeframe: None,
            credibility: CredibilityAssessment::default(),
        })
    } else {
        None
    }
}

fn detect_qualitative_goal(sentence: &str, lower: &str, section: &Section) -> Option<Prediction> {
    let goal_patterns = [
        "will improve", "will enhance", "aims to", "seeks to",
        "will ensure", "will establish", "will create", "will develop",
    ];
    let has_goal = goal_patterns.iter().any(|p| lower.contains(p));
    // Must NOT have a number (otherwise it would be caught by quantitative)
    let no_number = !sentence.chars().any(|c| c.is_ascii_digit());

    if has_goal && no_number && sentence.len() > 25 {
        Some(Prediction {
            id: uuid::Uuid::new_v4().to_string(),
            text: sentence.to_string(),
            section_id: section.id.clone(),
            section_title: section.title.clone(),
            prediction_type: PredictionType::QualitativeGoal,
            target_value: None,
            timeframe: extract_timeframe(lower),
            credibility: CredibilityAssessment::default(),
        })
    } else {
        None
    }
}

fn detect_commitment(sentence: &str, lower: &str, section: &Section) -> Option<Prediction> {
    let commit_patterns = ["we will", "we commit", "we pledge", "we guarantee", "we shall"];
    let has_commit = commit_patterns.iter().any(|p| lower.contains(p));

    if has_commit && sentence.len() > 20 {
        Some(Prediction {
            id: uuid::Uuid::new_v4().to_string(),
            text: sentence.to_string(),
            section_id: section.id.clone(),
            section_title: section.title.clone(),
            prediction_type: PredictionType::Commitment,
            target_value: None,
            timeframe: extract_timeframe(lower),
            credibility: CredibilityAssessment::default(),
        })
    } else {
        None
    }
}

/// Assess credibility of each prediction based on document evidence.
pub fn assess_credibility(predictions: &mut [Prediction], doc: &EvalDocument) {
    let all_evidence: Vec<&Evidence> = all_sections(&doc.sections)
        .iter()
        .flat_map(|s| s.evidence.iter())
        .collect();

    let all_claims: Vec<&Claim> = all_sections(&doc.sections)
        .iter()
        .flat_map(|s| s.claims.iter())
        .collect();

    for pred in predictions.iter_mut() {
        let mut supporting = Vec::new();
        let mut risks = Vec::new();
        let mut evidence_score = 0.0f64;

        let pred_lower = pred.text.to_lowercase();
        let pred_keywords = extract_keywords(&pred_lower);

        // Check supporting evidence
        for ev in &all_evidence {
            let ev_lower = ev.text.to_lowercase();
            let overlap = keyword_overlap(&pred_keywords, &extract_keywords(&ev_lower));
            if overlap > 0.2 {
                supporting.push(format!("Evidence: {} (relevance: {:.0}%)",
                    truncate(&ev.source, 50), overlap * 100.0));
                evidence_score += if ev.has_quantified_outcome { 0.3 } else { 0.15 };
            }
        }

        // Check supporting claims
        for claim in &all_claims {
            let claim_lower = claim.text.to_lowercase();
            let overlap = keyword_overlap(&pred_keywords, &extract_keywords(&claim_lower));
            if overlap > 0.3 {
                evidence_score += claim.specificity * 0.1;
            }
        }

        // Risk factors
        match pred.prediction_type {
            PredictionType::QuantitativeTarget => {
                if pred.timeframe.is_none() {
                    risks.push("No timeframe specified -- target is open-ended".into());
                    evidence_score *= 0.8;
                }
                if supporting.is_empty() {
                    risks.push("No evidence cited to support this target".into());
                    evidence_score *= 0.5;
                }
            }
            PredictionType::CostEstimate => {
                if supporting.is_empty() {
                    risks.push("Cost estimate not backed by evidence or breakdown".into());
                    evidence_score *= 0.5;
                }
            }
            PredictionType::ComparisonClaim => {
                if supporting.is_empty() {
                    risks.push("Comparison claim has no cited evidence base".into());
                    evidence_score *= 0.4;
                }
            }
            PredictionType::QualitativeGoal => {
                risks.push("Qualitative goal -- difficult to measure achievement".into());
                evidence_score *= 0.7;
            }
            PredictionType::Commitment => {
                if supporting.is_empty() {
                    risks.push("Commitment made without evidence of capacity to deliver".into());
                    evidence_score *= 0.6;
                }
            }
            PredictionType::Timeline => {
                if supporting.is_empty() {
                    risks.push("Timeline not supported by implementation evidence".into());
                    evidence_score *= 0.6;
                }
            }
        }

        let score = evidence_score.clamp(0.0, 1.0);
        let verdict = if score >= 0.7 { CredibilityVerdict::WellSupported }
            else if score >= 0.4 { CredibilityVerdict::PartiallySupported }
            else if score >= 0.15 { CredibilityVerdict::Aspirational }
            else { CredibilityVerdict::Unsupported };

        let confidence = if all_evidence.is_empty() { 0.3 }
            else { (supporting.len() as f64 / 3.0).min(1.0) * 0.5 + 0.3 };

        let explanation = format!(
            "{:?}: {} (credibility {:.0}%, confidence {:.0}%). {} supporting evidence items. {}",
            pred.prediction_type,
            match &verdict {
                CredibilityVerdict::WellSupported => "Well supported by document evidence",
                CredibilityVerdict::PartiallySupported => "Partially supported -- some evidence but gaps",
                CredibilityVerdict::Aspirational => "Aspirational -- plausible but limited evidence",
                CredibilityVerdict::Unsupported => "Unsupported -- no evidence base found",
                CredibilityVerdict::OverClaimed => "Over-claimed -- evidence doesn't support the scale",
            },
            score * 100.0, confidence * 100.0,
            supporting.len(),
            if risks.is_empty() { "No risk factors." } else { "Risk factors present." },
        );

        pred.credibility = CredibilityAssessment {
            score,
            confidence,
            supporting_evidence: supporting,
            risk_factors: risks,
            verdict,
            explanation,
        };
    }
}

/// Generate a prediction credibility report section.
pub fn prediction_report(predictions: &[Prediction]) -> String {
    if predictions.is_empty() {
        return "## Prediction Credibility\n\nNo predictions, forecasts, or targets found in the document.\n\n".into();
    }

    let mut r = String::from("## Prediction Credibility\n\n");
    r.push_str(&format!("{} predictions/targets extracted from the document.\n\n", predictions.len()));

    r.push_str("| Prediction | Type | Credibility | Verdict |\n|---|---|---|---|\n");
    for pred in predictions {
        let text = truncate(&pred.text, 60);
        r.push_str(&format!("| {} | {:?} | {:.0}% | {:?} |\n",
            text, pred.prediction_type, pred.credibility.score * 100.0, pred.credibility.verdict));
    }

    r.push_str("\n### Details\n\n");
    for (i, pred) in predictions.iter().enumerate() {
        r.push_str(&format!("**{}. {}**\n\n", i + 1, truncate(&pred.text, 80)));
        if let Some(tv) = &pred.target_value {
            r.push_str(&format!("- Target: {}\n", tv));
        }
        if let Some(tf) = &pred.timeframe {
            r.push_str(&format!("- Timeframe: {}\n", tf));
        }
        r.push_str(&format!("- Credibility: {:.0}% ({:?})\n", pred.credibility.score * 100.0, pred.credibility.verdict));
        if !pred.credibility.supporting_evidence.is_empty() {
            r.push_str("- Supporting evidence:\n");
            for ev in &pred.credibility.supporting_evidence {
                r.push_str(&format!("  - {}\n", ev));
            }
        }
        if !pred.credibility.risk_factors.is_empty() {
            r.push_str("- Risk factors:\n");
            for risk in &pred.credibility.risk_factors {
                r.push_str(&format!("  - {}\n", risk));
            }
        }
        r.push('\n');
    }

    r
}

/// Convert predictions to Turtle for the knowledge graph.
pub fn predictions_to_turtle(predictions: &[Prediction]) -> String {
    use crate::ingest::{iri_safe, turtle_escape};
    let mut t = String::from(
        "@prefix pred: <http://brain-in-the-fish.dev/prediction/> .\n\
         @prefix eval: <http://brain-in-the-fish.dev/eval/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n"
    );
    for p in predictions {
        let pid = iri_safe(&p.id);
        let pred_type = format!("{:?}", p.prediction_type);
        let verdict = format!("{:?}", p.credibility.verdict);
        t.push_str(&format!(
            "pred:{} a eval:Prediction ;\n\
             \teval:text \"{}\" ;\n\
             \teval:predictionType \"{}\" ;\n\
             \teval:credibility \"{}\"^^xsd:decimal ;\n\
             \teval:verdict \"{}\" .\n\n",
            pid, turtle_escape(&p.text),
            pred_type,
            p.credibility.score,
            verdict,
        ));
    }
    t
}

impl Default for CredibilityAssessment {
    fn default() -> Self {
        Self {
            score: 0.0,
            confidence: 0.0,
            supporting_evidence: Vec::new(),
            risk_factors: Vec::new(),
            verdict: CredibilityVerdict::Unsupported,
            explanation: String::new(),
        }
    }
}

// Helpers

fn all_sections(sections: &[Section]) -> Vec<&Section> {
    let mut result = Vec::new();
    for s in sections {
        result.push(s);
        result.extend(all_sections(&s.subsections));
    }
    result
}

fn split_sentences(text: &str) -> Vec<String> {
    text.split('.')
        .map(|s| s.trim().to_string())
        .filter(|s| s.len() > 10)
        .collect()
}

fn extract_number_with_unit(text: &str) -> Option<String> {
    let mut result = String::new();
    let mut in_number = false;
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == ',' || ch == '%' || ch == '\u{00a3}' || ch == '$' {
            result.push(ch);
            in_number = true;
        } else if in_number && (ch == 'M' || ch == 'B' || ch == 'K' || ch == 'm') {
            result.push(ch);
            break;
        } else if in_number && result.len() >= 2 {
            break;
        }
    }
    if result.len() >= 2 { Some(result) } else { None }
}

fn extract_timeframe(lower: &str) -> Option<String> {
    // "within 24 months"
    if let Some(pos) = lower.find("within") {
        let rest = &lower[pos..];
        let end = rest.find('.').or_else(|| rest.find(',')).unwrap_or(rest.len().min(40));
        return Some(rest[..end].to_string());
    }
    // "by March 2027"
    if let Some(pos) = lower.find("by ") {
        let rest = &lower[pos..];
        let end = rest.find('.').or_else(|| rest.find(',')).unwrap_or(rest.len().min(30));
        return Some(rest[..end].to_string());
    }
    // "months 1-6", "months 7-12"
    if let Some(pos) = lower.find("months") {
        let start = pos.saturating_sub(10);
        let end = (pos + 20).min(lower.len());
        return Some(lower[start..end].trim().to_string());
    }
    None
}

fn extract_keywords(text: &str) -> Vec<String> {
    let stops = ["the","a","an","and","or","of","in","on","to","for","is","are","was","were","be",
        "will","would","could","should","may","might","this","that","with","from","by","as","it","not"];
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !stops.contains(w))
        .map(|w| w.to_string())
        .collect()
}

fn keyword_overlap(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    let matches = a.iter().filter(|w| b.contains(w)).count();
    matches as f64 / a.len() as f64
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_doc() -> EvalDocument {
        EvalDocument {
            id: "p1".into(), title: "AI Policy".into(), doc_type: "policy".into(),
            total_pages: None, total_word_count: Some(500),
            sections: vec![Section {
                id: "s1".into(), title: "Objectives".into(),
                text: "This policy aims to reduce AI-related complaints by 50% within 24 months. \
                       The estimated cost is \u{00a3}45M over 5 years compared to \u{00a3}120M for the alternative. \
                       We will achieve 100% transparency reporting by March 2027. \
                       Option B achieves 85% of the risk reduction at 37% of the cost. \
                       We will establish a trained AI governance officer in every local authority within 18 months. \
                       The programme will improve public trust in AI-powered services.".into(),
                word_count: 80, page_range: None,
                claims: vec![Claim { id: "c1".into(), text: "complaints increased by 340%".into(), specificity: 0.9, verifiable: true }],
                evidence: vec![
                    Evidence { id: "e1".into(), source: "Local Government Ombudsman".into(), evidence_type: "statistical".into(),
                        text: "AI-related complaints increased by 340%".into(), has_quantified_outcome: true },
                    Evidence { id: "e2".into(), source: "Cost-benefit analysis".into(), evidence_type: "statistical".into(),
                        text: "Option B estimated cost \u{00a3}45M over 5 years".into(), has_quantified_outcome: true },
                ],
                subsections: vec![],
            }],
        }
    }

    #[test]
    fn test_extract_predictions() {
        let doc = policy_doc();
        let preds = extract_predictions(&doc);
        assert!(preds.len() >= 3, "Should find predictions, got {}", preds.len());
        assert!(preds.iter().any(|p| p.prediction_type == PredictionType::QuantitativeTarget));
        assert!(preds.iter().any(|p| p.prediction_type == PredictionType::CostEstimate));
    }

    #[test]
    fn test_assess_credibility() {
        let doc = policy_doc();
        let mut preds = extract_predictions(&doc);
        assess_credibility(&mut preds, &doc);
        for pred in &preds {
            assert!(pred.credibility.score >= 0.0 && pred.credibility.score <= 1.0);
            assert!(!pred.credibility.explanation.is_empty());
        }
    }

    #[test]
    fn test_unsupported_prediction() {
        let doc = EvalDocument {
            id: "d1".into(), title: "Test".into(), doc_type: "policy".into(),
            total_pages: None, total_word_count: Some(100),
            sections: vec![Section {
                id: "s1".into(), title: "Goals".into(),
                text: "We will reduce costs by 90% within 6 months.".into(),
                word_count: 10, page_range: None,
                claims: vec![], evidence: vec![], subsections: vec![],
            }],
        };
        let mut preds = extract_predictions(&doc);
        assess_credibility(&mut preds, &doc);
        assert!(!preds.is_empty());
        // No evidence -> should be unsupported or aspirational
        assert!(preds[0].credibility.verdict != CredibilityVerdict::WellSupported);
    }

    #[test]
    fn test_prediction_report() {
        let doc = policy_doc();
        let mut preds = extract_predictions(&doc);
        assess_credibility(&mut preds, &doc);
        let report = prediction_report(&preds);
        assert!(report.contains("Prediction Credibility"));
        assert!(report.contains("Credibility"));
    }

    #[test]
    fn test_predictions_to_turtle() {
        let doc = policy_doc();
        let mut preds = extract_predictions(&doc);
        assess_credibility(&mut preds, &doc);
        let turtle = predictions_to_turtle(&preds);
        assert!(turtle.contains("eval:Prediction"));
        assert!(turtle.contains("eval:credibility"));
    }

    #[test]
    fn test_extract_timeframe() {
        assert_eq!(extract_timeframe("reduce complaints within 24 months"), Some("within 24 months".into()));
        assert_eq!(extract_timeframe("achieve compliance by march 2027"), Some("by march 2027".into()));
        assert_eq!(extract_timeframe("no timeframe here"), None);
    }
}
