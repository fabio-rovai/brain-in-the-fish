//! Prediction credibility assessment.
//!
//! Extracts predictions, forecasts, targets, and commitments from documents
//! and assesses their credibility based on the evidence presented.
//!
//! NOTE: Benchmark shows subagent prediction matches raw Claude performance.
//! The value of this module is structured extraction + evidence verification +
//! audit trail, not improved accuracy over the base model.
//!
//! Stage 1: Subagent extraction (LLM-powered via extraction_prompt, with rule-based fallback)
//! Stage 2: Evidence verification (checks evidence actually exists and supports the prediction)
//!
//! This is NOT forecasting (MiroFish-style hallucination). It evaluates
//! whether predictions WITHIN the document are supported by evidence
//! WITHIN the document. Grounded, deterministic, auditable.

use crate::extract::{ExtractedItem, ExtractedType};
use crate::types::*;
use serde::Serialize;
use std::collections::HashSet;

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

/// Result of evidence verification for a prediction.
#[derive(Debug, Clone, Serialize)]
pub struct VerificationResult {
    pub prediction_id: String,
    pub evidence_found: usize,
    pub evidence_strength: f64,
    pub counter_evidence: usize,
    pub verification_score: f64,
    pub verified: bool,
    pub flag: Option<String>,
}

// ============================================================================
// Stage 1: Subagent extraction prompt (for LLM-powered extraction)
// ============================================================================

/// Generate a prompt for Claude to extract predictions from a document section.
/// Used when the module is called via MCP with Claude orchestrating.
pub fn extraction_prompt(text: &str) -> String {
    format!(
        "Extract every prediction, target, forecast, commitment, and future claim from this text.\n\
         \n\
         For each, provide:\n\
         - text: the exact prediction (first 100 chars)\n\
         - type: QuantitativeTarget | QualitativeGoal | CostEstimate | Timeline | Commitment | ComparisonClaim\n\
         - target_value: the specific number/percentage if any\n\
         - timeframe: when it should be achieved\n\
         - credibility: 0-100 based ONLY on evidence IN THIS DOCUMENT\n\
         - verdict: WellSupported | PartiallySupported | Aspirational | Unsupported\n\
         - supporting_evidence: what in the document supports this\n\
         - risk_factors: what undermines this prediction\n\
         - reason: one-line explanation of the credibility score\n\
         \n\
         RULES:\n\
         - Only include FORWARD-LOOKING claims, not historical facts\n\
         - 'Complaints increased 340%' is a FACT, not a prediction\n\
         - 'Will reduce complaints by 50%' IS a prediction\n\
         - Be skeptical: high credibility requires specific evidence, not just assertions\n\
         - No duplicates\n\
         \n\
         Text:\n{}\n\
         \n\
         Return as JSON array.",
        text
    )
}

// ============================================================================
// Stage 1: Rule-based extraction (fallback when no LLM available)
// ============================================================================

/// Past-tense verbs that indicate a fact, not a prediction.
const PAST_TENSE_MARKERS: &[&str] = &[
    "increased", "decreased", "was", "were", "showed", "found",
    "demonstrated", "revealed", "rose", "fell", "dropped", "grew",
    "declined", "surged", "doubled", "tripled", "halved",
    "experienced", "recorded", "reported", "observed",
];

/// Check if a sentence is a past-tense fact rather than a prediction.
fn is_past_tense_fact(lower: &str) -> bool {
    // Must contain a past-tense marker AND not contain a future-tense marker
    let has_past = PAST_TENSE_MARKERS.iter().any(|m| {
        // Match whole words to avoid false positives
        lower.split(|c: char| !c.is_alphanumeric()).any(|w| w == *m)
    });
    let has_future = lower.contains("will ")
        || lower.contains("shall ")
        || lower.contains("aims to")
        || lower.contains("plan to")
        || lower.contains("is expected to")
        || lower.contains("is projected to");

    has_past && !has_future
}

/// Compute Jaccard similarity between two strings (word-level).
fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let a_words: HashSet<&str> = a.split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();
    let b_words: HashSet<&str> = b.split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();
    if a_words.is_empty() || b_words.is_empty() {
        return 0.0;
    }
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();
    intersection as f64 / union as f64
}

/// Deduplicate predictions by text similarity (>35% Jaccard = duplicate).
/// A lower threshold catches semantically equivalent predictions phrased differently
/// (e.g. "reduce complaints by 50% within 24 months" vs "reduce AI complaints by 50% in 24 months").
/// Combined with same prediction_type, this avoids false deduplication of unrelated predictions.
fn deduplicate_predictions(predictions: &mut Vec<Prediction>) {
    let mut keep_indices: Vec<usize> = Vec::new();

    for i in 0..predictions.len() {
        let dominated = keep_indices.iter().any(|&j| {
            let same_type = predictions[j].prediction_type == predictions[i].prediction_type;
            let sim = jaccard_similarity(
                &predictions[j].text.to_lowercase(),
                &predictions[i].text.to_lowercase(),
            );
            // Same type + moderate text overlap = duplicate
            (same_type && sim > 0.35) || sim > 0.8
        });
        if !dominated {
            keep_indices.push(i);
        }
    }

    let kept: Vec<Prediction> = keep_indices.into_iter()
        .map(|i| predictions[i].clone())
        .collect();
    *predictions = kept;
}

/// Extract predictions from a document (rule-based, with bug fixes).
/// Filters out past-tense facts and deduplicates results.
pub fn extract_predictions(doc: &EvalDocument) -> Vec<Prediction> {
    let mut predictions = Vec::new();

    for section in all_sections(&doc.sections) {
        let sentences = split_sentences(&section.text);

        for sentence in &sentences {
            let lower = sentence.to_lowercase();

            // Filter: skip past-tense facts
            if is_past_tense_fact(&lower) {
                continue;
            }

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

    // Deduplicate: same sentence matched by multiple detectors or near-identical text
    deduplicate_predictions(&mut predictions);

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

// ============================================================================
// Stage 1b: Credibility assessment (rule-based)
// ============================================================================

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

// ============================================================================
// Stage 2: Evidence Verification
// ============================================================================

/// Verify a prediction's credibility against extracted evidence.
/// Checks if the claimed supporting evidence actually exists in the document
/// and if evidence text actually supports what the prediction claims.
pub fn verify_with_evidence(
    prediction: &Prediction,
    _doc: &EvalDocument,
    extracted_items: &[ExtractedItem],
) -> VerificationResult {
    let pred_keywords = extract_keywords(&prediction.text.to_lowercase());

    // Check 1: Does supporting evidence exist in the document?
    let mut evidence_found = 0;
    let mut evidence_strength = 0.0;
    for item in extracted_items {
        if item.item_type == ExtractedType::Statistic
            || item.item_type == ExtractedType::Citation
            || item.item_type == ExtractedType::Evidence
        {
            let item_keywords = extract_keywords(&item.text.to_lowercase());
            let overlap = keyword_overlap(&pred_keywords, &item_keywords);
            if overlap > 0.15 {
                evidence_found += 1;
                evidence_strength += item.confidence * overlap;
            }
        }
    }

    // Check 2: Is there counter-evidence?
    let mut counter_evidence = 0;
    for item in extracted_items {
        if item.item_type == ExtractedType::Claim {
            let item_keywords = extract_keywords(&item.text.to_lowercase());
            let overlap = keyword_overlap(&pred_keywords, &item_keywords);
            if overlap > 0.2 {
                // Same topic but different claim -- possible counter-evidence
                counter_evidence += 1;
            }
        }
    }

    // Compute verification score
    let evidence_score = (evidence_strength / 2.0).min(1.0);
    let counter_penalty = (counter_evidence as f64 * 0.1).min(0.3);
    let verification_score = (evidence_score - counter_penalty).max(0.0);

    let verified = evidence_found > 0;
    let flag = if !verified && prediction.credibility.score > 0.5 {
        Some("UNVERIFIABLE: Subagent rated high credibility but no supporting evidence found".into())
    } else if verified && prediction.credibility.score < 0.2 {
        Some("EVIDENCE CONFIRMS: Evidence exists but credibility rated low -- may be understated".into())
    } else {
        None
    };

    VerificationResult {
        prediction_id: prediction.id.clone(),
        evidence_found,
        evidence_strength,
        counter_evidence,
        verification_score,
        verified,
        flag,
    }
}

/// Run evidence verification on all predictions against extracted items.
pub fn verify_all(
    predictions: &[Prediction],
    doc: &EvalDocument,
    extracted_items: &[ExtractedItem],
) -> Vec<VerificationResult> {
    predictions.iter()
        .map(|p| verify_with_evidence(p, doc, extracted_items))
        .collect()
}

/// Backwards-compatible alias for `verify_with_evidence`.
#[deprecated(note = "Renamed to verify_with_evidence — SNN removed")]
pub fn verify_with_snn(
    prediction: &Prediction,
    doc: &EvalDocument,
    extracted_items: &[ExtractedItem],
) -> VerificationResult {
    verify_with_evidence(prediction, doc, extracted_items)
}

// ============================================================================
// Report generation
// ============================================================================

/// Generate a prediction credibility report section (with verification column).
pub fn prediction_report(predictions: &[Prediction]) -> String {
    prediction_report_with_verification(predictions, &[])
}

/// Generate a prediction credibility report section with evidence verification results.
pub fn prediction_report_with_verification(
    predictions: &[Prediction],
    verifications: &[VerificationResult],
) -> String {
    if predictions.is_empty() {
        return "## Prediction Credibility\n\nNo predictions, forecasts, or targets found in the document.\n\n".into();
    }

    let mut r = String::from("## Prediction Credibility\n\n");
    r.push_str(&format!("{} predictions/targets extracted from the document.\n\n", predictions.len()));

    r.push_str("| Prediction | Type | Credibility | Verdict | Verified | Flag |\n|---|---|---|---|---|---|\n");
    for pred in predictions {
        let text = truncate(&pred.text, 60);
        let verification = verifications.iter().find(|v| v.prediction_id == pred.id);
        let verified_str = match verification {
            Some(v) => if v.verified { "Yes" } else { "No" },
            None => "N/A",
        };
        let flag_str = match verification {
            Some(v) => v.flag.as_deref().unwrap_or("--"),
            None => "--",
        };
        r.push_str(&format!("| {} | {:?} | {:.0}% | {:?} | {} | {} |\n",
            text, pred.prediction_type, pred.credibility.score * 100.0, pred.credibility.verdict,
            verified_str, flag_str));
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

        // Add evidence verification info
        if let Some(v) = verifications.iter().find(|v| v.prediction_id == pred.id) {
            r.push_str(&format!("- Verified: {} (score: {:.2}, evidence items: {}, counter-evidence: {})\n",
                if v.verified { "Yes" } else { "No" },
                v.verification_score, v.evidence_found, v.counter_evidence));
            if let Some(flag) = &v.flag {
                r.push_str(&format!("- **FLAG:** {}\n", flag));
            }
        }

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

// ============================================================================
// Helpers
// ============================================================================

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

/// Extract a timeframe from lowercased text.
/// Fixed: "by X" patterns must start with temporal words, not percentages.
pub fn extract_timeframe(lower: &str) -> Option<String> {
    // "within 24 months"
    if let Some(pos) = lower.find("within") {
        let rest = &lower[pos..];
        let end = rest.find('.').or_else(|| rest.find(',')).unwrap_or(rest.len().min(40));
        return Some(rest[..end].to_string());
    }
    // "by March 2027" — but NOT "by 340%" (percentage is not a timeframe)
    if let Some(pos) = lower.find("by ") {
        let rest = &lower[pos + 3..]; // skip "by "
        let trimmed = rest.trim_start();
        // Must start with a month name, year, or temporal word — not a digit followed by %
        let starts_temporal = trimmed.starts_with("march")
            || trimmed.starts_with("april")
            || trimmed.starts_with("may")
            || trimmed.starts_with("june")
            || trimmed.starts_with("july")
            || trimmed.starts_with("august")
            || trimmed.starts_with("september")
            || trimmed.starts_with("october")
            || trimmed.starts_with("november")
            || trimmed.starts_with("december")
            || trimmed.starts_with("january")
            || trimmed.starts_with("february")
            || trimmed.starts_with("20") // year like 2025, 2027
            || trimmed.starts_with("the end of")
            || trimmed.starts_with("end of")
            || trimmed.starts_with("q1")
            || trimmed.starts_with("q2")
            || trimmed.starts_with("q3")
            || trimmed.starts_with("q4");

        if starts_temporal {
            let full_rest = &lower[pos..];
            let end = full_rest.find('.').or_else(|| full_rest.find(',')).unwrap_or(full_rest.len().min(30));
            return Some(full_rest[..end].to_string());
        }
    }
    // "months 1-6", "months 7-12"
    if let Some(pos) = lower.find("months") {
        let start = pos.saturating_sub(10);
        let end = (pos + 20).min(lower.len());
        return Some(lower[start..end].trim().to_string());
    }
    None
}

pub fn extract_keywords(text: &str) -> Vec<String> {
    let stops = ["the","a","an","and","or","of","in","on","to","for","is","are","was","were","be",
        "will","would","could","should","may","might","this","that","with","from","by","as","it","not"];
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !stops.contains(w))
        .map(|w| w.to_string())
        .collect()
}

pub fn keyword_overlap(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    let matches = a.iter().filter(|w| b.contains(w)).count();
    matches as f64 / a.len() as f64
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max]) }
}

// ============================================================================
// Tests
// ============================================================================

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
        // New: report should contain Verified column
        assert!(report.contains("Verified"));
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

    // ---- New tests ----

    #[test]
    fn test_fact_filtering() {
        // "complaints increased by 340%" should NOT be extracted as a prediction
        let doc = EvalDocument {
            id: "d1".into(), title: "Test".into(), doc_type: "policy".into(),
            total_pages: None, total_word_count: Some(100),
            sections: vec![Section {
                id: "s1".into(), title: "Facts".into(),
                text: "Between 2023 and 2025, AI-related complaints increased by 340%.".into(),
                word_count: 10, page_range: None,
                claims: vec![], evidence: vec![], subsections: vec![],
            }],
        };
        let preds = extract_predictions(&doc);
        assert!(preds.is_empty(), "Past-tense facts should not be predictions, got {} predictions", preds.len());
    }

    #[test]
    fn test_deduplication() {
        let doc = EvalDocument {
            id: "d1".into(), title: "Test".into(), doc_type: "policy".into(),
            total_pages: None, total_word_count: Some(100),
            sections: vec![Section {
                id: "s1".into(), title: "Goals".into(),
                text: "This policy will reduce complaints by 50% within 24 months. \
                       The objective is to reduce AI complaints by 50% in 24 months.".into(),
                word_count: 20, page_range: None,
                claims: vec![], evidence: vec![], subsections: vec![],
            }],
        };
        let preds = extract_predictions(&doc);
        assert_eq!(preds.len(), 1, "Near-duplicate predictions should be merged, got {}", preds.len());
    }

    #[test]
    fn test_timeframe_not_percentage() {
        let text = "complaints increased by 340% between 2023 and 2025";
        let tf = extract_timeframe(text);
        // Should NOT return "by 340%" as a timeframe
        assert!(tf.is_none() || !tf.as_ref().unwrap().contains("340"),
            "Timeframe should not capture percentages, got: {:?}", tf);
    }

    #[test]
    fn test_verification_no_evidence() {
        // Create a prediction and check evidence verification with no evidence
        let pred = Prediction {
            id: "p1".into(),
            text: "reduce costs by 50%".into(),
            section_id: "s1".into(),
            section_title: "Goals".into(),
            prediction_type: PredictionType::QuantitativeTarget,
            target_value: Some("50%".into()),
            timeframe: Some("within 12 months".into()),
            credibility: CredibilityAssessment {
                score: 0.8, confidence: 0.5,
                supporting_evidence: vec![], risk_factors: vec![],
                verdict: CredibilityVerdict::WellSupported,
                explanation: String::new(),
            },
        };
        let doc = EvalDocument {
            id: "d1".into(), title: "Test".into(), doc_type: "policy".into(),
            total_pages: None, total_word_count: Some(100),
            sections: vec![],
        };
        let items: Vec<ExtractedItem> = vec![]; // No evidence
        let result = verify_with_snn(&pred, &doc, &items);
        assert!(!result.verified, "Should not be verified with no evidence");
        assert!(result.flag.is_some(), "Should flag as unverifiable");
        assert!(result.flag.as_ref().unwrap().contains("UNVERIFIABLE"),
            "Flag should mention UNVERIFIABLE");
    }

    #[test]
    fn test_verification_with_evidence() {
        let pred = Prediction {
            id: "p1".into(),
            text: "reduce costs by 50% through automation".into(),
            section_id: "s1".into(),
            section_title: "Goals".into(),
            prediction_type: PredictionType::QuantitativeTarget,
            target_value: Some("50%".into()),
            timeframe: Some("within 12 months".into()),
            credibility: CredibilityAssessment {
                score: 0.6, confidence: 0.5,
                supporting_evidence: vec![], risk_factors: vec![],
                verdict: CredibilityVerdict::PartiallySupported,
                explanation: String::new(),
            },
        };
        let doc = EvalDocument {
            id: "d1".into(), title: "Test".into(), doc_type: "policy".into(),
            total_pages: None, total_word_count: Some(100),
            sections: vec![],
        };
        let items = vec![
            ExtractedItem {
                id: "e1".into(),
                item_type: ExtractedType::Statistic,
                text: "Automation reduced costs by 40% in the pilot programme".into(),
                source_span: None,
                extraction_method: crate::extract::ExtractionMethod::Rule,
                confidence: 0.9,
            },
        ];
        let result = verify_with_snn(&pred, &doc, &items);
        assert!(result.verified, "Should be verified with matching evidence");
        assert!(result.evidence_found >= 1);
    }

    #[test]
    fn test_extraction_prompt_content() {
        let prompt = extraction_prompt("Some document text about AI policy.");
        assert!(prompt.contains("FORWARD-LOOKING"), "Prompt should mention forward-looking");
        assert!(prompt.contains("FACT"), "Prompt should distinguish facts from predictions");
        assert!(prompt.contains("JSON array"), "Prompt should request JSON output");
    }

    #[test]
    fn test_past_tense_fact_detection() {
        assert!(is_past_tense_fact("complaints increased by 340%"));
        assert!(is_past_tense_fact("costs rose significantly last year"));
        assert!(!is_past_tense_fact("we will reduce costs by 50%"));
        assert!(!is_past_tense_fact("the plan aims to reduce costs"));
    }

    #[test]
    fn test_jaccard_similarity() {
        let sim = jaccard_similarity(
            "reduce complaints by 50% within 24 months",
            "reduce AI complaints by 50% in 24 months",
        );
        assert!(sim > 0.5, "Similar sentences should have high Jaccard, got {}", sim);

        let sim2 = jaccard_similarity(
            "reduce complaints by 50%",
            "improve public trust in services",
        );
        assert!(sim2 < 0.3, "Different sentences should have low Jaccard, got {}", sim2);
    }

    #[test]
    fn test_report_with_verification() {
        let pred = Prediction {
            id: "p1".into(),
            text: "reduce costs by 50%".into(),
            section_id: "s1".into(),
            section_title: "Goals".into(),
            prediction_type: PredictionType::QuantitativeTarget,
            target_value: Some("50%".into()),
            timeframe: None,
            credibility: CredibilityAssessment {
                score: 0.5, confidence: 0.5,
                supporting_evidence: vec![], risk_factors: vec![],
                verdict: CredibilityVerdict::PartiallySupported,
                explanation: "test".into(),
            },
        };
        let verification = VerificationResult {
            prediction_id: "p1".into(),
            evidence_found: 0,
            evidence_strength: 0.0,
            counter_evidence: 0,
            verification_score: 0.0,
            verified: false,
            flag: Some("UNVERIFIABLE: test".into()),
        };
        let report = prediction_report_with_verification(&[pred], &[verification]);
        assert!(report.contains("UNVERIFIABLE"));
        assert!(report.contains("Verified"));
        assert!(report.contains("FLAG"));
    }
}
