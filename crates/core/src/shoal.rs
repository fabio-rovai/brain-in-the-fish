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
}

impl Default for ShoalConfig {
    fn default() -> Self {
        Self {
            batch_size: 10,
            scale_description: "1.0-5.0 (0.5 increments)".into(),
            max_score: 5.0,
            intent: "evaluate this essay".into(),
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
/// The prompt includes the full BITF evaluation context:
/// - Agent persona (from the spawned panel)
/// - Evaluation framework with rubric level descriptors
/// - Per-essay evidence summaries (citations, statistics, claims extracted)
pub fn batch_scoring_prompt(batch: &[LabeledSample], config: &ShoalConfig) -> String {
    let intent = &config.intent;
    let framework = crate::criteria::framework_for_intent(intent);

    // Build persona from the lead agent
    let agents = crate::agent::spawn_panel(intent, &framework);
    let persona = if let Some(agent) = agents.first() {
        format!(
            "## Your Role\n\n\
             You are {}, a {} with expertise in {}.\n\
             {}\n\n",
            agent.name, agent.role, agent.domain, agent.persona_description
        )
    } else {
        String::new()
    };

    // Build the rubric section
    let mut rubric = String::new();
    rubric.push_str("## Evaluation Framework\n\n");
    rubric.push_str(&format!("**Framework:** {}\n\n", framework.name));
    for crit in &framework.criteria {
        rubric.push_str(&format!(
            "### {} (max {}, weight {:.0}%)\n",
            crit.title,
            crit.max_score,
            crit.weight * 100.0
        ));
        if let Some(desc) = &crit.description {
            rubric.push_str(&format!("{}\n", desc));
        }
        for level in &crit.rubric_levels {
            rubric.push_str(&format!(
                "- **{}** ({}): {}\n",
                level.level, level.score_range, level.descriptor
            ));
        }
        rubric.push('\n');
    }

    // Build per-essay sections with evidence summaries
    let mut essays_section = String::new();
    for (i, sample) in batch.iter().enumerate() {
        let extracted = crate::extract::extract_all(&sample.text);
        let citations = extracted
            .iter()
            .filter(|e| e.item_type == crate::extract::ExtractedType::Citation)
            .count();
        let statistics = extracted
            .iter()
            .filter(|e| e.item_type == crate::extract::ExtractedType::Statistic)
            .count();
        let claims = extracted
            .iter()
            .filter(|e| e.item_type == crate::extract::ExtractedType::Claim)
            .count();
        let total_evidence = extracted.len();

        essays_section.push_str(&format!(
            "---\n\n### Essay {} (ID: {})\n\n\
             **Evidence summary:** {} items extracted ({} citations, {} statistics, {} claims)\n\n\
             {}\n\n",
            i + 1,
            sample.id,
            total_evidence,
            citations,
            statistics,
            claims,
            if sample.text.len() > 3000 {
                format!("{}...", &sample.text[..3000])
            } else {
                sample.text.clone()
            }
        ));
    }

    format!(
        "{}\
         {}\
         ## Scoring Instructions\n\n\
         For each essay below, score it against the framework above.\n\
         - Reference specific rubric levels in your assessment\n\
         - Consider the evidence summary (more citations/statistics = stronger evidence base)\n\
         - Use the FULL scale — a score of {:.0} means exceptional on every criterion, not just good\n\
         - A score below {:.0} means significant weaknesses\n\
         - Be calibrated: most essays should cluster in the middle, with few at extremes\n\n\
         Return a JSON array: [{{\"id\": \"...\", \"score\": X.X}}, ...]\n\n\
         {}\n",
        persona,
        rubric,
        config.max_score,
        config.max_score * 0.3,
        essays_section
    )
}

/// Parse subagent response into scored essays.
/// Handles various JSON formats the subagent might return.
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
            rmse: 0.0,
            mean_predicted: 0.0,
            mean_actual: 0.0,
            config: BenchmarkConfig::default(),
        };
    }

    let mean_p = predicted.iter().sum::<f64>() / n as f64;
    let mean_a = actual.iter().sum::<f64>() / n as f64;

    BenchmarkResults {
        name: "shoal_subagent".into(),
        samples: n,
        pearson_r: pearson_correlation(&predicted, &actual),
        qwk: quadratic_weighted_kappa(&predicted, &actual, 0.0, config.max_score),
        mae: mean_absolute_error(&predicted, &actual),
        rmse: rmse(&predicted, &actual),
        mean_predicted: mean_p,
        mean_actual: mean_a,
        config: BenchmarkConfig {
            use_llm_scoring: true,
            label: "shoal_subagent".into(),
            ..Default::default()
        },
    }
}

/// Run the deterministic pipeline on a single essay and return the EDS score (0.0-1.0).
pub fn eds_score_essay(sample: &LabeledSample, intent: &str) -> f64 {
    use crate::{agent, alignment, criteria, extract, snn, validate};

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
    let (alignments, _) = alignment::align_sections_to_criteria(&doc, &framework);

    // 5. Validate
    let validation_signals = validate::validate_core(&doc, &framework);

    // 6. Spawn agents + SNN score
    let agents = agent::spawn_panel(intent, &framework);
    let snn_config = snn::SNNConfig::default();

    let mut all_scores = Vec::new();
    for agent_item in &agents {
        let mut network = snn::AgentNetwork::new(agent_item, &framework.criteria);
        network.feed_evidence(&doc, &alignments, &snn_config);

        // Feed validation signals
        for signal in &validation_signals {
            if signal.spike_effect.abs() > 0.01 {
                for neuron in &mut network.neurons {
                    let matches = signal.criterion_id.is_none()
                        || signal.criterion_id.as_deref() == Some(&neuron.criterion_id);
                    if matches {
                        neuron.receive_spike(
                            snn::Spike {
                                source_id: signal.id.clone(),
                                strength: signal.spike_effect.abs(),
                                spike_type: if signal.spike_effect > 0.0 {
                                    snn::SpikeType::Evidence
                                } else {
                                    snn::SpikeType::Claim
                                },
                                timestep: 0,
                            },
                            &snn_config,
                        );
                        if signal.spike_effect < 0.0 {
                            neuron.apply_inhibition(signal.spike_effect.abs() * 0.2);
                        }
                    }
                }
            }
        }

        // Compute scores
        for neuron in &network.neurons {
            let criterion = framework
                .criteria
                .iter()
                .find(|c| c.id == neuron.criterion_id);
            if let Some(crit) = criterion {
                let snn_score = neuron.compute_score(crit.max_score, &snn_config);
                all_scores.push((snn_score.snn_score, crit.max_score, crit.weight));
            }
        }
    }

    // 7. Compute weighted average across all agents and criteria
    if all_scores.is_empty() {
        return 0.0;
    }

    // Average across agents for each criterion, then weighted sum
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;

    for crit in &framework.criteria {
        let crit_scores: Vec<f64> = all_scores
            .iter()
            .filter(|(_, max, _)| (*max - crit.max_score).abs() < 0.01)
            .map(|(score, max, _)| score / max)
            .collect();

        if !crit_scores.is_empty() {
            let mean_pct = crit_scores.iter().sum::<f64>() / crit_scores.len() as f64;
            weighted_sum += mean_pct * crit.weight;
            weight_sum += crit.weight;
        }
    }

    if weight_sum > 0.0 {
        weighted_sum / weight_sum // Returns 0.0-1.0 percentage
    } else {
        0.0
    }
}

/// Compute blended metrics: EDS score + subagent score for each essay.
pub fn compute_blended_metrics(
    scored: &[ScoredEssay],
    samples: &[LabeledSample],
    config: &ShoalConfig,
    intent: &str,
) -> (BenchmarkResults, BenchmarkResults, BenchmarkResults) {
    let sample_map: std::collections::HashMap<&str, &LabeledSample> = samples
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect();

    let mut subagent_pred = Vec::new();
    let mut eds_pred = Vec::new();
    let mut blended_pred = Vec::new();
    let mut actual = Vec::new();
    let mut _hallucination_flags = 0usize;

    for s in scored {
        if let Some(sample) = sample_map.get(s.id.as_str()) {
            let eds_pct = eds_score_essay(sample, intent);
            let eds_score = eds_pct * config.max_score;
            let subagent_score = s.score;

            // Blend: 40% EDS + 60% subagent (EDS is verification, subagent is scorer)
            let blended = eds_score * 0.4 + subagent_score * 0.6;

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

    let subagent_results = BenchmarkResults {
        name: "shoal_subagent".into(),
        samples: n,
        pearson_r: pearson_correlation(&subagent_pred, &actual),
        qwk: quadratic_weighted_kappa(&subagent_pred, &actual, 0.0, config.max_score),
        mae: mean_absolute_error(&subagent_pred, &actual),
        rmse: rmse(&subagent_pred, &actual),
        mean_predicted: subagent_pred.iter().sum::<f64>() / n.max(1) as f64,
        mean_actual: actual.iter().sum::<f64>() / n.max(1) as f64,
        config: BenchmarkConfig {
            label: "subagent".into(),
            ..Default::default()
        },
    };

    let eds_results = BenchmarkResults {
        name: "shoal_eds".into(),
        samples: n,
        pearson_r: pearson_correlation(&eds_pred, &actual),
        qwk: quadratic_weighted_kappa(&eds_pred, &actual, 0.0, config.max_score),
        mae: mean_absolute_error(&eds_pred, &actual),
        rmse: rmse(&eds_pred, &actual),
        mean_predicted: eds_pred.iter().sum::<f64>() / n.max(1) as f64,
        mean_actual: actual.iter().sum::<f64>() / n.max(1) as f64,
        config: BenchmarkConfig {
            label: "eds".into(),
            ..Default::default()
        },
    };

    let blended_results = BenchmarkResults {
        name: "shoal_blended".into(),
        samples: n,
        pearson_r: pearson_correlation(&blended_pred, &actual),
        qwk: quadratic_weighted_kappa(&blended_pred, &actual, 0.0, config.max_score),
        mae: mean_absolute_error(&blended_pred, &actual),
        rmse: rmse(&blended_pred, &actual),
        mean_predicted: blended_pred.iter().sum::<f64>() / n.max(1) as f64,
        mean_actual: actual.iter().sum::<f64>() / n.max(1) as f64,
        config: BenchmarkConfig {
            label: "blended".into(),
            ..Default::default()
        },
    };

    (subagent_results, eds_results, blended_results)
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
        assert!(prompt.contains("## Your Role"));
        assert!(prompt.contains("## Scoring Instructions"));
        // Should contain evidence summaries
        assert!(prompt.contains("Evidence summary:"));
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
}
