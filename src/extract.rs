//! Hybrid evidence extraction — rule-based + LLM-assisted.
//!
//! Stage 1: Rule-based (fast, deterministic, high precision)
//! Stage 2: LLM-assisted (catches implicit claims, complex arguments)
//! Stage 3: Confidence scoring (each item gets extraction_method + confidence + provenance)
//!
//! The SNN spike strength is modulated by extraction confidence:
//!   spike_strength = base_strength × extraction_confidence

use crate::types::*;
use serde::{Deserialize, Serialize};

/// An extracted item with provenance and confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedItem {
    pub id: String,
    pub item_type: ExtractedType,
    pub text: String,
    pub source_span: Option<(usize, usize)>, // character offsets
    pub extraction_method: ExtractionMethod,
    pub confidence: f64, // 0.0-1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExtractedType {
    Claim,
    Statistic,
    Citation,
    Prediction,
    Commitment,
    Evidence,
    Argument,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExtractionMethod {
    Rule,   // regex/pattern — higher default confidence
    Llm,    // LLM-extracted — lower default confidence
    Hybrid, // confirmed by both — highest confidence
}

/// Run the full hybrid extraction pipeline on a section.
pub fn extract_all(text: &str) -> Vec<ExtractedItem> {
    let mut items = Vec::new();

    // Stage 1: Rule-based extraction
    items.extend(extract_citations_rule(text));
    items.extend(extract_statistics_rule(text));
    items.extend(extract_claims_rule(text));
    items.extend(extract_predictions_rule(text));
    items.extend(extract_commitments_rule(text));

    // Deduplicate overlapping extractions
    deduplicate(&mut items);

    items
}

/// Extract citations using regex patterns.
/// High confidence — citation formats are unambiguous.
fn extract_citations_rule(text: &str) -> Vec<ExtractedItem> {
    let mut items = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '('
            && let Some(close_offset) = chars[i..].iter().position(|&c| c == ')')
        {
            let inner: String = chars[i + 1..i + close_offset].iter().collect();
            // Check for author-year pattern
            if has_year_pattern(&inner) && inner.len() < 60 {
                let full: String = chars[i..=i + close_offset].iter().collect();
                items.push(ExtractedItem {
                    id: uuid::Uuid::new_v4().to_string(),
                    item_type: ExtractedType::Citation,
                    text: full,
                    source_span: Some((i, i + close_offset + 1)),
                    extraction_method: ExtractionMethod::Rule,
                    confidence: 0.95, // high — citation format is clear
                });
            }
            i += close_offset + 1;
            continue;
        }
        i += 1;
    }

    items
}

/// Extract statistics and quantified claims.
fn extract_statistics_rule(text: &str) -> Vec<ExtractedItem> {
    let mut items = Vec::new();
    let sentences = split_sentences(text);

    for sentence in &sentences {
        let has_number = sentence.chars().any(|c| c.is_ascii_digit());
        let has_unit = sentence.contains('%')
            || sentence.contains('\u{00a3}')
            || sentence.contains('$')
            || sentence.to_lowercase().contains("million")
            || sentence.to_lowercase().contains("billion")
            || sentence.to_lowercase().contains("percentage point");

        if has_number && has_unit {
            items.push(ExtractedItem {
                id: uuid::Uuid::new_v4().to_string(),
                item_type: ExtractedType::Statistic,
                text: sentence.clone(),
                source_span: find_span(text, sentence),
                extraction_method: ExtractionMethod::Rule,
                confidence: 0.85, // numbers with units are reliable
            });
        }
    }

    items
}

/// Extract explicit claims (argument markers).
fn extract_claims_rule(text: &str) -> Vec<ExtractedItem> {
    let mut items = Vec::new();
    let sentences = split_sentences(text);

    let claim_markers = [
        "argues that",
        "contends that",
        "demonstrates that",
        "shows that",
        "suggests that",
        "indicates that",
        "reveals that",
        "confirms that",
        "this essay argues",
        "i argue",
        "we argue",
        "it is argued",
        "the evidence shows",
        "the data suggests",
        "findings indicate",
        "this paper contends",
        "i contend",
        "we contend",
    ];

    for sentence in &sentences {
        let lower = sentence.to_lowercase();
        if claim_markers.iter().any(|m| lower.contains(m)) {
            items.push(ExtractedItem {
                id: uuid::Uuid::new_v4().to_string(),
                item_type: ExtractedType::Claim,
                text: sentence.clone(),
                source_span: find_span(text, sentence),
                extraction_method: ExtractionMethod::Rule,
                confidence: 0.80,
            });
        }
    }

    items
}

/// Extract predictions/forecasts.
fn extract_predictions_rule(text: &str) -> Vec<ExtractedItem> {
    let mut items = Vec::new();
    let sentences = split_sentences(text);

    let prediction_markers = [
        "will lead to",
        "will result in",
        "is expected to",
        "is projected to",
        "is likely to",
        "will increase",
        "will decrease",
        "will reduce",
        "is forecast",
        "is anticipated",
        "we predict",
        "the model predicts",
    ];

    for sentence in &sentences {
        let lower = sentence.to_lowercase();
        if prediction_markers.iter().any(|m| lower.contains(m)) {
            items.push(ExtractedItem {
                id: uuid::Uuid::new_v4().to_string(),
                item_type: ExtractedType::Prediction,
                text: sentence.clone(),
                source_span: find_span(text, sentence),
                extraction_method: ExtractionMethod::Rule,
                confidence: 0.75,
            });
        }
    }

    items
}

/// Extract commitments ("we will", "we commit").
fn extract_commitments_rule(text: &str) -> Vec<ExtractedItem> {
    let mut items = Vec::new();
    let sentences = split_sentences(text);

    let commit_markers = [
        "we will",
        "we shall",
        "we commit",
        "we pledge",
        "we guarantee",
        "the organisation will",
        "the company will",
        "the authority will",
    ];

    for sentence in &sentences {
        let lower = sentence.to_lowercase();
        if commit_markers.iter().any(|m| lower.contains(m)) {
            items.push(ExtractedItem {
                id: uuid::Uuid::new_v4().to_string(),
                item_type: ExtractedType::Commitment,
                text: sentence.clone(),
                source_span: find_span(text, sentence),
                extraction_method: ExtractionMethod::Rule,
                confidence: 0.85,
            });
        }
    }

    items
}

/// LLM-assisted extraction for a section.
/// Returns additional items the rules missed.
/// The extraction LLM call is SEPARATE from scoring — it never sees the rubric.
pub async fn extract_with_llm(
    text: &str,
    intent: &str,
) -> anyhow::Result<Vec<ExtractedItem>> {
    let claude = crate::llm::ClaudeClient::from_env()?;

    let prompt = format!(
        "Extract claims, evidence, arguments, and predictions from the following text.\n\
         Context: {}\n\n\
         For each item, classify as: Claim, Statistic, Citation, Evidence, Argument, Prediction, or Commitment.\n\n\
         Text:\n{}\n\n\
         Respond as JSON array: [{{\"type\": \"Claim\", \"text\": \"...\", \"specificity\": 0.8}}]\n\
         Only include items actually present in the text. Do not invent or infer.",
        intent, text
    );

    let response = claude.raw_completion(&prompt).await?;

    // Parse JSON response
    let parsed: Vec<LlmExtraction> = serde_json::from_str(&response).unwrap_or_default();

    let items = parsed
        .into_iter()
        .map(|e| {
            let item_type = match e.item_type.to_lowercase().as_str() {
                "claim" => ExtractedType::Claim,
                "statistic" => ExtractedType::Statistic,
                "citation" => ExtractedType::Citation,
                "evidence" => ExtractedType::Evidence,
                "argument" => ExtractedType::Argument,
                "prediction" => ExtractedType::Prediction,
                "commitment" => ExtractedType::Commitment,
                _ => ExtractedType::Claim,
            };

            ExtractedItem {
                id: uuid::Uuid::new_v4().to_string(),
                item_type,
                text: e.text,
                source_span: None, // LLM doesn't provide spans
                extraction_method: ExtractionMethod::Llm,
                confidence: 0.60 * e.specificity.unwrap_or(0.7), // lower base confidence
            }
        })
        .collect();

    Ok(items)
}

#[derive(Debug, Deserialize)]
struct LlmExtraction {
    #[serde(rename = "type")]
    item_type: String,
    text: String,
    specificity: Option<f64>,
}

/// Merge rule-based and LLM-based extractions.
/// Items found by both methods get boosted confidence (Hybrid).
pub fn merge_extractions(
    rule_items: &[ExtractedItem],
    llm_items: &[ExtractedItem],
) -> Vec<ExtractedItem> {
    let mut merged = rule_items.to_vec();

    for llm_item in llm_items {
        // Check if this overlaps with a rule-extracted item
        let overlap = merged
            .iter_mut()
            .find(|r| text_similarity(&r.text, &llm_item.text) > 0.6);

        if let Some(existing) = overlap {
            // Both methods found it — boost confidence
            existing.extraction_method = ExtractionMethod::Hybrid;
            existing.confidence = (existing.confidence + llm_item.confidence).min(1.0) * 0.9;
        } else {
            // Only LLM found it — add with LLM confidence
            merged.push(llm_item.clone());
        }
    }

    merged
}

/// Convert extracted items to the existing Claim/Evidence types for pipeline compatibility.
pub fn to_claims_and_evidence(items: &[ExtractedItem]) -> (Vec<Claim>, Vec<Evidence>) {
    let mut claims = Vec::new();
    let mut evidence = Vec::new();

    for item in items {
        match item.item_type {
            ExtractedType::Claim
            | ExtractedType::Argument
            | ExtractedType::Prediction
            | ExtractedType::Commitment => {
                claims.push(Claim {
                    id: item.id.clone(),
                    text: item.text.clone(),
                    specificity: item.confidence,
                    verifiable: matches!(
                        item.item_type,
                        ExtractedType::Prediction | ExtractedType::Commitment
                    ),
                });
            }
            ExtractedType::Statistic | ExtractedType::Citation | ExtractedType::Evidence => {
                evidence.push(Evidence {
                    id: item.id.clone(),
                    source: match item.extraction_method {
                        ExtractionMethod::Rule => "rule-extracted".into(),
                        ExtractionMethod::Llm => "llm-extracted".into(),
                        ExtractionMethod::Hybrid => "hybrid-extracted".into(),
                    },
                    evidence_type: format!("{:?}", item.item_type).to_lowercase(),
                    text: item.text.clone(),
                    has_quantified_outcome: item.item_type == ExtractedType::Statistic,
                });
            }
        }
    }

    (claims, evidence)
}

/// Compute SNN spike strength modulated by extraction confidence.
pub fn spike_strength(base: f64, item: &ExtractedItem) -> f64 {
    base * item.confidence
}

// ---- Helpers ----

fn has_year_pattern(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_digit()).any(|word| {
        word.len() == 4
            && word
                .parse::<u32>()
                .map(|y| (1900..=2030).contains(&y))
                .unwrap_or(false)
    })
}

fn split_sentences(text: &str) -> Vec<String> {
    text.split('.')
        .map(|s| s.trim().to_string())
        .filter(|s| s.len() > 15)
        .collect()
}

fn find_span(full_text: &str, sentence: &str) -> Option<(usize, usize)> {
    full_text
        .find(sentence)
        .map(|start| (start, start + sentence.len()))
}

fn deduplicate(items: &mut Vec<ExtractedItem>) {
    items.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen_texts: Vec<String> = Vec::new();
    items.retain(|item| {
        let dominated = seen_texts
            .iter()
            .any(|s| text_similarity(s, &item.text) > 0.7);
        if !dominated {
            seen_texts.push(item.text.clone());
        }
        !dominated
    });
}

fn text_similarity(a: &str, b: &str) -> f64 {
    let a_words: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let b_words: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if a_words.is_empty() || b_words.is_empty() {
        return 0.0;
    }
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();
    intersection as f64 / union as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_citations() {
        let text = "According to Joyce et al. (2012), QE was effective. Bernanke (2009) agreed.";
        let items = extract_citations_rule(text);
        assert!(
            items.len() >= 2,
            "Should find 2 citations, got {}",
            items.len()
        );
        assert!(items.iter().all(|i| i.item_type == ExtractedType::Citation));
        assert!(items.iter().all(|i| i.confidence > 0.9));
    }

    #[test]
    fn test_extract_statistics() {
        let text = "GDP increased by 2.5% in Q3. The programme cost \u{00a3}45 million over 3 years.";
        let items = extract_statistics_rule(text);
        assert!(!items.is_empty(), "Should find statistics");
        assert!(items
            .iter()
            .all(|i| i.item_type == ExtractedType::Statistic));
    }

    #[test]
    fn test_extract_claims() {
        let text =
            "This essay argues that QE was necessary. The evidence shows a strong correlation.";
        let items = extract_claims_rule(text);
        assert!(!items.is_empty(), "Should find claims");
    }

    #[test]
    fn test_extract_predictions() {
        let text = "This policy will lead to a 50% reduction. Costs are projected to increase.";
        let items = extract_predictions_rule(text);
        assert!(!items.is_empty(), "Should find predictions");
    }

    #[test]
    fn test_extract_commitments() {
        let text = "We will deliver the programme within 18 months. The organisation will hire 2 apprentices.";
        let items = extract_commitments_rule(text);
        assert_eq!(items.len(), 2, "Should find 2 commitments");
    }

    #[test]
    fn test_extract_all() {
        let text = "Joyce et al. (2012) found a 100 basis point reduction. This essay argues that QE was effective. We will deliver results within 12 months.";
        let items = extract_all(text);
        assert!(
            items.len() >= 3,
            "Should find citation + statistic + commitment, got {}",
            items.len()
        );
    }

    #[test]
    fn test_confidence_ordering() {
        let items = extract_all(
            "According to Bernanke (2009), inflation rose by 2.5%. This essay argues that monetary policy was effective.",
        );
        // Citations should have highest confidence, claims lower
        let citation_conf = items
            .iter()
            .filter(|i| i.item_type == ExtractedType::Citation)
            .map(|i| i.confidence)
            .next()
            .unwrap_or(0.0);
        let claim_conf = items
            .iter()
            .filter(|i| i.item_type == ExtractedType::Claim)
            .map(|i| i.confidence)
            .next()
            .unwrap_or(0.0);
        assert!(
            citation_conf >= claim_conf,
            "Citations ({}) should have >= confidence than claims ({})",
            citation_conf,
            claim_conf
        );
    }

    #[test]
    fn test_to_claims_and_evidence() {
        let items = vec![
            ExtractedItem {
                id: "1".into(),
                item_type: ExtractedType::Claim,
                text: "claim".into(),
                source_span: None,
                extraction_method: ExtractionMethod::Rule,
                confidence: 0.8,
            },
            ExtractedItem {
                id: "2".into(),
                item_type: ExtractedType::Statistic,
                text: "stat".into(),
                source_span: None,
                extraction_method: ExtractionMethod::Rule,
                confidence: 0.9,
            },
        ];
        let (claims, evidence) = to_claims_and_evidence(&items);
        assert_eq!(claims.len(), 1);
        assert_eq!(evidence.len(), 1);
    }

    #[test]
    fn test_spike_strength_modulation() {
        let item_high = ExtractedItem {
            id: "1".into(),
            item_type: ExtractedType::Statistic,
            text: "test".into(),
            source_span: None,
            extraction_method: ExtractionMethod::Rule,
            confidence: 0.95,
        };
        let item_low = ExtractedItem {
            id: "2".into(),
            item_type: ExtractedType::Claim,
            text: "test".into(),
            source_span: None,
            extraction_method: ExtractionMethod::Llm,
            confidence: 0.40,
        };

        assert!(spike_strength(1.0, &item_high) > spike_strength(1.0, &item_low));
    }

    #[test]
    fn test_merge_extractions() {
        let rule = vec![ExtractedItem {
            id: "1".into(),
            item_type: ExtractedType::Citation,
            text: "Joyce et al 2012 found significant effects".into(),
            source_span: None,
            extraction_method: ExtractionMethod::Rule,
            confidence: 0.9,
        }];
        let llm = vec![
            ExtractedItem {
                id: "2".into(),
                item_type: ExtractedType::Citation,
                text: "Joyce et al 2012 found significant effects on yields".into(),
                source_span: None,
                extraction_method: ExtractionMethod::Llm,
                confidence: 0.6,
            },
            ExtractedItem {
                id: "3".into(),
                item_type: ExtractedType::Argument,
                text: "The policy was counterproductive".into(),
                source_span: None,
                extraction_method: ExtractionMethod::Llm,
                confidence: 0.5,
            },
        ];
        let merged = merge_extractions(&rule, &llm);
        // Should merge the overlapping citation and add the new argument
        assert!(merged.len() >= 2);
        // The merged citation should have Hybrid method
        let hybrid = merged
            .iter()
            .find(|i| i.extraction_method == ExtractionMethod::Hybrid);
        assert!(
            hybrid.is_some(),
            "Overlapping items should become Hybrid"
        );
    }

    #[test]
    fn test_deduplicate() {
        let mut items = vec![
            ExtractedItem {
                id: "1".into(),
                item_type: ExtractedType::Claim,
                text: "QE was effective at reducing yields".into(),
                source_span: None,
                extraction_method: ExtractionMethod::Rule,
                confidence: 0.8,
            },
            ExtractedItem {
                id: "2".into(),
                item_type: ExtractedType::Claim,
                text: "QE was effective at reducing gilt yields".into(),
                source_span: None,
                extraction_method: ExtractionMethod::Rule,
                confidence: 0.7,
            },
        ];
        deduplicate(&mut items);
        assert_eq!(items.len(), 1, "Near-duplicates should be merged");
        assert_eq!(items[0].confidence, 0.8, "Should keep higher confidence");
    }
}
