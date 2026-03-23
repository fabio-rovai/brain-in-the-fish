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
    LogicalFallacy,
    HedgingBalance,
    TopicSentence,
    CounterArgument,
    TransitionQuality,
    Specificity,
    ReferencingConsistency,
    ArgumentFlow,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Core validation — 8 checks that are always useful.
pub fn validate_core(
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
    signals.extend(check_referencing_consistency(doc));

    signals
}

/// Deep validation — adds 7 more checks (some are noisy, all are expensive).
pub fn validate_deep(
    doc: &EvalDocument,
    framework: &EvaluationFramework,
) -> Vec<ValidationSignal> {
    let mut signals = validate_core(doc, framework);

    signals.extend(check_logical_fallacies(doc));
    signals.extend(check_hedging_language(doc));
    signals.extend(check_topic_sentences(doc));
    signals.extend(check_counter_arguments(doc));
    signals.extend(check_transition_quality(doc));
    signals.extend(check_specificity(doc));
    signals.extend(check_argument_flow(doc));

    signals
}

/// Run all validation checks on a document (alias for validate_deep, backward compat).
pub fn validate_document(
    doc: &EvalDocument,
    framework: &EvaluationFramework,
) -> Vec<ValidationSignal> {
    validate_deep(doc, framework)
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
                spike_effect: -0.05,
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
                    spike_effect: -0.05,
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
            spike_effect: -0.05,
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
                        spike_effect: -0.05,
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
                spike_effect: -0.05,
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
                        spike_effect: -0.05,
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
                spike_effect: -0.05,
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
// Logical fallacies
// ============================================================================

/// Detect common logical fallacies in the text.
fn check_logical_fallacies(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();

    let fallacy_patterns: Vec<(&str, &[&str], &str)> = vec![
        (
            "Ad hominem",
            &["they are wrong because", "critics fail to", "opponents are"],
            "Attacking the person rather than the argument weakens the reasoning.",
        ),
        (
            "Appeal to authority",
            &["experts agree", "studies show", "it is well known", "everyone knows"],
            "Unnamed authority claims lack verifiability. Name the source.",
        ),
        (
            "False dichotomy",
            &["either we", "the only option", "we must choose between", "there are only two"],
            "Presenting only two options ignores alternatives.",
        ),
        (
            "Slippery slope",
            &["inevitably", "will lead to", "slippery slope", "domino effect", "if we allow"],
            "Asserting an inevitable chain of consequences without evidence.",
        ),
        (
            "Hasty generalisation",
            &["all of them", "every single", "always without exception", "never once"],
            "Absolute claims from limited evidence are rarely defensible.",
        ),
        (
            "Straw man",
            &["some people foolishly claim", "opponents naively believe"],
            "Misrepresenting an opposing view weakens the argument.",
        ),
        (
            "Circular reasoning",
            &["this is true because it is true", "as we already proved"],
            "The conclusion restates the premise without new evidence.",
        ),
    ];

    for section in all_sections(&doc.sections) {
        let lower = section.text.to_lowercase();
        for (fallacy_name, markers, explanation) in &fallacy_patterns {
            for marker in *markers {
                if lower.contains(marker) {
                    signals.push(ValidationSignal {
                        id: uuid::Uuid::new_v4().to_string(),
                        signal_type: SignalType::LogicalFallacy,
                        severity: Severity::Warning,
                        section_id: Some(section.id.clone()),
                        criterion_id: None,
                        title: format!("Possible {}: '{}'", fallacy_name, marker),
                        description: format!(
                            "Section '{}' contains '{}'. {}",
                            section.title, marker, explanation
                        ),
                        spike_effect: -0.15,
                    });
                }
            }
        }
    }

    signals
}

// ============================================================================
// Hedging language balance
// ============================================================================

/// Detect over-hedging or under-hedging in the document.
fn check_hedging_language(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();

    let hedge_words: &[&str] = &[
        "might", "could", "perhaps", "possibly", "seems", "appears",
        "suggests", "may", "arguably", "potentially", "likely",
    ];
    let strong_words: &[&str] = &[
        "clearly", "obviously", "undoubtedly", "certainly",
        "proves", "demonstrates conclusively", "without question",
        "undeniably", "irrefutably",
    ];

    for section in all_sections(&doc.sections) {
        let words: Vec<&str> = section.text.split_whitespace().collect();
        let word_count = words.len();
        if word_count < 30 {
            continue;
        }

        let lower = section.text.to_lowercase();
        let hedge_count = hedge_words
            .iter()
            .map(|h| lower.matches(h).count())
            .sum::<usize>();
        let strong_count = strong_words
            .iter()
            .map(|s| lower.matches(s).count())
            .sum::<usize>();

        let hedge_pct = hedge_count as f64 / word_count as f64 * 100.0;
        let strong_pct = strong_count as f64 / word_count as f64 * 100.0;

        if hedge_pct > 8.0 {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::HedgingBalance,
                severity: Severity::Warning,
                section_id: Some(section.id.clone()),
                criterion_id: None,
                title: format!("Over-hedged section ({:.1}% hedging)", hedge_pct),
                description: format!(
                    "Section '{}' has {:.1}% hedging words. Excessive hedging weakens \
                     the argument. Ideal range is 3-8%.",
                    section.title, hedge_pct
                ),
                spike_effect: -0.1,
            });
        } else if hedge_pct < 1.0 && strong_pct > 3.0 {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::HedgingBalance,
                severity: Severity::Warning,
                section_id: Some(section.id.clone()),
                criterion_id: None,
                title: format!("Under-hedged section ({:.1}% strong assertions)", strong_pct),
                description: format!(
                    "Section '{}' has many strong assertions ({:.1}%) but little hedging ({:.1}%). \
                     This may indicate overclaiming.",
                    section.title, strong_pct, hedge_pct
                ),
                spike_effect: -0.1,
            });
        } else if (3.0..=8.0).contains(&hedge_pct) {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::HedgingBalance,
                severity: Severity::Info,
                section_id: Some(section.id.clone()),
                criterion_id: None,
                title: format!("Well-balanced hedging ({:.1}%)", hedge_pct),
                description: format!(
                    "Section '{}' shows appropriate hedging balance ({:.1}%).",
                    section.title, hedge_pct
                ),
                spike_effect: 0.05,
            });
        }
    }

    signals
}

// ============================================================================
// Topic sentences
// ============================================================================

/// Check paragraph structure: topic sentences and paragraph length.
fn check_topic_sentences(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();

    for section in all_sections(&doc.sections) {
        let paragraphs: Vec<&str> = section
            .text
            .split("\n\n")
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();

        for para in &paragraphs {
            let sentences: Vec<&str> = split_sentences(para);

            // Check paragraph length
            if sentences.len() > 10 {
                signals.push(ValidationSignal {
                    id: uuid::Uuid::new_v4().to_string(),
                    signal_type: SignalType::TopicSentence,
                    severity: Severity::Info,
                    section_id: Some(section.id.clone()),
                    criterion_id: None,
                    title: "Long paragraph (>10 sentences)".into(),
                    description: format!(
                        "A paragraph in '{}' has {} sentences. Consider splitting \
                         for readability.",
                        section.title,
                        sentences.len()
                    ),
                    spike_effect: -0.05,
                });
            }

            // Check topic sentence cohesion (need at least 4 sentences)
            if sentences.len() >= 4 {
                let first_words: HashSet<String> = sentences[0]
                    .split_whitespace()
                    .map(|w| w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
                    .filter(|w| w.len() > 3)
                    .collect();

                if first_words.is_empty() {
                    continue;
                }

                let rest = &sentences[1..];
                let matching = rest
                    .iter()
                    .filter(|s| {
                        let s_words: HashSet<String> = s
                            .split_whitespace()
                            .map(|w| w.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string())
                            .filter(|w| w.len() > 3)
                            .collect();
                        first_words.intersection(&s_words).next().is_some()
                    })
                    .count();

                let overlap_ratio = matching as f64 / rest.len() as f64;
                if overlap_ratio < 0.3 {
                    signals.push(ValidationSignal {
                        id: uuid::Uuid::new_v4().to_string(),
                        signal_type: SignalType::TopicSentence,
                        severity: Severity::Info,
                        section_id: Some(section.id.clone()),
                        criterion_id: None,
                        title: "Weak topic sentence".into(),
                        description: format!(
                            "In '{}', a paragraph's opening sentence shares few keywords \
                             with subsequent sentences ({:.0}% overlap). The topic sentence \
                             may not introduce the paragraph's main idea.",
                            section.title,
                            overlap_ratio * 100.0
                        ),
                        spike_effect: -0.05,
                    });
                }
            }
        }
    }

    signals
}

// ============================================================================
// Counter-arguments
// ============================================================================

/// Detect engagement with opposing views.
fn check_counter_arguments(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();

    let counter_markers: &[&str] = &[
        "however", "on the other hand", "critics argue", "alternatively",
        "despite this", "conversely", "nevertheless", "opponents suggest",
        "a counter-argument", "it could be objected", "some may argue",
        "an alternative view",
    ];

    // Skip non-argumentative document types
    if matches!(doc.doc_type.as_str(), "contract" | "legal") {
        return signals;
    }

    let total_words: u32 = all_sections(&doc.sections).iter().map(|s| s.word_count).sum();
    if total_words < 500 {
        return signals;
    }

    let mut total_counter = 0usize;

    for section in all_sections(&doc.sections) {
        if section.word_count < 200 {
            continue;
        }
        let lower = section.text.to_lowercase();
        let section_counter: usize = counter_markers
            .iter()
            .map(|m| lower.matches(m).count())
            .sum();
        total_counter += section_counter;

        if section_counter == 0 && section.word_count >= 200 {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::CounterArgument,
                severity: Severity::Info,
                section_id: Some(section.id.clone()),
                criterion_id: None,
                title: format!("No counter-argument engagement in '{}'", section.title),
                description: format!(
                    "Section '{}' ({} words) contains no counter-argument markers. \
                     Engaging with opposing views strengthens academic arguments.",
                    section.title, section.word_count
                ),
                spike_effect: -0.05,
            });
        }
    }

    if total_counter == 0 {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::CounterArgument,
            severity: Severity::Warning,
            section_id: None,
            criterion_id: None,
            title: "No counter-argument engagement in entire document".into(),
            description: format!(
                "The document ({} words) contains no counter-argument markers. \
                 One-sided arguments are weaker in academic and policy contexts.",
                total_words
            ),
            spike_effect: -0.15,
        });
    } else if total_counter >= 3 {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::CounterArgument,
            severity: Severity::Info,
            section_id: None,
            criterion_id: None,
            title: format!("Good counter-argument engagement ({} instances)", total_counter),
            description: "The document engages with opposing views, strengthening its argument.".into(),
            spike_effect: 0.1,
        });
    }

    signals
}

// ============================================================================
// Transition quality
// ============================================================================

/// Assess quality of transitions between sections and paragraphs.
fn check_transition_quality(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();

    let transition_words: &[&str] = &[
        "furthermore", "moreover", "additionally", "in addition",
        "however", "nevertheless", "although", "despite",
        "therefore", "consequently", "as a result", "because",
        "firstly", "secondly", "finally", "subsequently",
        "similarly", "likewise", "in contrast", "conversely",
        "building on", "turning to", "having established",
        "this section", "the following", "as discussed",
    ];

    let sections = all_sections(&doc.sections);
    let mut abrupt_count = 0usize;
    let mut smooth_count = 0usize;

    // Check transitions at section boundaries (skip first section)
    for section in sections.iter().skip(1) {
        let first_sentence = split_sentences(&section.text)
            .into_iter()
            .next()
            .unwrap_or("");
        let lower_first = first_sentence.to_lowercase();

        let has_transition = transition_words
            .iter()
            .any(|t| lower_first.contains(t));

        if has_transition {
            smooth_count += 1;
        } else {
            abrupt_count += 1;
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::TransitionQuality,
                severity: Severity::Info,
                section_id: Some(section.id.clone()),
                criterion_id: None,
                title: format!("Abrupt transition into '{}'", section.title),
                description: format!(
                    "Section '{}' begins without transitional language. \
                     Adding a transition improves flow and coherence.",
                    section.title
                ),
                spike_effect: -0.03,
            });
        }
    }

    if sections.len() >= 3 && abrupt_count == 0 && smooth_count >= 2 {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::TransitionQuality,
            severity: Severity::Info,
            section_id: None,
            criterion_id: None,
            title: "All section transitions are smooth".into(),
            description: format!(
                "All {} section boundaries use transitional language. Good coherence.",
                smooth_count
            ),
            spike_effect: 0.05,
        });
    }

    signals
}

// ============================================================================
// Specificity
// ============================================================================

/// Flag vague or generic language.
fn check_specificity(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();

    let vague_markers: &[&str] = &[
        "things", "stuff", "aspects", "various", "a number of",
        "several", "a lot of", "good", "bad", "interesting",
        "important", "significant", "nice", "great",
    ];

    for section in all_sections(&doc.sections) {
        let words: Vec<&str> = section.text.split_whitespace().collect();
        let word_count = words.len();
        if word_count < 20 {
            continue;
        }

        let lower = section.text.to_lowercase();
        let vague_count: usize = vague_markers
            .iter()
            .map(|m| {
                lower
                    .match_indices(m)
                    .filter(|(pos, _)| {
                        let before = if *pos > 0 { lower.as_bytes()[pos - 1] } else { b' ' };
                        let after_pos = pos + m.len();
                        let after = if after_pos < lower.len() { lower.as_bytes()[after_pos] } else { b' ' };
                        !before.is_ascii_alphanumeric() && !after.is_ascii_alphanumeric()
                    })
                    .count()
            })
            .sum();

        let per_100 = vague_count as f64 / word_count as f64 * 100.0;

        if per_100 > 5.0 {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::Specificity,
                severity: Severity::Warning,
                section_id: Some(section.id.clone()),
                criterion_id: None,
                title: format!("High vagueness density in '{}'", section.title),
                description: format!(
                    "Section '{}' has {:.1} vague terms per 100 words ({} in {} words). \
                     Replace generic language with specific details.",
                    section.title, per_100, vague_count, word_count
                ),
                spike_effect: -0.1,
            });
        } else if per_100 < 1.0 && word_count >= 50 {
            signals.push(ValidationSignal {
                id: uuid::Uuid::new_v4().to_string(),
                signal_type: SignalType::Specificity,
                severity: Severity::Info,
                section_id: Some(section.id.clone()),
                criterion_id: None,
                title: format!("Specific language in '{}'", section.title),
                description: format!(
                    "Section '{}' uses precise language with few vague terms ({:.1} per 100 words).",
                    section.title, per_100
                ),
                spike_effect: 0.05,
            });
        }
    }

    signals
}

// ============================================================================
// Referencing consistency
// ============================================================================

/// Check citation style consistency throughout the document.
fn check_referencing_consistency(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();
    let mut harvard_count = 0usize; // (Author, Year)
    let mut numeric_count = 0usize; // [1], [2]
    let mut footnote_count = 0usize; // superscript or footnote markers

    for section in all_sections(&doc.sections) {
        let text = &section.text;

        // Harvard-style: (Author, 2020) or (Author et al., 2020)
        harvard_count += extract_citations(text).len();

        // Numeric-style: [1], [2], [12]
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '['
                && let Some(close) = chars[i..].iter().position(|&c| c == ']')
            {
                let inner: String = chars[i + 1..i + close].iter().collect();
                if inner.trim().parse::<u32>().is_ok() {
                    numeric_count += 1;
                }
                i += close + 1;
                continue;
            }
            i += 1;
        }

        // Footnote markers: detect common patterns like ¹ ² ³ or ^1 ^2
        for ch in text.chars() {
            if "\u{00b9}\u{00b2}\u{00b3}\u{2074}\u{2075}\u{2076}\u{2077}\u{2078}\u{2079}".contains(ch) {
                footnote_count += 1;
            }
        }
        // Also count ^N patterns
        let words: Vec<&str> = text.split_whitespace().collect();
        for w in &words {
            if w.starts_with('^') && w[1..].parse::<u32>().is_ok() {
                footnote_count += 1;
            }
        }
    }

    let styles_used: Vec<(&str, usize)> = vec![
        ("Harvard (Author, Year)", harvard_count),
        ("Numeric [N]", numeric_count),
        ("Footnote", footnote_count),
    ]
    .into_iter()
    .filter(|(_, c)| *c > 0)
    .collect();

    if styles_used.len() > 1 {
        let style_list: Vec<String> = styles_used
            .iter()
            .map(|(name, count)| format!("{}: {} instances", name, count))
            .collect();
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::ReferencingConsistency,
            severity: Severity::Warning,
            section_id: None,
            criterion_id: None,
            title: "Mixed citation styles detected".into(),
            description: format!(
                "Multiple citation styles found: {}. Use one style consistently.",
                style_list.join("; ")
            ),
            spike_effect: -0.05,
        });
    } else if styles_used.len() == 1 && styles_used[0].1 >= 3 {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::ReferencingConsistency,
            severity: Severity::Info,
            section_id: None,
            criterion_id: None,
            title: format!("Consistent {} citation style", styles_used[0].0),
            description: format!(
                "All {} citations use {} style. Good consistency.",
                styles_used[0].1, styles_used[0].0
            ),
            spike_effect: 0.05,
        });
    }

    signals
}

// ============================================================================
// Argument flow
// ============================================================================

/// Detect logical argument progression across sections.
fn check_argument_flow(doc: &EvalDocument) -> Vec<ValidationSignal> {
    let mut signals = Vec::new();
    let sections = &doc.sections;

    if sections.len() < 3 {
        return signals;
    }

    // Identify intro, body, and conclusion by position and title
    let intro = &sections[0];
    let conclusion = &sections[sections.len() - 1];
    let body: Vec<&Section> = sections[1..sections.len() - 1].iter().collect();

    let intro_keywords = extract_content_keywords(&intro.text);
    let conclusion_keywords = extract_content_keywords(&conclusion.text);
    let body_keywords: HashSet<String> = body
        .iter()
        .flat_map(|s| extract_content_keywords(&s.text))
        .collect();

    if intro_keywords.is_empty() || conclusion_keywords.is_empty() {
        return signals;
    }

    // intro → body overlap
    let intro_body_overlap = intro_keywords
        .intersection(&body_keywords)
        .count() as f64
        / intro_keywords.len().max(1) as f64;

    // intro → conclusion overlap
    let intro_conclusion_overlap = intro_keywords
        .intersection(&conclusion_keywords)
        .count() as f64
        / intro_keywords.len().max(1) as f64;

    if intro_body_overlap < 0.2 {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::ArgumentFlow,
            severity: Severity::Warning,
            section_id: None,
            criterion_id: None,
            title: "Weak intro-to-body connection".into(),
            description: format!(
                "Only {:.0}% of introduction keywords appear in the body. \
                 The body may not develop the themes introduced.",
                intro_body_overlap * 100.0
            ),
            spike_effect: -0.1,
        });
    }

    if intro_conclusion_overlap < 0.2 {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::ArgumentFlow,
            severity: Severity::Warning,
            section_id: None,
            criterion_id: None,
            title: "Weak intro-to-conclusion connection".into(),
            description: format!(
                "Only {:.0}% of introduction keywords appear in the conclusion. \
                 The conclusion may not address the original objectives.",
                intro_conclusion_overlap * 100.0
            ),
            spike_effect: -0.1,
        });
    }

    // Check if conclusion introduces new concepts
    let new_in_conclusion: HashSet<&String> = conclusion_keywords
        .difference(&intro_keywords)
        .filter(|k| !body_keywords.contains(*k))
        .collect();
    let new_ratio = new_in_conclusion.len() as f64 / conclusion_keywords.len().max(1) as f64;

    if new_ratio > 0.5 {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::ArgumentFlow,
            severity: Severity::Warning,
            section_id: Some(conclusion.id.clone()),
            criterion_id: None,
            title: "Conclusion introduces new concepts".into(),
            description: format!(
                "{:.0}% of conclusion keywords do not appear in the introduction or body. \
                 A conclusion should synthesise, not introduce new material.",
                new_ratio * 100.0
            ),
            spike_effect: -0.1,
        });
    }

    if intro_body_overlap >= 0.4 && intro_conclusion_overlap >= 0.4 && new_ratio <= 0.3 {
        signals.push(ValidationSignal {
            id: uuid::Uuid::new_v4().to_string(),
            signal_type: SignalType::ArgumentFlow,
            severity: Severity::Info,
            section_id: None,
            criterion_id: None,
            title: "Strong argument flow".into(),
            description: "Introduction, body, and conclusion share consistent themes. \
                         The argument progresses logically."
                .into(),
            spike_effect: 0.1,
        });
    }

    signals
}

/// Extract significant content keywords (nouns/verbs, >4 chars, not stopwords).
fn extract_content_keywords(text: &str) -> HashSet<String> {
    let stopwords: HashSet<&str> = [
        "about", "above", "after", "again", "against", "along", "among",
        "around", "because", "before", "being", "below", "between", "beyond",
        "could", "would", "should", "these", "those", "their", "there",
        "through", "under", "until", "which", "while", "where", "other",
        "another", "every", "further", "having", "itself", "might", "since",
        "still", "than", "that", "them", "then", "this", "very", "what",
        "when", "with", "within", "without", "also", "been", "does", "from",
        "have", "here", "into", "just", "more", "most", "much", "must",
        "only", "over", "same", "some", "such", "will", "your",
    ]
    .iter()
    .copied()
    .collect();

    text.split_whitespace()
        .map(|w| {
            w.to_lowercase()
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|w| w.len() > 4 && !stopwords.contains(w.as_str()))
        .collect()
}

/// Split text into sentences (simple heuristic).
fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if (b == b'.' || b == b'!' || b == b'?')
            && (i + 1 >= bytes.len() || bytes[i + 1] == b' ' || bytes[i + 1] == b'\n')
        {
            let s = text[start..=i].trim();
            if !s.is_empty() {
                sentences.push(s);
            }
            start = i + 1;
        }
    }
    // Remainder
    let remainder = text[start..].trim();
    if !remainder.is_empty() && sentences.is_empty() {
        sentences.push(remainder);
    }
    sentences
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

    // ================================================================
    // Tests for 8 new validation checks
    // ================================================================

    fn make_section(id: &str, title: &str, text: &str, word_count: u32) -> Section {
        Section {
            id: id.into(),
            title: title.into(),
            text: text.into(),
            word_count,
            page_range: None,
            claims: vec![],
            evidence: vec![],
            subsections: vec![],
        }
    }

    #[test]
    fn test_check_logical_fallacies_detects_appeal_to_authority() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![make_section(
                "s1",
                "Body",
                "Studies show that this policy works. It is well known that GDP grows.",
                100,
            )],
        };
        let signals = check_logical_fallacies(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::LogicalFallacy
                && s.title.contains("Appeal to authority")),
            "Should detect appeal to authority, got: {:?}",
            signals.iter().map(|s| &s.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_check_logical_fallacies_no_false_positives() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![make_section(
                "s1",
                "Body",
                "According to Smith (2023), the policy had measurable effects on employment rates.",
                100,
            )],
        };
        let signals = check_logical_fallacies(&doc);
        assert!(
            signals.is_empty(),
            "Clean text should not trigger fallacy detection"
        );
    }

    #[test]
    fn test_check_hedging_over_hedged() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![make_section(
                "s1",
                "Analysis",
                "This might possibly suggest that it could perhaps be the case that \
                 it may potentially seem to appear that the results arguably could \
                 suggest a trend that might possibly exist in the data.",
                40,
            )],
        };
        let signals = check_hedging_language(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::HedgingBalance
                && s.title.contains("Over-hedged")),
            "Should detect over-hedging, got: {:?}",
            signals.iter().map(|s| &s.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_check_topic_sentences_long_paragraph() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![make_section(
                "s1",
                "Body",
                "First sentence. Second sentence. Third sentence. Fourth sentence. \
                 Fifth sentence. Sixth sentence. Seventh sentence. Eighth sentence. \
                 Ninth sentence. Tenth sentence. Eleventh sentence here.",
                100,
            )],
        };
        let signals = check_topic_sentences(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::TopicSentence
                && s.title.contains("Long paragraph")),
            "Should flag long paragraph"
        );
    }

    #[test]
    fn test_check_counter_arguments_absent() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(600),
            sections: vec![make_section(
                "s1",
                "Analysis",
                "The policy is effective. The evidence supports this claim. \
                 Data shows positive outcomes. Results are clear. The approach works. \
                 Implementation was successful. Outcomes exceeded expectations. \
                 Performance metrics improved substantially across all measured dimensions \
                 during the evaluation period under review.",
                600,
            )],
        };
        let signals = check_counter_arguments(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::CounterArgument
                && s.spike_effect < 0.0),
            "Should flag missing counter-arguments"
        );
    }

    #[test]
    fn test_check_counter_arguments_present() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(600),
            sections: vec![make_section(
                "s1",
                "Analysis",
                "The policy appears effective. However, critics argue that the sample \
                 size was small. Nevertheless, the results are statistically significant. \
                 On the other hand, implementation costs were high. Despite this, the \
                 long-term benefits outweigh costs. Furthermore additional data is needed \
                 to draw definitive conclusions from the available evidence base.",
                600,
            )],
        };
        let signals = check_counter_arguments(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::CounterArgument
                && s.spike_effect > 0.0),
            "Should reward counter-argument engagement"
        );
    }

    #[test]
    fn test_check_transition_quality_abrupt() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![
                make_section("s1", "Introduction", "The topic is important.", 50),
                make_section("s2", "Analysis", "Data was collected from participants.", 200),
            ],
        };
        let signals = check_transition_quality(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::TransitionQuality
                && s.title.contains("Abrupt")),
            "Should flag abrupt transition"
        );
    }

    #[test]
    fn test_check_transition_quality_smooth() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![
                make_section("s1", "Introduction", "The topic is important.", 50),
                make_section("s2", "Analysis", "Furthermore, the data reveals trends.", 200),
                make_section("s3", "Conclusion", "Therefore, the evidence supports the thesis.", 100),
            ],
        };
        let signals = check_transition_quality(&doc);
        assert!(
            signals.iter().any(|s| s.title.contains("smooth")),
            "Should reward smooth transitions"
        );
    }

    #[test]
    fn test_check_specificity_vague() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![make_section(
                "s1",
                "Body",
                "Various things are important and interesting. Several aspects are good \
                 and significant. A lot of stuff is nice and great for various reasons.",
                30,
            )],
        };
        let signals = check_specificity(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::Specificity
                && s.spike_effect < 0.0),
            "Should flag vague language, got: {:?}",
            signals.iter().map(|s| &s.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_check_specificity_precise() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![make_section(
                "s1",
                "Methodology",
                "The randomised controlled trial enrolled 342 participants across \
                 12 NHS trusts between January and March 2025. The primary endpoint \
                 was 30-day readmission rate measured via Hospital Episode Statistics. \
                 Secondary endpoints included patient-reported outcome measures using \
                 the EQ-5D-5L instrument administered at baseline and 90 days. \
                 Participants were stratified by age and comorbidity status. \
                 Statistical analysis used Cox proportional hazards regression \
                 with pre-specified covariates including deprivation quintile.",
                80,
            )],
        };
        let signals = check_specificity(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::Specificity
                && s.spike_effect > 0.0),
            "Should reward precise language, got: {:?}",
            signals.iter().map(|s| (&s.title, s.spike_effect)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_check_referencing_consistency_mixed() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![make_section(
                "s1",
                "Body",
                "According to (Smith, 2020), results were positive. \
                 Other research [1] agrees. Further evidence [2] supports this. \
                 (Jones, 2021) also found similar outcomes.",
                50,
            )],
        };
        let signals = check_referencing_consistency(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::ReferencingConsistency
                && s.title.contains("Mixed")),
            "Should flag mixed citation styles"
        );
    }

    #[test]
    fn test_check_referencing_consistency_consistent() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![make_section(
                "s1",
                "Body",
                "According to (Smith, 2020), results were positive. \
                 (Jones, 2021) agrees. (Brown, 2022) supports this.",
                50,
            )],
        };
        let signals = check_referencing_consistency(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::ReferencingConsistency
                && s.title.contains("Consistent")),
            "Should reward consistent citation style"
        );
    }

    #[test]
    fn test_check_argument_flow_disconnected() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![
                make_section(
                    "s1",
                    "Introduction",
                    "This essay examines monetary policy and quantitative easing effects.",
                    50,
                ),
                make_section(
                    "s2",
                    "Analysis",
                    "Climate change affects biodiversity and ecosystem services worldwide.",
                    200,
                ),
                make_section(
                    "s3",
                    "Conclusion",
                    "Healthcare spending should increase to address demographic ageing.",
                    100,
                ),
            ],
        };
        let signals = check_argument_flow(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::ArgumentFlow
                && s.spike_effect < 0.0),
            "Should flag disconnected argument flow"
        );
    }

    #[test]
    fn test_check_argument_flow_coherent() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![
                make_section(
                    "s1",
                    "Introduction",
                    "This essay examines quantitative easing and monetary policy transmission.",
                    50,
                ),
                make_section(
                    "s2",
                    "Analysis",
                    "Quantitative easing works through the portfolio rebalancing channel. \
                     Monetary policy transmission depends on bank lending. The easing measures \
                     affected asset prices and monetary conditions significantly.",
                    200,
                ),
                make_section(
                    "s3",
                    "Conclusion",
                    "Quantitative easing improved monetary policy transmission through \
                     portfolio rebalancing and bank lending channels.",
                    100,
                ),
            ],
        };
        let signals = check_argument_flow(&doc);
        assert!(
            signals.iter().any(|s| s.signal_type == SignalType::ArgumentFlow
                && s.spike_effect > 0.0),
            "Should reward coherent argument flow, got: {:?}",
            signals.iter().map(|s| (&s.title, s.spike_effect)).collect::<Vec<_>>()
        );
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
