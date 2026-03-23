//! Ontology alignment and semantic search integration.
//!
//! Bridges open-ontologies alignment (onto_align) and embedding (onto_embed)
//! into the evaluation pipeline for structured document<->criteria mapping.

use crate::ingest::iri_safe;
use crate::types::*;
use open_ontologies::graph::GraphStore;
use std::collections::HashMap;

/// Align document sections to evaluation criteria using keyword overlap.
///
/// This is the baseline alignment -- no LLM or embeddings needed.
/// For each criterion, finds document sections whose titles/text contain
/// keywords from the criterion title/description.
///
/// Returns AlignmentMapping with confidence scores (0.0-1.0).
pub fn align_sections_to_criteria(
    doc: &EvalDocument,
    framework: &EvaluationFramework,
) -> (Vec<AlignmentMapping>, Vec<Gap>) {
    let mut alignments = Vec::new();
    let mut gaps = Vec::new();

    for criterion in all_criteria(&framework.criteria) {
        let crit_words = extract_keywords(&criterion.title, criterion.description.as_deref());
        let mut best_confidence = 0.0;
        let mut best_section_id = None;

        for section in all_sections(&doc.sections) {
            let section_words = extract_keywords(&section.title, Some(&section.text));
            let confidence = keyword_overlap(&crit_words, &section_words);

            if confidence > 0.1 {
                alignments.push(AlignmentMapping {
                    section_id: section.id.clone(),
                    criterion_id: criterion.id.clone(),
                    confidence,
                });
            }

            if confidence > best_confidence {
                best_confidence = confidence;
                best_section_id = Some(section.id.clone());
            }
        }

        if best_confidence < 0.1 {
            gaps.push(Gap {
                criterion_id: criterion.id.clone(),
                criterion_title: criterion.title.clone(),
                best_partial_match: best_section_id.map(|sid| AlignmentMapping {
                    section_id: sid,
                    criterion_id: criterion.id.clone(),
                    confidence: best_confidence,
                }),
            });
        }
    }

    alignments.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    (alignments, gaps)
}

/// Flatten nested criteria into a flat list.
fn all_criteria(criteria: &[EvaluationCriterion]) -> Vec<&EvaluationCriterion> {
    let mut result = Vec::new();
    for c in criteria {
        result.push(c);
        result.extend(all_criteria(&c.sub_criteria));
    }
    result
}

/// Flatten nested sections into a flat list.
fn all_sections(sections: &[Section]) -> Vec<&Section> {
    let mut result = Vec::new();
    for s in sections {
        result.push(s);
        result.extend(all_sections(&s.subsections));
    }
    result
}

/// Extract meaningful keywords from title + description, lowercased and deduplicated.
fn extract_keywords(title: &str, description: Option<&str>) -> Vec<String> {
    let stop_words = [
        "the", "a", "an", "and", "or", "of", "in", "on", "at", "to", "for", "is", "are", "was",
        "were", "be", "been", "being", "have", "has", "had", "do", "does", "did", "will", "would",
        "could", "should", "may", "might", "shall", "can", "this", "that", "these", "those",
        "with", "from", "by", "as", "it", "its", "not", "no", "but", "if", "than", "so", "very",
        "all", "each", "every", "any", "some", "such", "only", "also", "how", "what", "which",
        "who", "whom", "when", "where", "why",
    ];

    let mut text = title.to_lowercase();
    if let Some(desc) = description {
        text.push(' ');
        text.push_str(&desc.to_lowercase());
    }

    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !stop_words.contains(w))
        .map(|w| w.to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect()
}

/// Calculate keyword overlap between two sets of keywords.
/// Returns Jaccard-like similarity (0.0-1.0).
fn keyword_overlap(set_a: &[String], set_b: &[String]) -> f64 {
    if set_a.is_empty() || set_b.is_empty() {
        return 0.0;
    }

    let mut matches = 0;
    for word_a in set_a {
        for word_b in set_b {
            // Exact match or substring match (for compound words)
            if word_a == word_b
                || word_b.contains(word_a.as_str())
                || word_a.contains(word_b.as_str())
            {
                matches += 1;
                break;
            }
        }
    }

    let overlap = matches as f64 / set_a.len() as f64;
    overlap.min(1.0)
}

/// Load alignment triples into the graph store.
/// Creates eval:alignedTo relationships between sections and criteria.
pub fn load_alignments(graph: &GraphStore, alignments: &[AlignmentMapping]) -> anyhow::Result<usize> {
    if alignments.is_empty() {
        return Ok(0);
    }

    let mut turtle = String::from(
        "@prefix doc: <http://brain-in-the-fish.dev/doc/> .\n\
         @prefix crit: <http://brain-in-the-fish.dev/criteria/> .\n\
         @prefix eval: <http://brain-in-the-fish.dev/eval/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n",
    );

    for alignment in alignments {
        let section_iri = iri_safe(&alignment.section_id);
        let criterion_iri = iri_safe(&alignment.criterion_id);
        turtle.push_str(&format!(
            "doc:{section_iri} eval:alignedTo [\n    \
             eval:criterion crit:{criterion_iri} ;\n    \
             eval:confidence \"{confidence}\"^^xsd:decimal\n] .\n\n",
            confidence = alignment.confidence
        ));
    }

    graph.load_turtle(&turtle, None)
}

/// Get the best matching sections for a criterion.
/// Returns sections sorted by alignment confidence (highest first).
pub fn sections_for_criterion(
    alignments: &[AlignmentMapping],
    criterion_id: &str,
    doc: &EvalDocument,
) -> Vec<(Section, f64)> {
    let section_map: HashMap<String, &Section> = all_sections(&doc.sections)
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();

    let mut matches: Vec<(Section, f64)> = alignments
        .iter()
        .filter(|a| a.criterion_id == criterion_id)
        .filter_map(|a| section_map.get(&a.section_id).map(|s| ((*s).clone(), a.confidence)))
        .collect();

    matches.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_keywords() {
        let kw = extract_keywords("Knowledge and Understanding", None);
        assert!(kw.contains(&"knowledge".to_string()));
        assert!(kw.contains(&"understanding".to_string()));
        assert!(!kw.contains(&"and".to_string())); // stop word
    }

    #[test]
    fn test_keyword_overlap_exact() {
        let a = vec!["knowledge".into(), "understanding".into()];
        let b = vec![
            "knowledge".into(),
            "depth".into(),
            "understanding".into(),
        ];
        let score = keyword_overlap(&a, &b);
        assert!(score > 0.9, "Both keywords match: {score}");
    }

    #[test]
    fn test_keyword_overlap_partial() {
        let a = vec!["communication".into(), "structure".into()];
        let b = vec!["essay".into(), "structure".into(), "paragraphs".into()];
        let score = keyword_overlap(&a, &b);
        assert!(
            score > 0.4 && score < 0.8,
            "One of two matches: {score}"
        );
    }

    #[test]
    fn test_keyword_overlap_none() {
        let a = vec!["analysis".into(), "evaluation".into()];
        let b = vec!["formatting".into(), "margins".into()];
        let score = keyword_overlap(&a, &b);
        assert!(score < 0.01, "No matches: {score}");
    }

    #[test]
    fn test_align_sections_to_criteria() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![
                Section {
                    id: "s1".into(),
                    title: "Knowledge and Theory".into(),
                    text: "Theoretical analysis of economic concepts and understanding".into(),
                    word_count: 100,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                },
                Section {
                    id: "s2".into(),
                    title: "Critical Analysis".into(),
                    text: "Evaluation of competing arguments and critical assessment".into(),
                    word_count: 100,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                },
            ],
        };
        let fw = EvaluationFramework {
            id: "f1".into(),
            name: "Test".into(),
            total_weight: 1.0,
            pass_mark: None,
            criteria: vec![
                EvaluationCriterion {
                    id: "c1".into(),
                    title: "Knowledge and Understanding".into(),
                    description: None,
                    max_score: 8.0,
                    weight: 0.5,
                    rubric_levels: vec![],
                    sub_criteria: vec![],
                },
                EvaluationCriterion {
                    id: "c2".into(),
                    title: "Analysis and Evaluation".into(),
                    description: None,
                    max_score: 12.0,
                    weight: 0.5,
                    rubric_levels: vec![],
                    sub_criteria: vec![],
                },
            ],
        };

        let (alignments, gaps) = align_sections_to_criteria(&doc, &fw);
        assert!(!alignments.is_empty(), "Should find alignments");
        assert!(gaps.is_empty(), "Should have no gaps");
        // Knowledge section should align to Knowledge criterion
        let knowledge_align = alignments
            .iter()
            .find(|a| a.criterion_id == "c1" && a.section_id == "s1");
        assert!(
            knowledge_align.is_some(),
            "Knowledge section should align to Knowledge criterion"
        );
    }

    #[test]
    fn test_gap_detection() {
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(100),
            sections: vec![Section {
                id: "s1".into(),
                title: "Introduction".into(),
                text: "A brief introduction.".into(),
                word_count: 10,
                page_range: None,
                claims: vec![],
                evidence: vec![],
                subsections: vec![],
            }],
        };
        let fw = EvaluationFramework {
            id: "f1".into(),
            name: "Test".into(),
            total_weight: 1.0,
            pass_mark: None,
            criteria: vec![EvaluationCriterion {
                id: "c1".into(),
                title: "Advanced Statistical Methods".into(),
                description: None,
                max_score: 10.0,
                weight: 1.0,
                rubric_levels: vec![],
                sub_criteria: vec![],
            }],
        };

        let (_, gaps) = align_sections_to_criteria(&doc, &fw);
        assert_eq!(gaps.len(), 1, "Should detect gap for unmatched criterion");
        assert_eq!(gaps[0].criterion_title, "Advanced Statistical Methods");
    }

    #[test]
    fn test_sections_for_criterion() {
        let alignments = vec![
            AlignmentMapping {
                section_id: "s1".into(),
                criterion_id: "c1".into(),
                confidence: 0.8,
            },
            AlignmentMapping {
                section_id: "s2".into(),
                criterion_id: "c1".into(),
                confidence: 0.5,
            },
            AlignmentMapping {
                section_id: "s1".into(),
                criterion_id: "c2".into(),
                confidence: 0.3,
            },
        ];
        let doc = EvalDocument {
            id: "d1".into(),
            title: "Test".into(),
            doc_type: "essay".into(),
            total_pages: None,
            total_word_count: Some(200),
            sections: vec![
                Section {
                    id: "s1".into(),
                    title: "Section 1".into(),
                    text: "Content 1".into(),
                    word_count: 100,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                },
                Section {
                    id: "s2".into(),
                    title: "Section 2".into(),
                    text: "Content 2".into(),
                    word_count: 100,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                },
            ],
        };

        let matches = sections_for_criterion(&alignments, "c1", &doc);
        assert_eq!(matches.len(), 2);
        assert!(
            matches[0].1 > matches[1].1,
            "Should be sorted by confidence"
        );
    }

    #[test]
    fn test_load_alignments() {
        let graph = GraphStore::new();
        let alignments = vec![AlignmentMapping {
            section_id: "s1".into(),
            criterion_id: "c1".into(),
            confidence: 0.8,
        }];
        let triples = load_alignments(&graph, &alignments).unwrap();
        assert!(triples > 0, "Should load alignment triples");
    }
}
