//! Document ingestion — PDF text extraction, section splitting, and RDF generation.

use std::path::Path;

use open_ontologies::graph::GraphStore;
use uuid::Uuid;

use crate::types::{Document, Section};

/// Detect section headers and split text into sections.
///
/// Recognises Markdown-style headers (`##`), numbered headers (`1.`, `1.1`),
/// and uppercase lines as section boundaries.
pub fn split_into_sections(text: &str) -> Vec<Section> {
    let lines: Vec<&str> = text.lines().collect();
    let mut sections = Vec::new();
    let mut current_title = String::from("Introduction");
    let mut current_lines: Vec<&str> = Vec::new();
    let mut section_idx = 0u32;

    let is_header = |line: &str| -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Markdown headers
        if trimmed.starts_with("##") {
            return Some(trimmed.trim_start_matches('#').trim().to_string());
        }
        // Numbered headers: "1.", "1.1", "2.3.1" etc at start of line
        if let Some(first_char) = trimmed.chars().next() {
            if first_char.is_ascii_digit() {
                let rest = trimmed.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
                if rest.len() < trimmed.len() && !rest.is_empty() && rest.starts_with(' ') {
                    let potential_title = rest.trim();
                    if potential_title.len() < 120 {
                        return Some(potential_title.to_string());
                    }
                }
            }
        }
        // ALL-CAPS lines (at least 3 words, under 120 chars)
        if trimmed.len() < 120
            && trimmed.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase())
            && trimmed.split_whitespace().count() >= 3
        {
            return Some(trimmed.to_string());
        }
        None
    };

    for line in &lines {
        if let Some(title) = is_header(line) {
            // Flush previous section
            if !current_lines.is_empty() {
                let body = current_lines.join("\n");
                let wc = body.split_whitespace().count() as u32;
                sections.push(Section {
                    id: format!("sec-{section_idx}"),
                    title: current_title.clone(),
                    text: body,
                    word_count: wc,
                    claims: Vec::new(),
                    evidence: Vec::new(),
                });
                section_idx += 1;
            }
            current_title = title;
            current_lines.clear();
        } else {
            current_lines.push(line);
        }
    }

    // Flush last section
    if !current_lines.is_empty() {
        let body = current_lines.join("\n");
        let wc = body.split_whitespace().count() as u32;
        sections.push(Section {
            id: format!("sec-{section_idx}"),
            title: current_title,
            text: body,
            word_count: wc,
            claims: Vec::new(),
            evidence: Vec::new(),
        });
    }

    sections
}

/// Extract text from a PDF file.
pub fn extract_pdf_text(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| anyhow::anyhow!("PDF extraction failed: {e}"))?;
    Ok(text)
}

/// Ingest raw text into a Document with sections.
pub fn ingest_text(title: &str, text: &str) -> Document {
    let sections = split_into_sections(text);
    let total_wc: u32 = sections.iter().map(|s| s.word_count).sum();
    Document {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        doc_type: "text".into(),
        total_pages: None,
        total_word_count: Some(total_wc),
        sections,
    }
}

/// Serialize a Document to RDF Turtle.
pub fn document_to_turtle(doc: &Document) -> String {
    let mut ttl = String::new();
    ttl.push_str("@prefix doc: <http://brain-in-the-fish.dev/doc/> .\n");
    ttl.push_str("@prefix arg: <http://brain-in-the-fish.dev/arg/> .\n");
    ttl.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n");

    let doc_iri = format!("doc:{}", doc.id);
    ttl.push_str(&format!(
        "{doc_iri} a doc:Document ;\n    rdfs:label \"{}\" .\n\n",
        escape_turtle(&doc.title)
    ));

    for section in &doc.sections {
        let sec_iri = format!("doc:{}", section.id);
        ttl.push_str(&format!(
            "{sec_iri} a doc:Section ;\n    rdfs:label \"{}\" ;\n    doc:partOf {doc_iri} ;\n    doc:wordCount {} .\n\n",
            escape_turtle(&section.title),
            section.word_count
        ));

        for claim in &section.claims {
            let claim_iri = format!("arg:{}", claim.id);
            ttl.push_str(&format!(
                "{claim_iri} a arg:SubClaim ;\n    arg:hasText \"{}\" ;\n    arg:supports {sec_iri} .\n\n",
                escape_turtle(&claim.text)
            ));
        }

        for ev in &section.evidence {
            let ev_iri = format!("arg:{}", ev.id);
            let ev_type = if ev.has_quantified_outcome {
                "arg:QuantifiedEvidence"
            } else {
                "arg:Evidence"
            };
            ttl.push_str(&format!(
                "{ev_iri} a {ev_type} ;\n    arg:hasText \"{}\" ;\n    arg:supports {sec_iri} .\n\n",
                escape_turtle(&ev.text)
            ));
        }
    }

    ttl
}

/// Load a document's RDF representation into a GraphStore.
pub fn load_document_ontology(graph: &GraphStore, doc: &Document) -> anyhow::Result<usize> {
    let ttl = document_to_turtle(doc);
    graph.load_turtle(&ttl, Some("http://brain-in-the-fish.dev/"))
}

/// Escape special characters for Turtle string literals.
fn escape_turtle(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
