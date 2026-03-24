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
}

impl Default for ShoalConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            scale_description: "1.0-5.0 (0.5 increments)".into(),
            max_score: 5.0,
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
pub fn batch_scoring_prompt(batch: &[LabeledSample], config: &ShoalConfig) -> String {
    let mut prompt = format!(
        "Score each essay on a {} scale. Read each essay carefully and assess: \
         clarity, argument quality, organization, language control, and persuasiveness.\n\n\
         Return ONLY a JSON array: [{{\"id\": \"...\", \"score\": X.X}}, ...]\n\n\
         No explanations. Just the scores.\n\n",
        config.scale_description
    );

    for (i, sample) in batch.iter().enumerate() {
        prompt.push_str(&format!(
            "--- Essay {} (ID: {}) ---\n{}\n\n",
            i + 1,
            sample.id,
            if sample.text.len() > 2000 {
                format!("{}...", &sample.text[..2000])
            } else {
                sample.text.clone()
            }
        ));
    }

    prompt
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
        assert!(prompt.contains("Essay 1"));
        assert!(prompt.contains("e1"));
        assert!(prompt.contains("Good essay"));
        assert!(prompt.contains("JSON array"));
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
}
