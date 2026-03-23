//! PDF ingestion and document ontology generation.

use crate::types::{Claim, EvalDocument, Evidence, Section};
use regex::Regex;
use std::fmt::Write;
use std::path::Path;

/// Raw section before LLM enrichment.
#[derive(Debug, Clone)]
pub struct RawSection {
    pub title: String,
    pub text: String,
    /// 0 = top-level, 1 = subsection, etc.
    pub level: u32,
    pub word_count: u32,
    /// Rough estimate based on character position in the source text.
    pub page_estimate: Option<u32>,
}

/// Extract text from a PDF file.
pub fn extract_pdf_text(path: &Path) -> anyhow::Result<String> {
    let text = pdf_extract::extract_text(path)
        .map_err(|e| anyhow::anyhow!("Failed to extract text from {}: {}", path.display(), e))?;
    Ok(text)
}

/// Determine nesting level from a numbered heading pattern.
/// "1." or "1" => 0, "1.2" => 1, "1.2.3" => 2, etc.
fn heading_level(numbering: &str) -> u32 {
    let trimmed = numbering.trim_end_matches('.');
    let parts = trimmed.split('.').count();
    if parts <= 1 {
        0
    } else {
        (parts - 1) as u32
    }
}

/// Count words in a string.
fn count_words(s: &str) -> u32 {
    s.split_whitespace().count() as u32
}

/// Estimate the page number based on character offset.
/// Assumes roughly 2500 characters per page (typical PDF text density).
fn estimate_page(char_offset: usize) -> u32 {
    (char_offset / 2500) as u32 + 1
}

/// Split raw text into sections based on heading patterns.
///
/// Detects headings by:
/// - Lines matching numbered patterns like "1.", "1.2", "1.2.3" followed by text
/// - Lines that are ALL CAPS (at least 3 chars, no lowercase)
/// - Lines that are short (< 80 chars) and followed by a blank line then a longer paragraph
pub fn split_into_sections(text: &str) -> Vec<RawSection> {
    // Pattern: numbered headings like "1. Introduction", "2.1 Data Collection", "3.2.1 Sub-topic"
    let numbered_re = Regex::new(r"^(\d+(?:\.\d+)*\.?)\s+(.+)$").unwrap();
    // Pattern: ALL CAPS lines (at least 3 word characters, no lowercase)
    let allcaps_re = Regex::new(r"^[A-Z][A-Z\s\d\-:,&]{2,}$").unwrap();

    let lines: Vec<&str> = text.lines().collect();
    let mut headings: Vec<(usize, String, u32)> = Vec::new(); // (line_index, title, level)

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check numbered heading pattern
        if let Some(caps) = numbered_re.captures(trimmed) {
            let numbering = caps.get(1).unwrap().as_str();
            let title_text = caps.get(2).unwrap().as_str().trim();
            // Heading text should be reasonably short and not look like a regular sentence
            if title_text.len() < 100 && !title_text.ends_with('.') {
                let level = heading_level(numbering);
                let full_title = trimmed.to_string();
                headings.push((i, full_title, level));
                continue;
            }
        }

        // Check ALL CAPS heading
        if allcaps_re.is_match(trimmed) && trimmed.len() < 80 {
            headings.push((i, trimmed.to_string(), 0));
            continue;
        }

        // Check short line followed by blank line then content (implicit heading)
        if trimmed.len() < 80
            && trimmed.len() >= 2
            && !trimmed.ends_with('.')
            && !trimmed.ends_with(',')
            && !trimmed.ends_with(';')
        {
            // Must be followed by a blank line and then a longer paragraph
            if i + 2 < lines.len() {
                let next = lines[i + 1].trim();
                let after = lines[i + 2].trim();
                if next.is_empty() && after.len() > trimmed.len() {
                    // Only if this looks like a heading (starts with uppercase or is short enough)
                    if trimmed.chars().next().is_some_and(|c| c.is_uppercase()) {
                        headings.push((i, trimmed.to_string(), 0));
                    }
                }
            }
        }
    }

    // If no headings found, return the whole text as one section
    if headings.is_empty() {
        let wc = count_words(text);
        return vec![RawSection {
            title: "Document".to_string(),
            text: text.to_string(),
            level: 0,
            word_count: wc,
            page_estimate: Some(1),
        }];
    }

    // Build sections from heading positions
    let mut sections = Vec::new();

    for (idx, (line_idx, title, level)) in headings.iter().enumerate() {
        // Section text runs from the line after the heading to the line before the next heading
        let content_start = line_idx + 1;
        let content_end = if idx + 1 < headings.len() {
            headings[idx + 1].0
        } else {
            lines.len()
        };

        let section_text: String = lines[content_start..content_end]
            .to_vec()
            .join("\n")
            .trim()
            .to_string();

        let wc = count_words(&section_text);

        // Estimate character offset for page estimation
        let char_offset: usize = lines[..*line_idx].iter().map(|l| l.len() + 1).sum();
        let page = estimate_page(char_offset);

        sections.push(RawSection {
            title: title.clone(),
            text: section_text,
            level: *level,
            word_count: wc,
            page_estimate: Some(page),
        });
    }

    sections
}

/// Build an EvalDocument from a file (PDF or plain text).
///
/// Claim and Evidence extraction happens later via LLM.
pub fn ingest_pdf(path: &Path, _intent: &str) -> anyhow::Result<(EvalDocument, Vec<RawSection>)> {
    let text = match path.extension().and_then(|e| e.to_str()) {
        Some("pdf") => extract_pdf_text(path)?,
        _ => std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?,
    };
    let raw_sections = split_into_sections(&text);

    let total_words: u32 = raw_sections.iter().map(|s| s.word_count).sum();

    let sections = raw_sections
        .iter()
        .map(|rs| Section {
            id: uuid::Uuid::new_v4().to_string(),
            title: rs.title.clone(),
            text: rs.text.clone(),
            word_count: rs.word_count,
            page_range: rs.page_estimate.map(|p| p.to_string()),
            claims: vec![],
            evidence: vec![],
            subsections: vec![],
        })
        .collect();

    let doc = EvalDocument {
        id: uuid::Uuid::new_v4().to_string(),
        title: String::new(),    // set later by LLM
        doc_type: String::new(), // set later based on intent
        total_pages: None,
        total_word_count: Some(total_words),
        sections,
    };

    Ok((doc, raw_sections))
}

// ============================================================================
// Document Ontology — Turtle/RDF generation
// ============================================================================

/// Sanitize a string for use as a Turtle IRI local name.
pub fn iri_safe(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

/// Escape a string for Turtle string literal.
pub fn turtle_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Write Turtle triples for a single section (and its claims, evidence, subsections).
fn write_section_turtle(out: &mut String, section: &Section, doc_id: &str) {
    let sec_id = iri_safe(&section.id);
    let _ = writeln!(out, "doc:{sec_id} a eval:Section ;");
    let _ = writeln!(out, "    eval:title \"{}\" ;", turtle_escape(&section.title));
    let _ = writeln!(out, "    eval:text \"{}\" ;", turtle_escape(&section.text));
    let _ = writeln!(
        out,
        "    eval:wordCount \"{}\"^^xsd:integer ;",
        section.word_count
    );
    let _ = writeln!(out, "    eval:parentDocument doc:{} .", iri_safe(doc_id));
    let _ = writeln!(out);

    for claim in &section.claims {
        write_claim_turtle(out, claim, &sec_id);
    }
    for ev in &section.evidence {
        write_evidence_turtle(out, ev, &sec_id);
    }
    for sub in &section.subsections {
        write_section_turtle(out, sub, doc_id);
    }
}

/// Write Turtle triples for a single claim.
fn write_claim_turtle(out: &mut String, claim: &Claim, section_id: &str) {
    let claim_id = iri_safe(&claim.id);
    let _ = writeln!(out, "doc:{claim_id} a eval:Claim ;");
    let _ = writeln!(out, "    eval:text \"{}\" ;", turtle_escape(&claim.text));
    let _ = writeln!(
        out,
        "    eval:specificity \"{}\"^^xsd:decimal ;",
        claim.specificity
    );
    let _ = writeln!(
        out,
        "    eval:verifiable \"{}\"^^xsd:boolean ;",
        claim.verifiable
    );
    let _ = writeln!(out, "    eval:inSection doc:{section_id} .");
    let _ = writeln!(out);
}

/// Write Turtle triples for a single piece of evidence.
fn write_evidence_turtle(out: &mut String, ev: &Evidence, section_id: &str) {
    let ev_id = iri_safe(&ev.id);
    let _ = writeln!(out, "doc:{ev_id} a eval:Evidence ;");
    let _ = writeln!(
        out,
        "    eval:source \"{}\" ;",
        turtle_escape(&ev.source)
    );
    let _ = writeln!(
        out,
        "    eval:evidenceType \"{}\" ;",
        turtle_escape(&ev.evidence_type)
    );
    let _ = writeln!(out, "    eval:text \"{}\" ;", turtle_escape(&ev.text));
    let _ = writeln!(
        out,
        "    eval:hasQuantifiedOutcome \"{}\"^^xsd:boolean ;",
        ev.has_quantified_outcome
    );
    let _ = writeln!(out, "    eval:inSection doc:{section_id} .");
    let _ = writeln!(out);
}

/// Convert an `EvalDocument` into a Turtle RDF string.
///
/// Uses the brain-in-the-fish vocabulary:
///   @prefix doc: <http://brain-in-the-fish.dev/doc/> .
///   @prefix eval: <http://brain-in-the-fish.dev/eval/> .
pub fn document_to_turtle(doc: &EvalDocument) -> String {
    let mut out = String::new();

    // Prefixes
    let _ = writeln!(out, "@prefix doc: <http://brain-in-the-fish.dev/doc/> .");
    let _ = writeln!(out, "@prefix eval: <http://brain-in-the-fish.dev/eval/> .");
    let _ = writeln!(
        out,
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> ."
    );
    let _ = writeln!(out);

    // Document node
    let doc_id = iri_safe(&doc.id);
    let _ = writeln!(out, "doc:{doc_id} a eval:Document ;");
    let _ = writeln!(out, "    eval:title \"{}\" ;", turtle_escape(&doc.title));
    let _ = writeln!(
        out,
        "    eval:docType \"{}\" ;",
        turtle_escape(&doc.doc_type)
    );
    let _ = writeln!(
        out,
        "    eval:totalWordCount \"{}\"^^xsd:integer .",
        doc.total_word_count.unwrap_or(0)
    );
    let _ = writeln!(out);

    // Sections (recursive)
    for section in &doc.sections {
        write_section_turtle(&mut out, section, &doc.id);
    }

    out
}

/// Load the document ontology into open-ontologies graph store.
///
/// Uses `open_ontologies::graph::GraphStore` directly.
pub fn load_document_ontology(
    graph: &open_ontologies::graph::GraphStore,
    doc: &EvalDocument,
) -> anyhow::Result<usize> {
    let turtle = document_to_turtle(doc);
    let triples = graph.load_turtle(&turtle, None)?;
    Ok(triples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_into_sections_numbered() {
        let text = "1. Introduction\n\nThis is the introduction paragraph with some content.\n\n2. Methodology\n\nThis section describes our methodology in detail.\n\n2.1 Data Collection\n\nWe collected data from multiple sources.\n\n3. Results\n\nThe results show significant improvement.";

        let sections = split_into_sections(text);
        assert!(
            sections.len() >= 3,
            "Should detect at least 3 sections, got {}",
            sections.len()
        );
        assert!(
            sections[0].title.contains("Introduction"),
            "First section should be Introduction, got: {}",
            sections[0].title
        );
        assert_eq!(sections[0].level, 0, "Top-level section should have level 0");
        // 2.1 Data Collection should be level 1
        let subsection = sections.iter().find(|s| s.title.contains("Data Collection"));
        assert!(subsection.is_some(), "Should find Data Collection subsection");
        assert_eq!(subsection.unwrap().level, 1, "2.1 should be level 1");
    }

    #[test]
    fn test_split_into_sections_allcaps() {
        let text = "EXECUTIVE SUMMARY\n\nThis document provides an overview of the project.\n\nMETHODOLOGY\n\nOur approach is based on best practices.";

        let sections = split_into_sections(text);
        assert!(
            sections.len() >= 2,
            "Should detect at least 2 ALL CAPS sections, got {}",
            sections.len()
        );
        assert!(sections[0].title.contains("EXECUTIVE SUMMARY"));
    }

    #[test]
    fn test_word_count() {
        let text = "1. Test Section\n\nThis has exactly five words.";
        let sections = split_into_sections(text);
        assert!(!sections.is_empty());
        // The text content is "This has exactly five words." but ends with period
        // so the heading regex won't eat it. Word count should be 5.
        assert!(
            sections[0].word_count >= 5,
            "Word count should be at least 5, got {}",
            sections[0].word_count
        );
    }

    #[test]
    fn test_no_headings_fallback() {
        let text = "This is just a plain paragraph with no headings at all. It should be returned as a single section.";
        let sections = split_into_sections(text);
        assert_eq!(sections.len(), 1, "Should return 1 section for plain text");
        assert_eq!(sections[0].title, "Document");
    }

    #[test]
    fn test_heading_level() {
        assert_eq!(heading_level("1."), 0);
        assert_eq!(heading_level("1"), 0);
        assert_eq!(heading_level("2.1"), 1);
        assert_eq!(heading_level("3.2.1"), 2);
    }

    #[test]
    fn test_page_estimate() {
        let sections = split_into_sections("1. First\n\nSome text.\n\n2. Second\n\nMore text.");
        assert!(sections.iter().all(|s| s.page_estimate.is_some()));
    }

    // ========================================================================
    // Turtle generation tests
    // ========================================================================

    use crate::types::{Claim, Evidence};

    #[test]
    fn test_turtle_escape() {
        assert_eq!(turtle_escape("hello \"world\""), "hello \\\"world\\\"");
        assert_eq!(turtle_escape("line1\nline2"), "line1\\nline2");
        assert_eq!(turtle_escape("tab\there"), "tab\\there");
        assert_eq!(turtle_escape("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_iri_safe() {
        assert_eq!(iri_safe("Section 3.2"), "Section_3_2");
        assert_eq!(iri_safe("hello-world!"), "hello_world_");
        assert_eq!(iri_safe("already_safe"), "already_safe");
    }

    #[test]
    fn test_document_to_turtle_basic() {
        let doc = EvalDocument {
            id: "test-doc-1".to_string(),
            title: "Test Document".to_string(),
            doc_type: "essay".to_string(),
            total_pages: Some(5),
            total_word_count: Some(1000),
            sections: vec![Section {
                id: "sec-1".to_string(),
                title: "Introduction".to_string(),
                text: "This is the intro.".to_string(),
                word_count: 5,
                page_range: None,
                claims: vec![],
                evidence: vec![],
                subsections: vec![],
            }],
        };

        let turtle = document_to_turtle(&doc);
        assert!(turtle.contains("eval:Document"));
        assert!(turtle.contains("eval:title"));
        assert!(turtle.contains("Introduction"));
        assert!(turtle.contains("eval:Section"));
        assert!(turtle.contains("eval:totalWordCount \"1000\"^^xsd:integer"));
        assert!(turtle.contains("eval:parentDocument doc:test_doc_1"));
    }

    #[test]
    fn test_document_to_turtle_with_claims_and_evidence() {
        let doc = EvalDocument {
            id: "doc-2".to_string(),
            title: "Full Doc".to_string(),
            doc_type: "proposal".to_string(),
            total_pages: None,
            total_word_count: Some(500),
            sections: vec![Section {
                id: "sec-a".to_string(),
                title: "Method".to_string(),
                text: "Our method is robust.".to_string(),
                word_count: 4,
                page_range: None,
                claims: vec![Claim {
                    id: "claim-1".to_string(),
                    text: "We achieved 99% accuracy.".to_string(),
                    specificity: 0.9,
                    verifiable: true,
                }],
                evidence: vec![Evidence {
                    id: "ev-1".to_string(),
                    source: "Internal report".to_string(),
                    evidence_type: "case_study".to_string(),
                    text: "The trial showed improvement.".to_string(),
                    has_quantified_outcome: true,
                }],
                subsections: vec![],
            }],
        };

        let turtle = document_to_turtle(&doc);
        assert!(turtle.contains("eval:Claim"));
        assert!(turtle.contains("eval:specificity \"0.9\"^^xsd:decimal"));
        assert!(turtle.contains("eval:verifiable \"true\"^^xsd:boolean"));
        assert!(turtle.contains("eval:Evidence"));
        assert!(turtle.contains("eval:evidenceType \"case_study\""));
        assert!(turtle.contains("eval:hasQuantifiedOutcome \"true\"^^xsd:boolean"));
    }

    #[test]
    fn test_document_to_turtle_escaping() {
        let doc = EvalDocument {
            id: "doc-esc".to_string(),
            title: "Doc with \"quotes\"".to_string(),
            doc_type: "report".to_string(),
            total_pages: None,
            total_word_count: Some(10),
            sections: vec![Section {
                id: "sec-esc".to_string(),
                title: "Line\nBreak".to_string(),
                text: "Text with \"special\" chars\nand newlines.".to_string(),
                word_count: 6,
                page_range: None,
                claims: vec![],
                evidence: vec![],
                subsections: vec![],
            }],
        };

        let turtle = document_to_turtle(&doc);
        // Quotes and newlines should be escaped
        assert!(turtle.contains("Doc with \\\"quotes\\\""));
        assert!(turtle.contains("Line\\nBreak"));
    }

    #[test]
    fn test_load_document_ontology() {
        let graph = open_ontologies::graph::GraphStore::new();
        let doc = EvalDocument {
            id: "load-test".to_string(),
            title: "Load Test".to_string(),
            doc_type: "test".to_string(),
            total_pages: None,
            total_word_count: Some(100),
            sections: vec![Section {
                id: "sec-lt".to_string(),
                title: "Only Section".to_string(),
                text: "Some content here.".to_string(),
                word_count: 3,
                page_range: None,
                claims: vec![],
                evidence: vec![],
                subsections: vec![],
            }],
        };

        let triples = load_document_ontology(&graph, &doc).expect("should load");
        // Document: 4 triples (a, title, docType, totalWordCount)
        // Section: 5 triples (a, title, text, wordCount, parentDocument)
        assert_eq!(triples, 9);
        assert_eq!(graph.triple_count(), 9);
    }
}
