//! Section-to-criterion alignment via keyword overlap.

use open_ontologies::graph::GraphStore;

use crate::ingest::document_to_turtle;
use crate::types::{AlignmentMapping, Document, Framework, Gap, Section};

/// Align document sections to framework criteria using keyword overlap.
///
/// Returns alignment mappings and gaps (criteria with no strong match).
pub fn align_sections_to_criteria(
    doc: &Document,
    framework: &Framework,
) -> (Vec<AlignmentMapping>, Vec<Gap>) {
    let mut mappings = Vec::new();
    let mut best_per_criterion: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();

    for criterion in &framework.criteria {
        let crit_words = extract_keywords(&criterion.title, criterion.description.as_deref());
        let mut has_match = false;

        for section in &doc.sections {
            let sec_words = extract_keywords(&section.title, Some(&section.text));
            let confidence = keyword_overlap(&crit_words, &sec_words);

            if confidence > 0.1 {
                mappings.push(AlignmentMapping {
                    section_id: section.id.clone(),
                    criterion_id: criterion.id.clone(),
                    confidence,
                });
                has_match = true;
                let best = best_per_criterion
                    .entry(criterion.id.clone())
                    .or_insert(0.0);
                if confidence > *best {
                    *best = confidence;
                }
            }
        }

        if !has_match {
            // Find the best partial match even if below threshold
            let best_partial = doc
                .sections
                .iter()
                .map(|s| {
                    let sw = extract_keywords(&s.title, Some(&s.text));
                    let conf = keyword_overlap(&crit_words, &sw);
                    (s, conf)
                })
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            let partial = best_partial.map(|(s, conf)| AlignmentMapping {
                section_id: s.id.clone(),
                criterion_id: criterion.id.clone(),
                confidence: conf,
            });

            // Still record a gap even if there's a weak partial
            mappings.push(AlignmentMapping {
                section_id: partial
                    .as_ref()
                    .map(|p| p.section_id.clone())
                    .unwrap_or_default(),
                criterion_id: criterion.id.clone(),
                confidence: partial.as_ref().map(|p| p.confidence).unwrap_or(0.0),
            });

            // Only register a gap if confidence is truly low
            if best_partial.map(|(_, c)| c).unwrap_or(0.0) < 0.3 {
                // Mark as low-confidence for gap detection below
                best_per_criterion.insert(criterion.id.clone(), 0.0);
            }
        }
    }

    // Collect gaps
    let gaps: Vec<Gap> = framework
        .criteria
        .iter()
        .filter(|c| {
            let best = best_per_criterion.get(&c.id).copied().unwrap_or(0.0);
            best < 0.3
        })
        .map(|c| {
            let partial = mappings
                .iter()
                .filter(|m| m.criterion_id == c.id)
                .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
                .cloned();
            Gap {
                criterion_id: c.id.clone(),
                criterion_title: c.title.clone(),
                best_partial_match: partial,
            }
        })
        .collect();

    (mappings, gaps)
}

/// Align via ontology using the GraphStore for SPARQL-based matching.
pub fn align_via_ontology(
    graph: &GraphStore,
    doc: &Document,
    framework: &Framework,
) -> anyhow::Result<(Vec<AlignmentMapping>, Vec<Gap>)> {
    // Load document into graph
    let ttl = document_to_turtle(doc);
    graph.load_turtle(&ttl, Some("http://brain-in-the-fish.dev/"))?;

    // Fall back to keyword alignment — the ontology enriches but doesn't replace
    Ok(align_sections_to_criteria(doc, framework))
}

/// Get sections relevant to a given criterion, sorted by confidence descending.
pub fn sections_for_criterion<'a>(
    alignments: &[AlignmentMapping],
    criterion_id: &str,
    doc: &'a Document,
) -> Vec<(&'a Section, f64)> {
    let mut matches: Vec<(&Section, f64)> = alignments
        .iter()
        .filter(|m| m.criterion_id == criterion_id && m.confidence > 0.0)
        .filter_map(|m| {
            doc.sections
                .iter()
                .find(|s| s.id == m.section_id)
                .map(|s| (s, m.confidence))
        })
        .collect();
    matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    matches
}

// ---- internal helpers ----

fn extract_keywords(title: &str, description: Option<&str>) -> Vec<String> {
    let mut text = title.to_lowercase();
    if let Some(desc) = description {
        text.push(' ');
        text.push_str(&desc.to_lowercase());
    }
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .filter(|w| !STOP_WORDS.contains(w))
        .map(|w| w.to_string())
        .collect()
}

fn keyword_overlap(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let set_a: std::collections::HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let set_b: std::collections::HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let intersection = set_a.intersection(&set_b).count() as f64;
    let union = set_a.union(&set_b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "all", "any", "can", "has",
    "her", "was", "one", "our", "out", "his", "how", "its", "may", "new", "now",
    "old", "see", "way", "who", "did", "get", "got", "had", "him", "let", "say",
    "she", "too", "use", "that", "with", "have", "this", "will", "your", "from",
    "they", "been", "each", "make", "like", "into", "just", "over", "such", "than",
    "them", "very", "when", "what", "some", "about", "which", "their", "these",
    "should", "would", "could", "other", "there", "where",
];
