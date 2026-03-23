//! Independent scoring engine with ReACT loop.
//!
//! Provides SPARQL queries to find relevant document content for criteria,
//! score insertion into the graph store, scoring round management, and
//! a scoring prompt generator for subagents.

use crate::agent::load_score;
use crate::ingest::iri_safe;
use crate::types::*;
use open_ontologies::graph::GraphStore;

// ============================================================================
// Query result types
// ============================================================================

/// A document section matched by SPARQL query.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SectionMatch {
    pub section_iri: String,
    pub title: String,
    pub text: String,
    pub word_count: u32,
}

/// A claim found within a section.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClaimMatch {
    pub claim_iri: String,
    pub text: String,
    pub specificity: f64,
    pub verifiable: bool,
}

/// Evidence found within a section.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EvidenceMatch {
    pub evidence_iri: String,
    pub source: String,
    pub evidence_type: String,
    pub has_quantified_outcome: bool,
}

// ============================================================================
// SPARQL result parsing helpers
// ============================================================================

/// Strip surrounding quotes and optional ^^<datatype> from an Oxigraph term string.
///
/// Oxigraph's `term.to_string()` returns literals as:
///   `"value"` or `"value"^^<http://...>`
/// and IRIs as `<http://...>`.
fn strip_literal(raw: &str) -> String {
    let s = raw.trim();
    // IRI: <http://...>
    if s.starts_with('<') && s.ends_with('>') {
        return s[1..s.len() - 1].to_string();
    }
    // Literal with datatype: "value"^^<type>
    if let Some(idx) = s.rfind("\"^^<") {
        let inner = &s[1..idx];
        return inner.to_string();
    }
    // Literal with language tag: "value"@en
    if let Some(idx) = s.rfind("\"@") {
        let inner = &s[1..idx];
        return inner.to_string();
    }
    // Plain literal: "value"
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        return s[1..s.len() - 1].to_string();
    }
    s.to_string()
}

/// Parse an Oxigraph term string as f64.
fn parse_decimal(raw: &str) -> f64 {
    strip_literal(raw).parse::<f64>().unwrap_or(0.0)
}

/// Parse an Oxigraph term string as u32.
fn parse_integer(raw: &str) -> u32 {
    strip_literal(raw).parse::<u32>().unwrap_or(0)
}

/// Parse an Oxigraph term string as bool.
fn parse_bool(raw: &str) -> bool {
    strip_literal(raw) == "true"
}

/// Parse SPARQL JSON results from GraphStore::sparql_select.
///
/// Returns a vec of rows, each row a map of variable name -> raw string value.
fn parse_sparql_results(
    json_str: &str,
) -> anyhow::Result<Vec<std::collections::HashMap<String, String>>> {
    let parsed: serde_json::Value = serde_json::from_str(json_str)?;
    let results = parsed
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Missing 'results' array in SPARQL response"))?;

    let mut rows = Vec::new();
    for row in results {
        let obj = row
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Expected object in results array"))?;
        let mut map = std::collections::HashMap::new();
        for (key, val) in obj {
            if let Some(s) = val.as_str() {
                map.insert(key.clone(), s.to_string());
            }
        }
        rows.push(map);
    }
    Ok(rows)
}

// ============================================================================
// SPARQL query functions
// ============================================================================

/// SPARQL query to find document sections.
///
/// Returns all sections in the graph. When `criterion_id` has alignment mappings,
/// only aligned sections are returned; otherwise all sections are returned as a
/// fallback for scoring.
pub fn query_sections_for_criterion(
    graph: &GraphStore,
    _criterion_id: &str,
) -> anyhow::Result<Vec<SectionMatch>> {
    // Query all sections — alignment filtering would require eval:alignedTo triples
    // which are created by a separate alignment step. For now, return all sections
    // so the scoring prompt has content to evaluate.
    let sparql = r#"PREFIX eval: <http://brain-in-the-fish.dev/eval/>
        PREFIX doc: <http://brain-in-the-fish.dev/doc/>
        SELECT ?section ?title ?text ?wordCount WHERE {
            ?section a eval:Section ;
                eval:title ?title ;
                eval:text ?text ;
                eval:wordCount ?wordCount .
        }"#;

    let json = graph.sparql_select(sparql)?;
    let rows = parse_sparql_results(&json)?;

    let mut matches = Vec::new();
    for row in rows {
        let section_iri = row.get("section").map(|s| strip_literal(s)).unwrap_or_default();
        let title = row.get("title").map(|s| strip_literal(s)).unwrap_or_default();
        let text = row.get("text").map(|s| strip_literal(s)).unwrap_or_default();
        let word_count = row.get("wordCount").map(|s| parse_integer(s)).unwrap_or(0);

        matches.push(SectionMatch {
            section_iri,
            title,
            text,
            word_count,
        });
    }
    Ok(matches)
}

/// SPARQL query to get all claims in a section.
pub fn query_claims_for_section(
    graph: &GraphStore,
    section_id: &str,
) -> anyhow::Result<Vec<ClaimMatch>> {
    let safe_id = iri_safe(section_id);
    let sparql = format!(
        r#"PREFIX eval: <http://brain-in-the-fish.dev/eval/>
        PREFIX doc: <http://brain-in-the-fish.dev/doc/>
        SELECT ?claim ?text ?specificity ?verifiable WHERE {{
            ?claim a eval:Claim ;
                eval:text ?text ;
                eval:specificity ?specificity ;
                eval:verifiable ?verifiable ;
                eval:inSection doc:{safe_id} .
        }}"#
    );

    let json = graph.sparql_select(&sparql)?;
    let rows = parse_sparql_results(&json)?;

    let mut matches = Vec::new();
    for row in rows {
        let claim_iri = row.get("claim").map(|s| strip_literal(s)).unwrap_or_default();
        let text = row.get("text").map(|s| strip_literal(s)).unwrap_or_default();
        let specificity = row.get("specificity").map(|s| parse_decimal(s)).unwrap_or(0.0);
        let verifiable = row.get("verifiable").map(|s| parse_bool(s)).unwrap_or(false);

        matches.push(ClaimMatch {
            claim_iri,
            text,
            specificity,
            verifiable,
        });
    }
    Ok(matches)
}

/// SPARQL query to get all evidence in a section.
pub fn query_evidence_for_section(
    graph: &GraphStore,
    section_id: &str,
) -> anyhow::Result<Vec<EvidenceMatch>> {
    let safe_id = iri_safe(section_id);
    let sparql = format!(
        r#"PREFIX eval: <http://brain-in-the-fish.dev/eval/>
        PREFIX doc: <http://brain-in-the-fish.dev/doc/>
        SELECT ?evidence ?source ?evidenceType ?hasQuantifiedOutcome WHERE {{
            ?evidence a eval:Evidence ;
                eval:source ?source ;
                eval:evidenceType ?evidenceType ;
                eval:hasQuantifiedOutcome ?hasQuantifiedOutcome ;
                eval:inSection doc:{safe_id} .
        }}"#
    );

    let json = graph.sparql_select(&sparql)?;
    let rows = parse_sparql_results(&json)?;

    let mut matches = Vec::new();
    for row in rows {
        let evidence_iri = row.get("evidence").map(|s| strip_literal(s)).unwrap_or_default();
        let source = row.get("source").map(|s| strip_literal(s)).unwrap_or_default();
        let evidence_type = row
            .get("evidenceType")
            .map(|s| strip_literal(s))
            .unwrap_or_default();
        let has_quantified_outcome = row
            .get("hasQuantifiedOutcome")
            .map(|s| parse_bool(s))
            .unwrap_or(false);

        matches.push(EvidenceMatch {
            evidence_iri,
            source,
            evidence_type,
            has_quantified_outcome,
        });
    }
    Ok(matches)
}

// ============================================================================
// Score recording and retrieval
// ============================================================================

/// Record a score in the graph store.
pub fn record_score(graph: &GraphStore, score: &Score) -> anyhow::Result<usize> {
    load_score(graph, score)
}

/// Get all scores for a specific round.
pub fn get_scores_for_round(graph: &GraphStore, round: u32) -> anyhow::Result<Vec<Score>> {
    let sparql = format!(
        r#"PREFIX eval: <http://brain-in-the-fish.dev/eval/>
        PREFIX agent: <http://brain-in-the-fish.dev/agent/>
        PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
        SELECT ?agent ?criterion ?score ?maxScore ?justification WHERE {{
            ?s a eval:Score ;
                eval:agent ?agent ;
                eval:criterion ?criterion ;
                eval:score ?score ;
                eval:maxScore ?maxScore ;
                eval:round "{round}"^^xsd:integer ;
                eval:justification ?justification .
        }}"#
    );

    let json = graph.sparql_select(&sparql)?;
    let rows = parse_sparql_results(&json)?;

    let mut scores = Vec::new();
    for row in rows {
        let agent_raw = row.get("agent").map(|s| strip_literal(s)).unwrap_or_default();
        let criterion_raw = row
            .get("criterion")
            .map(|s| strip_literal(s))
            .unwrap_or_default();
        // Extract the local name from the IRI
        let agent_id = extract_local_name(&agent_raw);
        let criterion_id = extract_local_name(&criterion_raw);
        let score_val = row.get("score").map(|s| parse_decimal(s)).unwrap_or(0.0);
        let max_score = row.get("maxScore").map(|s| parse_decimal(s)).unwrap_or(0.0);
        let justification = row
            .get("justification")
            .map(|s| strip_literal(s))
            .unwrap_or_default();

        scores.push(Score {
            agent_id,
            criterion_id,
            score: score_val,
            max_score,
            round,
            justification,
            evidence_used: vec![],
            gaps_identified: vec![],
        });
    }
    Ok(scores)
}

/// Get all scores for a specific agent and criterion across all rounds.
pub fn get_score_history(
    graph: &GraphStore,
    agent_id: &str,
    criterion_id: &str,
) -> anyhow::Result<Vec<Score>> {
    let safe_agent = iri_safe(agent_id);
    let safe_crit = iri_safe(criterion_id);
    let sparql = format!(
        r#"PREFIX eval: <http://brain-in-the-fish.dev/eval/>
        PREFIX agent: <http://brain-in-the-fish.dev/agent/>
        PREFIX crit: <http://brain-in-the-fish.dev/criteria/>
        PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
        SELECT ?score ?maxScore ?round ?justification WHERE {{
            ?s a eval:Score ;
                eval:agent agent:{safe_agent} ;
                eval:criterion crit:{safe_crit} ;
                eval:score ?score ;
                eval:maxScore ?maxScore ;
                eval:round ?round ;
                eval:justification ?justification .
        }}"#
    );

    let json = graph.sparql_select(&sparql)?;
    let rows = parse_sparql_results(&json)?;

    let mut scores = Vec::new();
    for row in rows {
        let score_val = row.get("score").map(|s| parse_decimal(s)).unwrap_or(0.0);
        let max_score = row.get("maxScore").map(|s| parse_decimal(s)).unwrap_or(0.0);
        let round_val = row.get("round").map(|s| parse_integer(s)).unwrap_or(0);
        let justification = row
            .get("justification")
            .map(|s| strip_literal(s))
            .unwrap_or_default();

        scores.push(Score {
            agent_id: agent_id.to_string(),
            criterion_id: criterion_id.to_string(),
            score: score_val,
            max_score,
            round: round_val,
            justification,
            evidence_used: vec![],
            gaps_identified: vec![],
        });
    }
    Ok(scores)
}

/// Extract the local name from a full IRI.
/// e.g. "http://brain-in-the-fish.dev/agent/agent_1" -> "agent_1"
fn extract_local_name(iri: &str) -> String {
    iri.rsplit('/').next().unwrap_or(iri).to_string()
}

// ============================================================================
// Prompt generation
// ============================================================================

/// Generate the scoring prompt for a subagent.
///
/// This prompt is given to a Claude subagent to score a single criterion.
/// The subagent performs the actual LLM reasoning externally.
pub fn generate_scoring_prompt(
    agent: &EvaluatorAgent,
    criterion: &EvaluationCriterion,
    sections: &[SectionMatch],
    round: u32,
) -> String {
    format!(
        r#"You are {name}, a {role} with expertise in {domain}.

Your persona: {persona}

## Scoring Round {round}

## Your Task

Evaluate the following document content against this criterion:

**Criterion: {criterion_title}**
{criterion_description}

**Maximum Score: {max_score}**

**Rubric Levels:**
{rubric_text}

## Document Content to Evaluate

{section_content}

## Instructions

1. Read the document content carefully
2. Assess how well it meets the criterion
3. Reference specific rubric levels in your justification. State which level the document meets and what would be needed to reach the next level.
4. Identify specific evidence that supports your score
5. Identify gaps or weaknesses
6. Provide your score and detailed justification

Respond in this JSON format:
{{
    "score": <number>,
    "justification": "<detailed justification with specific references to document content>",
    "evidence_used": ["<specific quotes or references>"],
    "gaps_identified": ["<specific gaps or weaknesses>"]
}}"#,
        name = agent.name,
        role = agent.role,
        domain = agent.domain,
        persona = agent.persona_description,
        criterion_title = criterion.title,
        criterion_description = criterion.description.as_deref().unwrap_or(""),
        max_score = criterion.max_score,
        rubric_text = format_rubric(&criterion.rubric_levels),
        section_content = format_sections(sections),
    )
}

// ============================================================================
// What-if re-evaluation
// ============================================================================

/// Result of a what-if text change simulation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WhatIfResult {
    pub criterion_id: String,
    pub criterion_title: String,
    pub current_score: f64,
    pub estimated_new_score: f64,
    pub delta: f64,
    pub reasoning: String,
}

/// Simulate a "what if" text change — estimate how replacing a section's text
/// would affect scores on aligned criteria.
///
/// Uses simple text-quality heuristics (word count, claim density, evidence
/// patterns) to estimate score deltas without calling an LLM.
pub fn what_if_rescore(
    session: &EvaluationSession,
    section_id: &str,
    new_text: &str,
    alignments: &[AlignmentMapping],
) -> Vec<WhatIfResult> {
    // Find which criteria this section maps to
    let affected_criteria: Vec<&str> = alignments
        .iter()
        .filter(|a| a.section_id == section_id)
        .map(|a| a.criterion_id.as_str())
        .collect();

    if affected_criteria.is_empty() {
        return Vec::new();
    }

    // Find the original section text
    let original_text = session
        .document
        .sections
        .iter()
        .find(|s| s.id == section_id)
        .map(|s| s.text.as_str())
        .unwrap_or("");

    // Compute text quality signals for old and new text
    let old_metrics = TextMetrics::from_text(original_text);
    let new_metrics = TextMetrics::from_text(new_text);

    let mut results = Vec::new();

    for crit_id in &affected_criteria {
        // Find current score and criterion details
        let current_score = session
            .final_scores
            .iter()
            .find(|ms| ms.criterion_id == *crit_id)
            .map(|ms| ms.consensus_score)
            .unwrap_or(0.0);

        let criterion = session
            .framework
            .criteria
            .iter()
            .find(|c| c.id == *crit_id);

        let (title, max_score) = criterion
            .map(|c| (c.title.clone(), c.max_score))
            .unwrap_or_else(|| (crit_id.to_string(), 10.0));

        // Estimate score change based on text quality delta
        let quality_delta = new_metrics.quality_signal() - old_metrics.quality_signal();
        let estimated_change = quality_delta * max_score * 0.3; // 30% weight to text quality
        let estimated_new = (current_score + estimated_change).clamp(0.0, max_score);

        let reasoning = format!(
            "Word count: {} -> {} ({:+}). Evidence keywords: {} -> {} ({:+}). \
             Specificity markers: {} -> {} ({:+}). \
             Estimated quality delta: {:.2}",
            old_metrics.word_count,
            new_metrics.word_count,
            new_metrics.word_count as i64 - old_metrics.word_count as i64,
            old_metrics.evidence_keywords,
            new_metrics.evidence_keywords,
            new_metrics.evidence_keywords as i64 - old_metrics.evidence_keywords as i64,
            old_metrics.specificity_markers,
            new_metrics.specificity_markers,
            new_metrics.specificity_markers as i64 - old_metrics.specificity_markers as i64,
            quality_delta,
        );

        results.push(WhatIfResult {
            criterion_id: crit_id.to_string(),
            criterion_title: title,
            current_score,
            estimated_new_score: estimated_new,
            delta: estimated_new - current_score,
            reasoning,
        });
    }

    results
}

/// Simple text quality metrics for what-if estimation.
struct TextMetrics {
    word_count: usize,
    evidence_keywords: usize,
    specificity_markers: usize,
}

impl TextMetrics {
    fn from_text(text: &str) -> Self {
        let word_count = text.split_whitespace().count();
        let lower = text.to_lowercase();

        // Count evidence-related keywords
        let evidence_words = [
            "evidence", "data", "study", "research", "survey", "report",
            "analysis", "finding", "result", "outcome", "demonstrated",
            "measured", "achieved", "improved", "reduced", "increased",
        ];
        let evidence_keywords = evidence_words
            .iter()
            .map(|w| lower.matches(w).count())
            .sum();

        // Count specificity markers (numbers, percentages, dates)
        let specificity_markers = text
            .split_whitespace()
            .filter(|w| {
                w.contains('%')
                    || w.parse::<f64>().is_ok()
                    || (w.len() == 4 && w.parse::<u32>().map(|y| (1900..=2100).contains(&y)).unwrap_or(false))
            })
            .count();

        Self {
            word_count,
            evidence_keywords,
            specificity_markers,
        }
    }

    /// Normalised quality signal in [0.0, 1.0].
    fn quality_signal(&self) -> f64 {
        // Word count component (diminishing returns after 500 words)
        let wc = (self.word_count as f64 / 500.0).min(1.0);
        // Evidence density (per 100 words)
        let ev = if self.word_count > 0 {
            (self.evidence_keywords as f64 / (self.word_count as f64 / 100.0)).min(1.0)
        } else {
            0.0
        };
        // Specificity density (per 100 words)
        let sp = if self.word_count > 0 {
            (self.specificity_markers as f64 / (self.word_count as f64 / 100.0)).min(1.0)
        } else {
            0.0
        };
        // Weighted average
        wc * 0.3 + ev * 0.4 + sp * 0.3
    }
}

fn format_rubric(levels: &[RubricLevel]) -> String {
    levels
        .iter()
        .map(|l| format!("- **{}** ({}): {}", l.level, l.score_range, l.descriptor))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_sections(sections: &[SectionMatch]) -> String {
    if sections.is_empty() {
        return "No document content found for this criterion.".to_string();
    }
    sections
        .iter()
        .map(|s| format!("### {}\n\n{}", s.title, s.text))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::load_document_ontology;

    fn make_test_doc() -> EvalDocument {
        EvalDocument {
            id: "test-doc-1".into(),
            title: "Test Document".into(),
            doc_type: "essay".into(),
            total_pages: Some(5),
            total_word_count: Some(500),
            sections: vec![Section {
                id: "sec-1".into(),
                title: "Introduction".into(),
                text: "This is the introduction with some content about the topic.".into(),
                word_count: 10,
                page_range: None,
                claims: vec![Claim {
                    id: "claim-1".into(),
                    text: "We achieved 99% accuracy.".into(),
                    specificity: 0.9,
                    verifiable: true,
                }],
                evidence: vec![Evidence {
                    id: "ev-1".into(),
                    source: "Internal report 2024".into(),
                    evidence_type: "case_study".into(),
                    text: "The trial showed improvement.".into(),
                    has_quantified_outcome: true,
                }],
                subsections: vec![],
            }],
        }
    }

    fn make_test_agent() -> EvaluatorAgent {
        EvaluatorAgent {
            id: "agent-1".into(),
            name: "Dr. Torres".into(),
            role: "Subject Expert".into(),
            domain: "Economics".into(),
            years_experience: Some(15),
            persona_description: "Deep knowledge of macroeconomics".into(),
            needs: vec![],
            trust_weights: vec![],
        }
    }

    fn make_test_criterion() -> EvaluationCriterion {
        EvaluationCriterion {
            id: "crit-1".into(),
            title: "Knowledge and Understanding".into(),
            description: Some("Demonstrates understanding of the subject.".into()),
            max_score: 10.0,
            weight: 0.4,
            rubric_levels: vec![
                RubricLevel {
                    level: "Excellent".into(),
                    score_range: "9-10".into(),
                    descriptor: "Outstanding depth and breadth of knowledge.".into(),
                },
                RubricLevel {
                    level: "Good".into(),
                    score_range: "7-8".into(),
                    descriptor: "Solid understanding with some depth.".into(),
                },
                RubricLevel {
                    level: "Adequate".into(),
                    score_range: "5-6".into(),
                    descriptor: "Basic understanding demonstrated.".into(),
                },
            ],
            sub_criteria: vec![],
        }
    }

    #[test]
    fn test_record_and_query_score() {
        let graph = GraphStore::new();
        let doc = make_test_doc();
        load_document_ontology(&graph, &doc).unwrap();

        let score = Score {
            agent_id: "agent-1".into(),
            criterion_id: "crit-1".into(),
            score: 7.0,
            max_score: 10.0,
            round: 1,
            justification: "Good evidence".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        };

        let triples = record_score(&graph, &score).unwrap();
        assert!(triples > 0, "Should insert triples, got {}", triples);

        let scores = get_scores_for_round(&graph, 1).unwrap();
        assert_eq!(scores.len(), 1, "Should find 1 score, got {}", scores.len());
        assert!(
            (scores[0].score - 7.0).abs() < 1e-10,
            "Score should be 7.0, got {}",
            scores[0].score
        );
        assert!(
            (scores[0].max_score - 10.0).abs() < 1e-10,
            "Max score should be 10.0, got {}",
            scores[0].max_score
        );
        assert_eq!(scores[0].justification, "Good evidence");
    }

    #[test]
    fn test_query_sections() {
        let graph = GraphStore::new();
        let doc = make_test_doc();
        load_document_ontology(&graph, &doc).unwrap();

        let sections = query_sections_for_criterion(&graph, "any-criterion").unwrap();
        assert!(
            !sections.is_empty(),
            "Should find loaded sections, got {}",
            sections.len()
        );
        assert_eq!(sections[0].title, "Introduction");
        assert!(sections[0].text.contains("introduction"));
        assert_eq!(sections[0].word_count, 10);
    }

    #[test]
    fn test_query_claims_for_section() {
        let graph = GraphStore::new();
        let doc = make_test_doc();
        load_document_ontology(&graph, &doc).unwrap();

        let claims = query_claims_for_section(&graph, "sec-1").unwrap();
        assert_eq!(claims.len(), 1, "Should find 1 claim, got {}", claims.len());
        assert!(claims[0].text.contains("99% accuracy"));
        assert!((claims[0].specificity - 0.9).abs() < 1e-10);
        assert!(claims[0].verifiable);
    }

    #[test]
    fn test_query_evidence_for_section() {
        let graph = GraphStore::new();
        let doc = make_test_doc();
        load_document_ontology(&graph, &doc).unwrap();

        let evidence = query_evidence_for_section(&graph, "sec-1").unwrap();
        assert_eq!(
            evidence.len(),
            1,
            "Should find 1 evidence, got {}",
            evidence.len()
        );
        assert_eq!(evidence[0].source, "Internal report 2024");
        assert_eq!(evidence[0].evidence_type, "case_study");
        assert!(evidence[0].has_quantified_outcome);
    }

    #[test]
    fn test_generate_scoring_prompt() {
        let agent = make_test_agent();
        let criterion = make_test_criterion();
        let sections = vec![SectionMatch {
            section_iri: "http://example.org/sec-1".into(),
            title: "Introduction".into(),
            text: "This is the content.".into(),
            word_count: 4,
        }];

        let prompt = generate_scoring_prompt(&agent, &criterion, &sections, 1);
        assert!(prompt.contains("Dr. Torres"), "Should contain agent name");
        assert!(
            prompt.contains("Knowledge and Understanding"),
            "Should contain criterion title"
        );
        assert!(prompt.contains("score"), "Should mention score");
        assert!(
            prompt.contains("Excellent"),
            "Should contain rubric level"
        );
        assert!(
            prompt.contains("9-10"),
            "Should contain score range"
        );
        assert!(
            prompt.contains("Introduction"),
            "Should contain section title"
        );
        assert!(
            prompt.contains("This is the content."),
            "Should contain section text"
        );
        assert!(
            prompt.contains("Round 1"),
            "Should contain round number"
        );
        assert!(
            prompt.contains("Economics"),
            "Should contain agent domain"
        );
    }

    #[test]
    fn test_generate_scoring_prompt_empty_sections() {
        let agent = make_test_agent();
        let criterion = make_test_criterion();
        let sections: Vec<SectionMatch> = vec![];

        let prompt = generate_scoring_prompt(&agent, &criterion, &sections, 1);
        assert!(
            prompt.contains("No document content found"),
            "Should indicate no content when sections empty"
        );
    }

    #[test]
    fn test_score_history() {
        let graph = GraphStore::new();

        // Record two scores for the same agent+criterion across different rounds
        let score_r1 = Score {
            agent_id: "agent-1".into(),
            criterion_id: "crit-1".into(),
            score: 6.0,
            max_score: 10.0,
            round: 1,
            justification: "Initial assessment".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        };
        let score_r2 = Score {
            agent_id: "agent-1".into(),
            criterion_id: "crit-1".into(),
            score: 8.0,
            max_score: 10.0,
            round: 2,
            justification: "Revised after debate".into(),
            evidence_used: vec![],
            gaps_identified: vec![],
        };

        record_score(&graph, &score_r1).unwrap();
        record_score(&graph, &score_r2).unwrap();

        let history = get_score_history(&graph, "agent-1", "crit-1").unwrap();
        assert_eq!(
            history.len(),
            2,
            "Should find 2 scores in history, got {}",
            history.len()
        );

        // Verify both rounds are present
        let rounds: Vec<u32> = history.iter().map(|s| s.round).collect();
        assert!(rounds.contains(&1), "Should contain round 1");
        assert!(rounds.contains(&2), "Should contain round 2");
    }

    #[test]
    fn test_get_scores_for_round_empty() {
        let graph = GraphStore::new();
        let scores = get_scores_for_round(&graph, 1).unwrap();
        assert!(scores.is_empty(), "Should return empty for no scores");
    }

    #[test]
    fn test_strip_literal() {
        assert_eq!(strip_literal("\"hello\""), "hello");
        assert_eq!(
            strip_literal("\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
            "42"
        );
        assert_eq!(
            strip_literal("<http://example.org/foo>"),
            "http://example.org/foo"
        );
        assert_eq!(strip_literal("\"text\"@en"), "text");
        assert_eq!(strip_literal("plain"), "plain");
    }

    #[test]
    fn test_extract_local_name() {
        assert_eq!(
            extract_local_name("http://brain-in-the-fish.dev/agent/agent_1"),
            "agent_1"
        );
        assert_eq!(extract_local_name("just_a_name"), "just_a_name");
    }

    #[test]
    fn test_format_rubric() {
        let levels = vec![
            RubricLevel {
                level: "Excellent".into(),
                score_range: "9-10".into(),
                descriptor: "Outstanding work.".into(),
            },
            RubricLevel {
                level: "Poor".into(),
                score_range: "1-3".into(),
                descriptor: "Below expectations.".into(),
            },
        ];
        let formatted = format_rubric(&levels);
        assert!(formatted.contains("**Excellent** (9-10): Outstanding work."));
        assert!(formatted.contains("**Poor** (1-3): Below expectations."));
    }

    #[test]
    fn test_format_sections() {
        let sections = vec![
            SectionMatch {
                section_iri: "iri1".into(),
                title: "First".into(),
                text: "Content 1.".into(),
                word_count: 2,
            },
            SectionMatch {
                section_iri: "iri2".into(),
                title: "Second".into(),
                text: "Content 2.".into(),
                word_count: 2,
            },
        ];
        let formatted = format_sections(&sections);
        assert!(formatted.contains("### First"));
        assert!(formatted.contains("### Second"));
        assert!(formatted.contains("---"));
    }

    #[test]
    fn test_multiple_scores_different_rounds() {
        let graph = GraphStore::new();

        for round in 1..=3 {
            let score = Score {
                agent_id: "agent-1".into(),
                criterion_id: "crit-1".into(),
                score: 5.0 + round as f64,
                max_score: 10.0,
                round,
                justification: format!("Round {} assessment", round),
                evidence_used: vec![],
                gaps_identified: vec![],
            };
            record_score(&graph, &score).unwrap();
        }

        let r1 = get_scores_for_round(&graph, 1).unwrap();
        let r2 = get_scores_for_round(&graph, 2).unwrap();
        let r3 = get_scores_for_round(&graph, 3).unwrap();

        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 1);
        assert_eq!(r3.len(), 1);

        assert!((r1[0].score - 6.0).abs() < 1e-10);
        assert!((r2[0].score - 7.0).abs() < 1e-10);
        assert!((r3[0].score - 8.0).abs() < 1e-10);
    }

    #[test]
    fn test_what_if_rescore_improved_text() {
        let session = EvaluationSession {
            id: "sess-1".into(),
            document: EvalDocument {
                id: "doc-1".into(),
                title: "Test".into(),
                doc_type: "essay".into(),
                total_pages: None,
                total_word_count: None,
                sections: vec![Section {
                    id: "sec-1".into(),
                    title: "Intro".into(),
                    text: "Short text.".into(),
                    word_count: 2,
                    page_range: None,
                    claims: vec![],
                    evidence: vec![],
                    subsections: vec![],
                }],
            },
            framework: EvaluationFramework {
                id: "fw-1".into(),
                name: "Test FW".into(),
                total_weight: 1.0,
                pass_mark: None,
                criteria: vec![EvaluationCriterion {
                    id: "crit-1".into(),
                    title: "Quality".into(),
                    description: None,
                    max_score: 10.0,
                    weight: 1.0,
                    rubric_levels: vec![],
                    sub_criteria: vec![],
                }],
            },
            agents: vec![],
            alignments: vec![],
            gaps: vec![],
            rounds: vec![],
            final_scores: vec![ModeratedScore {
                criterion_id: "crit-1".into(),
                consensus_score: 5.0,
                panel_mean: 5.0,
                panel_std_dev: 0.5,
                dissents: vec![],
            }],
            created_at: "2026-01-01T00:00:00Z".into(),
        };

        let alignments = vec![AlignmentMapping {
            section_id: "sec-1".into(),
            criterion_id: "crit-1".into(),
            confidence: 0.9,
        }];

        let new_text = "This is a much longer and more detailed section. \
            The research evidence demonstrates that outcomes improved by 25% \
            in the 2024 study. Data analysis showed measured results with \
            quantified findings across multiple surveys.";

        let results = what_if_rescore(&session, "sec-1", new_text, &alignments);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].criterion_id, "crit-1");
        // Better text should produce a positive delta
        assert!(results[0].delta > 0.0, "Expected positive delta, got {}", results[0].delta);
    }

    #[test]
    fn test_what_if_rescore_no_alignment() {
        let session = EvaluationSession {
            id: "sess-1".into(),
            document: EvalDocument {
                id: "doc-1".into(),
                title: "Test".into(),
                doc_type: "essay".into(),
                total_pages: None,
                total_word_count: None,
                sections: vec![],
            },
            framework: EvaluationFramework {
                id: "fw-1".into(),
                name: "Test FW".into(),
                total_weight: 1.0,
                pass_mark: None,
                criteria: vec![],
            },
            agents: vec![],
            alignments: vec![],
            gaps: vec![],
            rounds: vec![],
            final_scores: vec![],
            created_at: "2026-01-01T00:00:00Z".into(),
        };

        let results = what_if_rescore(&session, "sec-1", "new text", &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_scoring_prompt_includes_rubric_reference_instruction() {
        let agent = make_test_agent();
        let criterion = make_test_criterion();
        let sections = vec![];
        let prompt = generate_scoring_prompt(&agent, &criterion, &sections, 1);
        assert!(
            prompt.contains("Reference specific rubric levels"),
            "Prompt should instruct agents to reference rubric levels"
        );
        assert!(
            prompt.contains("which level the document meets"),
            "Prompt should ask which level is met"
        );
        assert!(
            prompt.contains("needed to reach the next level"),
            "Prompt should ask what's needed for next level"
        );
    }
}
