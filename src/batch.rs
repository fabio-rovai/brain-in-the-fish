//! Batch evaluation, calibration, and export.
//!
//! - Batch: evaluate multiple documents at once, produce comparative analysis
//! - Calibration: learn from teacher/expert marks to adjust scoring weights
//! - Export: output results as CSV/XLSX for external systems

use crate::memory::EvaluationRecord;
use crate::types::*;
use serde::Serialize;
use std::path::Path;

// ============================================================================
// Batch evaluation
// ============================================================================

/// Summary of a batch evaluation run.
#[derive(Debug, Clone, Serialize)]
pub struct BatchSummary {
    pub total_documents: usize,
    pub framework_name: String,
    pub mean_score: f64,
    pub median_score: f64,
    pub std_dev: f64,
    pub min_score: f64,
    pub max_score: f64,
    pub pass_count: usize,
    pub fail_count: usize,
    pub pass_rate: f64,
    pub criterion_averages: Vec<CriterionAverage>,
    pub distribution: Vec<DistributionBucket>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CriterionAverage {
    pub criterion_title: String,
    pub mean_percentage: f64,
    pub std_dev: f64,
    pub weakest_doc: String,
    pub strongest_doc: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DistributionBucket {
    pub range: String, // "0-10%", "10-20%", etc.
    pub count: usize,
}

/// Generate batch summary from a set of evaluation records.
pub fn summarise_batch(records: &[EvaluationRecord]) -> Option<BatchSummary> {
    if records.is_empty() {
        return None;
    }

    let scores: Vec<f64> = records.iter().map(|r| r.percentage).collect();
    let n = scores.len() as f64;
    let mean = scores.iter().sum::<f64>() / n;

    let mut sorted = scores.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
    } else {
        sorted[sorted.len() / 2]
    };

    let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    let std_dev = variance.sqrt();
    let min_score = sorted.first().copied().unwrap_or(0.0);
    let max_score = sorted.last().copied().unwrap_or(0.0);

    let pass_count = records.iter().filter(|r| r.passed == Some(true)).count();
    let fail_count = records
        .iter()
        .filter(|r| r.passed == Some(false))
        .count();
    let pass_rate = pass_count as f64 / records.len() as f64 * 100.0;

    // Per-criterion averages
    let mut criterion_map: std::collections::HashMap<String, Vec<(f64, String)>> =
        std::collections::HashMap::new();
    for record in records {
        for cs in &record.criterion_scores {
            criterion_map
                .entry(cs.criterion_title.clone())
                .or_default()
                .push((cs.percentage, record.document_title.clone()));
        }
    }

    let criterion_averages: Vec<CriterionAverage> = criterion_map
        .into_iter()
        .map(|(title, entries)| {
            let vals: Vec<f64> = entries.iter().map(|(v, _)| *v).collect();
            let avg = vals.iter().sum::<f64>() / vals.len() as f64;
            let var = vals
                .iter()
                .map(|v| (v - avg).powi(2))
                .sum::<f64>()
                / (vals.len() as f64 - 1.0).max(1.0);
            let weakest = entries
                .iter()
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
                .map(|(_, d)| d.clone())
                .unwrap_or_default();
            let strongest = entries
                .iter()
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
                .map(|(_, d)| d.clone())
                .unwrap_or_default();
            CriterionAverage {
                criterion_title: title,
                mean_percentage: avg,
                std_dev: var.sqrt(),
                weakest_doc: weakest,
                strongest_doc: strongest,
            }
        })
        .collect();

    // Distribution buckets (0-10, 10-20, ..., 90-100)
    let distribution: Vec<DistributionBucket> = (0..10)
        .map(|i| {
            let low = i as f64 * 10.0;
            let high = (i + 1) as f64 * 10.0;
            let count = scores
                .iter()
                .filter(|&&s| s >= low && (s < high || (i == 9 && s <= 100.0)))
                .count();
            DistributionBucket {
                range: format!("{low:.0}-{high:.0}%"),
                count,
            }
        })
        .collect();

    Some(BatchSummary {
        total_documents: records.len(),
        framework_name: records[0].framework_name.clone(),
        mean_score: mean,
        median_score: median,
        std_dev,
        min_score,
        max_score,
        pass_count,
        fail_count,
        pass_rate,
        criterion_averages,
        distribution,
    })
}

/// Generate a batch report as Markdown.
pub fn batch_report(summary: &BatchSummary) -> String {
    let mut r = String::new();
    r.push_str("# Batch Evaluation Report\n\n");
    r.push_str(&format!("**Framework:** {}\n", summary.framework_name));
    r.push_str(&format!(
        "**Documents evaluated:** {}\n\n",
        summary.total_documents
    ));

    r.push_str("## Overall Statistics\n\n");
    r.push_str("| Metric | Value |\n|---|---|\n");
    r.push_str(&format!("| Mean | {:.1}% |\n", summary.mean_score));
    r.push_str(&format!("| Median | {:.1}% |\n", summary.median_score));
    r.push_str(&format!("| Std Dev | {:.1}% |\n", summary.std_dev));
    r.push_str(&format!("| Min | {:.1}% |\n", summary.min_score));
    r.push_str(&format!("| Max | {:.1}% |\n", summary.max_score));
    r.push_str(&format!(
        "| Pass Rate | {:.0}% ({}/{}) |\n\n",
        summary.pass_rate, summary.pass_count, summary.total_documents
    ));

    r.push_str("## Per-Criterion Analysis\n\n");
    r.push_str("| Criterion | Mean | Std Dev | Weakest | Strongest |\n|---|---|---|---|---|\n");
    for ca in &summary.criterion_averages {
        r.push_str(&format!(
            "| {} | {:.1}% | {:.1}% | {} | {} |\n",
            ca.criterion_title, ca.mean_percentage, ca.std_dev, ca.weakest_doc, ca.strongest_doc
        ));
    }

    r.push_str("\n## Score Distribution\n\n");
    r.push_str("| Range | Count |\n|---|---|\n");
    for bucket in &summary.distribution {
        let bar = "#".repeat(bucket.count);
        r.push_str(&format!("| {} | {} {} |\n", bucket.range, bucket.count, bar));
    }

    r
}

// ============================================================================
// Calibration
// ============================================================================

/// A calibration data point — teacher/expert mark for a document.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CalibrationPoint {
    pub document_id: String,
    pub criterion_id: String,
    pub expert_score: f64,
    pub system_score: f64,
    pub max_score: f64,
}

/// Calibration result — adjustment factors per criterion.
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationResult {
    pub adjustments: Vec<CriterionAdjustment>,
    pub overall_bias: f64, // positive = system scores too high, negative = too low
    pub correlation: f64,  // how well system scores correlate with expert scores
    pub data_points: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CriterionAdjustment {
    pub criterion_id: String,
    pub criterion_title: String,
    pub bias: f64,         // system_mean - expert_mean
    pub scale_factor: f64, // expert_std / system_std (how to rescale)
    pub data_points: usize,
}

/// Compute calibration adjustments from expert marks vs system scores.
pub fn calibrate(points: &[CalibrationPoint], criteria: &[EvaluationCriterion]) -> CalibrationResult {
    let mut per_criterion: std::collections::HashMap<String, Vec<(f64, f64)>> =
        std::collections::HashMap::new();
    for p in points {
        per_criterion
            .entry(p.criterion_id.clone())
            .or_default()
            .push((p.expert_score, p.system_score));
    }

    let mut adjustments = Vec::new();
    let mut total_bias = 0.0;
    let mut total_points = 0;

    for (crit_id, pairs) in &per_criterion {
        let n = pairs.len() as f64;
        let expert_mean = pairs.iter().map(|(e, _)| e).sum::<f64>() / n;
        let system_mean = pairs.iter().map(|(_, s)| s).sum::<f64>() / n;
        let bias = system_mean - expert_mean;

        let expert_std = (pairs
            .iter()
            .map(|(e, _)| (e - expert_mean).powi(2))
            .sum::<f64>()
            / (n - 1.0).max(1.0))
        .sqrt();
        let system_std = (pairs
            .iter()
            .map(|(_, s)| (s - system_mean).powi(2))
            .sum::<f64>()
            / (n - 1.0).max(1.0))
        .sqrt();
        let scale_factor = if system_std > 0.01 {
            expert_std / system_std
        } else {
            1.0
        };

        let title = criteria
            .iter()
            .find(|c| c.id == *crit_id)
            .map(|c| c.title.clone())
            .unwrap_or_else(|| crit_id.clone());
        adjustments.push(CriterionAdjustment {
            criterion_id: crit_id.clone(),
            criterion_title: title,
            bias,
            scale_factor,
            data_points: pairs.len(),
        });
        total_bias += bias * pairs.len() as f64;
        total_points += pairs.len();
    }

    let overall_bias = if total_points > 0 {
        total_bias / total_points as f64
    } else {
        0.0
    };

    // Pearson correlation across all points
    let all_expert: Vec<f64> = points.iter().map(|p| p.expert_score / p.max_score).collect();
    let all_system: Vec<f64> = points.iter().map(|p| p.system_score / p.max_score).collect();
    let correlation = pearson_correlation(&all_expert, &all_system);

    CalibrationResult {
        adjustments,
        overall_bias,
        correlation,
        data_points: points.len(),
    }
}

/// Apply calibration adjustments to a score.
pub fn apply_calibration(score: f64, max_score: f64, adjustment: &CriterionAdjustment) -> f64 {
    let adjusted = (score - adjustment.bias) * adjustment.scale_factor;
    adjusted.max(0.0).min(max_score)
}

fn pearson_correlation(x: &[f64], y: &[f64]) -> f64 {
    if x.len() != y.len() || x.len() < 2 {
        return 0.0;
    }
    let n = x.len() as f64;
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let cov: f64 = x.iter().zip(y).map(|(xi, yi)| (xi - mx) * (yi - my)).sum();
    let sx = (x.iter().map(|xi| (xi - mx).powi(2)).sum::<f64>()).sqrt();
    let sy = (y.iter().map(|yi| (yi - my).powi(2)).sum::<f64>()).sqrt();
    if sx * sy < 1e-10 {
        0.0
    } else {
        cov / (sx * sy)
    }
}

// ============================================================================
// CSV/XLSX Export
// ============================================================================

/// Export evaluation records as CSV.
pub fn export_csv(records: &[EvaluationRecord], path: &Path) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;

    // Header
    wtr.write_record([
        "Document",
        "Type",
        "Framework",
        "Score",
        "Max",
        "Percentage",
        "Passed",
        "Agents",
        "Rounds",
        "Date",
    ])?;

    for r in records {
        wtr.write_record([
            &r.document_title,
            &r.doc_type,
            &r.framework_name,
            &format!("{:.1}", r.overall_score),
            &format!("{:.1}", r.max_possible),
            &format!("{:.1}", r.percentage),
            &r.passed
                .map(|p| if p { "PASS" } else { "FAIL" })
                .unwrap_or("N/A")
                .to_string(),
            &r.agent_count.to_string(),
            &r.debate_rounds.to_string(),
            &r.evaluated_at,
        ])?;
    }

    wtr.flush()?;
    Ok(())
}

/// Export per-criterion scores as CSV (detailed breakdown).
pub fn export_criteria_csv(records: &[EvaluationRecord], path: &Path) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;

    wtr.write_record(["Document", "Criterion", "Score", "Max", "Percentage"])?;

    for r in records {
        for cs in &r.criterion_scores {
            wtr.write_record([
                &r.document_title,
                &cs.criterion_title,
                &format!("{:.1}", cs.consensus_score),
                &format!("{:.1}", cs.max_score),
                &format!("{:.1}", cs.percentage),
            ])?;
        }
    }

    wtr.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::CriterionRecord;

    fn test_records() -> Vec<EvaluationRecord> {
        vec![
            EvaluationRecord {
                id: "r1".into(),
                document_title: "Essay A".into(),
                doc_type: "essay".into(),
                intent: "mark".into(),
                framework_name: "Academic".into(),
                overall_score: 65.0,
                max_possible: 100.0,
                percentage: 65.0,
                passed: Some(true),
                criterion_scores: vec![
                    CriterionRecord {
                        criterion_id: "c1".into(),
                        criterion_title: "Knowledge".into(),
                        consensus_score: 7.0,
                        max_score: 10.0,
                        percentage: 70.0,
                    },
                    CriterionRecord {
                        criterion_id: "c2".into(),
                        criterion_title: "Analysis".into(),
                        consensus_score: 6.0,
                        max_score: 10.0,
                        percentage: 60.0,
                    },
                ],
                agent_count: 4,
                debate_rounds: 2,
                evaluated_at: "2026-03-23T10:00:00Z".into(),
            },
            EvaluationRecord {
                id: "r2".into(),
                document_title: "Essay B".into(),
                doc_type: "essay".into(),
                intent: "mark".into(),
                framework_name: "Academic".into(),
                overall_score: 80.0,
                max_possible: 100.0,
                percentage: 80.0,
                passed: Some(true),
                criterion_scores: vec![
                    CriterionRecord {
                        criterion_id: "c1".into(),
                        criterion_title: "Knowledge".into(),
                        consensus_score: 8.5,
                        max_score: 10.0,
                        percentage: 85.0,
                    },
                    CriterionRecord {
                        criterion_id: "c2".into(),
                        criterion_title: "Analysis".into(),
                        consensus_score: 7.5,
                        max_score: 10.0,
                        percentage: 75.0,
                    },
                ],
                agent_count: 4,
                debate_rounds: 3,
                evaluated_at: "2026-03-23T11:00:00Z".into(),
            },
            EvaluationRecord {
                id: "r3".into(),
                document_title: "Essay C".into(),
                doc_type: "essay".into(),
                intent: "mark".into(),
                framework_name: "Academic".into(),
                overall_score: 45.0,
                max_possible: 100.0,
                percentage: 45.0,
                passed: Some(false),
                criterion_scores: vec![
                    CriterionRecord {
                        criterion_id: "c1".into(),
                        criterion_title: "Knowledge".into(),
                        consensus_score: 5.0,
                        max_score: 10.0,
                        percentage: 50.0,
                    },
                    CriterionRecord {
                        criterion_id: "c2".into(),
                        criterion_title: "Analysis".into(),
                        consensus_score: 4.0,
                        max_score: 10.0,
                        percentage: 40.0,
                    },
                ],
                agent_count: 4,
                debate_rounds: 1,
                evaluated_at: "2026-03-23T12:00:00Z".into(),
            },
        ]
    }

    #[test]
    fn test_batch_summary() {
        let records = test_records();
        let summary = summarise_batch(&records).unwrap();
        assert_eq!(summary.total_documents, 3);
        assert!((summary.mean_score - 63.33).abs() < 1.0);
        assert_eq!(summary.pass_count, 2);
        assert_eq!(summary.fail_count, 1);
    }

    #[test]
    fn test_batch_report() {
        let records = test_records();
        let summary = summarise_batch(&records).unwrap();
        let report = batch_report(&summary);
        assert!(report.contains("Batch Evaluation Report"));
        assert!(report.contains("Mean"));
        assert!(report.contains("Knowledge"));
    }

    #[test]
    fn test_calibration() {
        let points = vec![
            CalibrationPoint {
                document_id: "d1".into(),
                criterion_id: "c1".into(),
                expert_score: 7.0,
                system_score: 8.0,
                max_score: 10.0,
            },
            CalibrationPoint {
                document_id: "d2".into(),
                criterion_id: "c1".into(),
                expert_score: 5.0,
                system_score: 6.5,
                max_score: 10.0,
            },
            CalibrationPoint {
                document_id: "d3".into(),
                criterion_id: "c1".into(),
                expert_score: 8.0,
                system_score: 9.0,
                max_score: 10.0,
            },
        ];
        let criteria = vec![EvaluationCriterion {
            id: "c1".into(),
            title: "Knowledge".into(),
            description: None,
            max_score: 10.0,
            weight: 1.0,
            rubric_levels: vec![],
            sub_criteria: vec![],
        }];
        let result = calibrate(&points, &criteria);
        assert!(
            result.overall_bias > 0.0,
            "System should be biased high: {}",
            result.overall_bias
        );
        assert_eq!(result.data_points, 3);
    }

    #[test]
    fn test_apply_calibration() {
        let adj = CriterionAdjustment {
            criterion_id: "c1".into(),
            criterion_title: "Knowledge".into(),
            bias: 1.0,
            scale_factor: 0.9,
            data_points: 5,
        };
        let adjusted = apply_calibration(8.0, 10.0, &adj);
        // (8.0 - 1.0) * 0.9 = 6.3
        assert!((adjusted - 6.3).abs() < 0.01);
    }

    #[test]
    fn test_export_csv() {
        let records = test_records();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.csv");
        export_csv(&records, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Essay A"));
        assert!(content.contains("PASS"));
        assert!(content.contains("FAIL"));
    }

    #[test]
    fn test_export_criteria_csv() {
        let records = test_records();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("criteria.csv");
        export_criteria_csv(&records, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Knowledge"));
        assert!(content.contains("Analysis"));
    }

    #[test]
    fn test_pearson_correlation() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let y = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((pearson_correlation(&x, &y) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_distribution_buckets() {
        let records = test_records();
        let summary = summarise_batch(&records).unwrap();
        let total: usize = summary.distribution.iter().map(|d| d.count).sum();
        assert_eq!(total, 3, "All records should be in buckets");
    }
}
