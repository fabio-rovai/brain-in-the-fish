//! Document validation — deterministic fact-checking and quality signals.
//!
//! Extracts verifiable facts from the document and validates them:
//! - Citation format and recency
//! - Word count compliance against rubric requirements
//! - Internal consistency (numbers cited in different sections)
//! - Structure compliance (required sections present)
//! - Reading level (Flesch-Kincaid)
//! - Duplicate content detection
//!
//! Each check produces a `ValidationSignal` that feeds into the SNN
//! as a spike (positive) or anti-spike (negative/penalty).

use crate::types::*;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// A validation finding — positive or negative signal for the SNN.
#[derive(Debug, Clone, Serialize)]
pub struct ValidationSignal {
    pub id: String,
    pub signal_type: SignalType,
    pub severity: Severity,
    pub section_id: Option<String>,
    pub criterion_id: Option<String>,
    pub title: String,
    pub description: String,
    /// Positive = boosts SNN, Negative = reduces SNN, Neutral = informational
    pub spike_effect: f64, // -1.0 to +1.0
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum SignalType {
    CitationRecency,
    CitationFormat,
    WordCount,
    NumberConsistency,
    StructureCompliance,
    ReadingLevel,
    DuplicateContent,
    EvidenceQuality,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Run all validation checks on a document.
pub fn validate_document(
    doc: &EvalDocument,
    framework: &EvaluationFramework,
) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();

    signals.extend(check_citations(doc));
    signals.extend(check_word_count(doc, framework));
    signals.extend(check_number_consistency(doc));
    signals.extend(check_structure_compliance(doc, framework));
    signals.extend(check_reading_level(doc));
    signals.extend(check_duplicate_content(doc));
    signals.extend(check_evidence_quality(doc));

    signals
}

// ============================================================================
// Citation checks
// ============================================================================

/// Check citation recency and format.
/// Extracts years from parenthetical references and flags old/malformed citations.
fn check_citations(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();
    let current_year = 2026u32; // hardcoded for determinism

    for section in all_sections(&doc.sections) {
        let citations = extract_citations(&section.text);

        if citations.is_empty() && section.word_count > 100 {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::CitationFormat,
                severity: Severity::Warning,
                section_id: Some(section.id.clone()),
                criterion_id: None,
                title: format!("No citations in '{}'", section.title),
                description: format!(
                    "Section '{}' has {} words but no citations. \
                     Academic and policy documents typically require sourced claims.",
                    section.title, section.word_count
                ),
                spike_effect: -0.2,
            });
        }

        for (author, year) in &citations {
            let age = current_year.saturating_sub(*year);

            if age > 10 {
                signals.push(ValidationSignal {
                    id: uuid::Uuid::new_v4().to_string(),
                    signal_type: SignalType::CitationRecency,
                    severity: Severity::Warning,
                    section_id: Some(section.id.clone()),
                    criterion_id: None,
                    title: format!("Outdated source: {} ({})", author, year),
                    description: format!(
                        "Citation '{} ({})' is {} years old. \
                         Consider whether more recent evidence is available.",
                        author, year, age
                    ),
                    spike_effect: -0.1,
                });
            } else if age <= 3 {
                signals.push(ValidationSignal {
                    id: uuid::Uuid::new_v4().to_string(),
                    signal_type: SignalType::CitationRecency,
                    severity: Severity::Info,
                    section_id: Some(section.id.clone()),
                    criterion_id: None,
                    title: format!("Recent source: {} ({})", author, year),
                    description: format!(
                        "Citation '{} ({})' is recent ({} years old). \
                         This strengthens the evidence base.",
                        author, year, age
                    ),
                    spike_effect: 0.1,
                });
            }
        }
    }

    signals
}

/// Extract citations as (author, year) pairs from text.
/// Matches patterns like: (Author, 2023), (Author et al., 2021), Author (2020)
fn extract_citations(text: &str) -> Vec<(String, u32)> {
    let mut citations = Vec::new();

    // Pattern 1: (Author, YYYY) or (Author et al., YYYY)
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    while i < chars.len() {
        if chars[i] == '('
            && let Some(close) = chars[i..].iter().position(|&c| c == ')')
        {
            let inner: String = chars[i + 1..i + close].iter().collect();
            if let Some(year) = extract_year(&inner) {
                let author = inner.split(',').next().unwrap_or("").trim().to_string();
                if !author.is_empty() && author.len() < 50 {
                    citations.push((author, year));
                }
            }
            i += close + 1;
            continue;
        }
        i += 1;
    }

    citations
}

fn extract_year(text: &str) -> Option<u32> {
    // Find a 4-digit number that looks like a year (1900-2030)
    for word in text.split(|c: char| !c.is_ascii_digit()) {
        if word.len() == 4
            && let Ok(year) = word.parse::<u32>()
            && (1900..=2030).contains(&year)
        {
            return Some(year);
        }
    }
    None
}

// ============================================================================
// Word count compliance
// ============================================================================

/// Check if document word count meets expected range for its type.
fn check_word_count(
    doc: &EvalDocument,
    _framework: &EvaluationFramework,
) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();
    let total = doc.total_word_count.unwrap_or(0);

    // Expected ranges by document type
    let (min_words, max_words, doc_type_label) = match doc.doc_type.as_str() {
        "essay" => (500, 5000, "essay"),
        "bid" | "tender" => (1000, 50000, "tender bid"),
        "policy" => (2000, 30000, "policy document"),
        "survey" | "research" => (3000, 20000, "research report"),
        "contract" | "legal" => (500, 100000, "legal document"),
        _ => (100, 100000, "document"),
    };

    if total < min_words {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::WordCount,
            severity: Severity::Warning,
            section_id: None,
            criterion_id: None,
            title: format!("Document may be too short ({} words)", total),
            description: format!(
                "This {} has {} words. Typical {} documents are {}-{} words. \
                 Short documents may lack sufficient depth.",
                doc_type_label, total, doc_type_label, min_words, max_words
            ),
            spike_effect: -0.15,
        });
    }

    // Suppress unused variable warning
    let _ = max_words;

    // Check for very unbalanced sections
    let sections: Vec<&Section> = doc.sections.iter().collect();
    if sections.len() >= 2 {
        let max_section = sections.iter().map(|s| s.word_count).max().unwrap_or(0);
        let min_section = sections.iter().map(|s| s.word_count).min().unwrap_or(0);
        if min_section > 0 && max_section > min_section * 5 {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::WordCount,
                severity: Severity::Info,
                section_id: None,
                criterion_id: None,
                title: "Unbalanced section lengths".into(),
                description: format!(
                    "Longest section has {} words, shortest has {}. \
                     Significant imbalance may indicate uneven treatment of topics.",
                    max_section, min_section
                ),
                spike_effect: -0.05,
            });
        }
    }

    signals
}

// ============================================================================
// Number consistency
// ============================================================================

/// Check if numbers cited in different sections are consistent.
fn check_number_consistency(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();

    // Extract all numbers with context from each section
    let mut number_contexts: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for section in all_sections(&doc.sections) {
        for number in extract_significant_numbers(&section.text) {
            number_contexts
                .entry(number.value.clone())
                .or_default()
                .push((section.id.clone(), number.context.clone()));
        }
    }

    // Look for similar but not identical numbers (potential inconsistencies)
    let all_numbers: Vec<&str> = number_contexts.keys().map(|s| s.as_str()).collect();
    for i in 0..all_numbers.len() {
        for j in (i + 1)..all_numbers.len() {
            if numbers_suspiciously_similar(all_numbers[i], all_numbers[j]) {
                let contexts_a = &number_contexts[all_numbers[i]];
                let contexts_b = &number_contexts[all_numbers[j]];

                // Only flag if they appear in different sections (same section is fine)
                let sections_a: HashSet<&str> =
                    contexts_a.iter().map(|(s, _)| s.as_str()).collect();
                let sections_b: HashSet<&str> =
                    contexts_b.iter().map(|(s, _)| s.as_str()).collect();

                if sections_a.intersection(&sections_b).next().is_none() {
                    signals.push(ValidationSignal {
                        id: uuid::Uuid::new_v4().to_string(),
                        signal_type: SignalType::NumberConsistency,
                        severity: Severity::Warning,
                        section_id: None,
                        criterion_id: None,
                        title: format!(
                            "Possible number inconsistency: {} vs {}",
                            all_numbers[i], all_numbers[j]
                        ),
                        description: format!(
                            "'{}' appears in context: '{}'. '{}' appears in context: '{}'. \
                             These numbers are similar but different — verify they refer to different things.",
                            all_numbers[i], contexts_a[0].1,
                            all_numbers[j], contexts_b[0].1,
                        ),
                        spike_effect: -0.15,
                    });
                }
            }
        }
    }

    signals
}

struct NumberInContext {
    value: String,
    context: String,
}

fn extract_significant_numbers(text: &str) -> Vec<NumberInContext> {
    let mut numbers = Vec::new();
    let words: Vec<&str> = text.split_whitespace().collect();

    for (i, word) in words.iter().enumerate() {
        // Match: 45M, 340%, 1,200, 45%, 895 billion, etc.
        let cleaned = word.trim_matches(|c: char| {
            !c.is_ascii_digit() && c != '.' && c != ',' && c != '%' && c != '\u{00a3}' && c != '$'
        });
        if cleaned.len() >= 2 && cleaned.chars().any(|c| c.is_ascii_digit()) {
            // Get surrounding context (5 words each side)
            let start = i.saturating_sub(5);
            let end = (i + 6).min(words.len());
            let context: String = words[start..end].join(" ");
            numbers.push(NumberInContext {
                value: cleaned.to_string(),
                context,
            });
        }
    }

    numbers
}

fn numbers_suspiciously_similar(a: &str, b: &str) -> bool {
    // Only compare numbers with the SAME unit type
    let unit_a = number_unit(a);
    let unit_b = number_unit(b);
    if unit_a != unit_b {
        return false; // £45M vs 50% = different units, not suspicious
    }

    let clean_a: String = a
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let clean_b: String = b
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    if clean_a == clean_b || clean_a.is_empty() || clean_b.is_empty() {
        return false;
    }

    if let (Ok(va), Ok(vb)) = (clean_a.parse::<f64>(), clean_b.parse::<f64>()) {
        // Skip small numbers (< 10) — too many false positives
        if va < 10.0 || vb < 10.0 || va == 0.0 || vb == 0.0 {
            return false;
        }
        let ratio = (va / vb).max(vb / va);
        ratio > 1.0 && ratio < 1.15 // Tightened from 20% to 15%
    } else {
        false
    }
}

/// Classify the unit type of a number string.
fn number_unit(s: &str) -> &'static str {
    if s.contains('%') { "percent" }
    else if s.contains('£') || s.contains('$') || s.contains('€') { "currency" }
    else if s.to_lowercase().contains("billion") || s.to_lowercase().contains("million") { "currency_large" }
    else if s.contains('-') && s.len() < 8 { "range" } // e.g. "1-6"
    else { "plain" }
}

// ============================================================================
// Structure compliance
// ============================================================================

/// Check if document has expected sections for its type.
fn check_structure_compliance(
    doc: &EvalDocument,
    _framework: &EvaluationFramework,
) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();
    let section_titles: Vec<String> = doc.sections.iter().map(|s| s.title.to_lowercase()).collect();

    let expected_sections: Vec<(&str, &str)> = match doc.doc_type.as_str() {
        "essay" => vec![
            (
                "introduction",
                "An introduction sets context and states the thesis",
            ),
            ("conclusion", "A conclusion synthesises the argument"),
        ],
        "policy" => vec![
            (
                "rationale",
                "A rationale section explains why the policy is needed",
            ),
            ("objective", "Clear objectives should be stated"),
            ("option", "Options appraisal should consider alternatives"),
            (
                "implementation",
                "An implementation plan shows how the policy will be delivered",
            ),
        ],
        "bid" | "tender" => vec![
            (
                "approach",
                "A technical approach section describes the methodology",
            ),
            (
                "experience",
                "Evidence of relevant experience should be provided",
            ),
            (
                "team",
                "Team composition and expertise should be described",
            ),
        ],
        "research" | "survey" => vec![
            (
                "method",
                "A methodology section describes the research approach",
            ),
            ("result", "Results should be clearly presented"),
            (
                "discussion",
                "A discussion section interprets the findings",
            ),
        ],
        _ => vec![],
    };

    for (keyword, explanation) in &expected_sections {
        let found = section_titles.iter().any(|t| t.contains(keyword));
        if !found {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::StructureCompliance,
                severity: Severity::Warning,
                section_id: None,
                criterion_id: None,
                title: format!("Missing expected section: '{}'", keyword),
                description: explanation.to_string(),
                spike_effect: -0.1,
            });
        }
    }

    // Check for introduction and conclusion specifically
    let has_intro = section_titles
        .iter()
        .any(|t| t.contains("intro") || t.contains("summary") || t.contains("overview"));
    let has_conclusion = section_titles.iter().any(|t| {
        t.contains("conclu") || t.contains("summary") || t.contains("recommendation")
    });

    if has_intro && has_conclusion {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::StructureCompliance,
            severity: Severity::Info,
            section_id: None,
            criterion_id: None,
            title: "Document has introduction and conclusion".into(),
            description:
                "The document follows a complete structure with opening and closing sections."
                    .into(),
            spike_effect: 0.1,
        });
    }

    signals
}

// ============================================================================
// Reading level
// ============================================================================

/// Compute Flesch-Kincaid reading level and check appropriateness.
fn check_reading_level(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();
    let full_text: String = all_sections(&doc.sections)
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    if full_text.split_whitespace().count() < 50 {
        return signals; // Too short to measure
    }

    let fk = flesch_kincaid_grade(&full_text);

    let (expected_min, expected_max, audience) = match doc.doc_type.as_str() {
        "essay" => (10.0, 16.0, "academic"),
        "policy" => (12.0, 18.0, "professional/policy"),
        "bid" | "tender" => (10.0, 16.0, "professional"),
        "contract" | "legal" => (14.0, 20.0, "legal"),
        _ => (8.0, 18.0, "general"),
    };

    if fk < expected_min {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::ReadingLevel,
            severity: Severity::Info,
            section_id: None,
            criterion_id: None,
            title: format!("Reading level may be too simple (grade {:.1})", fk),
            description: format!(
                "Flesch-Kincaid grade level {:.1}. {} documents typically require \
                 grade {:.0}-{:.0}. The writing may lack the complexity expected \
                 for this audience.",
                fk, audience, expected_min, expected_max
            ),
            spike_effect: -0.05,
        });
    } else if fk > expected_max {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::ReadingLevel,
            severity: Severity::Info,
            section_id: None,
            criterion_id: None,
            title: format!("Reading level is very high (grade {:.1})", fk),
            description: format!(
                "Flesch-Kincaid grade level {:.1}. This is above the typical range \
                 for {} documents (grade {:.0}-{:.0}). Consider whether the language \
                 is accessible to the intended audience.",
                fk, audience, expected_min, expected_max
            ),
            spike_effect: 0.0, // Not necessarily negative
        });
    } else {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::ReadingLevel,
            severity: Severity::Info,
            section_id: None,
            criterion_id: None,
            title: format!("Reading level appropriate (grade {:.1})", fk),
            description: format!(
                "Flesch-Kincaid grade level {:.1} is within the expected range \
                 ({:.0}-{:.0}) for {} documents.",
                fk, expected_min, expected_max, audience
            ),
            spike_effect: 0.05,
        });
    }

    signals
}

/// Compute Flesch-Kincaid Grade Level.
/// FK = 0.39 * (words/sentences) + 11.8 * (syllables/words) - 15.59
fn flesch_kincaid_grade(text: &str) -> f64 {
    let words: Vec<&str> = text.split_whitespace().filter(|w| !w.is_empty()).collect();
    let word_count = words.len() as f64;
    if word_count == 0.0 {
        return 0.0;
    }

    let sentence_count = text
        .chars()
        .filter(|c| *c == '.' || *c == '!' || *c == '?')
        .count()
        .max(1) as f64;

    let syllable_count: f64 = words.iter().map(|w| count_syllables(w) as f64).sum();

    0.39 * (word_count / sentence_count) + 11.8 * (syllable_count / word_count) - 15.59
}

/// Approximate syllable count for a word.
fn count_syllables(word: &str) -> u32 {
    let w = word.to_lowercase();
    let w = w.trim_matches(|c: char| !c.is_alphabetic());
    if w.is_empty() {
        return 1;
    }

    let mut count = 0u32;
    let mut prev_vowel = false;
    let vowels = "aeiouy";

    for ch in w.chars() {
        let is_vowel = vowels.contains(ch);
        if is_vowel && !prev_vowel {
            count += 1;
        }
        prev_vowel = is_vowel;
    }

    // Adjust for silent e
    if w.ends_with('e') && count > 1 {
        count -= 1;
    }

    count.max(1)
}

// ============================================================================
// Duplicate content
// ============================================================================

/// Check for duplicate or near-duplicate paragraphs within the document.
fn check_duplicate_content(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();
    let sections = all_sections(&doc.sections);

    // Hash paragraphs (first 60 chars normalised) to detect duplicates
    let mut seen: HashMap<String, String> = HashMap::new(); // hash -> section_id

    for section in &sections {
        let paragraphs: Vec<&str> = section
            .text
            .split("\n\n")
            .map(|p| p.trim())
            .filter(|p| p.len() > 80) // only check substantial paragraphs
            .collect();

        for para in paragraphs {
            let normalised: String = para
                .chars()
                .filter(|c| c.is_alphanumeric() || c.is_whitespace())
                .collect::<String>()
                .to_lowercase();
            let hash = if normalised.len() >= 60 {
                normalised[..60].to_string()
            } else {
                normalised
            };

            if let Some(prev_section) = seen.get(&hash) {
                if prev_section != &section.id {
                    signals.push(ValidationSignal {
                        id: uuid::Uuid::new_v4().to_string(),
                        signal_type: SignalType::DuplicateContent,
                        severity: Severity::Warning,
                        section_id: Some(section.id.clone()),
                        criterion_id: None,
                        title: "Duplicate paragraph detected".into(),
                        description: format!(
                            "Content in '{}' appears to be duplicated from another section. \
                             Repeated content does not add to the evaluation.",
                            section.title
                        ),
                        spike_effect: -0.2,
                    });
                }
            } else {
                seen.insert(hash, section.id.clone());
            }
        }
    }

    signals
}

// ============================================================================
// Evidence quality
// ============================================================================

/// Assess overall evidence quality metrics.
fn check_evidence_quality(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();
    let sections = all_sections(&doc.sections);

    let total_claims: usize = sections.iter().map(|s| s.claims.len()).sum();
    let total_evidence: usize = sections.iter().map(|s| s.evidence.len()).sum();
    let quantified_evidence: usize = sections
        .iter()
        .flat_map(|s| s.evidence.iter())
        .filter(|e| e.has_quantified_outcome)
        .count();

    // Evidence-to-claim ratio
    if total_claims > 0 {
        let ratio = total_evidence as f64 / total_claims as f64;
        if ratio < 0.5 {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::EvidenceQuality,
                severity: Severity::Warning,
                section_id: None,
                criterion_id: None,
                title: format!("Low evidence-to-claim ratio ({:.1}:1)", ratio),
                description: format!(
                    "{} claims but only {} evidence items. Many claims are unsupported. \
                     Aim for at least 1 evidence item per claim.",
                    total_claims, total_evidence
                ),
                spike_effect: -0.15,
            });
        } else if ratio >= 1.0 {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::EvidenceQuality,
                severity: Severity::Info,
                section_id: None,
                criterion_id: None,
                title: format!("Strong evidence-to-claim ratio ({:.1}:1)", ratio),
                description: format!(
                    "{} claims supported by {} evidence items. Good evidence base.",
                    total_claims, total_evidence
                ),
                spike_effect: 0.1,
            });
        }
    }

    // Quantified evidence proportion
    if total_evidence > 0 {
        let quant_pct = quantified_evidence as f64 / total_evidence as f64 * 100.0;
        if quant_pct >= 50.0 {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::EvidenceQuality,
                severity: Severity::Info,
                section_id: None,
                criterion_id: None,
                title: format!("{:.0}% of evidence is quantified", quant_pct),
                description: format!(
                    "{} of {} evidence items include quantified outcomes \
                     (numbers, percentages, monetary values). \
                     This strengthens the empirical basis.",
                    quantified_evidence, total_evidence
                ),
                spike_effect: 0.1,
            });
        }
    }

    signals
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

/// Convert validation signals to Turtle for the knowledge graph.
pub fn signals_to_turtle(signals: &[ValidationSignal]) -> String {
    use crate::ingest::{iri_safe, turtle_escape};

    let mut turtle = String::from(
        "@prefix val: <http://brain-in-the-fish.dev/validation/> .\n\
         @prefix eval: <http://brain-in-the-fish.dev/eval/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n",
    );

    for signal in signals {
        let sid = iri_safe(&signal.id);
        turtle.push_str(&format!(
            "val:{} a eval:ValidationSignal ;\n\
             \teval:title \"{}\" ;\n\
             \teval:description \"{}\" ;\n\
             \teval:severity \"{:?}\" ;\n\
             \teval:spikeEffect \"{}\"^^xsd:decimal .\n\n",
            sid,
            turtle_escape(&signal.title),
            turtle_escape(&signal.description),
            signal.severity,
            signal.spike_effect,
        ));
    }

    turtle
}

/// Load validation signals into the graph store.
pub fn load_signals(
    graph: &open_ontologies::graph::GraphStore,
    signals: &[ValidationSignal],
) -> anyhow::Result<usize> {
    if signals.is_empty() {
        return Ok(0);
    }
    let turtle = signals_to_turtle(signals);
    graph.load_turtle(&turtle, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_citations() {
        let text = "According to (Bernanke, 2009), QE was effective. \
                    (Joyce et al., 2012) found similar results. \
                    (Kapetanios et al., 2012) disagreed.";
        let citations = extract_citations(text);
        assert!(
            citations.len() >= 2,
            "Should find citations, got {}",
            citations.len()
        );
        assert!(citations
            .iter()
            .any(|(a, y)| a.contains("Bernanke") && *y == 2009));
    }

    #[test]
    fn test_extract_year() {
        assert_eq!(extract_year("Bernanke, 2009"), Some(2009));
        assert_eq!(extract_year("no year here"), None);
        assert_eq!(extract_year("2012"), Some(2012));
    }

    #[test]
    fn test_flesch_kincaid() {
        let simple = "The cat sat on the mat. The dog ran to the park.";
        let complex = "The implementation of quantitative easing programmes \
                       necessitated unprecedented monetary policy interventions \
                       across multiple jurisdictions.";
        let fk_simple = flesch_kincaid_grade(simple);
        let fk_complex = flesch_kincaid_grade(complex);
        assert!(
            fk_simple < fk_complex,
            "Complex text should have higher grade level: simple={:.1} complex={:.1}",
            fk_simple,
            fk_complex
        );
    }

    #[test]
    fn test_count_syllables() {
        assert_eq!(count_syllables("the"), 1);
        assert_eq!(count_syllables("implementation"), 5);
        assert_eq!(count_syllables("quantitative"), 4);
    }

    #[test]
    fn test_numbers_suspiciously_similar() {
        assert!(numbers_suspiciously_similar("\u{00a3}45M", "\u{00a3}40M")); // 45 vs 40 = 12.5% diff
        assert!(!numbers_suspiciously_similar("\u{00a3}45M", "\u{00a3}45M")); // identical
        assert!(!numbers_suspiciously_similar("\u{00a3}45M", "\u{00a3}100M")); // too different
        assert!(!numbers_suspiciously_similar("340%", "45%")); // very different
    }

    #[test]
    fn test_validate_essay() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test Essay".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(2000),
            sections: vec![
                Section {
                    id: "s1".into(),
                    title: "Introduction".into(),
                    text: "This essay examines the impact of QE (Bernanke, 2009). \
                           The evidence shows significant effects (Joyce et al., 2012)."
                        .into(),
                    word_count: 200,
                    page_range: None,
                    claims: vec![Claim {
                        id: "c1".into(),
                        text: "QE had impact".into(),
                        specificity: 0.7,
                        verifiable: true,
                    }],
                    evidence: vec![Evidence {
                        id: "e1".into(),
                        source: "Bernanke 2009".into(),
                        evidence_type: "citation".into(),
                        text: "cited".into(),
                        has_quantified_outcome: false,
                    }],
                    subsections: vec![],
                },
                Section {
                    id: "s2".into(),
                    title: "Conclusion".into(),
                    text: "In conclusion, QE was effective but limited.".into(),
                    word_count: 50,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                },
            ],
        };
        let framework = crate::criteria::academic_essay_framework();
        let signals = validate_document(&doc, &framework);
        assert!(!signals.is_empty(), "Should produce validation signals");
        // Should find the citation
        assert!(signals
            .iter()
            .any(|s| s.signal_type == SignalType::CitationRecency));
    }

    #[test]
    fn test_structure_compliance_essay() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(1000),
            sections: vec![
                Section {
                    id: "s1".into(),
                    title: "Introduction".into(),
                    text: "Intro text.".into(),
                    word_count: 100,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                },
                Section {
                    id: "s2".into(),
                    title: "Analysis".into(),
                    text: "Analysis text.".into(),
                    word_count: 400,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                },
                Section {
                    id: "s3".into(),
                    title: "Conclusion".into(),
                    text: "Conclusion text.".into(),
                    word_count: 100,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                },
            ],
        };
        let framework = crate::criteria::academic_essay_framework();
        let signals = validate_document(&doc, &framework);
        // Should find "has introduction and conclusion"
        assert!(signals
            .iter()
            .any(|s| s.title.contains("introduction and conclusion")));
    }

    #[test]
    fn test_signals_to_turtle() {
        let signals = vec![ValidationSignal {
            id: "sig-1".into(),
            signal_type: SignalType::CitationRecency,
            severity: Severity::Warning,
            section_id: None,
            criterion_id: None,
            title: "Outdated source".into(),
            description: "Old citation".into(),
            spike_effect: -0.1,
        }];
        let turtle = signals_to_turtle(&signals);
        assert!(turtle.contains("eval:ValidationSignal"));
        assert!(turtle.contains("Outdated source"));
    }

    #[test]
    fn test_load_signals() {
        let graph = open_ontologies::graph::GraphStore::new();
        let signals = vec![ValidationSignal {
            id: "sig-1".into(),
            signal_type: SignalType::CitationRecency,
            severity: Severity::Warning,
            section_id: None,
            criterion_id: None,
            title: "Test signal".into(),
            description: "Test".into(),
            spike_effect: -0.1,
        }];
        let triples = load_signals(&graph, &signals).unwrap();
        assert!(triples > 0);
    }
}
