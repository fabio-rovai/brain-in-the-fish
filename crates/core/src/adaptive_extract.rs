//! Domain-adaptive evidence extraction.
//!
//! Extracts what MATTERS for each document type instead of always looking for
//! citations and statistics. For essays: argument thesis, counter-arguments,
//! vocabulary sophistication, rhetorical devices, cohesion, personal voice.
//! For tenders: claims, case studies, named staff, compliance refs.
//! For policies: objectives, predictions, evidence base, stakeholder coverage.
//! For contracts: obligations, conditions, definitions, risk clauses.

use crate::types::{Claim, Evidence};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single piece of domain-adaptive evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveEvidence {
    pub id: String,
    pub category: String,
    pub text: String,
    /// 0.0-1.0 quality score
    pub quality: f64,
    /// 0.0-1.0 relevance score
    pub relevance: f64,
}

impl AdaptiveEvidence {
    fn new(category: &str, text: String, quality: f64, relevance: f64) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            category: category.to_string(),
            text,
            quality,
            relevance,
        }
    }
}

// ---------------------------------------------------------------------------
// Intent detection
// ---------------------------------------------------------------------------

/// Classify intent into a domain category.
fn intent_domain(intent: &str) -> &str {
    let lower = intent.to_lowercase();
    if lower.contains("essay")
        || lower.contains("grade")
        || lower.contains("mark")
        || lower.contains("language")
        || lower.contains("writing")
    {
        "essay"
    } else if lower.contains("tender")
        || lower.contains("bid")
        || lower.contains("proposal")
        || lower.contains("rfp")
    {
        "tender"
    } else if lower.contains("policy")
        || lower.contains("legislation")
        || lower.contains("regulation")
        || lower.contains("white paper")
    {
        "policy"
    } else if lower.contains("contract")
        || lower.contains("agreement")
        || lower.contains("terms")
        || lower.contains("legal")
    {
        "contract"
    } else {
        "essay" // default — essays are the most common evaluation target
    }
}

// ---------------------------------------------------------------------------
// Category definitions per domain
// ---------------------------------------------------------------------------

/// Returns (category_name, description) pairs for the given intent.
pub fn extraction_categories(intent: &str) -> Vec<(&'static str, &'static str)> {
    match intent_domain(intent) {
        "essay" => vec![
            ("argument_thesis", "Central argument or thesis statement"),
            ("topic_sentences", "Opening sentence of each paragraph"),
            (
                "counter_arguments",
                "Acknowledgement of opposing viewpoints",
            ),
            (
                "vocabulary_sophistication",
                "Use of academic or advanced vocabulary",
            ),
            (
                "sentence_variety",
                "Complex sentences with subordinate clauses",
            ),
            (
                "cohesion_devices",
                "Transition words and linking expressions",
            ),
            ("personal_voice", "First-person opinion and stance markers"),
            (
                "rhetorical_devices",
                "Rhetorical questions, repetition, and persuasive techniques",
            ),
        ],
        "tender" => vec![
            ("claims", "Specific claims about capability or experience"),
            (
                "case_studies",
                "Named case studies with outcomes or references",
            ),
            ("named_staff", "Named individuals with roles or qualifications"),
            (
                "compliance_refs",
                "References to specification requirements or standards",
            ),
            (
                "quantified_outcomes",
                "Measurable results with numbers or percentages",
            ),
        ],
        "policy" => vec![
            ("objectives", "Stated policy objectives or goals"),
            (
                "predictions",
                "Predictions about outcomes or impact of the policy",
            ),
            (
                "evidence_base",
                "Citations, data, or research supporting the policy",
            ),
            (
                "stakeholder_coverage",
                "References to affected groups or stakeholders",
            ),
            (
                "implementation",
                "Implementation steps, timelines, or mechanisms",
            ),
        ],
        "contract" => vec![
            (
                "obligations",
                "Binding obligations on parties (shall, must, agrees to)",
            ),
            (
                "conditions",
                "Conditions, triggers, or contingencies (if, when, upon)",
            ),
            (
                "definitions",
                "Defined terms and their meanings",
            ),
            (
                "risk_clauses",
                "Limitation of liability, indemnity, force majeure, termination",
            ),
        ],
        _ => vec![
            ("claims", "General claims"),
            ("evidence", "Supporting evidence"),
        ],
    }
}

// ---------------------------------------------------------------------------
// LLM prompt generation
// ---------------------------------------------------------------------------

/// Generate an LLM prompt for domain-adaptive extraction.
pub fn adaptive_extraction_prompt(text: &str, intent: &str) -> String {
    let categories = extraction_categories(intent);
    let domain = intent_domain(intent);
    let truncated = if text.len() > 4000 {
        &text[..4000]
    } else {
        text
    };

    let mut prompt = String::new();
    prompt.push_str(&format!(
        "Extract domain-relevant evidence from this {} document.\n\n",
        domain
    ));
    prompt.push_str("For each item, provide:\n");
    prompt.push_str("- category: one of the categories below\n");
    prompt.push_str("- text: the exact quote or close paraphrase\n");
    prompt.push_str("- quality: 0.0-1.0 (how strong/clear is this item?)\n");
    prompt.push_str("- relevance: 0.0-1.0 (how relevant to evaluation?)\n\n");
    prompt.push_str("Categories:\n");
    for (name, desc) in &categories {
        prompt.push_str(&format!("- {}: {}\n", name, desc));
    }
    prompt.push_str(&format!("\n## Document\n\n{}\n\n", truncated));
    prompt.push_str("Return a JSON array of objects with fields: category, text, quality, relevance.\n");
    prompt.push_str("Return ONLY the JSON array.\n");

    prompt
}

// ---------------------------------------------------------------------------
// Rule-based extraction (no LLM)
// ---------------------------------------------------------------------------

/// Sentence splitter that respects abbreviations and decimals.
fn split_sentences(text: &str) -> Vec<&str> {
    // Simple sentence split on '. ', '? ', '! ' — good enough for extraction
    let mut sentences = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        if (bytes[i] == b'.' || bytes[i] == b'?' || bytes[i] == b'!')
            && i + 1 < bytes.len()
            && bytes[i + 1] == b' '
        {
            let s = text[start..=i].trim();
            if !s.is_empty() {
                sentences.push(s);
            }
            start = i + 2;
        }
    }
    // Last sentence (may not end with '. ')
    let last = text[start..].trim();
    if !last.is_empty() {
        sentences.push(last);
    }
    sentences
}

/// Split text into paragraphs (separated by double newline or blank line).
fn split_paragraphs(text: &str) -> Vec<&str> {
    text.split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Rule-based adaptive extraction — no LLM needed.
pub fn quick_adaptive_extract(text: &str, intent: &str) -> Vec<AdaptiveEvidence> {
    match intent_domain(intent) {
        "essay" => extract_essay(text),
        "tender" => extract_tender(text),
        "policy" => extract_policy(text),
        "contract" => extract_contract(text),
        _ => extract_essay(text),
    }
}

// ---- Essay extraction ----

const COUNTER_ARG_MARKERS: &[&str] = &[
    "however",
    "although",
    "on the other hand",
    "nevertheless",
    "despite",
    "conversely",
    "whereas",
    "yet ",
    "but ",
    "in contrast",
    "on the contrary",
];

const ACADEMIC_MARKERS: &[&str] = &[
    "furthermore",
    "consequently",
    "demonstrate",
    "implications",
    "significant",
    "analysis",
    "therefore",
    "moreover",
    "nevertheless",
    "notwithstanding",
    "paradigm",
    "hypothesis",
    "empirical",
    "methodology",
    "systematic",
    "facilitate",
    "subsequently",
    "considerable",
    "fundamental",
    "substantial",
];

const COHESION_STARTERS: &[&str] = &[
    "firstly",
    "secondly",
    "thirdly",
    "finally",
    "in addition",
    "moreover",
    "furthermore",
    "in conclusion",
    "to summarize",
    "to sum up",
    "as a result",
    "consequently",
    "therefore",
    "thus",
    "hence",
    "for example",
    "for instance",
    "specifically",
    "in particular",
    "on the other hand",
    "in contrast",
    "similarly",
    "likewise",
    "meanwhile",
    "next",
    "then",
    "additionally",
];

const PERSONAL_VOICE_MARKERS: &[&str] = &[
    "i think",
    "i believe",
    "in my opinion",
    "i feel",
    "i consider",
    "from my perspective",
    "in my view",
    "i would argue",
    "personally",
    "my view is",
];

const SUBORDINATE_MARKERS: &[&str] = &[
    "because",
    "although",
    "which",
    "that ",
    "while",
    "since",
    "unless",
    "whereas",
    "if ",
    "when ",
    "where ",
    "after ",
    "before ",
];

fn extract_essay(text: &str) -> Vec<AdaptiveEvidence> {
    let mut items = Vec::new();
    let sentences = split_sentences(text);
    let paragraphs = split_paragraphs(text);

    // Argument thesis: first substantial sentence (>20 chars)
    for s in &sentences {
        if s.len() > 20 {
            items.push(AdaptiveEvidence::new(
                "argument_thesis",
                s.to_string(),
                0.6,
                0.9,
            ));
            break;
        }
    }

    // Topic sentences: first sentence of each paragraph
    for para in &paragraphs {
        let para_sentences = split_sentences(para);
        if let Some(first) = para_sentences.first()
            && first.len() > 10
        {
            items.push(AdaptiveEvidence::new(
                "topic_sentences",
                first.to_string(),
                0.5,
                0.7,
            ));
        }
    }

    for s in &sentences {
        let lower = s.to_lowercase();

        // Counter-arguments
        if COUNTER_ARG_MARKERS
            .iter()
            .any(|m| lower.contains(m))
        {
            items.push(AdaptiveEvidence::new(
                "counter_arguments",
                s.to_string(),
                0.7,
                0.8,
            ));
        }

        // Vocabulary sophistication
        let academic_count = ACADEMIC_MARKERS
            .iter()
            .filter(|m| lower.contains(**m))
            .count();
        if academic_count > 0 {
            let quality = (academic_count as f64 * 0.3).min(1.0);
            items.push(AdaptiveEvidence::new(
                "vocabulary_sophistication",
                s.to_string(),
                quality,
                0.7,
            ));
        }

        // Sentence variety: complex sentences with subordinate clauses
        if SUBORDINATE_MARKERS
            .iter()
            .any(|m| lower.contains(m))
            && s.len() > 30
        {
            items.push(AdaptiveEvidence::new(
                "sentence_variety",
                s.to_string(),
                0.6,
                0.6,
            ));
        }

        // Cohesion devices: transition words at sentence start
        let start_lower = lower.trim_start();
        if COHESION_STARTERS
            .iter()
            .any(|m| start_lower.starts_with(m))
        {
            items.push(AdaptiveEvidence::new(
                "cohesion_devices",
                s.to_string(),
                0.6,
                0.7,
            ));
        }

        // Personal voice
        if PERSONAL_VOICE_MARKERS
            .iter()
            .any(|m| lower.contains(m))
        {
            items.push(AdaptiveEvidence::new(
                "personal_voice",
                s.to_string(),
                0.5,
                0.6,
            ));
        }

        // Rhetorical devices: questions
        if s.ends_with('?') {
            items.push(AdaptiveEvidence::new(
                "rhetorical_devices",
                s.to_string(),
                0.6,
                0.7,
            ));
        }
    }

    // Rhetorical devices: repetition (same word appears 3+ times across sentences)
    detect_repetition_patterns(text, &mut items);

    items
}

fn detect_repetition_patterns(text: &str, items: &mut Vec<AdaptiveEvidence>) {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 10 {
        return;
    }
    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for w in &words {
        let lower = w.to_lowercase();
        let clean: String = lower.chars().filter(|c| c.is_alphabetic()).collect();
        if clean.len() > 4 {
            *freq.entry(clean).or_insert(0) += 1;
        }
    }
    for (word, count) in &freq {
        if *count >= 3 {
            items.push(AdaptiveEvidence::new(
                "rhetorical_devices",
                format!("Repetition: \"{}\" appears {} times", word, count),
                0.4,
                0.5,
            ));
        }
    }
}

// ---- Tender extraction ----

fn extract_tender(text: &str) -> Vec<AdaptiveEvidence> {
    let mut items = Vec::new();
    let sentences = split_sentences(text);

    for s in &sentences {
        let lower = s.to_lowercase();

        // Claims
        if lower.contains("we have")
            || lower.contains("we deliver")
            || lower.contains("our team")
            || lower.contains("we provide")
            || lower.contains("we will")
            || lower.contains("our approach")
        {
            items.push(AdaptiveEvidence::new("claims", s.to_string(), 0.6, 0.8));
        }

        // Case studies
        if lower.contains("case study")
            || lower.contains("project:")
            || lower.contains("example:")
            || lower.contains("we delivered")
            || lower.contains("we completed")
        {
            items.push(AdaptiveEvidence::new(
                "case_studies",
                s.to_string(),
                0.7,
                0.9,
            ));
        }

        // Named staff: detect "Name (Role)" or "Dr/Mr/Ms/Mrs Name" patterns
        if has_named_person(s) {
            items.push(AdaptiveEvidence::new(
                "named_staff",
                s.to_string(),
                0.7,
                0.8,
            ));
        }

        // Compliance references
        if lower.contains("section ")
            || lower.contains("clause ")
            || lower.contains("requirement ")
            || lower.contains("as specified")
            || lower.contains("in accordance with")
            || lower.contains("compliant")
            || lower.contains("iso ")
        {
            items.push(AdaptiveEvidence::new(
                "compliance_refs",
                s.to_string(),
                0.7,
                0.9,
            ));
        }

        // Quantified outcomes
        if has_numbers(s)
            && (lower.contains('%')
                || lower.contains("reduction")
                || lower.contains("increase")
                || lower.contains("saved")
                || lower.contains("delivered")
                || lower.contains("achieved"))
        {
            items.push(AdaptiveEvidence::new(
                "quantified_outcomes",
                s.to_string(),
                0.8,
                0.9,
            ));
        }
    }

    items
}

fn has_named_person(s: &str) -> bool {
    let prefixes = ["Dr ", "Mr ", "Ms ", "Mrs ", "Prof "];
    for prefix in &prefixes {
        if s.contains(prefix) {
            return true;
        }
    }
    // Check for "Name (Role)" pattern — capitalized word followed by parenthetical
    s.contains('(') && s.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
}

fn has_numbers(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_digit())
}

// ---- Policy extraction ----

fn extract_policy(text: &str) -> Vec<AdaptiveEvidence> {
    let mut items = Vec::new();
    let sentences = split_sentences(text);

    for s in &sentences {
        let lower = s.to_lowercase();

        // Objectives
        if lower.contains("objective")
            || lower.contains("aim ")
            || lower.contains("goal ")
            || lower.contains("purpose")
            || lower.contains("seeks to")
            || lower.contains("intends to")
        {
            items.push(AdaptiveEvidence::new(
                "objectives",
                s.to_string(),
                0.7,
                0.9,
            ));
        }

        // Predictions
        if lower.contains("will result")
            || lower.contains("is expected")
            || lower.contains("projected")
            || lower.contains("forecast")
            || lower.contains("anticipated")
        {
            items.push(AdaptiveEvidence::new(
                "predictions",
                s.to_string(),
                0.6,
                0.8,
            ));
        }

        // Evidence base
        if lower.contains("research")
            || lower.contains("study")
            || lower.contains("data shows")
            || lower.contains("evidence")
            || lower.contains("according to")
        {
            items.push(AdaptiveEvidence::new(
                "evidence_base",
                s.to_string(),
                0.7,
                0.8,
            ));
        }

        // Stakeholder coverage
        if lower.contains("stakeholder")
            || lower.contains("community")
            || lower.contains("public")
            || lower.contains("citizens")
            || lower.contains("affected")
            || lower.contains("vulnerable")
        {
            items.push(AdaptiveEvidence::new(
                "stakeholder_coverage",
                s.to_string(),
                0.6,
                0.7,
            ));
        }

        // Implementation
        if lower.contains("implement")
            || lower.contains("timeline")
            || lower.contains("phase ")
            || lower.contains("by 20")
            || lower.contains("will be")
            || lower.contains("mechanism")
        {
            items.push(AdaptiveEvidence::new(
                "implementation",
                s.to_string(),
                0.6,
                0.8,
            ));
        }
    }

    items
}

// ---- Contract extraction ----

fn extract_contract(text: &str) -> Vec<AdaptiveEvidence> {
    let mut items = Vec::new();
    let sentences = split_sentences(text);

    for s in &sentences {
        let lower = s.to_lowercase();

        // Obligations
        if lower.contains("shall ")
            || lower.contains("must ")
            || lower.contains("agrees to")
            || lower.contains("is required to")
            || lower.contains("undertakes to")
        {
            items.push(AdaptiveEvidence::new(
                "obligations",
                s.to_string(),
                0.8,
                0.9,
            ));
        }

        // Conditions
        if lower.contains("if ")
            || lower.contains("upon ")
            || lower.contains("provided that")
            || lower.contains("subject to")
            || lower.contains("in the event")
        {
            items.push(AdaptiveEvidence::new(
                "conditions",
                s.to_string(),
                0.7,
                0.8,
            ));
        }

        // Definitions
        if lower.contains("means ")
            || lower.contains("defined as")
            || lower.contains("shall mean")
            || (s.contains('"') && lower.contains("means"))
        {
            items.push(AdaptiveEvidence::new(
                "definitions",
                s.to_string(),
                0.7,
                0.7,
            ));
        }

        // Risk clauses
        if lower.contains("liability")
            || lower.contains("indemnif")
            || lower.contains("force majeure")
            || lower.contains("terminat")
            || lower.contains("limitation")
            || lower.contains("waiver")
        {
            items.push(AdaptiveEvidence::new(
                "risk_clauses",
                s.to_string(),
                0.8,
                0.9,
            ));
        }
    }

    items
}

// ---------------------------------------------------------------------------
// Evidence summary for shoal prompt
// ---------------------------------------------------------------------------

/// Generate a formatted summary of adaptive evidence for inclusion in scoring prompts.
pub fn evidence_summary(items: &[AdaptiveEvidence], intent: &str) -> String {
    if items.is_empty() {
        return "- No domain-specific evidence detected.\n".to_string();
    }

    let categories = extraction_categories(intent);
    let mut summary = String::new();

    for (cat_name, cat_desc) in &categories {
        let cat_items: Vec<&AdaptiveEvidence> = items
            .iter()
            .filter(|i| i.category == *cat_name)
            .collect();

        if cat_items.is_empty() {
            summary.push_str(&format!("- {}: NONE detected\n", cat_desc));
        } else {
            summary.push_str(&format!(
                "- {} ({}): {} found",
                cat_desc,
                cat_name,
                cat_items.len()
            ));
            // Show best example (highest quality)
            if let Some(best) = cat_items.iter().max_by(|a, b| {
                a.quality
                    .partial_cmp(&b.quality)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                let text = if best.text.len() > 60 {
                    format!("{}...", &best.text[..60])
                } else {
                    best.text.clone()
                };
                summary.push_str(&format!(" — e.g. \"{}\"", text));
            }
            summary.push('\n');
        }
    }

    // Overall stats
    let total = items.len();
    let avg_quality = items.iter().map(|i| i.quality).sum::<f64>() / total as f64;
    summary.push_str(&format!(
        "- Total evidence items: {}, average quality: {:.2}\n",
        total, avg_quality
    ));

    summary
}

// ---------------------------------------------------------------------------
// Compatibility bridge to existing pipeline types
// ---------------------------------------------------------------------------

/// Convert adaptive evidence items into the existing Claim and Evidence types
/// for backward compatibility with the pipeline.
pub fn to_pipeline_types(items: &[AdaptiveEvidence]) -> (Vec<Claim>, Vec<Evidence>) {
    let mut claims = Vec::new();
    let mut evidence = Vec::new();

    for item in items {
        match item.category.as_str() {
            // These map naturally to Claims
            "argument_thesis" | "claims" | "objectives" | "obligations" => {
                claims.push(Claim {
                    id: item.id.clone(),
                    text: item.text.clone(),
                    specificity: item.quality,
                    verifiable: item.quality > 0.5,
                });
            }
            // These map naturally to Evidence
            "case_studies" | "quantified_outcomes" | "evidence_base" | "compliance_refs" => {
                evidence.push(Evidence {
                    id: item.id.clone(),
                    source: item.category.clone(),
                    evidence_type: map_evidence_type(&item.category),
                    text: item.text.clone(),
                    has_quantified_outcome: item.category == "quantified_outcomes",
                });
            }
            // Essay categories: topic sentences and thesis are claims, rest are evidence
            "topic_sentences" | "personal_voice" => {
                claims.push(Claim {
                    id: item.id.clone(),
                    text: item.text.clone(),
                    specificity: item.quality,
                    verifiable: false,
                });
            }
            _ => {
                // Everything else becomes evidence
                evidence.push(Evidence {
                    id: item.id.clone(),
                    source: item.category.clone(),
                    evidence_type: map_evidence_type(&item.category),
                    text: item.text.clone(),
                    has_quantified_outcome: false,
                });
            }
        }
    }

    (claims, evidence)
}

fn map_evidence_type(category: &str) -> String {
    match category {
        "case_studies" => "case_study".to_string(),
        "quantified_outcomes" => "statistic".to_string(),
        "evidence_base" => "citation".to_string(),
        "compliance_refs" => "citation".to_string(),
        "counter_arguments" => "primary_data".to_string(),
        "vocabulary_sophistication" => "primary_data".to_string(),
        "sentence_variety" => "primary_data".to_string(),
        "cohesion_devices" => "primary_data".to_string(),
        "rhetorical_devices" => "primary_data".to_string(),
        "definitions" => "citation".to_string(),
        "risk_clauses" => "primary_data".to_string(),
        "conditions" => "primary_data".to_string(),
        "predictions" => "primary_data".to_string(),
        "stakeholder_coverage" => "primary_data".to_string(),
        "implementation" => "primary_data".to_string(),
        "named_staff" => "primary_data".to_string(),
        _ => "primary_data".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_essay_categories() {
        let cats = extraction_categories("grade this essay");
        let names: Vec<&str> = cats.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"argument_thesis"));
        assert!(names.contains(&"counter_arguments"));
        assert!(names.contains(&"vocabulary_sophistication"));
        assert!(names.contains(&"sentence_variety"));
        assert!(names.contains(&"cohesion_devices"));
        assert!(names.contains(&"personal_voice"));
        assert!(names.contains(&"rhetorical_devices"));
        assert!(names.contains(&"topic_sentences"));
        assert_eq!(cats.len(), 8);
    }

    #[test]
    fn test_tender_categories() {
        let cats = extraction_categories("evaluate this tender bid");
        let names: Vec<&str> = cats.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"claims"));
        assert!(names.contains(&"case_studies"));
        assert!(names.contains(&"named_staff"));
        assert!(names.contains(&"compliance_refs"));
        assert!(names.contains(&"quantified_outcomes"));
        assert_eq!(cats.len(), 5);
    }

    #[test]
    fn test_policy_categories() {
        let cats = extraction_categories("review this policy document");
        let names: Vec<&str> = cats.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"objectives"));
        assert!(names.contains(&"predictions"));
        assert!(names.contains(&"evidence_base"));
    }

    #[test]
    fn test_contract_categories() {
        let cats = extraction_categories("analyse this contract agreement");
        let names: Vec<&str> = cats.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"obligations"));
        assert!(names.contains(&"conditions"));
        assert!(names.contains(&"definitions"));
        assert!(names.contains(&"risk_clauses"));
    }

    #[test]
    fn test_quick_extract_essay() {
        let text = "Climate change is a significant global challenge that affects everyone. \
                     However, some people argue that the economic costs are too high. \
                     Furthermore, the implications of inaction demonstrate considerable risk. \
                     I believe we must act now. \
                     Is it not time to take responsibility? \
                     Because the evidence shows rising temperatures, we cannot delay. \
                     In conclusion, immediate action is required.";

        let items = quick_adaptive_extract(text, "mark this essay");

        let categories: Vec<&str> = items.iter().map(|i| i.category.as_str()).collect();

        assert!(
            categories.contains(&"argument_thesis"),
            "Should detect thesis"
        );
        assert!(
            categories.contains(&"counter_arguments"),
            "Should detect 'however' as counter-argument"
        );
        assert!(
            categories.contains(&"vocabulary_sophistication"),
            "Should detect academic vocab (furthermore, implications, demonstrate, considerable)"
        );
        assert!(
            categories.contains(&"personal_voice"),
            "Should detect 'I believe'"
        );
        assert!(
            categories.contains(&"rhetorical_devices"),
            "Should detect rhetorical question"
        );
        assert!(
            categories.contains(&"sentence_variety"),
            "Should detect subordinate clause with 'because'"
        );
        assert!(
            categories.contains(&"cohesion_devices"),
            "Should detect 'In conclusion'"
        );
    }

    #[test]
    fn test_quick_extract_tender() {
        let text = "We have delivered 15 similar projects. \
                     Dr Smith (Lead Consultant) will manage the engagement. \
                     In accordance with section 3.2, our approach is fully compliant. \
                     We achieved a 30% reduction in processing time.";

        let items = quick_adaptive_extract(text, "evaluate this tender proposal");

        let categories: Vec<&str> = items.iter().map(|i| i.category.as_str()).collect();
        assert!(categories.contains(&"claims"));
        assert!(categories.contains(&"named_staff"));
        assert!(categories.contains(&"compliance_refs"));
        assert!(categories.contains(&"quantified_outcomes"));
    }

    #[test]
    fn test_evidence_summary() {
        let items = vec![
            AdaptiveEvidence::new(
                "argument_thesis",
                "Climate change is serious.".to_string(),
                0.6,
                0.9,
            ),
            AdaptiveEvidence::new(
                "counter_arguments",
                "However, costs are high.".to_string(),
                0.7,
                0.8,
            ),
            AdaptiveEvidence::new(
                "counter_arguments",
                "Although some disagree.".to_string(),
                0.5,
                0.7,
            ),
        ];

        let summary = evidence_summary(&items, "mark this essay");

        // Should mention category counts
        assert!(
            summary.contains("1 found"),
            "Should show count for thesis: {}",
            summary
        );
        assert!(
            summary.contains("2 found"),
            "Should show count for counter_arguments: {}",
            summary
        );
        // Should show best example
        assert!(
            summary.contains("e.g."),
            "Should include example: {}",
            summary
        );
        // Should show total
        assert!(
            summary.contains("Total evidence items: 3"),
            "Should show total: {}",
            summary
        );
        // Should note NONE for missing categories
        assert!(
            summary.contains("NONE"),
            "Should note missing categories: {}",
            summary
        );
    }

    #[test]
    fn test_to_pipeline_types() {
        let items = vec![
            AdaptiveEvidence::new(
                "argument_thesis",
                "Main argument here.".to_string(),
                0.7,
                0.9,
            ),
            AdaptiveEvidence::new(
                "counter_arguments",
                "However, there are problems.".to_string(),
                0.6,
                0.8,
            ),
            AdaptiveEvidence::new(
                "vocabulary_sophistication",
                "Furthermore, the implications are considerable.".to_string(),
                0.8,
                0.7,
            ),
        ];

        let (claims, evidence) = to_pipeline_types(&items);

        assert!(
            !claims.is_empty(),
            "Should produce at least one Claim from argument_thesis"
        );
        assert!(
            !evidence.is_empty(),
            "Should produce Evidence from counter_arguments and vocabulary"
        );

        // Thesis should become a claim
        assert!(claims.iter().any(|c| c.text.contains("Main argument")));
        // Counter-argument should become evidence
        assert!(evidence.iter().any(|e| e.text.contains("However")));
    }

    #[test]
    fn test_adaptive_extraction_prompt() {
        let prompt = adaptive_extraction_prompt("Some text here.", "grade this essay");
        assert!(prompt.contains("essay"));
        assert!(prompt.contains("argument_thesis"));
        assert!(prompt.contains("JSON array"));
        assert!(prompt.contains("Some text here"));
    }

    #[test]
    fn test_empty_text() {
        let items = quick_adaptive_extract("", "essay");
        assert!(items.is_empty());
    }

    #[test]
    fn test_intent_domain_detection() {
        assert_eq!(intent_domain("grade this essay"), "essay");
        assert_eq!(intent_domain("evaluate this tender bid"), "tender");
        assert_eq!(intent_domain("review this policy document"), "policy");
        assert_eq!(intent_domain("analyse this contract"), "contract");
        assert_eq!(intent_domain("mark this writing"), "essay");
        assert_eq!(intent_domain("check the proposal"), "tender");
    }
}
