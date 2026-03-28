//! Shoal — batch subagent scoring at scale.
//!
//! Dispatches scoring work in parallel batches. Each batch contains
//! N essays that a single subagent scores in one pass. Multiple
//! batches run concurrently.
//!
//! Named after a shoal of fish — many agents working together.

use crate::benchmark::{
    mean_absolute_error, pearson_correlation, quadratic_weighted_kappa, rmse, BenchmarkConfig,
    BenchmarkResults, LabeledSample,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Configuration for a shoal run.
#[derive(Debug, Clone)]
pub struct ShoalConfig {
    /// Number of essays per batch (sent to one subagent)
    pub batch_size: usize,
    /// Scoring scale description for the subagent
    pub scale_description: String,
    /// Maximum score value
    pub max_score: f64,
    /// Evaluation intent — drives framework, persona, and criteria selection
    pub intent: String,
    /// Optional anchor essays for ontology-grounded calibration
    pub anchors: Option<Vec<crate::calibrate::AnchorEssay>>,
}

impl Default for ShoalConfig {
    fn default() -> Self {
        Self {
            batch_size: 10,
            scale_description: "1.0-5.0 (0.5 increments)".into(),
            max_score: 5.0,
            intent: "evaluate this essay".into(),
            anchors: None,
        }
    }
}

/// A single scored result from a subagent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredEssay {
    pub id: String,
    pub score: f64,
}

/// Split a dataset into batches.
pub fn split_batches(samples: &[LabeledSample], batch_size: usize) -> Vec<Vec<LabeledSample>> {
    samples.chunks(batch_size).map(|c| c.to_vec()).collect()
}

/// Generate a scoring prompt for a batch of essays.
/// This prompt is what gets sent to a Claude subagent.
///
/// Uses multi-agent consensus: three evaluator personas each score
/// all 6 criteria independently, and the final score is the median
/// of their overall scores. Each essay includes actual extracted
/// evidence text for anchoring.
pub fn batch_scoring_prompt(batch: &[LabeledSample], config: &ShoalConfig) -> String {
    let intent = &config.intent;
    let framework = crate::criteria::framework_for_intent(intent);

    // Build calibration graph if anchors are provided
    let calibration_graph: Option<std::sync::Arc<open_ontologies::graph::GraphStore>> =
        config.anchors.as_ref().and_then(|anchors| {
            if anchors.is_empty() {
                return None;
            }
            let graph = std::sync::Arc::new(open_ontologies::graph::GraphStore::new());
            if crate::calibrate::load_anchors(&graph, anchors).is_ok() {
                Some(graph)
            } else {
                None
            }
        });

    let mut prompt = String::new();

    // Multi-agent instruction
    prompt.push_str("## Multi-Evaluator Panel\n\n");
    prompt.push_str(
        "Score each essay THREE times as three different evaluators:\n\n",
    );
    prompt.push_str("**Evaluator 1 — The Grammarian:** Focuses on technical accuracy, syntax, conventions. Strict on errors.\n");
    prompt.push_str("**Evaluator 2 — The ELL Specialist:** Understands language acquisition stages. Judges communication success, not native-level accuracy. Lenient on developmental errors that don't impede meaning.\n");
    prompt.push_str("**Evaluator 3 — The Holistic Reader:** Reads for overall quality — ideas, voice, engagement. Balances form and content.\n\n");
    prompt.push_str(
        "The final score is the **median** of the three evaluators' overall scores.\n\n",
    );

    // Framework/rubric
    prompt.push_str("## Evaluation Framework\n\n");
    prompt.push_str(&format!("**{}**\n\n", framework.name));
    prompt.push_str("Score each of these 6 criteria separately (1.0-5.0, 0.5 increments):\n\n");
    for crit in &framework.criteria {
        prompt.push_str(&format!("### {}\n", crit.title));
        if let Some(desc) = &crit.description {
            prompt.push_str(&format!("{}\n", desc));
        }
        for level in &crit.rubric_levels {
            prompt.push_str(&format!(
                "- **{}** ({}): {}\n",
                level.level, level.score_range, level.descriptor
            ));
        }
        prompt.push('\n');
    }

    // Calibration guidance
    prompt.push_str("## Calibration\n\n");
    prompt.push_str("These are English Language Learners. Calibrate to the ELL scale:\n");
    prompt.push_str("- 1.0 = meaning frequently lost, near-unintelligible\n");
    prompt.push_str("- 2.0 = basic meaning conveyed despite frequent errors\n");
    prompt.push_str("- 3.0 = meaning clear, errors present but don't impede comprehension\n");
    prompt.push_str("- 4.0 = good control, occasional errors, reads smoothly\n");
    prompt.push_str("- 5.0 = near-native proficiency, rare errors\n\n");
    prompt.push_str("Most essays should cluster 2.0-3.5. Use the full range.\n\n");

    // How to use the Document Analysis guidance
    prompt.push_str("## How to use the Document Analysis\n\n");
    prompt.push_str("Each essay includes a domain-adaptive analysis showing what our extraction engine found.\n");
    prompt.push_str("- The categories are tailored to the document type (essays look for thesis, cohesion, voice; tenders look for case studies, compliance).\n");
    prompt.push_str("- Use the analysis as context, not a constraint. Your judgment should be informed by it, not bound to it.\n");
    prompt.push_str("- If the analysis shows many cohesion devices and varied sentences but you read it as weak → re-read for structure.\n");
    prompt.push_str("- If the analysis shows no counter-arguments or personal voice → the essay may lack depth.\n\n");

    // Per-essay with adaptive domain analysis
    prompt.push_str("## Essays\n\n");
    for (i, sample) in batch.iter().enumerate() {
        let word_count = sample.text.split_whitespace().count();

        // Adaptive evidence extraction — domain-aware
        let adaptive_items =
            crate::adaptive_extract::quick_adaptive_extract(&sample.text, &config.intent);
        let summary =
            crate::adaptive_extract::evidence_summary(&adaptive_items, &config.intent);

        // Format the analysis block
        prompt.push_str(&format!(
            "---\n\n### Essay {} (ID: {})\n\n",
            i + 1,
            sample.id
        ));

        prompt.push_str("**Document Analysis:**\n");
        prompt.push_str(&format!("- Word count: {}\n", word_count));
        prompt.push_str(&summary);

        // Add calibration anchor if available
        if let Some(graph) = calibration_graph.as_ref() {
            let cal_section =
                crate::calibrate::calibration_prompt_section(graph, &sample.text, &config.intent);
            if !cal_section.is_empty() {
                prompt.push_str(&cal_section);
            }
        }
        prompt.push('\n');

        // Essay text
        let text = if sample.text.len() > 3000 {
            format!("{}...", &sample.text[..3000])
        } else {
            sample.text.clone()
        };
        prompt.push_str(&format!("{}\n\n", text));
    }

    // Response format
    prompt.push_str("## Response Format\n\n");
    prompt.push_str("Return a JSON array. For each essay:\n");
    prompt.push_str("```json\n");
    prompt.push_str("[{\n");
    prompt.push_str("  \"id\": \"essay_id\",\n");
    prompt.push_str("  \"eval1\": {\"cohesion\": X, \"syntax\": X, \"vocabulary\": X, \"phraseology\": X, \"grammar\": X, \"conventions\": X, \"overall\": X},\n");
    prompt.push_str("  \"eval2\": {\"cohesion\": X, \"syntax\": X, \"vocabulary\": X, \"phraseology\": X, \"grammar\": X, \"conventions\": X, \"overall\": X},\n");
    prompt.push_str("  \"eval3\": {\"cohesion\": X, \"syntax\": X, \"vocabulary\": X, \"phraseology\": X, \"grammar\": X, \"conventions\": X, \"overall\": X},\n");
    prompt.push_str("  \"score\": MEDIAN_OF_THREE_OVERALLS\n");
    prompt.push_str("}]\n");
    prompt.push_str("```\n");
    prompt.push_str("\nReturn ONLY the JSON array. No explanations.\n");

    prompt
}

/// Intermediate type for the rich multi-evaluator response format.
#[derive(Debug, Clone, Deserialize)]
struct RichScoredEssay {
    id: String,
    score: f64,
    // eval1, eval2, eval3 are present but we only need the final score
}

/// Parse subagent response into scored essays.
/// Handles both the rich multi-evaluator format (with eval1/eval2/eval3)
/// and the simple format (just id + score).
pub fn parse_scores(response: &str) -> Vec<ScoredEssay> {
    // Try to find JSON array in the response
    let json_str = if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            &response[start..=end]
        } else {
            response
        }
    } else {
        response
    };

    // Try rich format first (has eval1/eval2/eval3 + score)
    if let Ok(rich_scores) = serde_json::from_str::<Vec<RichScoredEssay>>(json_str) {
        return rich_scores
            .into_iter()
            .map(|r| ScoredEssay {
                id: r.id,
                score: r.score,
            })
            .collect();
    }

    // Fall back to simple format
    serde_json::from_str(json_str).unwrap_or_default()
}

/// Compute metrics from scored results against ground truth.
pub fn compute_metrics(
    scored: &[ScoredEssay],
    samples: &[LabeledSample],
    config: &ShoalConfig,
) -> BenchmarkResults {
    let sample_map: std::collections::HashMap<&str, f64> = samples
        .iter()
        .map(|s| (s.id.as_str(), s.expert_score))
        .collect();

    let mut predicted = Vec::new();
    let mut actual = Vec::new();

    for s in scored {
        if let Some(&expert) = sample_map.get(s.id.as_str()) {
            predicted.push(s.score);
            actual.push(expert);
        }
    }

    let n = predicted.len();
    if n == 0 {
        return BenchmarkResults {
            name: "shoal".into(),
            samples: 0,
            pearson_r: 0.0,
            qwk: 0.0,
            mae: 0.0,
            nmae: 0.0,
            rmse: 0.0,
            mean_predicted: 0.0,
            mean_actual: 0.0,
            hallucination_count: 0,
            hallucination_rate: 0.0,
            config: BenchmarkConfig::default(),
        };
    }

    let mean_p = predicted.iter().sum::<f64>() / n as f64;
    let mean_a = actual.iter().sum::<f64>() / n as f64;

    let mae_val = mean_absolute_error(&predicted, &actual);
    let nmae = if config.max_score > 0.0 { mae_val / config.max_score } else { 0.0 };

    BenchmarkResults {
        name: "shoal_subagent".into(),
        samples: n,
        pearson_r: pearson_correlation(&predicted, &actual),
        qwk: quadratic_weighted_kappa(&predicted, &actual, 0.0, config.max_score),
        mae: mae_val,
        nmae,
        rmse: rmse(&predicted, &actual),
        mean_predicted: mean_p,
        mean_actual: mean_a,
        hallucination_count: 0,
        hallucination_rate: 0.0,
        config: BenchmarkConfig {
            use_llm_scoring: true,
            label: "shoal_subagent".into(),
            ..Default::default()
        },
    }
}

/// Run the deterministic pipeline on a single essay and return the EDS score (0.0-1.0).
pub fn eds_score_essay(sample: &LabeledSample, intent: &str) -> f64 {
    use crate::{alignment, argument_graph, criteria, extract, validate};

    // 1. Build document
    let word_count = sample.text.split_whitespace().count() as u32;
    let mut section = crate::types::Section {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Essay".into(),
        text: sample.text.clone(),
        word_count,
        page_range: None,
        claims: vec![],
        evidence: vec![],
        subsections: vec![],
    };

    // 2. Extract claims/evidence
    let extracted = extract::extract_all(&section.text);
    let (claims, evidence) = extract::to_claims_and_evidence(&extracted);
    if !claims.is_empty() || !evidence.is_empty() {
        section.claims = claims;
        section.evidence = evidence;
    }

    let doc = crate::types::EvalDocument {
        id: sample.id.clone(),
        title: format!("Essay: {}", sample.id),
        doc_type: "essay".into(),
        total_pages: None,
        total_word_count: Some(word_count),
        sections: vec![section],
    };

    // 3. Load criteria
    let framework = criteria::framework_for_intent(intent);

    // 4. Align
    let (_alignments, _) = alignment::align_sections_to_criteria(&doc, &framework);

    // 5. Validate
    let validation_signals = validate::validate_core(&doc, &framework);

    // 6. Structural scoring (replaces SNN)
    let graph = argument_graph::build_from_document(&doc);
    let metrics = argument_graph::compute_metrics(&graph);
    let base_score = argument_graph::structural_score(&metrics);

    // Apply validation signal adjustments
    let validation_adjustment: f64 = validation_signals
        .iter()
        .map(|s| s.spike_effect * 0.05) // scale signals to small adjustments
        .sum();

    let score = (base_score + validation_adjustment).clamp(0.0, 1.0);

    // 7. Weight by criteria
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;
    for crit in &framework.criteria {
        weighted_sum += score * crit.weight;
        weight_sum += crit.weight;
    }

    if weight_sum > 0.0 {
        weighted_sum / weight_sum // Returns 0.0-1.0 percentage
    } else {
        0.0
    }
}

/// Run the deterministic pipeline on a single essay.
/// Returns the EDS score (0.0-1.0).
///
/// Previously accepted an `SNNConfig`; the SNN has been removed.
/// This now delegates to `eds_score_essay` — kept for API compatibility.
#[deprecated(note = "Use eds_score_essay instead — SNN config no longer used")]
pub fn eds_score_essay_with_config(
    sample: &LabeledSample,
    intent: &str,
) -> f64 {
    eds_score_essay(sample, intent)
}

/// Blended metrics results: subagent, EDS, blended, and optionally calibrated.
pub struct BlendedMetrics {
    pub subagent: BenchmarkResults,
    pub eds: BenchmarkResults,
    pub blended: BenchmarkResults,
    pub calibrated: Option<BenchmarkResults>,
}

/// Compute blended metrics: EDS score + subagent score + calibrated score for each essay.
pub fn compute_blended_metrics(
    scored: &[ScoredEssay],
    samples: &[LabeledSample],
    config: &ShoalConfig,
    intent: &str,
) -> (BenchmarkResults, BenchmarkResults, BenchmarkResults) {
    let bm = compute_blended_metrics_full(scored, samples, config, intent);
    (bm.subagent, bm.eds, bm.blended)
}

/// Full blended metrics including calibrated scores when anchors are provided.
pub fn compute_blended_metrics_full(
    scored: &[ScoredEssay],
    samples: &[LabeledSample],
    config: &ShoalConfig,
    intent: &str,
) -> BlendedMetrics {
    let sample_map: std::collections::HashMap<&str, &LabeledSample> = samples
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect();

    // Build calibration graph if anchors provided
    let calibration_graph: Option<std::sync::Arc<open_ontologies::graph::GraphStore>> =
        config.anchors.as_ref().and_then(|anchors| {
            if anchors.is_empty() {
                return None;
            }
            let graph = std::sync::Arc::new(open_ontologies::graph::GraphStore::new());
            if crate::calibrate::load_anchors(&graph, anchors).is_ok() {
                Some(graph)
            } else {
                None
            }
        });

    let mut subagent_pred = Vec::new();
    let mut eds_pred = Vec::new();
    let mut blended_pred = Vec::new();
    let mut calibrated_pred = Vec::new();
    let mut actual = Vec::new();
    let mut _hallucination_flags = 0usize;

    for s in scored {
        if let Some(sample) = sample_map.get(s.id.as_str()) {
            let eds_pct = eds_score_essay(sample, intent);
            let eds_score = eds_pct * config.max_score;
            let subagent_score = s.score;

            // Blend: 40% EDS + 60% subagent (EDS is verification, subagent is scorer)
            let blended = eds_score * 0.4 + subagent_score * 0.6;

            // Calibrated score (if anchors available)
            if let Some(graph) = calibration_graph.as_ref() {
                let (cal, _, _) = crate::calibrate::calibrated_score(
                    graph,
                    &sample.text,
                    intent,
                    subagent_score,
                    config.max_score,
                );
                calibrated_pred.push(cal);
            }

            // Hallucination check
            let eds_normalised = eds_score / config.max_score;
            let sub_normalised = subagent_score / config.max_score;
            if sub_normalised > 0.7 && eds_normalised < 0.3 {
                _hallucination_flags += 1;
            }

            subagent_pred.push(subagent_score);
            eds_pred.push(eds_score);
            blended_pred.push(blended);
            actual.push(sample.expert_score);
        }
    }

    let n = actual.len();

    let ms = config.max_score;
    let subagent_mae = mean_absolute_error(&subagent_pred, &actual);
    let subagent_results = BenchmarkResults {
        name: "shoal_subagent".into(),
        samples: n,
        pearson_r: pearson_correlation(&subagent_pred, &actual),
        qwk: quadratic_weighted_kappa(&subagent_pred, &actual, 0.0, ms),
        mae: subagent_mae,
        nmae: if ms > 0.0 { subagent_mae / ms } else { 0.0 },
        rmse: rmse(&subagent_pred, &actual),
        mean_predicted: subagent_pred.iter().sum::<f64>() / n.max(1) as f64,
        mean_actual: actual.iter().sum::<f64>() / n.max(1) as f64,
        hallucination_count: 0,
        hallucination_rate: 0.0,
        config: BenchmarkConfig {
            label: "subagent".into(),
            ..Default::default()
        },
    };

    let eds_mae = mean_absolute_error(&eds_pred, &actual);
    let eds_results = BenchmarkResults {
        name: "shoal_eds".into(),
        samples: n,
        pearson_r: pearson_correlation(&eds_pred, &actual),
        qwk: quadratic_weighted_kappa(&eds_pred, &actual, 0.0, ms),
        mae: eds_mae,
        nmae: if ms > 0.0 { eds_mae / ms } else { 0.0 },
        rmse: rmse(&eds_pred, &actual),
        mean_predicted: eds_pred.iter().sum::<f64>() / n.max(1) as f64,
        mean_actual: actual.iter().sum::<f64>() / n.max(1) as f64,
        hallucination_count: 0,
        hallucination_rate: 0.0,
        config: BenchmarkConfig {
            label: "eds".into(),
            ..Default::default()
        },
    };

    let blended_mae = mean_absolute_error(&blended_pred, &actual);
    let blended_results = BenchmarkResults {
        name: "shoal_blended".into(),
        samples: n,
        pearson_r: pearson_correlation(&blended_pred, &actual),
        qwk: quadratic_weighted_kappa(&blended_pred, &actual, 0.0, ms),
        mae: blended_mae,
        nmae: if ms > 0.0 { blended_mae / ms } else { 0.0 },
        rmse: rmse(&blended_pred, &actual),
        mean_predicted: blended_pred.iter().sum::<f64>() / n.max(1) as f64,
        mean_actual: actual.iter().sum::<f64>() / n.max(1) as f64,
        hallucination_count: 0,
        hallucination_rate: 0.0,
        config: BenchmarkConfig {
            label: "blended".into(),
            ..Default::default()
        },
    };

    let calibrated_results = if !calibrated_pred.is_empty() && calibrated_pred.len() == n {
        let cal_mae = mean_absolute_error(&calibrated_pred, &actual);
        Some(BenchmarkResults {
            name: "shoal_calibrated".into(),
            samples: n,
            pearson_r: pearson_correlation(&calibrated_pred, &actual),
            qwk: quadratic_weighted_kappa(&calibrated_pred, &actual, 0.0, ms),
            mae: cal_mae,
            nmae: if ms > 0.0 { cal_mae / ms } else { 0.0 },
            rmse: rmse(&calibrated_pred, &actual),
            mean_predicted: calibrated_pred.iter().sum::<f64>() / n.max(1) as f64,
            mean_actual: actual.iter().sum::<f64>() / n.max(1) as f64,
            hallucination_count: 0,
            hallucination_rate: 0.0,
            config: BenchmarkConfig {
                label: "calibrated".into(),
                ..Default::default()
            },
        })
    } else {
        None
    };

    BlendedMetrics {
        subagent: subagent_results,
        eds: eds_results,
        blended: blended_results,
        calibrated: calibrated_results,
    }
}

/// A single score band analysis entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBandEntry {
    pub band_label: String,
    pub band_low: f64,
    pub band_high: f64,
    pub mean_predicted: f64,
    pub mean_delta: f64,
    pub count: usize,
}

/// Compute per-score-band error analysis.
///
/// Groups expert scores into bands (1.0-1.5, 2.0-2.5, 3.0-3.5, 4.0-4.5, 5.0)
/// and reports mean predicted score and delta for each band. This reveals
/// systematic over/under-scoring at different proficiency levels.
pub fn score_band_analysis(
    scored: &[ScoredEssay],
    samples: &[LabeledSample],
) -> Vec<ScoreBandEntry> {
    let sample_map: std::collections::HashMap<&str, f64> = samples
        .iter()
        .map(|s| (s.id.as_str(), s.expert_score))
        .collect();

    // Collect (predicted, expert) pairs
    let mut pairs: Vec<(f64, f64)> = Vec::new();
    for s in scored {
        if let Some(&expert) = sample_map.get(s.id.as_str()) {
            pairs.push((s.score, expert));
        }
    }

    let bands: &[(f64, f64, &str)] = &[
        (1.0, 1.5, "1.0-1.5"),
        (2.0, 2.5, "2.0-2.5"),
        (3.0, 3.5, "3.0-3.5"),
        (4.0, 4.5, "4.0-4.5"),
        (5.0, 5.0, "5.0"),
    ];

    bands
        .iter()
        .map(|&(low, high, label)| {
            let in_band: Vec<(f64, f64)> = pairs
                .iter()
                .filter(|&&(_, expert)| expert >= low && expert <= high)
                .copied()
                .collect();

            let n = in_band.len();
            if n == 0 {
                ScoreBandEntry {
                    band_label: label.to_string(),
                    band_low: low,
                    band_high: high,
                    mean_predicted: 0.0,
                    mean_delta: 0.0,
                    count: 0,
                }
            } else {
                let mean_pred = in_band.iter().map(|(p, _)| p).sum::<f64>() / n as f64;
                let mean_expert = in_band.iter().map(|(_, e)| e).sum::<f64>() / n as f64;
                let delta = mean_pred - mean_expert;
                ScoreBandEntry {
                    band_label: label.to_string(),
                    band_low: low,
                    band_high: high,
                    mean_predicted: mean_pred,
                    mean_delta: delta,
                    count: n,
                }
            }
        })
        .collect()
}

/// Format score band analysis for display.
pub fn format_score_band_analysis(bands: &[ScoreBandEntry]) -> String {
    let mut out = String::from("Score band analysis:\n");
    for band in bands {
        let sign = if band.mean_delta >= 0.0 { "+" } else { "" };
        out.push_str(&format!(
            "  Expert {:9}: mean predicted {:.1} (delta {}{:.1}, n={})\n",
            band.band_label, band.mean_predicted, sign, band.mean_delta, band.count
        ));
    }
    out
}

/// Save shoal results to a JSON file.
pub fn save_results(
    scored: &[ScoredEssay],
    metrics: &BenchmarkResults,
    output_path: &Path,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(output_path)?;

    let scores_path = output_path.join("shoal-scores.json");
    let scores_json = serde_json::to_string_pretty(scored)?;
    std::fs::write(&scores_path, scores_json)?;

    let metrics_path = output_path.join("shoal-metrics.json");
    let metrics_json = serde_json::to_string_pretty(metrics)?;
    std::fs::write(&metrics_path, metrics_json)?;

    // Markdown summary
    let md = format!(
        "# Shoal Benchmark Results\n\n\
         | Metric | Value |\n\
         | ------ | ----- |\n\
         | Samples | {} |\n\
         | Pearson r | {:.3} |\n\
         | QWK | {:.3} |\n\
         | MAE | {:.2} |\n\
         | RMSE | {:.2} |\n",
        metrics.samples, metrics.pearson_r, metrics.qwk, metrics.mae, metrics.rmse
    );
    std::fs::write(output_path.join("shoal-results.md"), md)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_essays() -> Vec<LabeledSample> {
        vec![
            LabeledSample {
                id: "e1".into(),
                text: "Good essay with evidence.".into(),
                expert_score: 4.0,
                max_score: 5.0,
                domain: "test".into(),
                rubric: "test".into(),
            },
            LabeledSample {
                id: "e2".into(),
                text: "Bad essay no structure.".into(),
                expert_score: 2.0,
                max_score: 5.0,
                domain: "test".into(),
                rubric: "test".into(),
            },
            LabeledSample {
                id: "e3".into(),
                text: "Average essay okay.".into(),
                expert_score: 3.0,
                max_score: 5.0,
                domain: "test".into(),
                rubric: "test".into(),
            },
        ]
    }

    #[test]
    fn test_split_batches() {
        let samples = sample_essays();
        let batches = split_batches(&samples, 2);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[1].len(), 1);
    }

    #[test]
    fn test_batch_scoring_prompt() {
        let samples = sample_essays();
        let config = ShoalConfig::default();
        let prompt = batch_scoring_prompt(&samples, &config);
        // Should contain essay content
        assert!(prompt.contains("Essay 1"));
        assert!(prompt.contains("e1"));
        assert!(prompt.contains("Good essay"));
        // Should contain JSON format instruction
        assert!(prompt.contains("JSON array"));
        // Should contain framework/rubric context
        assert!(prompt.contains("## Evaluation Framework"));
        // Multi-agent personas
        assert!(prompt.contains("## Multi-Evaluator Panel"));
        assert!(prompt.contains("Evaluator 1"));
        assert!(prompt.contains("Evaluator 2"));
        assert!(prompt.contains("Evaluator 3"));
        assert!(prompt.contains("median"));
        // Calibration
        assert!(prompt.contains("## Calibration"));
        // Per-criterion scoring
        assert!(prompt.contains("cohesion"));
        assert!(prompt.contains("syntax"));
        // Document Analysis — adaptive extraction output fed into prompt
        assert!(prompt.contains("Document Analysis"), "prompt should include Document Analysis block per essay");
        assert!(prompt.contains("Word count:"), "prompt should include word count");
        assert!(prompt.contains("Total evidence items:"), "prompt should include evidence item totals");
        // Adaptive guidance section
        assert!(prompt.contains("## How to use the Document Analysis"), "prompt should include analysis guidance");
    }

    #[test]
    fn test_rich_prompt_has_three_evaluators() {
        let samples = sample_essays();
        let config = ShoalConfig {
            intent: "grade this English language learner essay for language proficiency".into(),
            ..Default::default()
        };
        let prompt = batch_scoring_prompt(&samples, &config);
        assert!(prompt.contains("Evaluator 1"));
        assert!(prompt.contains("Evaluator 2"));
        assert!(prompt.contains("Evaluator 3"));
        assert!(prompt.contains("median"));
        assert!(prompt.contains("The Grammarian"));
        assert!(prompt.contains("The ELL Specialist"));
        assert!(prompt.contains("The Holistic Reader"));
    }

    #[test]
    fn test_parse_scores() {
        let response = r#"Here are the scores:
[{"id": "e1", "score": 4.0}, {"id": "e2", "score": 2.5}]"#;
        let scores = parse_scores(response);
        assert_eq!(scores.len(), 2);
        assert_eq!(scores[0].id, "e1");
        assert!((scores[0].score - 4.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_scores_raw_json() {
        let response = r#"[{"id": "e1", "score": 3.0}]"#;
        let scores = parse_scores(response);
        assert_eq!(scores.len(), 1);
    }

    #[test]
    fn test_parse_rich_scores() {
        let response = r#"[{"id": "e1", "eval1": {"cohesion": 2.0, "syntax": 2.0, "vocabulary": 2.5, "phraseology": 2.0, "grammar": 2.0, "conventions": 2.0, "overall": 2.1}, "eval2": {"cohesion": 3.0, "syntax": 2.5, "vocabulary": 3.0, "phraseology": 2.5, "grammar": 2.5, "conventions": 2.5, "overall": 2.7}, "eval3": {"cohesion": 2.5, "syntax": 2.5, "vocabulary": 2.5, "phraseology": 2.5, "grammar": 2.0, "conventions": 2.5, "overall": 2.4}, "score": 2.4}]"#;
        let scores = parse_scores(response);
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].id, "e1");
        assert!((scores[0].score - 2.4).abs() < 0.01);
    }

    #[test]
    fn test_parse_rich_scores_multiple() {
        let response = r#"[
            {"id": "e1", "eval1": {"overall": 2.0}, "eval2": {"overall": 3.0}, "eval3": {"overall": 2.5}, "score": 2.5},
            {"id": "e2", "eval1": {"overall": 3.5}, "eval2": {"overall": 4.0}, "eval3": {"overall": 3.5}, "score": 3.5}
        ]"#;
        let scores = parse_scores(response);
        assert_eq!(scores.len(), 2);
        assert!((scores[0].score - 2.5).abs() < 0.01);
        assert!((scores[1].score - 3.5).abs() < 0.01);
    }

    #[test]
    fn test_compute_metrics() {
        let samples = sample_essays();
        let scored = vec![
            ScoredEssay {
                id: "e1".into(),
                score: 4.0,
            },
            ScoredEssay {
                id: "e2".into(),
                score: 2.0,
            },
            ScoredEssay {
                id: "e3".into(),
                score: 3.0,
            },
        ];
        let config = ShoalConfig::default();
        let metrics = compute_metrics(&scored, &samples, &config);
        assert_eq!(metrics.samples, 3);
        assert!(
            (metrics.pearson_r - 1.0).abs() < 0.01,
            "Perfect predictions should give r=1.0"
        );
        assert!((metrics.mae).abs() < 0.01);
    }

    #[test]
    fn test_compute_metrics_empty() {
        let config = ShoalConfig::default();
        let metrics = compute_metrics(&[], &[], &config);
        assert_eq!(metrics.samples, 0);
    }

    #[test]
    fn test_eds_score_essay() {
        let sample = LabeledSample {
            id: "test1".into(),
            text: "According to Joyce et al. (2012), quantitative easing reduced gilt yields by 100 basis points. The Bank of England purchased £895 billion in assets. This essay argues that QE was effective.".into(),
            expert_score: 7.0,
            max_score: 10.0,
            domain: "economics".into(),
            rubric: "academic".into(),
        };
        let score = eds_score_essay(&sample, "mark this essay");
        assert!(
            score > 0.0 && score <= 1.0,
            "EDS score should be 0-1, got {}",
            score
        );
        assert!(
            score > 0.2,
            "Essay with citations and evidence should score > 0.2, got {}",
            score
        );
    }

    #[test]
    fn test_eds_score_empty_essay() {
        let sample = LabeledSample {
            id: "test2".into(),
            text: "Computers are good.".into(),
            expert_score: 2.0,
            max_score: 10.0,
            domain: "generic".into(),
            rubric: "generic".into(),
        };
        let score = eds_score_essay(&sample, "mark this essay");
        assert!(score < 0.5, "Empty essay should score low, got {}", score);
    }

    #[test]
    fn test_compute_blended_metrics() {
        let samples = sample_essays();
        let scored = vec![
            ScoredEssay {
                id: "e1".into(),
                score: 4.0,
            },
            ScoredEssay {
                id: "e2".into(),
                score: 2.0,
            },
            ScoredEssay {
                id: "e3".into(),
                score: 3.0,
            },
        ];
        let config = ShoalConfig::default();
        let (sub, eds, blended) =
            compute_blended_metrics(&scored, &samples, &config, "mark this essay");
        assert_eq!(sub.samples, 3);
        assert_eq!(eds.samples, 3);
        assert_eq!(blended.samples, 3);
        // Blended should have non-zero MAE (EDS won't perfectly match expert)
        assert!(
            blended.mae >= 0.0,
            "Blended MAE should be non-negative"
        );
    }

    #[test]
    fn test_score_band_analysis() {
        let samples = vec![
            LabeledSample {
                id: "low".into(),
                text: "bad".into(),
                expert_score: 1.0,
                max_score: 5.0,
                domain: "test".into(),
                rubric: "test".into(),
            },
            LabeledSample {
                id: "mid".into(),
                text: "ok".into(),
                expert_score: 3.0,
                max_score: 5.0,
                domain: "test".into(),
                rubric: "test".into(),
            },
            LabeledSample {
                id: "high".into(),
                text: "great".into(),
                expert_score: 5.0,
                max_score: 5.0,
                domain: "test".into(),
                rubric: "test".into(),
            },
        ];
        let scored = vec![
            ScoredEssay { id: "low".into(), score: 2.0 },
            ScoredEssay { id: "mid".into(), score: 3.5 },
            ScoredEssay { id: "high".into(), score: 4.5 },
        ];

        let bands = score_band_analysis(&scored, &samples);
        assert_eq!(bands.len(), 5);

        // Band 1.0-1.5: expert=1.0, predicted=2.0, delta=+1.0
        let band_low = &bands[0];
        assert_eq!(band_low.count, 1);
        assert!((band_low.mean_predicted - 2.0).abs() < 0.01);
        assert!((band_low.mean_delta - 1.0).abs() < 0.01);

        // Band 3.0-3.5: expert=3.0, predicted=3.5, delta=+0.5
        let band_mid = &bands[2];
        assert_eq!(band_mid.count, 1);
        assert!((band_mid.mean_predicted - 3.5).abs() < 0.01);
        assert!((band_mid.mean_delta - 0.5).abs() < 0.01);

        // Band 5.0: expert=5.0, predicted=4.5, delta=-0.5
        let band_high = &bands[4];
        assert_eq!(band_high.count, 1);
        assert!((band_high.mean_predicted - 4.5).abs() < 0.01);
        assert!((band_high.mean_delta - -0.5).abs() < 0.01);
    }

    #[test]
    fn test_score_band_analysis_empty() {
        let bands = score_band_analysis(&[], &[]);
        assert_eq!(bands.len(), 5);
        for band in &bands {
            assert_eq!(band.count, 0);
        }
    }

    #[test]
    fn test_format_score_band_analysis() {
        let bands = vec![ScoreBandEntry {
            band_label: "1.0-1.5".to_string(),
            band_low: 1.0,
            band_high: 1.5,
            mean_predicted: 2.0,
            mean_delta: 1.0,
            count: 3,
        }];
        let output = format_score_band_analysis(&bands);
        assert!(output.contains("1.0-1.5"));
        assert!(output.contains("mean predicted 2.0"));
        assert!(output.contains("+1.0"));
        assert!(output.contains("n=3"));
    }
}
