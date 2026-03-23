//! PDF ingestion and document ontology generation.

use crate::types::{EvalDocument, Section};
use regex::Regex;
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
                    if trimmed.chars().next().map_or(false, |c| c.is_uppercase()) {
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
            .iter()
            .copied()
            .collect::<Vec<&str>>()
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

/// Build an EvalDocument from a PDF file (text extraction + section splitting only).
///
/// Claim and Evidence extraction happens later via LLM (Task 4).
pub fn ingest_pdf(path: &Path, _intent: &str) -> anyhow::Result<(EvalDocument, Vec<RawSection>)> {
    let text = extract_pdf_text(path)?;
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
}
